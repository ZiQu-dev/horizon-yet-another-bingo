//! Advanced objectives (`horizon_bingo::server::objectives`): one_of, all_of, inverse, stats and advancement, checked
//! against expectations worked out by hand from the original's rules (card/objective/*.kt, see the module's note). The
//! facts are a fake: a player's inventory, statistics and advancements are written out in each test.
use std::collections::{HashMap, HashSet};

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::card_tile_action::CardTileAction;
use horizon_bingo::card_tile_image::{Decoration, ItemStyle};
use horizon_bingo::server::objectives::{
    advancement_done_json, card_from_objectives, expand, from_card, team_achieves, update_team, Ctx, Facts, Objective,
    Runtime, Square, Tags,
};
use horizon_bingo::server::scoring;
use horizon_bingo::server::types::{Card, Objective as PlainSquare, TeamProgress};

use horizon_bingo::update_card_tiles::CardTilesPacket;

const COAL: &str = "minecraft:coal";
const IRON: &str = "minecraft:iron_ingot";
const WHITE: &str = "minecraft:white_banner";
const RED: &str = "minecraft:red_banner";
const BLUE: &str = "minecraft:blue_banner";
const MINED_STONE: &str = "minecraft:mined|minecraft:stone";
const MINE_STONE_ADV: &str = "minecraft:story/mine_stone";

/// The fake facts: held counts, stat counters and done advancements per player.
#[derive(Default)]
struct Fake {
    held: HashMap<(u128, String), u32>,
    stats: HashMap<(u128, String), i64>,
    done: HashSet<(u128, String)>,
}

impl Fake {
    fn hold(mut self, uuid: u128, item: &str, count: u32) -> Self {
        self.held.insert((uuid, item.to_string()), count);
        self
    }
    fn stat(mut self, uuid: u128, key: &str, value: i64) -> Self {
        self.stats.insert((uuid, key.to_string()), value);
        self
    }
    fn advancement(mut self, uuid: u128, id: &str) -> Self {
        self.done.insert((uuid, id.to_string()));
        self
    }
}

impl Facts for Fake {
    fn held(&self, uuid: u128, item: &str) -> u32 {
        self.held.get(&(uuid, item.to_string())).copied().unwrap_or(0)
    }
    fn stat(&self, uuid: u128, stat_type: &str, stat_name: Option<&str>) -> Option<i64> {
        let key = format!("{stat_type}|{}", stat_name.unwrap_or(""));
        self.stats.get(&(uuid, key)).copied()
    }
    fn advancement_done(&self, uuid: u128, id: &str) -> bool {
        self.done.contains(&(uuid, id.to_string()))
    }
}

/// `#minecraft:banners` holds three banners.
fn tags() -> Tags {
    let mut tags = Tags::new();
    tags.insert("#minecraft:banners".to_string(), vec![WHITE.to_string(), RED.to_string(), BLUE.to_string()]);
    tags
}

fn achieves(objective: &Objective, members: &[u128], facts: &Fake) -> bool {
    let tags = tags();
    let ctx = Ctx { facts, tags: &tags };
    team_achieves(0, objective, members, &ctx, &mut Runtime::default())
}

fn one_of(entries: &[&str]) -> Objective {
    Objective::OneOf { entries: entries.iter().map(|e| e.to_string()).collect(), permanent: false }
}

fn all_of(entries: &[&str]) -> Objective {
    Objective::AllOf { entries: entries.iter().map(|e| e.to_string()).collect(), permanent: false }
}

fn inverse_of_coal(permanent: bool) -> Objective {
    Objective::Inverse { of: Box::new(Objective::Item { item: COAL.to_string() }), permanent }
}

fn mined_stone(min: i64, max: i64, relative: bool) -> Objective {
    Objective::Stats {
        stat_type: "minecraft:mined".to_string(),
        stat_name: Some("minecraft:stone".to_string()),
        min,
        max,
        relative,
        permanent: false,
    }
}

const P1: u128 = 1;
const P2: u128 = 2;

#[test]
fn expand_replaces_tags_and_drops_duplicates_in_order() {
    let items = expand(&[COAL.into(), "#minecraft:banners".into(), COAL.into(), "#minecraft:missing".into()], &tags());
    assert_eq!(items, vec![COAL, WHITE, RED, BLUE]);
    // A list of nothing but an unknown tag expands to nothing.
    assert!(expand(&["#minecraft:missing".into()], &tags()).is_empty());
}

#[test]
fn one_of_holds_with_any_one_entry_or_a_tag_item() {
    let objective = one_of(&["#minecraft:banners", COAL]);
    // A red banner is one of the tag's items: one is enough.
    assert!(achieves(&objective, &[P1], &Fake::default().hold(P1, RED, 1)));
    // A coal held directly counts too.
    assert!(achieves(&objective, &[P1], &Fake::default().hold(P1, COAL, 2)));
    // Holding none of them does not; a count of 0 is not held.
    assert!(!achieves(&objective, &[P1], &Fake::default().hold(P1, IRON, 5)));
    assert!(!achieves(&objective, &[P1], &Fake::default().hold(P1, RED, 0)));
}

#[test]
fn one_of_with_no_entries_holds_nothing() {
    assert!(!achieves(&one_of(&[]), &[P1], &Fake::default().hold(P1, COAL, 1)));
    assert!(!achieves(&one_of(&["#minecraft:missing"]), &[P1], &Fake::default().hold(P1, COAL, 1)));
}

#[test]
fn all_of_needs_every_entry_held_by_one_player() {
    let objective = all_of(&[COAL, IRON]);
    assert!(!achieves(&objective, &[P1], &Fake::default().hold(P1, COAL, 1)));
    assert!(achieves(&objective, &[P1], &Fake::default().hold(P1, COAL, 1).hold(P1, IRON, 1)));
    // Split between two team members does not count: the original checks one player's captures.
    let split = Fake::default().hold(P1, COAL, 1).hold(P2, IRON, 1);
    assert!(!achieves(&objective, &[P1, P2], &split));
}

#[test]
fn all_of_over_a_tag_needs_all_of_its_items() {
    let objective = all_of(&["#minecraft:banners"]);
    let two = Fake::default().hold(P1, WHITE, 1).hold(P1, RED, 1);
    assert!(!achieves(&objective, &[P1], &two));
    let three = two.hold(P1, BLUE, 1);
    assert!(achieves(&objective, &[P1], &three));
}

#[test]
fn all_of_with_nothing_to_hold_holds_nothing() {
    assert!(!achieves(&all_of(&["#minecraft:missing"]), &[P1], &Fake::default()));
    assert!(!achieves(&all_of(&[]), &[P1], &Fake::default()));
}

#[test]
fn inverse_holds_from_the_start_with_an_empty_inventory() {
    // Nobody holds coal yet, so the team holds the inverse tile at once.
    assert!(achieves(&inverse_of_coal(true), &[P1, P2], &Fake::default()));
}

#[test]
fn inverse_is_lost_while_a_member_holds_the_item_and_stays_lost_when_permanent() {
    let objective = inverse_of_coal(true);
    let tags = tags();
    let mut runtime = Runtime::default();
    let mut runtime_check = |facts: &Fake| {
        let ctx = Ctx { facts, tags: &tags };
        team_achieves(0, &objective, &[P1, P2], &ctx, &mut runtime)
    };
    assert!(runtime_check(&Fake::default()));
    assert!(!runtime_check(&Fake::default().hold(P2, COAL, 1)));
    // P2 drops the coal: permanent, so P2 was once a holder and the team does not regain the tile.
    assert!(!runtime_check(&Fake::default()));
}

#[test]
fn inverse_comes_back_when_not_permanent() {
    let objective = inverse_of_coal(false);
    let tags = tags();
    let mut runtime = Runtime::default();
    let mut check = |facts: &Fake| {
        let ctx = Ctx { facts, tags: &tags };
        team_achieves(0, &objective, &[P1], &ctx, &mut runtime)
    };
    assert!(check(&Fake::default()));
    assert!(!check(&Fake::default().hold(P1, COAL, 1)));
    assert!(check(&Fake::default()));
}

#[test]
fn stats_count_from_the_first_value_seen_when_relative() {
    let objective = mined_stone(2, 5, true);
    let tags = tags();
    let mut runtime = Runtime::default();
    let mut check = |value: i64| {
        let facts = Fake::default().stat(P1, MINED_STONE, value);
        let ctx = Ctx { facts: &facts, tags: &tags };
        team_achieves(0, &objective, &[P1], &ctx, &mut runtime)
    };
    // First seen at 10: the base is 10, so 0 counted: not within 2..=5.
    assert!(!check(10));
    // 12 - 10 = 2: the lower bound counts.
    assert!(check(12));
    // 15 - 10 = 5: the upper bound counts.
    assert!(check(15));
    // 16 - 10 = 6: out of range.
    assert!(!check(16));
}

#[test]
fn stats_absolute_when_not_relative() {
    let objective = mined_stone(2, 5, false);
    assert!(achieves(&objective, &[P1], &Fake::default().stat(P1, MINED_STONE, 3)));
    assert!(!achieves(&objective, &[P1], &Fake::default().stat(P1, MINED_STONE, 1)));
    assert!(!achieves(&objective, &[P1], &Fake::default().stat(P1, MINED_STONE, 6)));
}

#[test]
fn stats_without_a_readable_counter_hold_nothing() {
    assert!(!achieves(&mined_stone(1, 100, false), &[P1], &Fake::default()));
}

#[test]
fn advancement_is_held_by_the_player_who_has_it_done() {
    let objective = Objective::Advancement { id: MINE_STONE_ADV.to_string(), permanent: false };
    assert!(achieves(&objective, &[P1, P2], &Fake::default().advancement(P2, MINE_STONE_ADV)));
    assert!(!achieves(&objective, &[P1], &Fake::default().advancement(P2, MINE_STONE_ADV)));
}

#[test]
fn update_team_matches_the_plain_item_scoring_on_an_all_item_board() {
    // Hand-computed: p1 holds square 0's coal, p2 holds square 1's iron. Found: [0, 1], newly [0, 1].
    let card = Card {
        squares: vec![
            PlainSquare { item: COAL.into(), tier: TierLabel::S },
            PlainSquare { item: IRON.into(), tier: TierLabel::A },
            PlainSquare { item: WHITE.into(), tier: TierLabel::B },
        ],
    };
    let squares = from_card(&card);
    let p1_held: HashMap<String, u32> = [(COAL.to_string(), 1)].into();
    let p2_held: HashMap<String, u32> = [(IRON.to_string(), 1)].into();
    let facts = Fake::default().hold(P1, COAL, 1).hold(P2, IRON, 1);
    let tags = tags();
    let ctx = Ctx { facts: &facts, tags: &tags };
    let mut found = Vec::new();
    let newly = update_team(&squares, &mut found, &[P1, P2], &ctx, &mut Runtime::default(), false);
    assert_eq!(found, vec![0, 1]);
    assert_eq!(newly, vec![0, 1]);

    // The same through the existing item scoring, one player after the other.
    let mut progress = TeamProgress::default();
    scoring::score_inventory(&card, &mut progress, &p1_held);
    scoring::score_inventory(&card, &mut progress, &p2_held);
    assert_eq!(progress.found, found);
}

#[test]
fn found_squares_are_kept_when_retained_and_dropped_in_inventory_mode() {
    let squares = vec![Square {
        objective: Objective::Advancement { id: MINE_STONE_ADV.to_string(), permanent: false },
        tier: TierLabel::C,
    }];
    let tags = tags();
    let mut runtime = Runtime::default();
    let mut found = Vec::new();

    let done = Fake::default().advancement(P1, MINE_STONE_ADV);
    let ctx = Ctx { facts: &done, tags: &tags };
    assert_eq!(update_team(&squares, &mut found, &[P1], &ctx, &mut runtime, false), vec![0]);

    // The advancement is no longer reported done: retained in standard mode.
    let undone = Fake::default();
    let ctx = Ctx { facts: &undone, tags: &tags };
    assert!(update_team(&squares, &mut found, &[P1], &ctx, &mut runtime, false).is_empty());
    assert_eq!(found, vec![0]);

    // In inventory mode the square goes again.
    update_team(&squares, &mut found, &[P1], &ctx, &mut runtime, true);
    assert!(found.is_empty());
}

#[test]
fn stats_and_inverse_are_not_retained() {
    let squares = vec![Square { objective: mined_stone(2, 5, true), tier: TierLabel::B }];
    let tags = tags();
    let mut runtime = Runtime::default();
    let mut found = Vec::new();
    let step = |value: i64, found: &mut Vec<usize>, runtime: &mut Runtime| {
        let facts = Fake::default().stat(P1, MINED_STONE, value);
        let ctx = Ctx { facts: &facts, tags: &tags };
        update_team(&squares, found, &[P1], &ctx, runtime, false)
    };
    // Base 100, then 103 (counted 3): found.
    assert!(step(100, &mut found, &mut runtime).is_empty());
    assert_eq!(step(103, &mut found, &mut runtime), vec![0]);
    // 110 - 100 = 10: out of range, so the square goes.
    assert!(step(110, &mut found, &mut runtime).is_empty());
    assert!(found.is_empty());
}

#[test]
fn advancement_done_reads_vanilla_progress_json() {
    let progress = serde_json::json!({
        "DataVersion": 4000,
        "minecraft:story/root": { "criteria": { "crafting_table": "2026-01-01 10:00:00 +0000" }, "done": true },
        "minecraft:story/mine_stone": { "criteria": {}, "done": false }
    });
    assert!(advancement_done_json(&progress, "minecraft:story/root"));
    assert!(!advancement_done_json(&progress, MINE_STONE_ADV));
    assert!(!advancement_done_json(&progress, "minecraft:story/missing"));
}

#[test]
fn card_packet_carries_each_kind_as_the_client_reads_it() {
    let mut squares: Vec<Square> = (0..25)
        .map(|_| Square { objective: Objective::Item { item: IRON.into() }, tier: TierLabel::D })
        .collect();
    squares[0] = Square { objective: one_of(&["#minecraft:banners", COAL]), tier: TierLabel::S };
    squares[1] = Square { objective: inverse_of_coal(true), tier: TierLabel::A };
    squares[2] = Square {
        objective: Objective::Advancement { id: MINE_STONE_ADV.into(), permanent: false },
        tier: TierLabel::B,
    };
    squares[3] = Square { objective: mined_stone(2, 5, true), tier: TierLabel::C };
    let item_id = |name: &str| match name {
        COAL => Some(10),
        WHITE => Some(20),
        _ => None,
    };
    let (path, bytes) = card_from_objectives("bingo_red", &squares, &[0, 2], &item_id, &tags(), true).unwrap();
    assert_eq!(path, "update_card_tiles");
    let packet: CardTilesPacket = decode(&bytes, CardTilesPacket::read).unwrap();
    assert_eq!(packet.team_key.as_deref(), Some("bingo_red"));
    assert!(packet.should_notify);
    assert_eq!(packet.tiles.len(), 25);
    assert_eq!(packet.tiles[0].0, 0);
    let one_of_tile = &packet.tiles[0].1;
    // The first banner with an id is the picture; the list is every expanded item in order: white, red, blue, coal.
    assert_eq!(one_of_tile.image.item.as_ref().map(|s| s.item), Some(20));
    assert_eq!(one_of_tile.image.item_style, ItemStyle::OneOf);
    assert_eq!(one_of_tile.image.decoration, Decoration::MultiItem);
    let list: Vec<Option<i32>> = one_of_tile.image_list.iter().map(|tile| tile.item.as_ref().map(|s| s.item)).collect();
    assert_eq!(list, vec![Some(20), None, None, Some(10)]);
    assert!(one_of_tile.image.is_achieved);
    assert_eq!(one_of_tile.progress, 1.0);
    assert_eq!(one_of_tile.team_keys, vec!["bingo_red".to_string()]);
    assert_eq!(one_of_tile.item_tier, Some(TierLabel::S));
    assert_eq!(one_of_tile.action, CardTileAction::None);

    let inverse_tile = &packet.tiles[1].1;
    assert_eq!(inverse_tile.image.decoration, Decoration::Forbidden);
    assert_eq!(inverse_tile.image.item.as_ref().map(|s| s.item), Some(10));
    assert_eq!(inverse_tile.progress, 0.0);
    assert!(inverse_tile.team_keys.is_empty());

    let advancement_tile = &packet.tiles[2].1;
    assert_eq!(advancement_tile.action, CardTileAction::Advancement(MINE_STONE_ADV.to_string()));
    assert_eq!(advancement_tile.image.decoration, Decoration::Advancement);
    assert_eq!(advancement_tile.progress, 1.0);

    let stats_tile = &packet.tiles[3].1;
    assert_eq!(stats_tile.image.decoration, Decoration::Advancement);
    assert_eq!(stats_tile.action, CardTileAction::None);
    assert_eq!(stats_tile.item_tier, Some(TierLabel::C));
}

#[test]
fn card_packet_needs_25_squares() {
    let squares: Vec<Square> = (0..24)
        .map(|_| Square { objective: Objective::Item { item: IRON.into() }, tier: TierLabel::D })
        .collect();
    let error = card_from_objectives("bingo_red", &squares, &[], &|_| None, &tags(), false).unwrap_err();
    assert!(error.contains("25"), "{error}");
}

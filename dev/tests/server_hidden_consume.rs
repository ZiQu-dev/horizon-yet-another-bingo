//! Hidden-items and consume-items card modes. Expected values are written by hand from the original's code
//! (see the module docs of horizon_bingo::server::hidden_consume) and do not come from the code under test.
//! Square i needs the item "minecraft:item{i}" and has tier TIERS[i % 5] (S, A, B, C, D, S, A, ...).
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::hidden_consume::{
    capturing_player, confiscate, consume_on_score, consumes_on_score, is_hidden, record_seen, tile_views, CardMode,
    HeldStack, HiddenConsume, Sight, TileView, CONSUME_ITEMS_KEY, HIDDEN_ITEMS_KEY,
};
use horizon_bingo::server::types::{Card, Objective};
use serde_json::{json, Map, Value};

const TIERS: [TierLabel; 5] = [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D];

fn card() -> Card {
    Card {
        squares: (0..25)
            .map(|i| Objective { item: format!("minecraft:item{i}"), tier: TIERS[i % 5] })
            .collect(),
    }
}

fn hidden_on() -> HiddenConsume {
    HiddenConsume { hidden_items: true, consume_items: false }
}

fn consume_on() -> HiddenConsume {
    HiddenConsume { hidden_items: false, consume_items: true }
}

fn stack(item: &str, count: u32) -> HeldStack {
    HeldStack { item: item.to_string(), count, bingo_ignore: false, nested: Vec::new() }
}

fn oak(count: u32) -> HeldStack {
    stack("minecraft:oak_log", count)
}

fn sight<'a>(seen: &'a [usize], found: &'a [usize], finished: bool) -> Option<Sight<'a>> {
    Some(Sight { seen, found, finished })
}

// ---- options: flags, defaults and JSON -------------------------------------------------------------------------

#[test]
fn both_flags_default_to_off_and_write_the_original_keys() {
    assert_eq!(HiddenConsume::default(), HiddenConsume { hidden_items: false, consume_items: false });
    assert_eq!(
        HiddenConsume::default().to_value(),
        json!({ "isHiddenItemsMode": false, "isConsumeItemsMode": false })
    );
}

#[test]
fn the_json_keys_are_the_originals_property_names() {
    assert_eq!(HIDDEN_ITEMS_KEY, "isHiddenItemsMode");
    assert_eq!(CONSUME_ITEMS_KEY, "isConsumeItemsMode");
}

#[test]
fn hidden_items_on_writes_true_for_that_key_only() {
    assert_eq!(
        hidden_on().to_value(),
        json!({ "isHiddenItemsMode": true, "isConsumeItemsMode": false })
    );
    assert_eq!(
        consume_on().to_value(),
        json!({ "isHiddenItemsMode": false, "isConsumeItemsMode": true })
    );
}

#[test]
fn reads_both_flags_from_the_card_object() {
    let value = json!({ "isHiddenItemsMode": true, "isConsumeItemsMode": true });
    assert_eq!(HiddenConsume::from_value(&value), HiddenConsume { hidden_items: true, consume_items: true });
}

#[test]
fn a_missing_flag_is_off_and_other_keys_are_ignored() {
    let value = json!({ "isLockoutMode": true, "isInventoryMode": true, "isHiddenItemsMode": true });
    assert_eq!(HiddenConsume::from_value(&value), hidden_on());
}

#[test]
fn a_non_boolean_flag_is_off() {
    let value = json!({ "isHiddenItemsMode": "yes", "isConsumeItemsMode": 1 });
    assert_eq!(HiddenConsume::from_value(&value), HiddenConsume::default());
}

#[test]
fn a_value_that_is_not_an_object_gives_both_off() {
    assert_eq!(HiddenConsume::from_value(&json!(true)), HiddenConsume::default());
    assert_eq!(HiddenConsume::from_value(&json!(null)), HiddenConsume::default());
}

#[test]
fn all_four_combinations_round_trip() {
    for hidden_items in [false, true] {
        for consume_items in [false, true] {
            let modes = HiddenConsume { hidden_items, consume_items };
            assert_eq!(HiddenConsume::from_value(&modes.to_value()), modes);
        }
    }
}

#[test]
fn write_into_keeps_the_other_keys_of_the_object() {
    let mut object = Map::new();
    object.insert("goal".to_string(), json!({ "type": "lines", "n": 1 }));
    hidden_on().write_into(&mut object);
    assert_eq!(
        Value::Object(object),
        json!({
            "goal": { "type": "lines", "n": 1 },
            "isHiddenItemsMode": true,
            "isConsumeItemsMode": false
        })
    );
}

#[test]
fn read_from_an_object_with_both_flags_missing_is_off() {
    let object: Map<String, Value> = Map::new();
    assert_eq!(HiddenConsume::read_from(&object), HiddenConsume::default());
}

#[test]
fn set_changes_only_the_named_flag() {
    let mut modes = HiddenConsume::default();
    modes.set(CardMode::HiddenItems, true);
    assert_eq!(modes, hidden_on());
    modes.set(CardMode::ConsumeItems, true);
    assert_eq!(modes, HiddenConsume { hidden_items: true, consume_items: true });
    modes.set(CardMode::HiddenItems, false);
    assert_eq!(modes, consume_on());
}

// ---- hidden items: who sees which square ----------------------------------------------------------------------

#[test]
fn with_the_mode_off_every_square_shows_for_everyone() {
    let views = tile_views(&card(), HiddenConsume::default(), false, None, false);
    assert_eq!(views.len(), 25);
    for (i, view) in views.iter().enumerate() {
        assert_eq!(
            *view,
            TileView {
                index: i,
                hidden: false,
                item: Some(format!("minecraft:item{i}")),
                tier: Some(TIERS[i % 5]),
            }
        );
    }
}

#[test]
fn with_the_mode_off_a_viewer_who_has_seen_nothing_still_sees_all() {
    let views = tile_views(&card(), HiddenConsume::default(), false, sight(&[], &[], false), false);
    assert!(views.iter().all(|v| !v.hidden && v.item.is_some() && v.tier.is_some()));
}

#[test]
fn a_spectator_on_no_team_sees_every_square_hidden_without_tiers() {
    let views = tile_views(&card(), hidden_on(), false, None, false);
    assert_eq!(views.len(), 25);
    for view in &views {
        assert!(view.hidden, "square {} should be hidden", view.index);
        assert_eq!(view.item, None);
        assert_eq!(view.tier, None);
    }
}

#[test]
fn show_hidden_tiers_gives_the_tier_of_a_hidden_square_but_not_its_item() {
    let views = tile_views(&card(), hidden_on(), true, None, false);
    assert!(views.iter().all(|v| v.hidden && v.item.is_none()));
    // index 0 -> S, 1 -> A, 2 -> B, 3 -> C, 4 -> D, 5 -> S, 24 -> D (24 % 5 == 4)
    assert_eq!(views[0].tier, Some(TierLabel::S));
    assert_eq!(views[1].tier, Some(TierLabel::A));
    assert_eq!(views[4].tier, Some(TierLabel::D));
    assert_eq!(views[5].tier, Some(TierLabel::S));
    assert_eq!(views[24].tier, Some(TierLabel::D));
}

#[test]
fn a_team_sees_only_the_squares_it_has_seen() {
    let views = tile_views(&card(), hidden_on(), false, sight(&[3, 12], &[], false), false);
    let shown: Vec<usize> = views.iter().filter(|v| !v.hidden).map(|v| v.index).collect();
    assert_eq!(shown, vec![3, 12]);
    assert_eq!(views[3].item.as_deref(), Some("minecraft:item3"));
    assert_eq!(views[3].tier, Some(TierLabel::C)); // 3 % 5 == 3
    assert_eq!(views[12].item.as_deref(), Some("minecraft:item12"));
    assert_eq!(views[12].tier, Some(TierLabel::B)); // 12 % 5 == 2
    // a hidden square has neither item nor tier (show_hidden_tiers is off)
    assert_eq!(views[0].item, None);
    assert_eq!(views[0].tier, None);
}

#[test]
fn a_found_square_counts_as_seen_even_if_teams_seen_lacks_it() {
    let views = tile_views(&card(), hidden_on(), false, sight(&[], &[7], false), false);
    let shown: Vec<usize> = views.iter().filter(|v| !v.hidden).map(|v| v.index).collect();
    assert_eq!(shown, vec![7]);
}

#[test]
fn seen_and_found_squares_are_both_shown() {
    let views = tile_views(&card(), hidden_on(), false, sight(&[1], &[2], false), false);
    let shown: Vec<usize> = views.iter().filter(|v| !v.hidden).map(|v| v.index).collect();
    assert_eq!(shown, vec![1, 2]);
}

#[test]
fn a_hidden_square_shows_its_tier_only_with_show_hidden_tiers_and_the_visible_one_always() {
    // team has seen square 0 (tier S); square 1 (tier A) is hidden
    let views = tile_views(&card(), hidden_on(), false, sight(&[0], &[], false), false);
    assert_eq!(views[0].tier, Some(TierLabel::S));
    assert_eq!(views[1].tier, None);
    let with_tiers = tile_views(&card(), hidden_on(), true, sight(&[0], &[], false), false);
    assert_eq!(with_tiers[1].tier, Some(TierLabel::A));
    assert_eq!(with_tiers[1].item, None);
}

#[test]
fn postgame_reveals_everything_even_to_a_spectator() {
    let views = tile_views(&card(), hidden_on(), false, None, true);
    assert!(views.iter().all(|v| !v.hidden && v.item.is_some() && v.tier.is_some()));
}

#[test]
fn a_finished_team_sees_everything() {
    let views = tile_views(&card(), hidden_on(), false, sight(&[], &[], true), false);
    assert!(views.iter().all(|v| !v.hidden));
    assert_eq!(views[24].item.as_deref(), Some("minecraft:item24"));
}

#[test]
fn is_hidden_truth_table() {
    // hidden on, square 5, nothing seen, playing
    assert!(is_hidden(true, 5, sight(&[], &[], false), false));
    // hidden on, square 5, no team
    assert!(is_hidden(true, 5, None, false));
    // hidden on, square 5 seen
    assert!(!is_hidden(true, 5, sight(&[5], &[], false), false));
    // hidden on, square 5 found (teamsAchieved)
    assert!(!is_hidden(true, 5, sight(&[], &[5], false), false));
    // hidden on, square 5 unseen but the team has finished
    assert!(!is_hidden(true, 5, sight(&[], &[], true), false));
    // hidden on, no team, but postgame
    assert!(!is_hidden(true, 5, None, true));
    // hidden off: nothing is hidden
    assert!(!is_hidden(false, 5, None, false));
}

#[test]
fn a_square_seen_by_another_team_is_still_hidden_for_this_one() {
    // Team A has seen square 9; Team B's view is built from its own seen list, which is empty.
    let team_a = tile_views(&card(), hidden_on(), false, sight(&[9], &[], false), false);
    let team_b = tile_views(&card(), hidden_on(), false, sight(&[], &[], false), false);
    assert!(!team_a[9].hidden);
    assert!(team_b[9].hidden);
}

// ---- seen is additive ------------------------------------------------------------------------------------------

#[test]
fn record_seen_keeps_order_and_skips_repeats() {
    let mut seen = vec![4, 1];
    record_seen(&mut seen, &[1, 9, 4, 9]);
    assert_eq!(seen, vec![4, 1, 9]);
}

#[test]
fn record_seen_with_nothing_held_changes_nothing() {
    let mut seen = vec![2, 7];
    record_seen(&mut seen, &[]);
    assert_eq!(seen, vec![2, 7]);
}

#[test]
fn a_seen_square_stays_seen_after_its_item_leaves_the_inventory() {
    let mut seen = Vec::new();
    record_seen(&mut seen, &[2]); // the item is held
    record_seen(&mut seen, &[]); // the item has left the inventory: nothing new held
    assert_eq!(seen, vec![2]);
    // In inventory mode the found list shrinks, but the square is still seen.
    let views = tile_views(&card(), hidden_on(), false, sight(&seen, &[], false), false);
    assert!(!views[2].hidden);
}

// ---- consume items: when it applies -----------------------------------------------------------------------------

#[test]
fn consume_applies_only_when_on_outside_inventory_mode_and_in_lobby_mode() {
    // (consume on, inventory mode, lobby mode) -> applies
    let cases = [
        ((true, false, true), true),
        ((true, false, false), false),
        ((true, true, true), false),
        ((true, true, false), false),
        ((false, false, true), false),
        ((false, true, true), false),
        ((false, false, false), false),
        ((false, true, false), false),
    ];
    for ((consume, inventory, lobby), expected) in cases {
        let modes = HiddenConsume { hidden_items: false, consume_items: consume };
        assert_eq!(
            consumes_on_score(modes, inventory, lobby),
            expected,
            "consume={consume} inventory={inventory} lobby={lobby}"
        );
    }
}

#[test]
fn hidden_items_does_not_make_consume_apply() {
    let modes = HiddenConsume { hidden_items: true, consume_items: false };
    assert!(!consumes_on_score(modes, false, true));
}

#[test]
fn with_consume_off_scoring_takes_nothing() {
    let mut stacks = vec![oak(10)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 1, HiddenConsume::default(), false, true);
    assert!(taken.is_empty());
    assert_eq!(stacks, vec![oak(10)]);
}

#[test]
fn in_inventory_mode_scoring_takes_nothing() {
    let mut stacks = vec![oak(10)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 1, consume_on(), true, true);
    assert!(taken.is_empty());
    assert_eq!(stacks, vec![oak(10)]);
}

#[test]
fn outside_lobby_mode_scoring_takes_nothing() {
    let mut stacks = vec![oak(10)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 1, consume_on(), false, false);
    assert!(taken.is_empty());
    assert_eq!(stacks, vec![oak(10)]);
}

// ---- consume items: what is removed -----------------------------------------------------------------------------

#[test]
fn one_item_comes_out_of_a_stack_of_ten() {
    let mut stacks = vec![oak(10)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 1, consume_on(), false, true);
    assert_eq!(taken, vec![("minecraft:oak_log".to_string(), 1)]);
    assert_eq!(stacks, vec![oak(9)]);
}

#[test]
fn three_items_come_from_the_first_stack_then_the_next_in_slot_order() {
    // oak 1 is taken whole, then 2 from the oak stack after the stone
    let mut stacks = vec![oak(1), stack("minecraft:stone", 4), oak(5)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 3, consume_on(), false, true);
    assert_eq!(
        taken,
        vec![("minecraft:oak_log".to_string(), 1), ("minecraft:oak_log".to_string(), 2)]
    );
    assert_eq!(stacks, vec![oak(0), stack("minecraft:stone", 4), oak(3)]);
}

#[test]
fn a_stack_of_exactly_the_needed_count_is_emptied() {
    let mut stacks = vec![oak(3)];
    let taken = confiscate(&mut stacks, "minecraft:oak_log", 3);
    assert_eq!(taken, vec![("minecraft:oak_log".to_string(), 3)]);
    assert_eq!(stacks, vec![oak(0)]);
}

#[test]
fn when_the_player_has_too_few_the_rest_is_simply_not_there() {
    let mut stacks = vec![oak(2)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 5, consume_on(), false, true);
    assert_eq!(taken, vec![("minecraft:oak_log".to_string(), 2)]);
    assert_eq!(stacks, vec![oak(0)]);
}

#[test]
fn a_stack_tagged_bingo_ignore_is_never_taken() {
    let mut ignored = oak(4);
    ignored.bingo_ignore = true;
    let mut stacks = vec![ignored.clone(), oak(4)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 2, consume_on(), false, true);
    assert_eq!(taken, vec![("minecraft:oak_log".to_string(), 2)]);
    assert_eq!(stacks, vec![ignored, oak(2)]);
}

#[test]
fn other_items_are_left_alone() {
    let mut stacks = vec![stack("minecraft:stone", 9)];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 1, consume_on(), false, true);
    assert!(taken.is_empty());
    assert_eq!(stacks, vec![stack("minecraft:stone", 9)]);
}

#[test]
fn a_bundle_is_not_taken_but_its_matching_contents_are() {
    let mut bundle = stack("minecraft:bundle", 1);
    bundle.nested = vec![oak(3)];
    let mut stacks = vec![bundle];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 2, consume_on(), false, true);
    assert_eq!(taken, vec![("minecraft:oak_log".to_string(), 2)]);
    assert_eq!(stacks[0].count, 1);
    assert_eq!(stacks[0].nested, vec![oak(1)]);
}

#[test]
fn a_matching_stack_is_visited_before_its_contents() {
    // outer oak 2 is taken whole (remaining 1), then the nested oak 5 gives 1
    let mut outer = oak(2);
    outer.nested = vec![oak(5)];
    let mut stacks = vec![outer];
    let taken = consume_on_score(&mut stacks, "minecraft:oak_log", 3, consume_on(), false, true);
    assert_eq!(
        taken,
        vec![("minecraft:oak_log".to_string(), 2), ("minecraft:oak_log".to_string(), 1)]
    );
    assert_eq!(stacks[0].count, 0);
    assert_eq!(stacks[0].nested, vec![oak(4)]);
}

#[test]
fn an_empty_stack_gives_nothing_and_is_not_listed() {
    let mut stacks = vec![oak(0), oak(2)];
    let taken = confiscate(&mut stacks, "minecraft:oak_log", 2);
    assert_eq!(taken, vec![("minecraft:oak_log".to_string(), 2)]);
    assert_eq!(stacks, vec![oak(0), oak(0)]);
}

#[test]
fn a_zero_count_takes_nothing() {
    let mut stacks = vec![oak(4)];
    assert!(confiscate(&mut stacks, "minecraft:oak_log", 0).is_empty());
    assert_eq!(stacks, vec![oak(4)]);
}

#[test]
fn the_capturing_player_is_the_team_member_with_the_latest_capture() {
    assert_eq!(capturing_player(&[(1, 100), (2, 300), (3, 300)], &[1, 2, 3]), Some(2));
}

#[test]
fn a_capture_by_someone_off_the_team_does_not_count() {
    assert_eq!(capturing_player(&[(9, 500), (1, 100)], &[1, 2]), Some(1));
}

#[test]
fn no_capture_by_a_member_gives_no_player() {
    assert_eq!(capturing_player(&[(9, 500)], &[1]), None);
    assert_eq!(capturing_player(&[], &[1]), None);
}

#[test]
fn on_equal_instants_the_first_capture_listed_wins() {
    assert_eq!(capturing_player(&[(5, 50), (6, 50)], &[5, 6]), Some(5));
    assert_eq!(capturing_player(&[(6, 50), (5, 50)], &[5, 6]), Some(6));
}

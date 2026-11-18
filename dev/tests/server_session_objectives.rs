//! Session with objective squares: a card whose squares are the original's challenges (all of, a statistic, an inverse
//! of an item and an inverse of a statistic) and a vanilla advancement, next to plain items. The fake host gives the
//! inventories, statistics and advancements the session reads. Expected values are counted by hand: a statistic
//! counts from the first value seen (an absent counter reads 0, as Horizon's `native_player_stat` gives it), the
//! poll runs every 20 ticks, and an inverse is lost for good once a member has held what it forbids.
use std::collections::{HashMap, HashSet};

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::CardTile;
use horizon_bingo::card_tile_image::Decoration;
use horizon_bingo::server::card_gen::{load_pool, Pool};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::objectives::with_objectives;
use horizon_bingo::server::options::{Mode, Options};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const TIER_LISTS_ADVANCEMENTS: &str = horizon_bingo::data::advancements_tierlist_json::JSON;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const SEED: u64 = 7;

const COPPER: &str = "objective:yet-another-minecraft-bingo:challenge/all_copper_armor";
const CREEPER: &str = "objective:yet-another-minecraft-bingo:challenge/stat_killed_creeper";
const FLINT: &str = "objective:yet-another-minecraft-bingo:challenge/never_get_flint_and_steel";
const SKELETON: &str = "objective:yet-another-minecraft-bingo:challenge/never_killed_by_skeleton";
const ARBALISTIC: &str = "objective:minecraft:adventure/arbalistic";

/// The tile ids of the five squares above, as `objectives::describe` names them.
const COPPER_ID: &str = "minecraft:copper_boots, minecraft:copper_chestplate, minecraft:copper_helmet, minecraft:copper_leggings";
const CREEPER_ID: &str = "minecraft:creeper";
const FLINT_ID: &str = "minecraft:flint_and_steel";
const SKELETON_ID: &str = "minecraft:skeleton";
const ARBALISTIC_ID: &str = "minecraft:adventure/arbalistic";

const KILLED: &str = "minecraft:killed";
const KILLED_BY: &str = "minecraft:killed_by";

#[derive(Debug, Clone)]
struct Sent {
    player: u128,
    path: String,
    payload: Vec<u8>,
}

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<Sent>,
    broadcasts: Vec<String>,
    commands: Vec<String>,
    files: HashMap<String, String>,
    /// Counters by (player, stat type, stat name). An absent counter reads 0.
    stats: HashMap<(u128, String, String), i64>,
    advancements: HashSet<(u128, String)>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push(Sent { player, path: path.to_string(), payload });
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, line: &str) {
        self.broadcasts.push(line.to_string());
    }

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn command(&mut self, line: &str) {
        self.commands.push(line.to_string());
    }

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }

    fn stat_value(&self, player: u128, stat_type: &str, stat_name: Option<&str>) -> Option<i64> {
        let name = stat_name?;
        Some(self.stats.get(&(player, stat_type.to_string(), name.to_string())).copied().unwrap_or(0))
    }

    fn has_advancement(&self, player: u128, id: &str) -> bool {
        self.advancements.contains(&(player, id.to_string()))
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(rules: Options) -> SessionOptions {
    SessionOptions {
        seed: SEED,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        rules,
        ..SessionOptions::default()
    }
}

/// The 25 squares of the test card: the five objective squares, then 20 plain items. The first nine are tier C, the
/// rest tier D, so the card holds every square of the pool.
fn card_keys() -> Vec<String> {
    let mut keys: Vec<String> = [COPPER, CREEPER, FLINT, SKELETON, ARBALISTIC].iter().map(|key| key.to_string()).collect();
    keys.extend((0..20).map(|i| format!("minecraft:plain_{i}")));
    keys
}

fn card_pool() -> Pool {
    let keys = card_keys();
    Pool {
        tiers: [vec![], vec![], vec![], keys[..9].to_vec(), keys[9..].to_vec()],
        weights: [vec![], vec![], vec![], vec![1.0; 9], vec![1.0; 16]],
        groups: Vec::new(),
        skipped_tags: Vec::new(),
        tag_files: 0,
    }
}

/// Alice on red, Bob on blue, both online and both reported an empty inventory (as the server sees them when they join).
/// The game is started; what the start sent is kept.
fn start(host: &mut FakeHost, rules: Options) -> Session {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(rules));
    session.set_pool(card_pool());
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(session.command(host, Some(ALICE), "join red").starts_with("You joined"));
    assert!(session.command(host, Some(BOB), "join blue").starts_with("You joined"));
    host.sent.clear();
    host.commands.clear();
    // `ignore_warnings`: the lockout rounds here have two teams and the lines goal, which the original's start refuses
    // without it (`CommandStartLockoutWithLinesGoal`). Every player is on a team, so nothing else changes.
    assert_eq!(session.command(host, None, "start ignore_warnings"), "Game started.");
    for uuid in [ALICE, BOB] {
        session.on_inventory(host, uuid, &HashMap::new());
    }
    session
}

fn inventory(items: &[&str]) -> HashMap<String, u32> {
    items.iter().map(|item| (item.to_string(), 1)).collect()
}

/// Twenty ticks: one poll of the objective squares falls inside them.
fn poll(host: &mut FakeHost, session: &mut Session) {
    for _ in 0..20 {
        session.tick(host);
    }
}

/// The last card packet a player was sent.
fn last_tiles(host: &FakeHost, player: u128) -> CardTilesPacket {
    let payload = host
        .sent
        .iter()
        .filter(|s| s.player == player && s.path == "update_card_tiles")
        .map(|s| s.payload.clone())
        .last()
        .expect("a card packet was sent");
    decode(&payload, CardTilesPacket::read).expect("update_card_tiles decodes")
}

/// The tile whose id is `id`. Each id is on the card once.
fn tile<'a>(packet: &'a CardTilesPacket, id: &str) -> &'a CardTile {
    let found: Vec<&CardTile> = packet.tiles.iter().map(|(_, tile)| tile).filter(|tile| tile.id.as_deref() == Some(id)).collect();
    assert_eq!(found.len(), 1, "one tile named {id}");
    found[0]
}

fn is_found(packet: &CardTilesPacket, id: &str) -> bool {
    tile(packet, id).progress == 1.0
}

fn announced(host: &FakeHost, line: &str) -> bool {
    host.broadcasts.iter().any(|sent| sent == line)
}

#[test]
fn each_kind_of_square_scores_in_a_running_game() {
    let mut host = FakeHost::default();
    host.stats.insert((ALICE, KILLED.into(), "minecraft:creeper".into()), 0);
    let mut session = start(&mut host, Options::default());

    // At the start nobody holds the forbidden items, so both inverse squares are held at once.
    let at_start = last_tiles(&host, ALICE);
    assert!(is_found(&at_start, FLINT_ID));
    assert!(is_found(&at_start, SKELETON_ID));
    assert!(!is_found(&at_start, COPPER_ID));
    assert!(!is_found(&at_start, CREEPER_ID));
    assert!(!is_found(&at_start, ARBALISTIC_ID));

    // An all-of square needs every entry in one player's inventory.
    session.on_inventory(&mut host, ALICE, &inventory(&["minecraft:copper_boots", "minecraft:copper_chestplate", "minecraft:copper_helmet"]));
    assert!(!is_found(&last_tiles(&host, ALICE), COPPER_ID));
    session.on_inventory(&mut host, ALICE, &inventory(&["minecraft:copper_boots", "minecraft:copper_chestplate", "minecraft:copper_helmet", "minecraft:copper_leggings"]));
    assert!(is_found(&last_tiles(&host, ALICE), COPPER_ID));

    // A statistic counts from the value first seen (0): one kill is enough, found on the poll.
    host.stats.insert((ALICE, KILLED.into(), "minecraft:creeper".into()), 1);
    poll(&mut host, &mut session);
    assert!(is_found(&last_tiles(&host, ALICE), CREEPER_ID));

    // An advancement is held when it is done, found on the poll.
    host.advancements.insert((ALICE, ARBALISTIC_ID.into()));
    poll(&mut host, &mut session);
    assert!(is_found(&last_tiles(&host, ALICE), ARBALISTIC_ID));

    // A member killed by a skeleton loses the inverse square.
    host.stats.insert((ALICE, KILLED_BY.into(), "minecraft:skeleton".into()), 1);
    poll(&mut host, &mut session);
    assert!(!is_found(&last_tiles(&host, ALICE), SKELETON_ID));
    assert!(announced(&host, "Alice got minecraft:adventure/arbalistic"));
}

#[test]
fn an_inverse_is_lost_for_good_when_a_member_holds_the_item() {
    let mut host = FakeHost::default();
    let mut session = start(&mut host, Options::default());
    assert!(is_found(&last_tiles(&host, ALICE), FLINT_ID));

    session.on_inventory(&mut host, ALICE, &inventory(&["minecraft:flint_and_steel"]));
    assert!(!is_found(&last_tiles(&host, ALICE), FLINT_ID));

    // Dropping the item does not bring the square back: the inverse is permanent.
    session.on_inventory(&mut host, ALICE, &inventory(&[]));
    assert!(!is_found(&last_tiles(&host, ALICE), FLINT_ID));
    // Bob's team is not Alice's: his square is still held.
    assert!(is_found(&last_tiles(&host, BOB), FLINT_ID));
}

#[test]
fn in_lockout_the_first_team_to_hold_a_square_keeps_it() {
    let mut host = FakeHost::default();
    let mut session = start(&mut host, Options { mode: Mode::Lockout, ..Options::default() });
    // Red holds the advancement first and keeps it; blue, holding it later, is refused.
    host.advancements.insert((ALICE, ARBALISTIC_ID.into()));
    poll(&mut host, &mut session);
    assert!(is_found(&last_tiles(&host, ALICE), ARBALISTIC_ID));
    host.advancements.insert((BOB, ARBALISTIC_ID.into()));
    poll(&mut host, &mut session);
    assert!(!is_found(&last_tiles(&host, BOB), ARBALISTIC_ID));
    assert!(is_found(&last_tiles(&host, ALICE), ARBALISTIC_ID));
    assert!(!announced(&host, "Bob got minecraft:adventure/arbalistic"));
}

#[test]
fn a_restored_game_keeps_its_squares_and_the_statistics_baseline() {
    let mut host = FakeHost::default();
    let mut session = start(&mut host, Options::default());
    host.stats.insert((ALICE, KILLED.into(), "minecraft:creeper".into()), 1);
    poll(&mut host, &mut session);
    assert!(is_found(&last_tiles(&host, ALICE), CREEPER_ID));
    let saved = session.snapshot_json();
    assert!(saved.contains("objectiveRuntime"), "an objective round saves its runtime");

    // The restarted server: the counter is still 1. Without the saved baseline (0) it would read as no kill, and the
    // square would be lost on the first read.
    let mut restarted_host = FakeHost { online: vec![player(ALICE, "Alice"), player(BOB, "Bob")], stats: host.stats.clone(), files: host.files.clone(), ..FakeHost::default() };
    let mut restarted = Session::new(options(Options::default()));
    restarted.set_pool(card_pool());
    restarted.restore_from_json(&saved).expect("the saved game restores");
    for p in restarted_host.online.clone() {
        restarted.on_join(&mut restarted_host, &p);
    }
    assert!(is_found(&last_tiles(&restarted_host, ALICE), CREEPER_ID));
    restarted.on_inventory(&mut restarted_host, ALICE, &HashMap::new());
    assert!(is_found(&last_tiles(&restarted_host, ALICE), CREEPER_ID));
}

#[test]
fn a_team_waits_for_every_online_member_to_report_an_inventory() {
    let mut host = FakeHost::default();
    let session = start(&mut host, Options::default());
    let saved = session.snapshot_json();

    // Restored, nobody has reported yet: the squares are not read, so an advancement is not scored on the poll.
    let mut restarted_host = FakeHost { online: vec![player(ALICE, "Alice"), player(BOB, "Bob")], ..FakeHost::default() };
    let mut restarted = Session::new(options(Options::default()));
    restarted.set_pool(card_pool());
    restarted.restore_from_json(&saved).expect("the saved game restores");
    for p in restarted_host.online.clone() {
        restarted.on_join(&mut restarted_host, &p);
    }
    restarted_host.advancements.insert((ALICE, ARBALISTIC_ID.into()));
    poll(&mut restarted_host, &mut restarted);
    assert!(!is_found(&last_tiles(&restarted_host, ALICE), ARBALISTIC_ID));

    // Once Alice and Bob have reported, the poll scores it.
    restarted.on_inventory(&mut restarted_host, ALICE, &HashMap::new());
    restarted.on_inventory(&mut restarted_host, BOB, &HashMap::new());
    assert!(is_found(&last_tiles(&restarted_host, ALICE), ARBALISTIC_ID));
}

#[test]
fn the_objectives_option_is_saved_only_when_on() {
    let plain = store::options_to_json(&Options::default());
    assert!(!plain.contains("objectives"), "a round without the option keeps its text");
    let on = Options { objectives: true, ..Options::default() };
    let text = store::options_to_json(&on);
    assert!(text.contains("\"objectives\": true"));
    assert!(store::options_from_json(&text).objectives);
    assert!(!store::options_from_json(&plain).objectives);
}

#[test]
fn a_plain_item_round_saves_no_objective_keys() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    // The built-in pool, which has items only, and the option off.
    let mut session = Session::new(options(Options::default()));
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    assert!(session.command(&mut host, Some(ALICE), "join red").starts_with("You joined"));
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    assert!(!session.snapshot_json().contains("objectiveRuntime"));
}

#[test]
fn the_pool_with_objectives_adds_the_advancements_and_scorable_challenges_only() {
    let plain = load_pool();
    let pool = with_objectives(&plain);
    let keys: Vec<&String> = pool.tiers.iter().flatten().filter(|entry| entry.starts_with("objective:")).collect();
    // Every advancement of the original's advancements tier list (123 in all, counted from the file here), and the 60
    // challenges the port can score (36 statistics, 16 all-of, 8 inverses; the item and NBT challenges are left out).
    let advancements: serde_json::Value = serde_json::from_str(TIER_LISTS_ADVANCEMENTS).unwrap();
    let advancement_count: usize = ["S", "A", "B", "C", "D"].iter().map(|tier| advancements[tier].as_array().unwrap().len()).sum();
    assert_eq!(advancement_count, 123);
    assert_eq!(keys.iter().filter(|key| key.starts_with("objective:minecraft:")).count(), advancement_count);
    assert_eq!(keys.iter().filter(|key| key.starts_with("objective:yet-another-minecraft-bingo:challenge/")).count(), 60);
    assert!(keys.iter().any(|key| key.as_str() == COPPER));
    assert!(!keys.iter().any(|key| key.contains("many_coal") || key.contains("nbt_mending_book")));
    // The plain items stay as they were.
    assert!(plain.tiers.iter().flatten().all(|item| pool.tiers.iter().flatten().any(|entry| entry == item)));
}

#[test]
fn a_round_with_the_option_on_draws_squares_that_are_not_items() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(Options { objectives: true, ..Options::default() }));
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    assert!(session.command(&mut host, Some(ALICE), "join red").starts_with("You joined"));
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    let packet = last_tiles(&host, ALICE);
    let non_items = packet
        .tiles
        .iter()
        .filter(|(_, tile)| tile.image.decoration != Decoration::None || !tile.image_list.is_empty())
        .count();
    assert!(non_items >= 1, "a card of advanced squares has at least one non-item square");
}

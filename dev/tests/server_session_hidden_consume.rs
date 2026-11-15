//! Hidden-items and consume-items modes in the server session: which squares a team's players are sent, when a square
//! is revealed (found, postgame, kept after its item leaves), the `clear` commands that consume items queue, and the
//! flags saved with the round. The card of a seed is the same in every session, so the item names come from a round
//! without the modes; a hidden square carries no item. Expected values are written out by hand.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::CardTile;
use horizon_bingo::card_tile_action::CardTileAction;
use horizon_bingo::server::commands::{parse, requires_op, usage, Command};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::hidden_consume::{CardMode, HiddenConsume};
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::options::{Mode, Options};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const SEED: u64 = 7;

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
    commands: Vec<String>,
    /// The mod's data folder: file name to text.
    files: HashMap<String, String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push(Sent { player, path: path.to_string(), payload });
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    /// The id of an item is the length of its name, so a test can predict it.
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
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(seed: u64, rules: Options) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        rules,
        ..SessionOptions::default()
    }
}

fn card_rules(hidden_items: bool, consume_items: bool, mode: Mode) -> Options {
    Options { mode, hidden_consume: HiddenConsume { hidden_items, consume_items }, ..Options::default() }
}

/// Alice on red and Bob on blue, both online. Drains what was sent and the commands.
fn lobby(host: &mut FakeHost, session: &mut Session) {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(session.command(host, Some(ALICE), "join red").starts_with("You joined"));
    assert!(session.command(host, Some(BOB), "join blue").starts_with("You joined"));
    host.sent.clear();
    host.commands.clear();
}

/// A started game under `rules`. What the start sent is kept.
fn start(rules: Options) -> (FakeHost, Session) {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(SEED, rules));
    lobby(&mut host, &mut session);
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    (host, session)
}

fn payloads(host: &FakeHost, player: u128, path: &str) -> Vec<Vec<u8>> {
    host.sent.iter().filter(|s| s.player == player && s.path == path).map(|s| s.payload.clone()).collect()
}

/// The one payload of this kind sent to this player.
fn only(host: &FakeHost, player: u128, path: &str) -> Vec<u8> {
    let found = payloads(host, player, path);
    assert_eq!(found.len(), 1, "one {path} for player {player}");
    found[0].clone()
}

fn tiles_of(payload: &[u8]) -> CardTilesPacket {
    decode(payload, CardTilesPacket::read).expect("update_card_tiles decodes")
}

/// The last card packet a player was sent.
fn last_tiles(host: &FakeHost, player: u128) -> CardTilesPacket {
    let all = payloads(host, player, "update_card_tiles");
    tiles_of(all.last().expect("a card packet was sent"))
}

fn tile(packet: &CardTilesPacket, index: i32) -> &CardTile {
    &packet.tiles.iter().find(|(i, _)| *i == index).expect("the square is in the packet").1
}

/// The item of each square of the card for `SEED`, from a round where nothing is hidden.
fn card_items() -> Vec<String> {
    let (host, _) = start(Options::default());
    let packet = tiles_of(&only(&host, ALICE, "update_card_tiles"));
    packet.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every square shows its item")).collect()
}

/// The held items of an inventory: each named item, once.
fn held(items: &[String]) -> HashMap<String, u32> {
    items.iter().map(|item| (item.clone(), 1)).collect()
}

fn path_of(item: &str) -> &str {
    item.strip_prefix("minecraft:").unwrap_or(item)
}

#[test]
fn with_the_mode_off_every_square_is_sent_with_its_item() {
    let (host, _) = start(Options::default());
    let packet = tiles_of(&only(&host, ALICE, "update_card_tiles"));
    assert_eq!(packet.tiles.len(), 25);
    assert!(packet.tiles.iter().all(|(_, tile)| !tile.is_hidden && tile.id.is_some()));
}

#[test]
fn at_the_start_every_square_is_hidden_and_no_item_name_is_sent() {
    let items = card_items();
    let (host, _) = start(card_rules(true, false, Mode::Standard));
    let payload = only(&host, ALICE, "update_card_tiles");
    let packet = tiles_of(&payload);
    assert_eq!(packet.tiles.len(), 25);
    for (index, tile) in &packet.tiles {
        assert!(tile.is_hidden, "square {index} is hidden");
        assert!(tile.id.is_none() && tile.name.is_none() && tile.lore.is_empty(), "square {index} has no item text");
        assert!(tile.image.item.is_none(), "square {index} has no picture");
        assert!(matches!(tile.action, CardTileAction::None), "square {index} has no action");
        assert_eq!(tile.item_tier, None, "square {index} shows no tier while hidden tiers are off");
        assert!(tile.team_keys.is_empty());
    }
    let text = String::from_utf8_lossy(&payload);
    for item in &items {
        assert!(!text.contains(path_of(item)), "the packet does not name {item}");
    }
}

#[test]
fn a_find_reveals_its_square_to_the_finding_team_and_not_to_the_other() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(true, false, Mode::Standard));
    host.sent.clear();

    session.on_inventory(&mut host, ALICE, &held(&items[0..1]));

    let alice = tiles_of(&only(&host, ALICE, "update_card_tiles"));
    let revealed = tile(&alice, 0);
    assert!(!revealed.is_hidden);
    assert_eq!(revealed.id.as_deref(), Some(items[0].as_str()));
    assert!(alice.tiles.iter().filter(|(index, _)| *index != 0).all(|(_, tile)| tile.is_hidden));

    // Blue (Bob) is sent its own card after the find, and square 0 stays hidden for it.
    let bob = last_tiles(&host, BOB);
    let hidden = tile(&bob, 0);
    assert!(hidden.is_hidden && hidden.id.is_none());
    assert!(hidden.team_keys.is_empty(), "the other team's find is not shown on a hidden square");
}

#[test]
fn the_other_team_sees_a_square_once_its_own_player_holds_the_item() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(true, false, Mode::Standard));
    session.on_inventory(&mut host, ALICE, &held(&items[0..1]));
    host.sent.clear();

    session.on_inventory(&mut host, BOB, &held(&items[0..1]));

    let bob = tiles_of(&only(&host, BOB, "update_card_tiles"));
    let square = tile(&bob, 0);
    assert!(!square.is_hidden);
    assert_eq!(square.id.as_deref(), Some(items[0].as_str()));
    assert_eq!(square.progress, 1.0, "blue found it too");
    assert_eq!(square.team_keys, vec!["bingo_red".to_string()], "red's find is listed for blue");
}

#[test]
fn a_seen_square_stays_shown_after_its_item_leaves_the_inventory() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(true, false, Mode::Inventory));
    session.on_inventory(&mut host, ALICE, &held(&items[3..4]));
    host.sent.clear();

    // The item is gone: in inventory mode the square is lost, but the team has seen it.
    session.on_inventory(&mut host, ALICE, &HashMap::new());

    let packet = tiles_of(&only(&host, ALICE, "update_card_tiles"));
    let square = tile(&packet, 3);
    assert!(!square.is_hidden, "a seen square is not hidden");
    assert_eq!(square.id.as_deref(), Some(items[3].as_str()));
    assert_eq!(square.progress, 0.0, "the square is no longer achieved");
    assert!(tile(&packet, 0).is_hidden, "a square never seen stays hidden");
}

#[test]
fn postgame_shows_every_square_to_each_team() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(true, false, Mode::Standard));
    session.on_inventory(&mut host, ALICE, &held(&items[0..1]));
    assert!(last_tiles(&host, BOB).tiles.iter().any(|(_, tile)| tile.is_hidden), "blue has hidden squares in play");
    host.sent.clear();

    assert_eq!(session.command(&mut host, None, "end"), "Game ended.");

    for viewer in [ALICE, BOB] {
        let packet = last_tiles(&host, viewer);
        assert!(packet.tiles.iter().all(|(_, tile)| !tile.is_hidden && tile.id.is_some()), "player {viewer} sees all");
    }
}

#[test]
fn a_square_is_consumed_once_from_the_player_who_captured_it() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(false, true, Mode::Standard));
    host.commands.clear();

    session.on_inventory(&mut host, ALICE, &held(&items[0..1]));
    assert_eq!(host.commands, vec![format!("clear Alice {} 1", items[0])]);

    // The same inventory again captures nothing, so nothing is cleared again.
    host.commands.clear();
    session.on_inventory(&mut host, ALICE, &held(&items[0..1]));
    assert!(host.commands.is_empty());

    // A new square found in the same report is cleared, the old one is not.
    session.on_inventory(&mut host, ALICE, &held(&items[0..2]));
    assert_eq!(host.commands, vec![format!("clear Alice {} 1", items[1])]);

    // Bob captures a square: it is Bob's item that goes.
    host.commands.clear();
    session.on_inventory(&mut host, BOB, &held(&items[4..5]));
    assert_eq!(host.commands, vec![format!("clear Bob {} 1", items[4])]);
}

#[test]
fn with_consume_items_off_no_item_is_cleared() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(false, false, Mode::Standard));
    host.commands.clear();
    session.on_inventory(&mut host, ALICE, &held(&items[0..3]));
    assert!(host.commands.is_empty());
}

#[test]
fn consume_items_leaves_inventory_mode_and_inventory_mode_turns_it_off() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(SEED, Options::default()));
    lobby(&mut host, &mut session);

    session.command(&mut host, None, "mode inventory");
    let answer = session.command(&mut host, None, "mode consume_items on");
    assert!(answer.contains("Mode: standard"), "consume on sets standard: {answer}");
    let saved = store::options_from_json(&host.files["options.json"]);
    assert_eq!(saved.mode, Mode::Standard);
    assert!(saved.hidden_consume.consume_items);

    session.command(&mut host, None, "mode inventory");
    let saved = store::options_from_json(&host.files["options.json"]);
    assert_eq!(saved.mode, Mode::Inventory);
    assert!(!saved.hidden_consume.consume_items, "inventory mode turns consume off");

    // With consume off and inventory mode on, a find takes nothing from the inventory.
    session.command(&mut host, None, "mode consume_items on");
    session.command(&mut host, None, "mode inventory");
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    let names = tiles_of(&only(&host, ALICE, "update_card_tiles"));
    let item = names.tiles[0].1.id.clone().expect("every square shows its item when nothing is hidden");
    host.commands.clear();
    session.on_inventory(&mut host, ALICE, &held(&[item]));
    assert!(host.commands.is_empty());
}

#[test]
fn the_card_flags_are_saved_and_a_new_session_reads_them() {
    let mut first = FakeHost::default();
    let mut session = Session::new(options(SEED, Options::default()));
    lobby(&mut first, &mut session);
    session.command(&mut first, None, "mode hidden_items on");
    session.command(&mut first, None, "mode consume_items on");
    let saved = store::options_from_json(&first.files["options.json"]);
    assert_eq!(saved.hidden_consume, HiddenConsume { hidden_items: true, consume_items: true });

    let items = card_items();
    let mut second = FakeHost { files: first.files.clone(), ..FakeHost::default() };
    let mut restarted = Session::new(options(SEED, Options::default()));
    lobby(&mut second, &mut restarted);
    assert_eq!(restarted.command(&mut second, None, "start"), "Game started.");
    let packet = tiles_of(&only(&second, ALICE, "update_card_tiles"));
    assert!(packet.tiles.iter().all(|(_, tile)| tile.is_hidden), "hidden items was read back");

    second.commands.clear();
    restarted.on_inventory(&mut second, ALICE, &held(&items[0..1]));
    assert_eq!(second.commands, vec![format!("clear Alice {} 1", items[0])], "consume items was read back");
}

#[test]
fn an_options_file_from_before_the_modes_loads_with_both_off() {
    let loaded = store::options_from_json(r#"{"goal":{"type":"items","n":5},"mode":"lockout"}"#);
    assert_eq!(loaded.hidden_consume, HiddenConsume::default());
    assert_eq!(loaded.mode, Mode::Lockout);
}

#[test]
fn a_restarted_game_keeps_the_squares_the_team_has_seen() {
    let items = card_items();
    let (mut host, mut session) = start(card_rules(true, false, Mode::Inventory));
    session.on_inventory(&mut host, ALICE, &held(&items[3..4]));
    session.on_inventory(&mut host, ALICE, &HashMap::new());
    assert!(host.files.contains_key("game-state.json"), "the game is saved");

    let mut restarted_host = FakeHost { online: vec![player(ALICE, "Alice")], files: host.files.clone(), ..FakeHost::default() };
    let mut restarted = Session::new(options(SEED, Options::default()));
    restarted.on_join(&mut restarted_host, &player(ALICE, "Alice"));

    let packet = tiles_of(&only(&restarted_host, ALICE, "update_card_tiles"));
    assert!(!tile(&packet, 3).is_hidden, "the square the team saw is still shown");
    assert!(tile(&packet, 0).is_hidden);
}

#[test]
fn the_card_flags_are_op_only_and_parse_with_on_or_off() {
    assert_eq!(parse("mode hidden_items on"), Ok(Command::CardMode(CardMode::HiddenItems, true)));
    assert_eq!(parse("MODE Consume_Items OFF"), Ok(Command::CardMode(CardMode::ConsumeItems, false)));
    assert_eq!(parse("mode hidden_items maybe").unwrap_err(), "mode hidden_items needs on or off, got 'maybe'");

    assert!(requires_op(&parse("mode hidden_items on").unwrap()));
    assert!(requires_op(&parse("mode consume_items off").unwrap()));
    assert!(!requires_op(&parse("options").unwrap()));
    assert!(usage().contains("hidden_items on|off|consume_items on|off"));
}

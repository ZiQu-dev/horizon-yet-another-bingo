//! The starting kit (`bingo kit`), the translatable chat lines (`texts`). Expected values are written by hand from the
//! command list and the original's behaviour: the kit is given to team players when play begins, and its items are kept
//! off the cards (`excludeSpawnKitItemsFromCards`, on by default in the original).
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::server::card_gen::{DEFAULT_DISTRIBUTION, generate_card, load_pool};
use horizon_bingo::server::commands::{Command, parse, requires_op};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::kit::{self, Kit, KitCommand, MAX_ITEMS};
use horizon_bingo::server::options::Options;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store;
use horizon_bingo::server::texts::{self, Line};
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;

/// A host that records what the session sends, and treats one item name as unknown to the registry.
#[derive(Default)]
struct Host {
    online: Vec<Player>,
    commands: Vec<String>,
    files: HashMap<String, String>,
    broadcasts: Vec<String>,
    /// Each keyed line as `key|english`, for the lines sent with `broadcast_line`.
    keyed: Vec<String>,
    sent: Vec<(u128, String, Vec<u8>)>,
}

impl ServerHost for Host {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push((player, path.to_string(), payload));
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, line: &str) {
        self.broadcasts.push(line.to_string());
    }

    fn broadcast_line(&mut self, line: &Line) {
        self.keyed.push(format!("{}|{}", line.key, line.render()));
        self.broadcasts.push(line.render());
    }

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    /// Every item is known except `minecraft:nothing`; the id is the name's length, which tests do not depend on.
    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        (item != "minecraft:nothing").then(|| item.len() as i32)
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

fn options(seed: u64) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut Host, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Alice on red and Bob on blue, both online and joined; the commands and sent packets so far are dropped.
fn lobby(host: &mut Host, session: &mut Session) {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(run(session, host, Some(ALICE), "join red").starts_with("Moved player"));
    assert!(run(session, host, Some(BOB), "join blue").starts_with("Moved player"));
    host.commands.clear();
    host.sent.clear();
}

/// The item names of the card as Alice sees it: the one `update_card_tiles` she is sent.
fn card_names(host: &mut Host) -> Vec<String> {
    let payload = host
        .sent
        .iter()
        .find(|(player, path, _)| *player == ALICE && path == "update_card_tiles")
        .map(|(_, _, payload)| payload.clone())
        .expect("Alice is sent her card");
    let tiles: CardTilesPacket = decode(&payload, CardTilesPacket::read).expect("update_card_tiles decodes");
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

// Parsing: the words after `bingo`, and the operator rule.

#[test]
fn kit_commands_parse_with_minecraft_ids_and_a_default_count_of_one() {
    assert_eq!(parse("kit"), Ok(Command::Kit(KitCommand::Show)));
    assert_eq!(parse("kit clear"), Ok(Command::Kit(KitCommand::Clear)));
    assert_eq!(parse("kit remove bread"), Ok(Command::Kit(KitCommand::Remove("minecraft:bread".into()))));
    assert_eq!(
        parse("kit add bread"),
        Ok(Command::Kit(KitCommand::Add { item: "minecraft:bread".into(), count: 1 }))
    );
    assert_eq!(
        parse("kit add Diamond_Sword 3"),
        Ok(Command::Kit(KitCommand::Add { item: "minecraft:diamond_sword".into(), count: 3 }))
    );
    assert_eq!(
        parse("kit add minecraft:bread 64"),
        Ok(Command::Kit(KitCommand::Add { item: "minecraft:bread".into(), count: 64 }))
    );
}

#[test]
fn kit_commands_reject_bad_counts_and_names() {
    assert_eq!(parse("kit add bread 0").unwrap_err(), "kit add needs a number of items from 1 to 6400, got '0'");
    assert_eq!(parse("kit add bread 6401").unwrap_err(), "kit add needs a number of items from 1 to 6400, got '6401'");
    assert_eq!(parse("kit add bread x").unwrap_err(), "kit add needs a number of items from 1 to 6400, got 'x'");
    assert_eq!(parse("kit add a:b").unwrap_err(), "Unknown item 'a:b'. Use an item id such as diamond or minecraft:diamond.");
    assert_eq!(parse("kit add Bread!").unwrap_err(), "Unknown item 'Bread!'. Use an item id such as diamond or minecraft:diamond.");
    assert_eq!(parse("kit add").unwrap_err(), "Usage: kit add <item> [count]");
    assert_eq!(parse("kit fly").unwrap_err(), "Usage: kit [add <item> [count] | remove <item> | clear]");
}

#[test]
fn only_changing_the_kit_needs_an_operator() {
    assert!(!requires_op(&Command::Kit(KitCommand::Show)));
    assert!(requires_op(&Command::Kit(KitCommand::Clear)));
    assert!(requires_op(&Command::Kit(KitCommand::Remove("minecraft:bread".into()))));
    assert!(requires_op(&Command::Kit(KitCommand::Add { item: "minecraft:bread".into(), count: 1 })));
}

// The kit itself.

#[test]
fn adding_an_item_twice_sets_its_count_and_keeps_its_place() {
    let mut kit = Kit::default();
    kit.add("minecraft:bread", 16).unwrap();
    kit.add("minecraft:diamond", 1).unwrap();
    kit.add("minecraft:bread", 8).unwrap();
    assert_eq!(kit.names(), vec!["minecraft:bread", "minecraft:diamond"]);
    assert_eq!(kit.give_commands("Alice"), vec!["give Alice minecraft:bread 8", "give Alice minecraft:diamond 1"]);
}

#[test]
fn a_kit_holds_36_items_and_the_37th_is_refused() {
    let mut kit = Kit::default();
    for n in 0..MAX_ITEMS {
        kit.add(&format!("minecraft:item_{n}"), 1).unwrap();
    }
    assert_eq!(kit.add("minecraft:one_more", 1), Err("The kit has 36 items already. Remove one first.".to_string()));
    assert!(kit.remove("minecraft:item_0"));
    assert!(kit.add("minecraft:one_more", 1).is_ok());
}

#[test]
fn removing_an_item_that_is_not_in_the_kit_says_so() {
    let mut kit = Kit::default();
    kit.add("minecraft:bread", 2).unwrap();
    assert!(kit.remove("minecraft:bread"));
    assert!(!kit.remove("minecraft:bread"));
    assert!(kit.is_empty());
}

#[test]
fn the_kit_is_listed_in_one_line_and_an_empty_one_says_so() {
    let mut kit = Kit::default();
    assert_eq!(kit.describe(), "Kit: none. Team players get nothing at the start.");
    kit.add("minecraft:bread", 16).unwrap();
    kit.add("minecraft:diamond", 1).unwrap();
    assert_eq!(
        kit.describe(),
        "Kit: 16 minecraft:bread, 1 minecraft:diamond. Team players get these at the start of each round."
    );
}

#[test]
fn kit_names_are_normalised_and_limited_as_documented() {
    assert_eq!(kit::normalise_item("Bread"), Ok("minecraft:bread".to_string()));
    assert_eq!(kit::normalise_item("minecraft:bread"), Ok("minecraft:bread".to_string()));
    assert!(kit::normalise_item("other:bread").is_err());
    assert!(kit::normalise_item("minecraft:").is_err());
}

// Saved options: written only with items, read back, bad entries skipped.

#[test]
fn an_empty_kit_is_not_written_and_a_kit_round_trips() {
    assert!(!store::options_to_json(&Options::default()).contains("\"kit\""));

    let mut options = Options::default();
    options.kit.add("minecraft:bread", 16).unwrap();
    let text = store::options_to_json(&options);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["kit"], serde_json::json!([{ "item": "minecraft:bread", "count": 16 }]));
    assert_eq!(store::options_from_json(&text).kit, options.kit);
}

#[test]
fn a_saved_kit_skips_entries_that_are_not_valid_and_keeps_the_first_of_a_repeat() {
    let text = r#"{ "kit": [
        { "item": "minecraft:bread", "count": 0 },
        { "item": "bad:x", "count": 1 },
        { "item": "minecraft:stone", "count": 2 },
        { "item": "minecraft:stone", "count": 5 },
        { "item": "stone", "count": 6401 }
    ] }"#;
    let kit = store::options_from_json(text).kit;
    assert_eq!(kit.items.len(), 1);
    assert_eq!((kit.items[0].item.as_str(), kit.items[0].count), ("minecraft:stone", 2));
}

// The cards: the kit's items are kept off them.

#[test]
fn a_kit_item_is_kept_off_the_card_that_would_have_held_it() {
    let pool = load_pool();
    // An item of seed 1's card: without the kit it is on that card, so the test sees it at all.
    let wanted = generate_card(1, &pool, DEFAULT_DISTRIBUTION).squares[0].item.clone();
    let found = (1..=50u64).any(|seed| {
        generate_card(seed, &pool, DEFAULT_DISTRIBUTION).squares.iter().any(|square| square.item == wanted)
    });
    assert!(found, "{wanted} should appear on some card without a kit");

    let without = kit::pool_without(&pool, std::slice::from_ref(&wanted));
    for seed in 1..=50u64 {
        let card = generate_card(seed, &without, DEFAULT_DISTRIBUTION);
        assert_eq!(card.squares.len(), 25);
        assert!(card.squares.iter().all(|square| square.item != wanted), "seed {seed} holds {wanted}");
    }
}

#[test]
fn an_empty_kit_leaves_the_pool_unchanged() {
    let pool = load_pool();
    assert_eq!(kit::pool_without(&pool, &[]), pool);
}

// The session: the kit goes out at the start, the change is refused once play has begun, and the file is saved.

#[test]
fn team_players_get_the_kit_when_play_begins_and_a_spectator_does_not() {
    let mut host = Host::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    // Carol is online but on no team: she is a spectator at the start.
    let carol = player(3, "Carol");
    host.online.push(carol.clone());
    session.on_join(&mut host, &carol);
    host.commands.clear();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "kit add bread 16"), "Kit: 16 minecraft:bread set.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "kit add diamond"), "Kit: 1 minecraft:diamond set.");
    assert!(host.files.get("options.json").unwrap().contains("minecraft:diamond"));

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start ignore_warnings"), "Starting the game...");
    let gives: Vec<String> = host.commands.iter().filter(|c| c.starts_with("give ")).cloned().collect();
    assert_eq!(
        gives,
        vec![
            "give Alice minecraft:bread 16",
            "give Alice minecraft:diamond 1",
            "give Bob minecraft:bread 16",
            "give Bob minecraft:diamond 1",
        ]
    );
    assert!(host.commands.iter().any(|c| c == "gamemode spectator Carol"));
}

#[test]
fn a_kit_change_after_the_start_is_refused_and_the_kit_is_unchanged() {
    let mut host = Host::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    run(&mut session, &mut host, Some(ALICE), "kit add bread 16");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert_eq!(
        run(&mut session, &mut host, Some(ALICE), "kit add stone 1"),
        "The kit can be changed only before the game starts."
    );
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "kit clear"), "The kit can be changed only before the game starts.");
    assert_eq!(run(&mut session, &mut host, None, "kit"), "Kit: 16 minecraft:bread. Team players get these at the start of each round.");
}

#[test]
fn an_unknown_item_and_a_missing_one_are_refused_without_saving() {
    let mut host = Host::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    assert_eq!(
        run(&mut session, &mut host, Some(ALICE), "kit add nothing"),
        "Unknown item 'minecraft:nothing'. Use an item id such as diamond or minecraft:diamond."
    );
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "kit remove bread"), "minecraft:bread is not in the kit.");
    assert!(!host.files.contains_key("options.json"));
}

#[test]
fn the_cards_leave_out_the_items_of_the_kit() {
    // The same seed with no kit: its card holds three items; with those three in the kit, none of them is on the card.
    let mut host = Host::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    run(&mut session, &mut host, Some(ALICE), "start");
    let plain = card_names(&mut host);
    let kit_items: Vec<String> = plain.iter().take(3).cloned().collect();

    let mut host = Host::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    for item in &kit_items {
        assert!(run(&mut session, &mut host, Some(ALICE), &format!("kit add {item}")).starts_with("Kit: 1 "));
    }
    run(&mut session, &mut host, Some(ALICE), "start");
    let with_kit = card_names(&mut host);
    assert_eq!(with_kit.len(), 25);
    for item in &kit_items {
        assert!(!with_kit.contains(item), "{item} is in the kit and on the card");
    }
}

// The lines: English as rendered, and the arguments and escapes.

#[test]
fn lines_render_their_english_with_arguments_and_percent_escapes() {
    assert_eq!(texts::team_reached_goal("Red").render(), "Red reached the goal!");
    assert_eq!(texts::game_continues().render(), "The game continues.");
    assert_eq!(
        texts::stalemate_lockout().render(),
        "A stalemate: every square is claimed and no team leads, so the game ends."
    );
    assert_eq!(Line::new("k", "100%% sure, %s and %s", &["a"]).render(), "100% sure, a and %s");
    assert_eq!(Line::new("k", "50% off", &[]).render(), "50% off");
}

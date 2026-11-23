//! Multi-card rounds of the server session: `playto cards <n>` and `playto infinite` (a team completes a card, gets the
//! next one, and the round ends when the cards are done), the fresh card and progress of a team that completes a card,
//! the stalemate behaviours (`stalemate end_game` and `reroll_card`) on a lockout round with no possible win, the saved
//! options, the snapshot of a multi-card round, and the operator rule of the new commands. Expected values are counted by
//! hand: 20 ticks per second, a row is five squares, and a card completed within 3 ticks of being made ends the game.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::server::commands::{parse, requires_op, usage, Command};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::multicard::{StalemateBehavior, WinCondition};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;
use serde_json::Value;

const ALICE: u128 = 1;
const BOB: u128 = 2;

/// Squares of the card that Red (Alice) and Blue (Bob) claim in the lockout round: no line is wholly one team's, and
/// every line holds a square of each, so no team can reach a line once both have scored.
const RED_SQUARES: [usize; 8] = [1, 9, 12, 15, 16, 18, 22, 24];
const BLUE_SQUARES: [usize; 10] = [2, 3, 6, 7, 8, 10, 13, 14, 17, 23];

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
}

impl FakeHost {
    fn take(&mut self) -> Vec<Sent> {
        std::mem::take(&mut self.sent)
    }

    fn take_broadcasts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.broadcasts)
    }
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

fn options(seed: u64, countdown_secs: i32) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

fn status(session: &mut Session, host: &mut FakeHost) -> String {
    run(session, host, None, "status")
}

/// Alice on red and Bob on blue, the round set up with `setup`, and started. The game starts at once (no countdown).
/// The sent packets and the broadcasts of the start are drained.
fn start(seed: u64, setup: &[&str]) -> (Session, FakeHost) {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(seed, 0));
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    assert!(run(&mut session, &mut host, Some(ALICE), "join red").starts_with("Moved player"));
    assert!(run(&mut session, &mut host, Some(BOB), "join blue").starts_with("Moved player"));
    for line in setup {
        let answer = run(&mut session, &mut host, None, line);
        assert!(
            !answer.starts_with("Usage") && !answer.starts_with("Unknown") && !answer.contains("only before"),
            "{line}: {answer}"
        );
    }
    // Lockout with the lines goal and two teams is refused unless warnings are ignored (`CommandStartLockoutWithLinesGoal`).
    assert_eq!(run(&mut session, &mut host, None, "start ignore_warnings"), "Starting the game...");
    host.take_broadcasts();
    (session, host)
}

/// Runs the game clock: each tick is 50 ms of play.
fn ticks(session: &mut Session, host: &mut FakeHost, count: usize) {
    for _ in 0..count {
        session.tick(host);
    }
}

/// The items of the squares of the card a player's team plays, in square order, from the newest card tiles sent to
/// the player.
fn tiles_names(host: &FakeHost, player: u128) -> Vec<String> {
    let last = host
        .sent
        .iter()
        .rev()
        .find(|s| s.player == player && s.path == "update_card_tiles")
        .expect("the player was sent card tiles");
    let tiles = decode(&last.payload, CardTilesPacket::read).expect("update_card_tiles decodes");
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

/// Scores the top row of the card the player's team plays: its five items are held. Returns the card's items.
fn complete_row(session: &mut Session, host: &mut FakeHost, player: u128) -> Vec<String> {
    let names = tiles_names(host, player);
    session.on_inventory(host, player, &held(&names[0..5]));
    names
}

/// The one packet of this kind sent to this player.
fn only(sent: &[Sent], player: u128, path: &str) -> Vec<u8> {
    let found: Vec<&Sent> = sent.iter().filter(|s| s.player == player && s.path == path).collect();
    assert_eq!(found.len(), 1, "one {path} for player {player}");
    found[0].payload.clone()
}

#[test]
fn two_cards_round_gives_the_team_its_next_card_and_ends_on_the_second() {
    let (mut session, mut host) = start(7, &["playto cards 2"]);
    ticks(&mut session, &mut host, 4);
    let first = tiles_names(&host, ALICE);

    complete_row(&mut session, &mut host, ALICE);
    let lines = host.take_broadcasts();
    assert!(lines.contains(&"Red completed card 1/2!".to_string()), "{lines:?}");
    assert!(!lines.iter().any(|line| line.contains("won")), "the round goes on: {lines:?}");
    assert!(status(&mut session, &mut host).contains("Playing"));
    let second = tiles_names(&host, ALICE);
    assert_eq!(second.len(), 25);
    assert_ne!(second, first, "the second card is a new one");
    // First win: the other teams on the card are moved on without a win, so Blue gets the card Red now plays.
    assert_eq!(tiles_names(&host, BOB), second, "Blue is moved on to the same next card");
    assert!(!lines.iter().any(|line| line.starts_with("Blue completed")), "{lines:?}");

    ticks(&mut session, &mut host, 4);
    complete_row(&mut session, &mut host, ALICE);
    let lines = host.take_broadcasts();
    assert!(lines.contains(&"Red completed card 2/2!".to_string()), "{lines:?}");
    assert!(status(&mut session, &mut host).contains("Postgame"));
    let sent = host.take();
    let over = decode(&only(&sent, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner.as_deref(), Some("bingo_red"));
    assert!(over.is_winner);
}

#[test]
fn a_round_of_three_cards_ends_only_after_the_third() {
    let (mut session, mut host) = start(7, &["playto cards 3"]);
    for (done, total) in [(1, 3), (2, 3)] {
        ticks(&mut session, &mut host, 4);
        complete_row(&mut session, &mut host, ALICE);
        let lines = host.take_broadcasts();
        assert!(lines.contains(&format!("Red completed card {done}/{total}!")), "{lines:?}");
        assert!(status(&mut session, &mut host).contains("Playing"), "after card {done}");
    }
    ticks(&mut session, &mut host, 4);
    complete_row(&mut session, &mut host, ALICE);
    let lines = host.take_broadcasts();
    assert!(lines.contains(&"Red completed card 3/3!".to_string()), "{lines:?}");
    assert!(status(&mut session, &mut host).contains("Postgame"));
}

#[test]
fn a_team_on_the_next_card_gets_a_fresh_card_and_fresh_progress_while_the_other_keeps_its_own() {
    // `endwhen all_win`: no team is moved on when another completes a card, so Blue keeps its card 0 progress.
    let (mut session, mut host) = start(7, &["playto cards 2", "endwhen all_win"]);
    ticks(&mut session, &mut host, 4);
    let card0 = tiles_names(&host, BOB);
    complete_row(&mut session, &mut host, ALICE);
    ticks(&mut session, &mut host, 4);
    let card1 = tiles_names(&host, ALICE);
    assert_ne!(card1, card0);

    // Red finds two squares of its new card; Blue finds three squares of the first card.
    session.on_inventory(&mut host, ALICE, &held(&card1[0..2]));
    session.on_inventory(&mut host, BOB, &held(&card0[0..3]));
    let text = status(&mut session, &mut host);
    assert!(text.contains("Red: 1 players, 2 items"), "{text}");
    assert!(text.contains("Blue: 1 players, 3 items"), "{text}");

    // Blue completes the first card: it moves on to the card Red is on, with nothing found yet.
    complete_row(&mut session, &mut host, BOB);
    let lines = host.take_broadcasts();
    assert!(lines.contains(&"Blue completed card 1/2!".to_string()), "{lines:?}");
    assert_eq!(tiles_names(&host, BOB), card1, "Blue is sent the card Red plays");
    let sent = host.take();
    assert!(sent.iter().any(|s| s.player == BOB && s.path == "card_reset"), "Blue's card is reset");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Blue: 1 players, 0 items"), "{text}");
    assert!(text.contains("Red: 1 players, 2 items"), "Red keeps its progress: {text}");
}

#[test]
fn infinite_never_ends_the_round_by_cards() {
    let (mut session, mut host) = start(7, &["playto infinite"]);
    for done in 1..=3 {
        ticks(&mut session, &mut host, 4);
        complete_row(&mut session, &mut host, ALICE);
        let lines = host.take_broadcasts();
        assert!(lines.contains(&format!("Red completed card {done}!")), "{lines:?}");
        assert!(status(&mut session, &mut host).contains("Playing"), "after card {done}");
    }
}

#[test]
fn a_card_completed_within_three_ticks_of_being_made_ends_the_game() {
    let (mut session, mut host) = start(7, &["playto cards 2"]);
    complete_row(&mut session, &mut host, ALICE);
    host.take_broadcasts();
    assert!(status(&mut session, &mut host).contains("Postgame"));
}

/// The lockout round: Red finds its squares of the card, then Blue finds its squares. Returns the session, the host
/// and the card's items. Red's squares alone do not stalemate the card; both teams' squares leave no line possible.
fn lockout_round(setup: &[&str]) -> (Session, FakeHost, Vec<String>) {
    let mut lines = vec!["mode lockout"];
    lines.extend_from_slice(setup);
    let (mut session, mut host) = start(7, &lines);
    let names = tiles_names(&host, ALICE);
    let red: Vec<String> = RED_SQUARES.iter().map(|&i| names[i].clone()).collect();
    session.on_inventory(&mut host, ALICE, &held(&red));
    assert!(status(&mut session, &mut host).contains("Playing"), "Red's squares alone do not end the round");
    host.take();
    host.take_broadcasts();
    let blue: Vec<String> = BLUE_SQUARES.iter().map(|&i| names[i].clone()).collect();
    session.on_inventory(&mut host, BOB, &held(&blue));
    (session, host, names)
}

#[test]
fn stalemate_end_game_ends_a_lockout_round_with_no_possible_win() {
    let (mut session, mut host, _names) = lockout_round(&["stalemate end_game"]);
    assert!(status(&mut session, &mut host).contains("Postgame"));
    let lines = host.take_broadcasts();
    assert!(lines.contains(&"A stalemate: no team can reach the goal, so the game ends.".to_string()), "{lines:?}");
    let sent = host.take();
    let over = decode(&only(&sent, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner, None, "nobody wins a stalemate");
    assert!(!over.is_winner);
}

#[test]
fn stalemate_reroll_card_gives_every_team_a_new_card_and_carries_on() {
    let (mut session, mut host, names) = lockout_round(&["stalemate reroll_card"]);
    assert!(status(&mut session, &mut host).contains("Playing"));
    let lines = host.take_broadcasts();
    assert!(lines.contains(&"A stalemate: every team on the card gets a new card.".to_string()), "{lines:?}");
    let red_card = tiles_names(&host, ALICE);
    assert_ne!(red_card, names, "Red plays a new card");
    assert_eq!(tiles_names(&host, BOB), red_card, "Blue plays the same new card");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Red: 1 players, 0 items") && text.contains("Blue: 1 players, 0 items"), "{text}");
}

#[test]
fn the_win_condition_and_stalemate_are_saved_and_loaded_and_defaults_stay_out_of_the_file() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0));
    assert!(run(&mut session, &mut host, None, "playto cards 3").contains("Win: 3 cards."));
    assert!(run(&mut session, &mut host, None, "stalemate reroll_card").contains("Stalemate: reroll card."));
    let saved: Value = serde_json::from_str(&host.files["options.json"]).expect("options.json is JSON");
    assert_eq!(saved["winCondition"], serde_json::json!({ "type": "cards", "n": 3 }));
    assert_eq!(saved["stalemate"], Value::from("reroll_card"));

    let mut loaded = Session::new(options(7, 0));
    let text = run(&mut loaded, &mut host, None, "options");
    assert!(text.contains("Win: 3 cards.") && text.contains("Stalemate: reroll card."), "{text}");

    let mut plain = FakeHost::default();
    let mut session = Session::new(options(7, 0));
    run(&mut session, &mut plain, None, "goal items 2");
    let saved: Value = serde_json::from_str(&plain.files["options.json"]).expect("options.json is JSON");
    assert!(saved.get("winCondition").is_none(), "a default win condition is not written");
    assert!(saved.get("stalemate").is_none(), "a default stalemate behaviour is not written");
}

#[test]
fn a_multi_card_round_restores_its_cards_and_progress_from_its_snapshot() {
    let (mut session, mut host) = start(7, &["playto cards 2"]);
    ticks(&mut session, &mut host, 4);
    complete_row(&mut session, &mut host, ALICE);
    let card1 = tiles_names(&host, ALICE);
    ticks(&mut session, &mut host, 4);
    session.on_inventory(&mut host, ALICE, &held(&card1[0..2]));
    let saved = session.snapshot_json();

    let mut restored_host = FakeHost::default();
    restored_host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut restored = Session::new(options(7, 0));
    restored.restore_from_json(&saved).expect("the snapshot restores");
    let text = status(&mut restored, &mut restored_host);
    assert!(text.contains("Playing") && text.contains("Red: 1 players, 2 items"), "{text}");

    // Red completes its second card, which is the card the snapshot kept.
    restored.on_inventory(&mut restored_host, ALICE, &held(&card1[0..5]));
    restored_host.take_broadcasts();
    assert!(status(&mut restored, &mut restored_host).contains("Postgame"));
}

#[test]
fn the_round_commands_parse_and_need_an_operator() {
    assert_eq!(parse("playto cards 2"), Ok(Command::PlayTo(WinCondition::Cards(2))));
    assert_eq!(parse("PLAYTO Infinite"), Ok(Command::PlayTo(WinCondition::Infinite)));
    assert_eq!(parse("playto replace_goals"), Ok(Command::PlayTo(WinCondition::ReplaceGoals)));
    assert_eq!(parse("stalemate end_game"), Ok(Command::Stalemate(StalemateBehavior::EndGame)));
    assert_eq!(parse("stalemate reroll_card"), Ok(Command::Stalemate(StalemateBehavior::RerollCard)));
    assert_eq!(parse("stalemate do_nothing"), Ok(Command::Stalemate(StalemateBehavior::Nothing)));
    assert!(parse("playto cards 0").is_err());
    assert_eq!(parse("playto cards").unwrap_err(), "Usage: playto cards <n>");
    assert!(parse("playto sideways").unwrap_err().starts_with("Unknown playto"));
    assert!(parse("stalemate nothing").unwrap_err().starts_with("Unknown stalemate"));

    for line in ["playto cards 2", "playto infinite", "playto replace_goals", "stalemate end_game", "stalemate do_nothing"] {
        assert!(requires_op(&parse(line).unwrap()), "{line} needs an operator");
    }
    for line in ["status", "options", "card", "stats"] {
        assert!(!requires_op(&parse(line).unwrap()), "{line} is open to everyone");
    }
    let text = usage();
    assert!(text.contains("playto cards <n>|infinite|replace_goals"), "{text}");
    assert!(text.contains("stalemate end_game|reroll_card|do_nothing"), "{text}");
}

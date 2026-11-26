//! The `replace_goals` win condition of the server session: a scored square (an items round) or a full line (a lines
//! round) is replaced by a new item of its tier at once, the team keeps the score it made in its persistent score, and
//! the goal is read from the total. The round goes on until its end rule holds. Expected values are counted by hand: a
//! row of the card is five squares, a square found is one item, and a replaced square is a new item that is not on the
//! card.
use std::collections::{HashMap, HashSet};

use horizon_bingo::buf::decode;
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;

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

/// Alice on red and Bob on blue, the round set up with `setup`, and started. The game starts at once.
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
    assert_eq!(run(&mut session, &mut host, None, "start"), "Starting the game...");
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

fn distinct(names: &[String]) -> bool {
    names.iter().collect::<HashSet<_>>().len() == names.len()
}

#[test]
fn items_round_replaces_each_scored_square_and_keeps_its_score() {
    // Goal: 3 items. Red finds 2 squares, they are replaced at once; its score is kept in the persistent score.
    let (mut session, mut host) = start(7, &["playto replace_goals", "goal items 3", "endwhen never"]);
    ticks(&mut session, &mut host, 4);
    let before = tiles_names(&host, ALICE);
    assert_eq!(before.len(), 25);
    assert!(distinct(&before));

    session.on_inventory(&mut host, ALICE, &held(&before[0..2]));
    let lines = host.take_broadcasts();
    assert!(!lines.iter().any(|line| line.contains("reached")), "2 of 3 items: {lines:?}");
    // 2 items scored, so 2 in the persistent score and none on the card: the total is 2.
    let text = status(&mut session, &mut host);
    assert!(text.contains("Red: 1 players, 2 items, 0 lines"), "{text}");
    let after = tiles_names(&host, ALICE);
    assert_eq!(after.len(), 25);
    assert_eq!(after[2..], before[2..], "the squares not scored keep their items");
    assert!(after[0..2].iter().all(|name| !before.contains(name)), "the scored squares get new items: {after:?}");
    assert!(distinct(&after), "no item is on the card twice");

    // One of the new squares is found: the total is 2 + 1 = 3, the goal. Red has reached it and the round goes on.
    session.on_inventory(&mut host, ALICE, &held(&after[0..1]));
    let lines = host.take_broadcasts();
    assert!(!lines.iter().any(|line| line.contains("reached the goal")), "{lines:?}");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Red: 1 players, 3 items, 0 lines"), "{text}");
    assert!(text.contains("Playing"), "endwhen never keeps the round going: {text}");
    let again = tiles_names(&host, ALICE);
    assert_ne!(again[0], after[0], "the square found is replaced");
    assert_eq!(again[1..], after[1..], "only the square found changes");
}

#[test]
fn lines_round_replaces_a_full_line_at_once_and_leaves_a_partial_row_alone() {
    // Goal: 2 lines. Four squares of the top row are no line, so nothing is replaced. The fifth makes the row: its five
    // squares are replaced, one line goes to the persistent score. The second row makes the second line: the goal.
    let (mut session, mut host) = start(7, &["playto replace_goals", "goal lines 2", "endwhen never"]);
    ticks(&mut session, &mut host, 4);
    let before = tiles_names(&host, ALICE);

    session.on_inventory(&mut host, ALICE, &held(&before[0..4]));
    assert_eq!(tiles_names(&host, ALICE), before, "a partial row is not replaced");
    assert!(status(&mut session, &mut host).contains("Red: 1 players, 4 items, 0 lines"));

    session.on_inventory(&mut host, ALICE, &held(&before[0..5]));
    let lines = host.take_broadcasts();
    assert!(!lines.iter().any(|line| line.contains("reached")), "1 of 2 lines: {lines:?}");
    // The row's 5 squares are scored: 5 items and 1 line persistent; none found on the card.
    assert!(status(&mut session, &mut host).contains("Red: 1 players, 5 items, 1 lines"));
    let row0 = tiles_names(&host, ALICE);
    assert!(row0[0..5].iter().all(|name| !before.contains(name)), "the whole row is new: {row0:?}");
    assert_eq!(row0[5..], before[5..], "the other rows stay");

    // The second row is found on the new card: 5 + 5 items and 1 + 1 lines. Two lines: the goal is reached.
    session.on_inventory(&mut host, ALICE, &held(&row0[5..10]));
    let lines = host.take_broadcasts();
    assert!(!lines.iter().any(|line| line.contains("reached the goal")), "{lines:?}");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Red: 1 players, 10 items, 2 lines"), "{text}");
    assert!(text.contains("Playing"), "{text}");
}

#[test]
fn the_round_goes_on_for_the_other_teams_until_its_end_rule_holds() {
    // endwhen all_win: Red reaching the goal does not end the round; Blue reaching it does, and Red won first.
    let (mut session, mut host) = start(7, &["playto replace_goals", "goal items 2", "endwhen all_win"]);
    ticks(&mut session, &mut host, 4);
    let card = tiles_names(&host, ALICE);
    session.on_inventory(&mut host, ALICE, &held(&card[0..2]));
    assert!(!host.take_broadcasts().iter().any(|line| line.contains("reached the goal")));
    assert!(status(&mut session, &mut host).contains("Playing"));

    // Red and Blue play the same card, so Blue is sent the squares Red's goals were replaced with.
    let card = tiles_names(&host, BOB);
    assert_eq!(card, tiles_names(&host, ALICE), "one card for both teams");
    session.on_inventory(&mut host, BOB, &held(&card[0..2]));
    host.take_broadcasts();
    assert!(status(&mut session, &mut host).contains("Postgame"));
}

#[test]
fn a_square_another_team_has_found_loses_that_team_its_progress_when_it_is_replaced() {
    // Blue finds the first square of the card (not a line, so it stays). Red's top row replaces that square, so Blue
    // holds no item of the round now: its progress was on the old square.
    let (mut session, mut host) = start(7, &["playto replace_goals", "goal lines 1", "endwhen never"]);
    ticks(&mut session, &mut host, 4);
    let card = tiles_names(&host, ALICE);
    session.on_inventory(&mut host, BOB, &held(&card[0..1]));
    assert!(status(&mut session, &mut host).contains("Blue: 1 players, 1 items, 0 lines"));

    session.on_inventory(&mut host, ALICE, &held(&card[0..5]));
    let lines = host.take_broadcasts();
    assert!(!lines.iter().any(|line| line.contains("reached the goal")), "{lines:?}");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Red: 1 players, 5 items, 1 lines"), "{text}");
    assert!(text.contains("Blue: 1 players, 0 items, 0 lines"), "{text}");
}

#[test]
fn a_cards_one_round_keeps_its_squares_when_a_goal_is_reached() {
    // The default win condition: the same goal is reached, but no square is replaced.
    let (mut session, mut host) = start(7, &["goal items 2", "endwhen never"]);
    ticks(&mut session, &mut host, 4);
    let before = tiles_names(&host, ALICE);
    session.on_inventory(&mut host, ALICE, &held(&before[0..2]));
    assert!(!host.take_broadcasts().iter().any(|line| line.contains("reached the goal")));
    assert_eq!(tiles_names(&host, ALICE), before, "the card keeps its squares");
    assert!(status(&mut session, &mut host).contains("Red: 1 players, 2 items, 0 lines"));
}

#[test]
fn a_replace_goals_round_restores_its_persistent_score_from_its_snapshot() {
    let (mut session, mut host) = start(7, &["playto replace_goals", "goal items 3", "endwhen never"]);
    ticks(&mut session, &mut host, 4);
    let before = tiles_names(&host, ALICE);
    session.on_inventory(&mut host, ALICE, &held(&before[0..2]));
    let after = tiles_names(&host, ALICE);
    let saved = session.snapshot_json();
    let value: serde_json::Value = serde_json::from_str(&saved).expect("the snapshot is JSON");
    assert_eq!(value["persistent"]["bingo_red"]["items"], 2, "{saved}");

    let mut restored_host = FakeHost::default();
    restored_host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut restored = Session::new(options(7, 0));
    restored.restore_from_json(&saved).expect("the snapshot restores");
    let text = status(&mut restored, &mut restored_host);
    assert!(text.contains("Red: 1 players, 2 items, 0 lines"), "the persistent score is kept: {text}");

    // One more square of the card the snapshot kept: 2 + 1 = 3, the goal.
    restored.on_inventory(&mut restored_host, ALICE, &held(&after[0..1]));
    let lines = restored_host.take_broadcasts();
    assert!(!lines.iter().any(|line| line.contains("reached the goal")), "{lines:?}");
    assert!(status(&mut restored, &mut restored_host).contains("Red: 1 players, 3 items, 0 lines"));
}

#[test]
fn first_win_ends_the_replace_goals_round_at_the_first_goal() {
    let (mut session, mut host) = start(7, &["playto replace_goals", "goal items 2"]);
    ticks(&mut session, &mut host, 4);
    let card = tiles_names(&host, ALICE);
    session.on_inventory(&mut host, ALICE, &held(&card[0..2]));
    host.take_broadcasts();
    assert!(status(&mut session, &mut host).contains("Postgame"));
    let sent = host.take();
    let over = sent
        .iter()
        .find(|s| s.player == ALICE && s.path == "game_over_v2")
        .expect("Red is sent the game over");
    let over = decode(&over.payload, GameOverPacket::read_v2).expect("game over decodes");
    assert_eq!(over.winner.as_deref(), Some("bingo_red"));
}

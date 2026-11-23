//! Persistence of a running game (`game-state.json`): the snapshot of the session, its restore after a restart, and the
//! points where the file is written or emptied. A restart is a new session on the files of the old one. Expected values
//! are counted by hand: 20 ticks per second, so 130 ticks is 6500 ms and a 600 s limit then shows 593 s; a countdown of
//! 0 starts play at once; the default card has 9 squares of tier C and 16 of tier D.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};
use horizon_bingo::score_update::ScoredItemPacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store::parse_history;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;
use serde_json::{json, Value};

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;
/// The clock of the fake host, in ms since the Unix epoch.
const NOW_MS: i64 = 1_700_000_000_000;
const STATE_FILE: &str = "game-state.json";
const HISTORY_FILE: &str = "history.jsonl";

#[derive(Debug, Clone)]
struct Sent {
    player: u128,
    path: String,
    payload: Vec<u8>,
}

struct FakeHost {
    online: Vec<Player>,
    sent: Vec<Sent>,
    broadcasts: Vec<String>,
    commands: Vec<String>,
    /// The mod's data folder: file name to text.
    files: HashMap<String, String>,
    /// What `now_ms` returns.
    now: i64,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self { online: Vec::new(), sent: Vec::new(), broadcasts: Vec::new(), commands: Vec::new(), files: HashMap::new(), now: NOW_MS }
    }
}

impl FakeHost {
    fn take(&mut self) -> Vec<Sent> {
        std::mem::take(&mut self.sent)
    }

    fn take_broadcasts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.broadcasts)
    }

    fn take_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.commands)
    }

    /// The host after a restart: the same data folder and clock, and nobody online yet.
    fn restarted(&self) -> FakeHost {
        FakeHost { files: self.files.clone(), now: self.now, ..FakeHost::default() }
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

    fn append_line(&mut self, name: &str, line: &str) {
        let text = self.files.entry(name.to_string()).or_default();
        text.push_str(line);
        text.push('\n');
    }

    fn now_ms(&self) -> i64 {
        self.now
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn uuid(player: u128) -> String {
    format!("{player:032x}")
}

fn options(seed: u64, countdown_secs: i32, time_limit_secs: Option<i32>) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs, time_limit_secs },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Sets who is online, has each join the server, and puts each listed player on a team with `join <team>`. Drains what
/// was sent and the commands, so a test starts from a known point.
fn lobby(host: &mut FakeHost, session: &mut Session, online: &[(u128, &str)], teams: &[(u128, &str)]) {
    host.online = online.iter().map(|&(uuid, name)| player(uuid, name)).collect();
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    for &(uuid, team) in teams {
        let answer = run(session, host, Some(uuid), &format!("join {team}"));
        assert!(answer.starts_with("Moved player"), "joining {team}: {answer}");
    }
    host.take();
    host.take_commands();
}

fn payloads(sent: &[Sent], player: u128, path: &str) -> Vec<Vec<u8>> {
    sent.iter().filter(|s| s.player == player && s.path == path).map(|s| s.payload.clone()).collect()
}

/// The one payload of this kind sent to this player.
fn only(sent: &[Sent], player: u128, path: &str) -> Vec<u8> {
    let found = payloads(sent, player, path);
    assert_eq!(found.len(), 1, "one {path} for player {player}");
    found[0].clone()
}

/// The item names of the card a player was sent, in square order.
fn card_names_in(sent: &[Sent], player: u128) -> Vec<String> {
    let tiles = decode(&only(sent, player, "update_card_tiles"), CardTilesPacket::read).expect("update_card_tiles decodes");
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

fn card_names(host: &mut FakeHost, player: u128) -> Vec<String> {
    card_names_in(&host.take(), player)
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

/// The saved game as JSON, or `None` when the file is missing or empty.
fn saved(host: &FakeHost) -> Option<Value> {
    let text = host.files.get(STATE_FILE)?;
    if text.is_empty() {
        return None;
    }
    Some(serde_json::from_str(text).expect("the saved game is JSON"))
}

/// The text of a snapshot with one edit made to it.
fn with_edit(text: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(text).expect("the snapshot is JSON");
    edit(&mut value);
    value.to_string()
}

/// Alice on red, Bob on blue, Carol a spectator; lockout mode, a goal of 6 items and a time limit of 10 minutes. Started
/// with no countdown, so play has begun. Returns the names of the card's squares, in square order.
fn lockout_round(host: &mut FakeHost, session: &mut Session) -> Vec<String> {
    lobby(host, session, &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol")], &[(ALICE, "red"), (BOB, "blue")]);
    assert_eq!(run(session, host, Some(CAROL), "spectator"), "You are now a spectator.");
    assert!(run(session, host, Some(ALICE), "goal items 6").contains("Goal: 6 item(s)"));
    assert!(run(session, host, Some(ALICE), "mode lockout").contains("Mode: lockout"));
    assert_eq!(run(session, host, Some(ALICE), "timelimit 10"), "Time limit set to 10 minutes");
    assert_eq!(run(session, host, Some(ALICE), "start"), "Starting the game...");
    let names = card_names(host, ALICE);
    host.take_commands();
    host.take_broadcasts();
    names
}

#[test]
fn the_snapshot_of_a_running_game_has_every_field_with_the_hand_counted_values() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = lockout_round(&mut host, &mut session);
    for _ in 0..130 {
        session.tick(&mut host);
    }
    host.take();
    // Alice finds squares 0 and 5. Bob holds 5 too, but red owns it under lockout, so Bob gets only 7.
    session.on_inventory(&mut host, ALICE, &held(&[names[0].clone(), names[5].clone()]));
    session.on_inventory(&mut host, BOB, &held(&[names[5].clone(), names[7].clone()]));

    let mut value: Value = serde_json::from_str(&session.snapshot_json()).expect("the snapshot is JSON");
    let card = value.as_object_mut().unwrap().remove("card").expect("a card");
    // The per-player capture ledger is checked on its own below.
    let captures = value.as_object_mut().unwrap().remove("captures").expect("the captures");
    assert_eq!(
        value,
        json!({
            "version": 1,
            "phase": "Playing",
            "countdownTicks": 0,
            "elapsedTicks": 130,
            "timeLimitSecs": 600,
            "seed": 7,
            "rules": {
                "goal": { "type": "items", "n": 6 },
                "mode": "lockout",
                "endWhen": { "type": "first_win", "n": 0 },
                "pvp": true,
                "keepInventory": false,
                "elytra": false,
                "nightVision": false,
                "spawnDistance": 25
            },
            "addedTeams": ["bingo_red", "bingo_blue"],
            "progress": { "bingo_red": [0, 5], "bingo_blue": [7] },
            "owners": { "0": "bingo_red", "5": "bingo_red", "7": "bingo_blue" },
            "reached": [],
            "votes": {},
            "members": [
                { "uuid": uuid(ALICE), "team": "bingo_red" },
                { "uuid": uuid(BOB), "team": "bingo_blue" }
            ],
            "spectators": [uuid(CAROL)]
        })
    );
    // Alice (red) found squares 0 and 5, Bob (blue) square 7: 2 and 1 items, and each square's finder.
    assert_eq!(captures, json!({
        "items": { uuid(ALICE): 2, uuid(BOB): 1 },
        "finders": { "bingo_red": { "0": uuid(ALICE), "5": uuid(ALICE) }, "bingo_blue": { "7": uuid(BOB) } }
    }));
    let card = card.as_array().expect("the card is a list");
    assert_eq!(card.len(), 25);
    assert_eq!(card.iter().filter(|square| square["tier"] == "C").count(), 9);
    assert_eq!(card.iter().filter(|square| square["tier"] == "D").count(), 16);
    let items: Vec<String> = card.iter().map(|square| square["item"].as_str().unwrap().to_string()).collect();
    assert_eq!(items, names);
}

#[test]
fn restoring_a_running_game_gives_the_same_snapshot_status_and_card_and_the_owners_stay() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = lockout_round(&mut host, &mut session);
    for _ in 0..130 {
        session.tick(&mut host);
    }
    session.on_inventory(&mut host, ALICE, &held(&[names[0].clone(), names[5].clone()]));
    session.on_inventory(&mut host, BOB, &held(&[names[5].clone(), names[7].clone()]));
    let text = session.snapshot_json();

    let mut copy = Session::new(options(7, 0, None));
    copy.restore_from_json(&text).expect("a snapshot restores");
    assert_eq!(copy.snapshot_json(), text, "restore then snapshot gives the same text");

    // Time left: 600 s less 6500 ms of play is 593.5 s, shown as 593 s.
    let mut blank = FakeHost::default();
    assert_eq!(
        run(&mut copy, &mut blank, None, "status"),
        "Status: Playing. Time left: 593 s.\nRed: 1 players, 2 items, 0 lines\nBlue: 1 players, 1 items, 0 lines"
    );
    assert_eq!(
        run(&mut copy, &mut blank, None, "options"),
        "Goal: 6 item(s). Mode: lockout. End: first win. PvP: on. Keep inventory: off. Elytra: off. Night vision: off. Spawn distance: 25 chunks."
    );

    // A player who joins the restored game is sent the same card.
    blank.online = vec![player(ALICE, "Alice")];
    copy.on_join(&mut blank, &player(ALICE, "Alice"));
    assert_eq!(card_names(&mut blank, ALICE), names);

    // Bob holds square 5 again, which red owns: nothing changes.
    copy.on_inventory(&mut blank, BOB, &held(&[names[5].clone()]));
    assert_eq!(copy.snapshot_json(), text);
}

#[test]
fn a_restart_mid_game_keeps_teams_card_and_progress_and_the_game_can_end() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "goal items 3").contains("Goal: 3 item(s)"));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    let names = card_names(&mut host, ALICE);
    host.take_commands();
    host.take();
    for _ in 0..40 {
        session.tick(&mut host);
    }
    host.take();
    session.on_inventory(&mut host, ALICE, &held(&[names[0].clone(), names[1].clone()]));
    assert_eq!(saved(&host).unwrap()["elapsedTicks"], json!(40));

    // The server restarts. A new session reads the files; the players come back.
    let mut after = host.restarted();
    after.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7, 0, None));
    session.on_join(&mut after, &player(ALICE, "Alice"));
    session.on_join(&mut after, &player(BOB, "Bob"));
    let sent = after.take();

    // Alice gets her team's card, the game time so far (40 ticks, 2 s), and red's two squares.
    assert_eq!(card_names_in(&sent, ALICE), names);
    let status = decode(&only(&sent, ALICE, "game_status"), GameStatusPacket::read).expect("game_status decodes");
    assert_eq!(status.ingame_duration, Some(JavaDuration { seconds: 2, nanos: 0 }));
    let red = payloads(&sent, ALICE, "score_update")
        .iter()
        .map(|payload| decode(payload, ScoredItemPacket::read).expect("score_update decodes"))
        .find(|score| score.team == "bingo_red")
        .expect("red's score is sent");
    assert_eq!(red.score.items, 2);
    assert_eq!(run(&mut session, &mut after, None, "status"), "Status: Playing.\nRed: 1 players, 2 items, 0 lines\nBlue: 1 players, 0 items, 0 lines");

    // Play goes on from 40 ticks: 20 more make 60 ticks, 3000 ms. The third square is red's third item, the goal of 3.
    for _ in 0..20 {
        session.tick(&mut after);
    }
    after.take();
    after.take_broadcasts();
    session.on_inventory(&mut after, ALICE, &held(&[names[2].clone()]));
    after.take_broadcasts();
    assert_eq!(after.files.get(STATE_FILE).map(String::as_str), Some(""), "the ended game is no longer saved");

    let entries = parse_history(&after.files[HISTORY_FILE]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].duration_ms, 3000);
    assert_eq!(entries[0].winner.as_deref(), Some("bingo_red"));
    assert!(!entries[0].forfeit);
    assert_eq!(entries[0].teams[0].items, 3);
    assert!(entries[0].teams[0].won);
    assert_eq!(entries[0].players[0].items, 3, "Alice's team's squares");
}

#[test]
fn a_game_in_progress_keeps_its_saved_state_when_everyone_leaves() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    host.take();
    for _ in 0..10 {
        session.tick(&mut host);
    }

    let mut after = host.restarted();
    let mut session = Session::new(options(7, 0, None));
    // The server is empty: 100 ticks would reset a game nobody is in, but this one was just restored.
    for _ in 0..100 {
        session.tick(&mut after);
    }
    assert!(run(&mut session, &mut after, None, "status").starts_with("Status: Playing."));

    after.online = vec![player(ALICE, "Alice")];
    session.on_join(&mut after, &player(ALICE, "Alice"));
    for _ in 0..100 {
        session.tick(&mut after);
    }
    assert!(run(&mut session, &mut after, None, "status").starts_with("Status: Playing."));

    // Alice leaves: the round keeps running with nobody online, and its saved state is kept (not emptied), so a stop
    // with nobody online loses nothing.
    after.online.clear();
    for _ in 0..200 {
        session.tick(&mut after);
    }
    assert!(run(&mut session, &mut after, None, "status").starts_with("Status: Playing."));
    let saved = after.files.get(STATE_FILE).cloned().unwrap_or_default();
    assert!(saved.contains("\"phase\": \"Playing\""), "the game in progress stays saved: {saved}");
}

#[test]
fn the_elapsed_time_of_a_restored_game_is_the_saved_one_not_the_wall_clock() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "timelimit 10"), "Time limit set to 10 minutes");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    for _ in 0..130 {
        session.tick(&mut host);
    }
    // The time limit changed during play is a save point, so the file holds the 130 ticks.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "timelimit 10"), "Time limit set to 10 minutes");
    assert_eq!(saved(&host).unwrap()["elapsedTicks"], json!(130));

    // Ten minutes of wall clock pass while the server is down.
    let mut after = host.restarted();
    after.now += 600_000;
    after.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 0, None));
    session.on_join(&mut after, &player(ALICE, "Alice"));
    assert_eq!(run(&mut session, &mut after, None, "status"), "Status: Playing. Time left: 593 s.\nRed: 1 players, 0 items, 0 lines");

    // Twenty more ticks: 150 ticks in all, 7500 ms, so 592.5 s are left.
    for _ in 0..20 {
        session.tick(&mut after);
    }
    assert!(run(&mut session, &mut after, None, "status").starts_with("Status: Playing. Time left: 592 s."));
}

#[test]
fn every_phase_round_trips_through_the_snapshot() {
    // Pregame: a lobby with teams and a goal, not yet started.
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, Some(5)));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    let pregame = session.snapshot_json();
    assert_eq!(serde_json::from_str::<Value>(&pregame).unwrap()["phase"], "Pregame");

    // Starting: 25 ticks into a 3 s countdown, so 25 countdown ticks.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    for _ in 0..25 {
        session.tick(&mut host);
    }
    let starting = session.snapshot_json();
    let starting_value: Value = serde_json::from_str(&starting).unwrap();
    assert_eq!(starting_value["phase"], "Starting");
    assert_eq!(starting_value["countdownTicks"], json!(25));
    assert_eq!(starting_value["card"], Value::Null);

    // Playing and Postgame: a round with a vote from Alice after the forfeit.
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    for _ in 0..10 {
        session.tick(&mut host);
    }
    let playing = session.snapshot_json();
    assert_eq!(serde_json::from_str::<Value>(&playing).unwrap()["phase"], "Playing");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "end"), "Stopped the game!");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote keep"), "Vote recorded (1 of 2 votes).");
    let postgame = session.snapshot_json();
    let postgame_value: Value = serde_json::from_str(&postgame).unwrap();
    assert_eq!(postgame_value["phase"], "Postgame");
    assert_eq!(postgame_value["votes"], json!({ uuid(ALICE): "keep_playing" }));

    for (phase, text) in [("Pregame", pregame), ("Starting", starting), ("Playing", playing), ("Postgame", postgame)] {
        let mut copy = Session::new(options(7, 0, None));
        copy.restore_from_json(&text).unwrap_or_else(|error| panic!("{phase}: {error}"));
        assert_eq!(copy.snapshot_json(), text, "{phase} round trip");
    }
}

#[test]
fn corrupt_snapshots_are_rejected_and_the_session_does_not_change() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = lockout_round(&mut host, &mut session);
    for _ in 0..130 {
        session.tick(&mut host);
    }
    session.on_inventory(&mut host, ALICE, &held(&[names[0].clone(), names[5].clone()]));
    session.on_inventory(&mut host, BOB, &held(&[names[5].clone(), names[7].clone()]));
    let valid = session.snapshot_json();

    // The valid text is accepted, so each failure below is the edit's own.
    Session::new(options(7, 0, None)).restore_from_json(&valid).expect("the valid snapshot restores");

    let cases: Vec<(&str, String)> = vec![
        ("empty text", String::new()),
        ("not JSON", "{".to_string()),
        ("not an object", "[]".to_string()),
        ("seed missing", with_edit(&valid, |v| drop(v.as_object_mut().unwrap().remove("seed")))),
        ("seed as text", with_edit(&valid, |v| v["seed"] = json!("7"))),
        ("unknown version", with_edit(&valid, |v| v["version"] = json!(2))),
        ("unknown phase", with_edit(&valid, |v| v["phase"] = json!("Dancing"))),
        ("negative elapsed ticks", with_edit(&valid, |v| v["elapsedTicks"] = json!(-1))),
        ("negative time limit", with_edit(&valid, |v| v["timeLimitSecs"] = json!(-5))),
        ("unknown goal type", with_edit(&valid, |v| v["rules"]["goal"]["type"] = json!("bogus"))),
        ("no card while playing", with_edit(&valid, |v| v["card"] = Value::Null)),
        ("card of 24 squares", with_edit(&valid, |v| drop(v["card"].as_array_mut().unwrap().pop()))),
        ("unknown tier", with_edit(&valid, |v| v["card"][0]["tier"] = json!("Z"))),
        ("progress square off the card", with_edit(&valid, |v| v["progress"]["bingo_red"] = json!([25]))),
        ("owner square off the card", with_edit(&valid, |v| v["owners"]["30"] = json!("bingo_red"))),
        ("member uuid not hex", with_edit(&valid, |v| v["members"][0]["uuid"] = json!("xyz"))),
        ("member uuid one digit short", with_edit(&valid, |v| v["members"][0]["uuid"] = json!("0000000000000000000000000000001"))),
        ("member on an unknown team", with_edit(&valid, |v| v["members"][0]["team"] = json!("bingo_purple"))),
        ("team key without prefix", with_edit(&valid, |v| v["addedTeams"] = json!(["red"]))),
        ("player both member and spectator", with_edit(&valid, |v| v["spectators"] = json!([uuid(ALICE)]))),
        ("unknown vote", with_edit(&valid, |v| v["votes"] = json!({ uuid(BOB): "maybe" }))),
    ];
    for (case, text) in &cases {
        let before = session.snapshot_json();
        let result = session.restore_from_json(text);
        assert!(result.is_err(), "{case} was accepted");
        assert_eq!(session.snapshot_json(), before, "{case} changed the session");
    }
    // The running game still answers as before.
    assert!(run(&mut session, &mut host, None, "status").starts_with("Status: Playing."));
}

#[test]
fn a_damaged_or_non_playing_saved_game_leaves_the_lobby() {
    // A snapshot of a game in its countdown, the phase a restart does not resume.
    let mut source_host = FakeHost::default();
    let mut source = Session::new(options(7, 3, None));
    lobby(&mut source_host, &mut source, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    assert_eq!(run(&mut source, &mut source_host, Some(ALICE), "start"), "Starting the game...");
    let starting = source.snapshot_json();
    assert_eq!(serde_json::from_str::<Value>(&starting).unwrap()["phase"], "Starting");

    let cases = [("damaged file", "{ not json".to_string()), ("a Starting game", starting)];
    for (case, text) in cases {
        let mut host = FakeHost { files: HashMap::from([(STATE_FILE.to_string(), text)]), ..FakeHost::default() };
        host.online = vec![player(ALICE, "Alice")];
        let mut session = Session::new(options(7, 0, None));
        session.on_join(&mut host, &player(ALICE, "Alice"));
        assert!(run(&mut session, &mut host, None, "status").starts_with("Status: Pregame."), "{case} started a game");
    }
}

#[test]
fn an_ended_or_reset_game_leaves_no_restorable_state() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert!(saved(&host).is_some());

    // `/bingo end` ends the game: the file is emptied, and a restart finds no game.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "end"), "Stopped the game!");
    assert_eq!(host.files.get(STATE_FILE).map(String::as_str), Some(""));
    assert!(saved(&host).is_none());
    let mut after = host.restarted();
    let mut restarted = Session::new(options(7, 0, None));
    assert!(run(&mut restarted, &mut after, None, "status").starts_with("Status: Pregame."));
    assert!(Session::new(options(7, 0, None)).restore_from_json("").is_err());

    // A reset of the lobby after the end keeps the file empty too.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "reset"), "Resetting the game...");
    assert_eq!(host.files.get(STATE_FILE).map(String::as_str), Some(""));
}

#[test]
fn a_game_that_runs_out_of_time_empties_the_saved_game_at_tick_1200() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "timelimit 1"), "Time limit set to 1 minutes");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    // One minute is 1200 ticks. The game is still running, and still saved, one tick before the end. The saved
    // elapsed time is the one from the start, since nothing was scored or changed during play.
    for _ in 0..1199 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").starts_with("Status: Playing."));
    assert_eq!(saved(&host).unwrap()["phase"], "Playing");
    session.tick(&mut host);
    assert_eq!(host.files.get(STATE_FILE).map(String::as_str), Some(""));
    assert!(run(&mut session, &mut host, None, "status").starts_with("Status: Postgame."));
}

#[test]
fn the_saved_game_is_written_when_play_begins_and_at_each_change_during_play() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    // The countdown is not saved: 59 ticks in, nothing is written.
    for _ in 0..59 {
        session.tick(&mut host);
    }
    assert!(!host.files.contains_key(STATE_FILE));
    // Play begins at tick 60, and the game is saved with an empty card progress.
    session.tick(&mut host);
    let names = card_names(&mut host, ALICE);
    let begun = saved(&host).expect("saved at the start of play");
    assert_eq!(begun["phase"], "Playing");
    assert_eq!(begun["elapsedTicks"], json!(0));
    assert_eq!(begun["card"].as_array().unwrap().len(), 25);

    // A find is saved with the squares it gave.
    session.on_inventory(&mut host, ALICE, &held(&[names[3].clone()]));
    assert_eq!(saved(&host).unwrap()["progress"], json!({ "bingo_red": [3], "bingo_blue": [] }));

    // A spectator who joins during play is saved as one; joining a team is saved too.
    host.online.push(player(CAROL, "Carol"));
    session.on_join(&mut host, &player(CAROL, "Carol"));
    assert_eq!(saved(&host).unwrap()["spectators"], json!([uuid(CAROL)]));
    assert_eq!(run(&mut session, &mut host, Some(CAROL), "join red"), "Moved player Carol onto team Red!");
    let members = saved(&host).unwrap()["members"].clone();
    assert!(members.as_array().unwrap().contains(&json!({ "uuid": uuid(CAROL), "team": "bingo_red" })), "{members}");

    // The time limit set during play is saved in seconds.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "timelimit 5"), "Time limit set to 5 minutes");
    assert_eq!(saved(&host).unwrap()["timeLimitSecs"], json!(300));
}

#[test]
fn a_resumed_game_is_saved_again_and_keeps_its_elapsed_time() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    for _ in 0..25 {
        session.tick(&mut host);
    }
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "end"), "Stopped the game!");
    assert_eq!(host.files.get(STATE_FILE).map(String::as_str), Some(""));

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "resume"), "The game continues.");
    let resumed = saved(&host).expect("a resumed game is saved");
    assert_eq!(resumed["phase"], "Playing");
    assert_eq!(resumed["elapsedTicks"], json!(25));

    // The restart resumes the game: it is Playing before anyone joins, and the joining player finds it so.
    let mut after = host.restarted();
    after.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 0, None));
    session.on_join(&mut after, &player(ALICE, "Alice"));
    assert!(run(&mut session, &mut after, None, "status").starts_with("Status: Playing."));
}

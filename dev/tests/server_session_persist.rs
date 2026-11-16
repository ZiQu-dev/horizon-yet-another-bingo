//! What the server session saves and reads back: the round options (`options.json`), the players' settings
//! (`player-settings.json`, from the `update_settings` packet), the history of finished rounds (`history.jsonl`) and the
//! `stats` read from it, and the settings' effect on the find lines and the boss bars. Files live in the host's map, so
//! a test can start a new session on the same files, as a restart does. Expected values are counted by hand: 20 ticks
//! per second, so 130 ticks is 6500 ms, and a countdown of 0 starts play at once.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::options::Goal;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store::{self, entry_to_line, parse_history, HistoryEntry, HistoryPlayer, HistoryTeam};
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;
/// The clock of the fake host, in ms since the Unix epoch.
const NOW_MS: i64 = 1_700_000_000_000;

#[derive(Debug, Clone)]
struct Sent {
    player: u128,
    path: String,
    payload: Vec<u8>,
}

/// A boss bar as the session set it.
#[derive(Debug, Clone)]
struct Bar {
    player: u128,
    id: String,
    title: String,
    progress: f32,
    visible: bool,
}

struct FakeHost {
    online: Vec<Player>,
    sent: Vec<Sent>,
    broadcasts: Vec<String>,
    commands: Vec<String>,
    tells: Vec<(u128, String)>,
    bars: Vec<Bar>,
    /// The mod's data folder: file name to text.
    files: HashMap<String, String>,
    /// What `now_ms` returns.
    now: i64,
    /// The operator warning lines the session logged (`ServerHost::warn`).
    warnings: Vec<String>,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self {
            online: Vec::new(),
            sent: Vec::new(),
            broadcasts: Vec::new(),
            commands: Vec::new(),
            tells: Vec::new(),
            bars: Vec::new(),
            files: HashMap::new(),
            now: NOW_MS,
            warnings: Vec::new(),
        }
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

    fn take_tells(&mut self) -> Vec<(u128, String)> {
        std::mem::take(&mut self.tells)
    }

    fn take_bars(&mut self) -> Vec<Bar> {
        std::mem::take(&mut self.bars)
    }
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push(Sent { player, path: path.to_string(), payload });
    }

    fn tell(&mut self, player: u128, line: &str) {
        self.tells.push((player, line.to_string()));
    }

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

    fn boss_bar(&mut self, player: u128, id: &str, title: &str, progress: f32, visible: bool) {
        self.bars.push(Bar { player, id: id.to_string(), title: title.to_string(), progress, visible });
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

    fn warn(&mut self, line: &str) {
        self.warnings.push(line.to_string());
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
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
        assert!(answer.starts_with("You joined"), "joining {team}: {answer}");
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

fn tiles_of(payload: &[u8]) -> CardTilesPacket {
    decode(payload, CardTilesPacket::read).expect("update_card_tiles decodes")
}

fn names_of(tiles: &CardTilesPacket) -> Vec<String> {
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

/// The names of the squares of the card, as seen by one player (the card is the same for every team). Every square
/// has its own item, so the names are distinct.
fn card_names(host: &mut FakeHost, player: u128) -> Vec<String> {
    names_of(&tiles_of(&only(&host.take(), player, "update_card_tiles")))
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

/// `minecraft:oak_log` as the chat shows it.
fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn status(session: &mut Session, host: &mut FakeHost) -> String {
    run(session, host, None, "status")
}

/// The finished rounds in the history file, as the session wrote them.
fn history(host: &FakeHost) -> Vec<HistoryEntry> {
    parse_history(host.files.get("history.jsonl").map_or("", String::as_str))
}

/// A round Alice's red team won, as an earlier server wrote it.
fn red_win(id: u128, duration_ms: i64, alice_items: i32) -> HistoryEntry {
    HistoryEntry {
        id,
        ended_at_ms: 1_699_999_000_000,
        duration_ms,
        winner: Some("bingo_red".to_string()),
        forfeit: false,
        teams: vec![
            HistoryTeam { key: "bingo_red".into(), name: "Red".into(), items: alice_items, lines: 0, won: true },
            HistoryTeam { key: "bingo_blue".into(), name: "Blue".into(), items: 0, lines: 0, won: false },
        ],
        players: vec![
            HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: "bingo_red".into(), items: alice_items },
            HistoryPlayer { uuid: BOB, name: "Bob".into(), team: "bingo_blue".into(), items: 0 },
        ],
        rules: None,
    }
}

/// Alice and Bob on red and blue, the goal of red set to `items`, and the round started. Returns the card's names.
fn started_round(host: &mut FakeHost, session: &mut Session, items: u32) -> Vec<String> {
    lobby(host, session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(session, host, Some(ALICE), &format!("goal items {items}"));
    assert_eq!(run(session, host, Some(ALICE), "start"), "Game started.");
    let names = card_names(host, ALICE);
    host.take_commands();
    names
}

#[test]
fn a_goal_set_in_chat_is_saved_and_read_by_a_new_session() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "goal items 4").contains("Goal: 4 item(s)"));
    assert_eq!(store::options_from_json(&host.files["options.json"]).goal, Goal::Items(4));

    // A new session on the same files, as after a restart.
    let mut restarted = FakeHost { files: host.files.clone(), ..FakeHost::default() };
    let mut session = Session::new(options(7, 0, None));
    assert!(run(&mut session, &mut restarted, None, "options").contains("Goal: 4 item(s)"));
}

#[test]
fn broken_options_file_gives_the_default_goal() {
    let mut host = FakeHost::default();
    host.files.insert("options.json".to_string(), "not json".to_string());
    let mut session = Session::new(options(7, 0, None));
    assert!(run(&mut session, &mut host, None, "options").contains("Goal: 1 line(s)"));
}

#[test]
fn a_win_appends_one_history_line_and_a_forfeit_appends_the_next() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = started_round(&mut host, &mut session, 2);

    // 130 ticks of play, then Alice finds the two items the goal needs.
    for _ in 0..130 {
        session.tick(&mut host);
    }
    host.take();
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    assert!(status(&mut session, &mut host).contains("Postgame"));

    assert_eq!(host.files["history.jsonl"].lines().count(), 1);
    let entries = history(&host);
    assert_eq!(entries.len(), 1);
    let won = &entries[0];
    assert_eq!(won.winner.as_deref(), Some("bingo_red"));
    assert!(!won.forfeit);
    assert_eq!(won.ended_at_ms, NOW_MS);
    assert_eq!(won.duration_ms, 130 * 50, "130 ticks of 50 ms");
    let team = |key: &str| won.teams.iter().find(|t| t.key == key).expect("a team of the round");
    assert_eq!((team("bingo_red").won, team("bingo_blue").won), (true, false));
    assert_eq!(team("bingo_red").name, "Red");
    let alice = won.players.iter().find(|p| p.uuid == ALICE).expect("Alice played");
    assert_eq!((alice.name.as_str(), alice.team.as_str(), alice.items), ("Alice", "bingo_red", 2));

    // Both players vote for the lobby, the next round is started and ended by the console: a forfeit, no winner.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote lobby"), "Vote recorded (1 of 2 votes).");
    assert_eq!(run(&mut session, &mut host, Some(BOB), "vote lobby"), "The vote is over: back to the lobby.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Game started.");
    assert_eq!(run(&mut session, &mut host, None, "end"), "Game ended.");

    assert_eq!(host.files["history.jsonl"].lines().count(), 2);
    let entries = history(&host);
    assert_eq!(entries.len(), 2);
    assert!(entries[1].forfeit);
    assert_eq!(entries[1].winner, None);
    assert!(entries[1].teams.iter().all(|t| !t.won));
}

#[test]
fn stats_count_the_win_with_its_best_time_and_the_items_found() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = started_round(&mut host, &mut session, 2);
    for _ in 0..130 {
        session.tick(&mut host);
    }
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    host.take();
    host.take_broadcasts();

    // 6500 ms is 6 s: `00m 06s`. Alice found 2 items and won; Bob's blue team lost and he found none.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "stats"), "Bingo stats for Alice:\nAll Time:\n  Games: 1, Win/Loss: 1/0, Items: 2\n  Playtime: 00m 06s\nMonthly:\n  Games: 1, Win/Loss: 1/0, Items: 2\n  Playtime: 00m 06s");
    assert_eq!(run(&mut session, &mut host, Some(BOB), "stats"), "Bingo stats for Bob:\nAll Time:\n  Games: 1, Win/Loss: 0/1, Items: 0\n  Playtime: 00m 06s\nMonthly:\n  Games: 1, Win/Loss: 0/1, Items: 0\n  Playtime: 00m 06s");
    assert_eq!(run(&mut session, &mut host, None, "stats"), "Only a player has statistics.");
}

#[test]
fn history_loaded_at_start_counts_in_stats() {
    let mut host = FakeHost::default();
    let first = red_win(0x11, 1500, 1);
    let second = red_win(0x12, 3000, 2);
    host.files.insert("history.jsonl".to_string(), format!("{}\n{}\n", entry_to_line(&first), entry_to_line(&second)));
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);

    // Both rounds were won, both started in the month of the clock: 1500 + 3000 ms is 4 s. Items 1 + 2.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "stats"), "Bingo stats for Alice:\nAll Time:\n  Games: 2, Win/Loss: 2/0, Items: 3\n  Playtime: 00m 04s\nMonthly:\n  Games: 2, Win/Loss: 2/0, Items: 3\n  Playtime: 00m 04s");
    assert_eq!(host.files["history.jsonl"].lines().count(), 2, "reading the history writes nothing");
}

#[test]
fn a_find_is_told_only_to_the_player_who_wants_it_when_another_turned_item_messages_off() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = started_round(&mut host, &mut session, 5);

    // Alice turns the item messages off with `prefs`: her client's packets do not change her saved settings.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs item_messages off"), "Set 'Item Scored' = Off");
    let saved = store::settings_from_json(&host.files["player-settings.json"]);
    assert!(!saved[&ALICE].item_messages);

    // The start's banner went to the team players as tells; this test is about the finds, so it is read and dropped.
    host.take_tells();
    // Bob finds a square: Alice, who turned the item messages off, gets no find line; Bob gets his own. The find makes
    // blue the leader, and every player who keeps the leading messages on is told it (`GameMessageService`).
    session.on_inventory(&mut host, BOB, &held(&names[0..1]));
    host.take();
    assert_eq!(
        host.take_tells(),
        [
            (BOB, format!("Bob found {}", short(&names[0]))),
            (ALICE, "Blue is in the lead with 1 item!".to_string()),
            (BOB, "Blue is in the lead with 1 item!".to_string()),
        ]
    );
    assert!(host.take_broadcasts().is_empty(), "no broadcast when someone opted out");
}

#[test]
fn without_settings_a_find_is_broadcast() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = started_round(&mut host, &mut session, 5);
    // The start's banner went to the team players as tells; it is read and dropped before the find.
    host.take_tells();

    session.on_inventory(&mut host, BOB, &held(&names[0..1]));
    host.take();
    assert_eq!(host.take_broadcasts(), [format!("Bob found {}", short(&names[0]))]);
    // The find makes blue the leader: the leading message goes to each player, as the original sends it.
    assert_eq!(
        host.take_tells(),
        [(ALICE, "Blue is in the lead with 1 item!".to_string()), (BOB, "Blue is in the lead with 1 item!".to_string())]
    );
}

#[test]
fn the_boss_bar_follows_the_bossbar_setting_of_each_player() {
    for alice_wants_bar in [false, true] {
        let mut host = FakeHost::default();
        let mut session = Session::new(options(7, 0, None));
        lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
        let said = if alice_wants_bar { "On" } else { "Off" };
        assert_eq!(run(&mut session, &mut host, Some(ALICE), &format!("prefs bossbar {}", said.to_lowercase())), format!("Set 'Bossbar' = {said}"));
        assert!(run(&mut session, &mut host, None, "timelimit 1").starts_with("Time limit set to 1"));
        run(&mut session, &mut host, Some(ALICE), "start");
        host.take();

        // Ten ticks in: 500 ms of a 60 s limit have passed, so 59.5 s are left.
        for _ in 0..10 {
            session.tick(&mut host);
        }
        let bars = host.take_bars();
        let bob_bar = bars.iter().find(|b| b.player == BOB).expect("Bob sees the bar");
        assert!(bob_bar.visible);
        assert_eq!((bob_bar.id.as_str(), bob_bar.title.as_str()), ("bingo:bingo_blue", "00:59 left"));
        assert!((bob_bar.progress - 59_500.0 / 60_000.0).abs() < 1e-4, "progress {}", bob_bar.progress);

        let alice_has_bar = bars.iter().any(|b| b.player == ALICE);
        assert_eq!(alice_has_bar, alice_wants_bar, "Alice's bar with bossbar {alice_wants_bar}");
        if alice_wants_bar {
            assert!(bars.iter().any(|b| b.player == ALICE && b.id == "bingo:bingo_red"));
        }
    }
}

const CAROL: u128 = 3;

/// Alice and Bob on red, Carol online with no team, goal of 3 items, and the round started at 0 ticks.
fn red_pair_round(host: &mut FakeHost, session: &mut Session) -> Vec<String> {
    lobby(host, session, &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol")], &[(ALICE, "red"), (BOB, "red")]);
    assert!(run(session, host, Some(ALICE), "goal items 3").contains("Goal: 3 item(s)"));
    // Carol is not on a team: `ignore_warnings` makes her a spectator, so she is not in the roster.
    assert_eq!(run(session, host, Some(ALICE), "start ignore_warnings"), "Game started.");
    let names = card_names(host, ALICE);
    host.take_commands();
    names
}

#[test]
fn a_win_records_each_players_own_items_and_the_team_roster_not_everyone_online() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = red_pair_round(&mut host, &mut session);
    for _ in 0..130 {
        session.tick(&mut host);
    }
    host.take();
    // Alice finds one square, Bob two: red's goal of 3 is reached on Bob's inventory.
    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    session.on_inventory(&mut host, BOB, &held(&names[1..3]));
    assert!(status(&mut session, &mut host).contains("Postgame"));

    let entries = history(&host);
    assert_eq!(entries.len(), 1);
    let won = &entries[0];
    let mut players: Vec<(u128, i32)> = won.players.iter().map(|p| (p.uuid, p.items)).collect();
    players.sort();
    assert_eq!(players, vec![(ALICE, 1), (BOB, 2)], "each player's own finds; Carol, online with no team, is not in the roster");
    assert_eq!(won.teams[0].items, 3, "the team's count stays the team's");
    assert_eq!(won.rules.as_deref().map(str::len), Some(16), "the round carries its rules key");
}

#[test]
fn the_end_screen_carries_the_win_streak_best_time_and_items_of_each_viewer() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = started_round(&mut host, &mut session, 2);
    // First round: 130 ticks (6500 ms), Alice finds 2 and wins.
    for _ in 0..130 {
        session.tick(&mut host);
    }
    host.take();
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    let sent = host.take();
    let alice = decode(&only(&sent, ALICE, "game_over_v2"), GameOverPacket::read_v2).expect("decodes");
    assert_eq!(alice.win_streak, Some(1));
    assert_eq!((alice.best_win_streak, alice.is_best_win_streak), (None, false));
    assert_eq!(alice.captured_items, Some(2));
    assert_eq!((alice.best_captured_items, alice.is_best_captured_items), (None, false));
    assert!(alice.is_best_time, "the first finished round sets the best time");
    assert_eq!(alice.prev_best_time, None);
    let bob = decode(&only(&sent, BOB, "game_over_v2"), GameOverPacket::read_v2).expect("decodes");
    assert_eq!(bob.win_streak, Some(0), "Bob's blue team lost");
    assert_eq!(bob.captured_items, Some(0));

    // Second round: 100 ticks (5000 ms), Alice wins again with 2 items.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote lobby"), "Vote recorded (1 of 2 votes).");
    assert_eq!(run(&mut session, &mut host, Some(BOB), "vote lobby"), "The vote is over: back to the lobby.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Game started.");
    let names = card_names(&mut host, ALICE);
    host.take_commands();
    host.take();
    for _ in 0..100 {
        session.tick(&mut host);
    }
    host.take();
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    let sent = host.take();
    let again = decode(&only(&sent, ALICE, "game_over_v2"), GameOverPacket::read_v2).expect("decodes");
    assert_eq!(again.win_streak, Some(2));
    assert_eq!(again.best_win_streak, Some(1));
    assert!(again.is_best_win_streak, "2 is above the best streak of 1");
    assert!(again.is_best_time, "5000 ms is shorter than 6500 ms");
    assert_eq!(again.prev_best_time.map(|d| (d.seconds, d.nanos)), Some((6, 500_000_000)));
    assert_eq!(again.captured_items, Some(2));
    assert_eq!(again.best_captured_items, Some(2));
    assert!(!again.is_best_captured_items, "2 is not above the best 2");

    // The `stats` command for an online player, by operator: Bob's blue team lost both rounds.
    assert_eq!(run(&mut session, &mut host, None, "stats Bob").lines().nth(1), Some("All Time:"));
    assert!(run(&mut session, &mut host, None, "stats Bob").contains("Games: 2, Win/Loss: 0/2, Items: 0"));
    assert_eq!(run(&mut session, &mut host, None, "stats Zed"), "No player named Zed is online.");
}

/// A session on a fresh host whose `game-state.json` is `text`, after the first command has loaded the files.
fn session_with_saved_game(text: &str) -> (Session, FakeHost) {
    let mut host = FakeHost::default();
    host.files.insert("game-state.json".to_string(), text.to_string());
    let mut session = Session::new(options(7, 0, None));
    status(&mut session, &mut host);
    (session, host)
}

#[test]
fn a_corrupt_saved_game_leaves_the_session_as_if_there_were_no_file_and_warns_once() {
    let text = &"{\"phase\": \"Playing\", \"version\": 1, \"game\": {".repeat(3);
    let (mut session, mut host) = session_with_saved_game(text);
    let (mut fresh, mut fresh_host) = session_with_saved_game("");
    assert_eq!(host.warnings.len(), 1, "one warning, got {:?}", host.warnings);
    assert!(host.warnings[0].contains("game-state.json"));
    assert!(fresh_host.warnings.is_empty());
    assert_eq!(session.snapshot_json(), fresh.snapshot_json());
    assert_eq!(status(&mut session, &mut host), status(&mut fresh, &mut fresh_host));
    assert!(session.is_pregame());
}

#[test]
fn an_empty_saved_game_is_no_game_and_warns_nothing() {
    let (session, host) = session_with_saved_game("");
    assert!(host.warnings.is_empty(), "got {:?}", host.warnings);
    assert!(session.is_pregame());
    let (session, host) = session_with_saved_game("  \n");
    assert!(host.warnings.is_empty(), "got {:?}", host.warnings);
    assert!(session.is_pregame());
}

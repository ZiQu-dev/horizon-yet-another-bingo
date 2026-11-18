//! Team chat and the global aliases (`chat.rs`, the original's `ChatMessageController` and `ChatCommand`), the chat
//! options in `options.json`, and the session's chat routing. Expected values are written by hand from the original's
//! rules: during Starting and Playing a player's line goes to their team, a player on no team gets the spectators' line,
//! and `g`, `global` or `all` send a line to everyone.
use horizon_bingo::server::chat::{self, Routed};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::options::Options;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store::{options_from_json, options_to_json};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;
const DAN: u128 = 4;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    commands: Vec<String>,
    broadcasts: Vec<String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}
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
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn session_with(rules: Options, countdown_secs: i32) -> Session {
    Session::new(SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        rules,
        ..SessionOptions::default()
    })
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Alice and Bob on Red and Blue, Carol a spectator, Dan on no team and not spectating.
fn table(session: &mut Session, host: &mut FakeHost) {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob"), player(CAROL, "Carol"), player(DAN, "Dan")];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(run(session, host, Some(ALICE), "join Red").starts_with("Moved player"));
    assert!(run(session, host, Some(BOB), "join Blue").starts_with("Moved player"));
    assert!(run(session, host, Some(CAROL), "spectator").starts_with("You are"), "spectating");
    host.commands.clear();
}

// Pure routing.

#[test]
fn a_team_line_goes_to_the_members_of_that_team_only() {
    let online = [(ALICE, Some("bingo_red")), (BOB, Some("bingo_blue")), (CAROL, Some("bingo_red")), (DAN, None)];
    let routed = chat::route("Alice", Some("bingo_red"), &online, "hello");
    assert_eq!(routed.recipients, vec![ALICE, CAROL]);
    assert_eq!(routed.line, "[Red] <Alice> hello");
}

#[test]
fn a_player_on_no_team_talks_to_the_other_players_on_no_team() {
    let online = [(ALICE, Some("bingo_red")), (CAROL, None), (DAN, None)];
    let routed = chat::route("Carol", None, &online, "hi all");
    assert_eq!(routed, Routed { recipients: vec![CAROL, DAN], line: "[Spectators] <Carol> hi all".to_string() });
}

#[test]
fn a_team_label_is_the_preset_name_and_an_unknown_team_is_its_key() {
    let online = [(ALICE, Some("bingo_pink"))];
    assert_eq!(chat::route("Alice", Some("bingo_pink"), &online, "x").line, "[Pink] <Alice> x");
    assert_eq!(chat::route("Alice", Some("bingo_mauve"), &online, "x").line, "[bingo_mauve] <Alice> x");
}

#[test]
fn a_team_with_no_one_online_has_no_recipients() {
    let online = [(ALICE, None)];
    assert!(chat::route("Alice", Some("bingo_gray"), &online, "anyone?").recipients.is_empty());
}

#[test]
fn global_commands_take_their_text_and_nothing_else() {
    assert_eq!(chat::global_text("g hello"), Some("hello"));
    assert_eq!(chat::global_text("global  two words "), Some("two words"));
    assert_eq!(chat::global_text("all x"), Some("x"));
    assert_eq!(chat::global_text("g"), None, "no text");
    assert_eq!(chat::global_text("g   "), None, "blank text");
    assert_eq!(chat::global_text("gg hi"), None, "another word");
    assert_eq!(chat::global_text("go home"), None);
    assert_eq!(chat::global_text("bingo g x"), None);
    assert_eq!(chat::global_line("Alice", "hi"), "<Alice> hi");
}

// Options.

#[test]
fn team_chat_and_the_sidebar_are_on_by_default_and_written_only_when_off() {
    let default = Options::default();
    assert!(default.team_chat && default.sidebar);
    let text = options_to_json(&default);
    assert!(!text.contains("teamChat") && !text.contains("sidebar"), "{text}");

    let off = Options { team_chat: false, sidebar: false, ..Options::default() };
    let text = options_to_json(&off);
    assert!(text.contains("\"teamChat\": false") && text.contains("\"sidebar\": false"), "{text}");
    assert_eq!(options_from_json(&text), off);
}

#[test]
fn a_damaged_chat_flag_takes_its_default() {
    let read = options_from_json(r#"{"teamChat": "no", "sidebar": false}"#);
    assert!(read.team_chat);
    assert!(!read.sidebar);
}

// Session.

#[test]
fn team_chat_is_on_only_while_a_game_starts_or_plays() {
    let mut session = session_with(Options::default(), 2);
    let mut host = FakeHost::default();
    table(&mut session, &mut host);
    assert!(!session.team_chat_on(), "Pregame");

    assert!(run(&mut session, &mut host, None, "start ignore_warnings").starts_with("Starting the game"));
    assert!(session.team_chat_on(), "Starting, the countdown");
}

#[test]
fn team_chat_stays_off_when_the_option_is_off() {
    let mut session = session_with(Options { team_chat: false, ..Options::default() }, 2);
    let mut host = FakeHost::default();
    table(&mut session, &mut host);
    run(&mut session, &mut host, None, "start ignore_warnings");
    assert!(!session.team_chat_on());
}

#[test]
fn a_session_routes_a_line_to_the_sender_team_or_to_the_spectators() {
    let mut session = session_with(Options::default(), 2);
    let mut host = FakeHost::default();
    table(&mut session, &mut host);
    run(&mut session, &mut host, None, "start ignore_warnings");

    let online = host.online.clone();
    let from_alice = session.team_chat_route(&online, ALICE, "push");
    assert_eq!(from_alice.recipients, vec![ALICE]);
    assert_eq!(from_alice.line, "[Red] <Alice> push");

    let from_carol = session.team_chat_route(&online, CAROL, "watching");
    assert_eq!(from_carol.recipients, vec![CAROL, DAN], "spectators and the player on no team");
    assert_eq!(from_carol.line, "[Spectators] <Carol> watching");
}

#[test]
fn a_session_route_for_an_offline_sender_has_no_name_but_keeps_the_text() {
    let mut session = session_with(Options::default(), 2);
    let mut host = FakeHost::default();
    table(&mut session, &mut host);
    run(&mut session, &mut host, None, "start ignore_warnings");

    let routed = session.team_chat_route(&[player(BOB, "Bob")], ALICE, "gone");
    assert_eq!(routed.line, "[Red] <> gone");
}

#[test]
fn the_sidebar_is_the_lobby_rows_with_the_settings_and_the_waiting_line() {
    let mut session = session_with(Options::default(), 2);
    let mut host = FakeHost::default();
    table(&mut session, &mut host);
    assert_eq!(
        horizon_bingo::server::sidebar::english_lines(&session.sidebar_rows(&[]).expect("sidebar on")),
        ["Mode: Standard", "Features: \u{1F5E1}", "Goal: 1 line", "Time: 00m 00s", "", "Waiting to start..."]
    );

    let off = session_with(Options { sidebar: false, ..Options::default() }, 2);
    assert_eq!(off.sidebar_rows(&[]), None);
}

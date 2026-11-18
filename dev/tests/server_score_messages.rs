//! The line and leading-team messages of the server session (`GameMessageService.kt`, `ScoreUpdateService.kt`,
//! `ScoredItemCheck.kt`): which player gets a "scored a line" message and which gets a "in the lead" message. Expected
//! lines are written from the original's English (`en_us.json`) under its keys, and the squares are counted by hand: row
//! 0 of the card is squares 0 to 4, so the card's first five tile names are a line.
use horizon_bingo::buf::decode;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts::Line;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;
use std::collections::HashMap;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

/// One translatable line sent to one player: its key and what the player reads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Told {
    player: u128,
    key: String,
    text: String,
}

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<(u128, String, Vec<u8>)>,
    told: Vec<Told>,
    broadcasts: Vec<String>,
    commands: Vec<String>,
}

impl FakeHost {
    fn take_told(&mut self) -> Vec<Told> {
        std::mem::take(&mut self.told)
    }

    /// The lines one player was told, in order, as what they read.
    fn texts_of(told: &[Told], player: u128) -> Vec<String> {
        told.iter().filter(|t| t.player == player).map(|t| t.text.clone()).collect()
    }
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push((player, path.to_string(), payload));
    }

    fn tell(&mut self, player: u128, line: &str) {
        self.told.push(Told { player, key: String::new(), text: line.to_string() });
    }

    fn tell_line(&mut self, player: u128, line: &Line) {
        self.told.push(Told { player, key: line.key.to_string(), text: line.render() });
    }

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

fn options() -> SessionOptions {
    SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

fn lobby(host: &mut FakeHost, session: &mut Session, online: &[(u128, &str)], teams: &[(u128, &str)]) {
    host.online = online.iter().map(|&(uuid, name)| player(uuid, name)).collect();
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    for &(uuid, team) in teams {
        let answer = run(session, host, Some(uuid), &format!("join {team}"));
        assert!(answer.starts_with("Moved player"), "joining {team}: {answer}");
    }
    host.sent.clear();
    host.commands.clear();
    host.told.clear();
    host.broadcasts.clear();
}

/// The names of the card's squares, in square order, from the tiles one player was sent. Drains the sent packets.
fn card_names(host: &mut FakeHost, player: u128) -> Vec<String> {
    let payload = host
        .sent
        .iter()
        .find(|(to, path, _)| *to == player && path == "update_card_tiles")
        .map(|(_, _, payload)| payload.clone())
        .expect("the player has the card");
    let tiles = decode(&payload, CardTilesPacket::read).expect("update_card_tiles decodes");
    host.sent.clear();
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

fn ticks(session: &mut Session, host: &mut FakeHost, count: u32) {
    for _ in 0..count {
        session.tick(host);
    }
}

const LINE_KEY: &str = "bingo.game.message.scored_line";
const LEADING_KEY: &str = "bingo.game.message.leading_with_score";
const TIED_KEY: &str = "bingo.game.message.leading_tied_with_score";

/// Alice (red) and Bob (blue) play a card whose goal is `goal`, and the game starts. Returns the card's names.
fn start_two_teams(goal: &str) -> (FakeHost, Session, Vec<String>) {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), &format!("goal {goal}")).starts_with("Goal:"));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    let names = card_names(&mut host, ALICE);
    host.told.clear();
    host.broadcasts.clear();
    host.commands.clear();
    (host, session, names)
}

#[test]
fn a_line_is_announced_to_everyone_then_the_new_leader_is_announced() {
    // A goal of two lines, so the first line does not end the game. Red finds its top row: 1 line, and red leads
    // (blue has none). Both teams were tied at nothing before, so no team led.
    let (mut host, mut session, names) = start_two_teams("lines 2");
    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));

    let told = host.take_told();
    assert_eq!(FakeHost::texts_of(&told, ALICE), ["Red has scored 1 line!", "Red is in the lead with 1 line!"]);
    assert_eq!(FakeHost::texts_of(&told, BOB), ["Red has scored 1 line!", "Red is in the lead with 1 line!"]);
    assert_eq!(told.iter().filter(|t| t.key == LINE_KEY).count(), 2);
}

#[test]
fn a_player_who_turned_a_message_off_does_not_get_it_and_a_spectator_gets_the_line_but_not_the_lead_when_they_turn_it_off() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "goal lines 2");
    run(&mut session, &mut host, Some(BOB), "prefs score_messages off");
    run(&mut session, &mut host, Some(CAROL), "prefs leading_messages off");
    run(&mut session, &mut host, Some(ALICE), "start ignore_warnings");
    let names = card_names(&mut host, ALICE);
    host.told.clear();

    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));
    let told = host.take_told();
    // Bob keeps the leading message and loses the line message; Carol, a spectator, is told the line but not the lead.
    assert_eq!(FakeHost::texts_of(&told, ALICE), ["Red has scored 1 line!", "Red is in the lead with 1 line!"]);
    assert_eq!(FakeHost::texts_of(&told, BOB), ["Red is in the lead with 1 line!"]);
    assert_eq!(FakeHost::texts_of(&told, CAROL), ["Red has scored 1 line!"]);
}

#[test]
fn two_teams_tied_on_a_line_are_announced_as_tied_with_the_team_that_changed_last() {
    let (mut host, mut session, names) = start_two_teams("lines 2");
    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));
    host.take_told();
    ticks(&mut session, &mut host, 5);

    // Blue finds the same top row a few ticks later: both teams have 1 line, so no team leads, and blue is the team whose
    // score changed last.
    session.on_inventory(&mut host, BOB, &held(&names[0..5]));
    let told = host.take_told();
    assert_eq!(FakeHost::texts_of(&told, ALICE), ["Blue has scored 1 line!", "Blue is tied in the lead with 1 line!"]);
    let tied: Vec<&Told> = told.iter().filter(|t| t.key == TIED_KEY).collect();
    assert_eq!(tied.len(), 2, "every player with leading messages on is told the tie");
}

#[test]
fn a_goal_that_is_not_lines_announces_no_line_but_the_lead_with_items() {
    let (mut host, mut session, names) = start_two_teams("items 10");
    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));

    let told = host.take_told();
    assert!(told.iter().all(|t| t.key != LINE_KEY), "the lines are not a goal here, so no line is announced");
    assert_eq!(FakeHost::texts_of(&told, ALICE), ["Red is in the lead with 5 items!"]);
}

#[test]
fn the_winning_line_ends_the_game_and_is_announced_but_no_lead_is_announced_after_it() {
    // The default goal is one line, and the first team with one wins: the same change ends the game.
    let (mut host, mut session, names) = start_two_teams("lines 1");
    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));

    // The end's own chat (the winner, the ranking) is told too; only the score messages are checked here.
    let told = host.take_told();
    let messages: Vec<&Told> = told.iter().filter(|t| t.key.starts_with("bingo.game.message.")).collect();
    assert_eq!(messages.iter().filter(|t| t.player == ALICE).map(|t| t.text.as_str()).collect::<Vec<_>>(), ["Red has scored 1 line!"]);
    assert!(messages.iter().all(|t| t.key != LEADING_KEY && t.key != TIED_KEY));
}

#[test]
fn a_single_team_is_not_announced_as_leading() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "goal lines 2");
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.told.clear();

    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));
    let told = host.take_told();
    assert_eq!(FakeHost::texts_of(&told, ALICE), ["Red has scored 1 line!"]);
}

//! The end of a game as the original's `GameOverController` writes it in chat: a rule, the reason, the duration, the ranking
//! of the teams when there is more than one (and the rule `endwhen` allows it), and a rule. It goes to the players on a
//! team. Driven through a fake host that records every chat line.
//!
//! Expected lines are written out by hand: 20 ticks per second, so a 60 s time limit ends after 1200 ticks, and a
//! 100-tick round lasts 5 s.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<(u128, String, Vec<u8>)>,
    tells: Vec<(u128, String)>,
    broadcasts: Vec<String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push((player, path.to_string(), payload));
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

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
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

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Alice on red, Bob on blue and Carol online with no team (a spectator), in a started round. `setup` runs first.
fn started(host: &mut FakeHost, session: &mut Session, setup: &[&str]) {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob"), player(CAROL, "Carol")];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(run(session, host, Some(ALICE), "join red").starts_with("Moved player"));
    assert!(run(session, host, Some(BOB), "join blue").starts_with("Moved player"));
    for line in setup {
        run(session, host, None, line);
    }
    assert_eq!(run(session, host, None, "start ignore_warnings"), "Starting the game...");
    // The start's packets stay in `sent`: a test reads the card the round dealt from them.
    host.tells.clear();
    host.broadcasts.clear();
}

fn tells_of(host: &FakeHost, player: u128) -> Vec<String> {
    host.tells.iter().filter(|(to, _)| *to == player).map(|(_, line)| line.clone()).collect()
}

/// The item of square 0 on the card the player's team plays, from the last card sent to the player.
fn first_square_item(host: &FakeHost, player: u128) -> String {
    let payload = host
        .sent
        .iter()
        .rev()
        .find(|(to, path, _)| *to == player && path == "update_card_tiles")
        .map(|(_, _, payload)| payload.clone())
        .expect("the card was sent");
    let packet = decode(&payload, CardTilesPacket::read).expect("update_card_tiles decodes");
    let (_, tile) = packet.tiles.iter().find(|(index, _)| *index == 0).expect("square 0");
    tile.id.clone().expect("a plain item square")
}

/// A rule of 40 dashes, as the original's game_end.json has it.
fn rule() -> String {
    "┄".repeat(40)
}

#[test]
fn ending_by_command_says_the_game_was_stopped_and_ranks_the_teams() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["endwhen all_win"]);

    assert_eq!(run(&mut session, &mut host, None, "end"), "Stopped the game!");
    let lines = tells_of(&host, ALICE);
    assert_eq!(lines.len(), 6, "rule, reason, duration, two ranking rows, rule: {lines:?}");
    assert_eq!(lines[0], rule());
    assert_eq!(lines[1], "Game stopped.");
    assert_eq!(lines[2], "Game duration: 00m 00s", "no tick ran, so no time has passed");
    // Nothing scored, so both teams have 0 and the game's duration; the places are 1 and 2, in some order of the teams.
    let rows = &lines[3..5];
    assert!(rows[0].starts_with("1. ") && rows[1].starts_with("2. "), "{rows:?}");
    let first = rows[0].strip_prefix("1. ").unwrap().split(" - ").next().unwrap();
    let second = rows[1].strip_prefix("2. ").unwrap().split(" - ").next().unwrap();
    let mut teams = [first, second];
    teams.sort_unstable();
    assert_eq!(teams, ["Blue", "Red"], "both teams are ranked");
    for row in rows {
        assert!(row.ends_with(" - 0 - 00m 00s"), "{row}");
    }
    assert_eq!(lines[5], rule());
    assert!(tells_of(&host, CAROL).is_empty(), "a spectator gets no end chat");
}

#[test]
fn the_time_limit_ends_the_game_with_out_of_time() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["timelimit 1"]);

    let mut ticks = 0;
    while !session.is_postgame() {
        session.tick(&mut host);
        ticks += 1;
        assert!(ticks <= 1300, "the one-minute limit ends the game");
    }
    // The first ranking is not shown: the default `endwhen first_win` ends the game with nobody's win.
    assert_eq!(
        tells_of(&host, ALICE),
        vec![rule(), "Out of time!".to_string(), "Game duration: 01m 00s".to_string(), rule()]
    );
}

#[test]
fn a_team_that_completes_its_card_is_the_reason() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["goal items 1"]);

    for _ in 0..100 {
        session.tick(&mut host);
    }
    let item = first_square_item(&host, ALICE);
    host.tells.clear();
    let held: HashMap<String, u32> = [(item, 1)].into_iter().collect();
    session.on_inventory(&mut host, ALICE, &held);

    let expected = vec![rule(), "Red completed their card!".to_string(), "Game duration: 00m 05s".to_string(), rule()];
    assert_eq!(tells_of(&host, ALICE), expected);
    assert_eq!(tells_of(&host, BOB), expected, "the other team on the server hears the end too");
    assert!(tells_of(&host, CAROL).is_empty(), "a spectator gets no end chat");
}

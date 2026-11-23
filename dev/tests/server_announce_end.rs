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
fn ending_by_command_names_who_stopped_it_and_ranks_the_teams_with_dnf() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["endwhen all_win"]);

    assert_eq!(run(&mut session, &mut host, None, "end"), "Stopped the game!");
    // game_end.json: rule, reason, duration, seed, a blank line and one row per team (the scores), rule; each text line
    // after the rule is indented two spaces. Neither team finished, so both rows are DNF and share place 1.
    let lines = tells_of(&host, ALICE);
    assert_eq!(lines.len(), 8, "{lines:?}");
    assert_eq!(lines[0], rule());
    assert_eq!(lines[1], "  Stopped by RCON", "no sender name from the console: RCON");
    assert_eq!(lines[2], "  Game duration: 00m 00s", "no tick ran, so no time has passed");
    assert_eq!(lines[3], "  Seed: [7]");
    assert_eq!(lines[4], "", "a blank line before the ranking");
    let mut rows = vec![lines[5].clone(), lines[6].clone()];
    rows.sort();
    assert_eq!(rows, ["1. Blue - 0 - DNF", "1. Red - 0 - DNF"]);
    assert_eq!(lines[7], rule());
    assert!(tells_of(&host, CAROL).is_empty(), "a spectator gets no end chat");
}

#[test]
fn a_player_who_stops_the_game_is_named_in_the_reason() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    // With the default `endwhen first_win` the draw message would take the reason's place in the chat.
    started(&mut host, &mut session, &["endwhen all_win"]);
    run(&mut session, &mut host, Some(ALICE), "end");
    assert_eq!(tells_of(&host, BOB)[1], "  Stopped by Alice");
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
    // The ranking is not shown (the default `endwhen first_win`), and with no team ahead the message is the draw
    // (`GameOverService.getMessage`), which replaces the reason in the chat.
    assert_eq!(
        tells_of(&host, ALICE),
        vec![
            rule(),
            "  The game is a draw!".to_string(),
            "  Game duration: 01m 00s".to_string(),
            "  Seed: [7]".to_string(),
            rule()
        ]
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

    let expected = vec![
        rule(),
        "  Red completed their card!".to_string(),
        "  Game duration: 00m 05s".to_string(),
        "  Seed: [7]".to_string(),
        rule(),
    ];
    assert_eq!(tells_of(&host, ALICE), expected);
    assert_eq!(tells_of(&host, BOB), expected, "the other team on the server hears the end too");
    assert!(tells_of(&host, CAROL).is_empty(), "a spectator gets no end chat");
}

/// A host that keeps the styled pieces of every line.
#[derive(Default)]
struct StyledHost {
    inner: FakeHost,
    spans: Vec<Vec<horizon_bingo::server::texts::Span>>,
}

impl ServerHost for StyledHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.inner.send(player, path, payload);
    }
    fn tell(&mut self, player: u128, line: &str) {
        self.inner.tell(player, line);
    }
    fn tell_spans(&mut self, player: u128, spans: &[horizon_bingo::server::texts::Span]) {
        if player == ALICE {
            self.spans.push(spans.to_vec());
        }
    }
    fn broadcast(&mut self, line: &str) {
        self.inner.broadcast(line);
    }
    fn players(&self) -> Vec<Player> {
        self.inner.players()
    }
    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        self.inner.item_numeric_id(item)
    }
}

#[test]
fn the_end_chat_is_yellow_ruled_with_a_green_time_and_a_copyable_seed() {
    use horizon_bingo::server::texts::Span;
    let mut host = StyledHost::default();
    host.inner.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7));
    for p in host.inner.online.clone() {
        session.on_join(&mut host, &p);
    }
    session.command(&mut host, Some(ALICE), "join red");
    session.command(&mut host, Some(BOB), "join blue");
    session.command(&mut host, None, "endwhen all_win");
    session.command(&mut host, None, "start ignore_warnings");
    host.spans.clear();
    session.command(&mut host, None, "end");

    let rule_line = vec![Span::Text { text: rule(), color: Some("yellow"), copy: None }];
    assert_eq!(host.spans.first(), Some(&rule_line));
    assert_eq!(host.spans.last(), Some(&rule_line));
    let duration = &host.spans[2];
    assert!(matches!(&duration[1], Span::Translate { key: "bingo.game.end.duration", args, .. }
        if args == &[Span::Text { text: "00m 00s".into(), color: Some("green"), copy: None }]), "{duration:?}");
    let seed = &host.spans[3];
    assert!(matches!(&seed[1], Span::Translate { key: "bingo.game.end.seed", args, .. }
        if args == &[Span::Text { text: "[7]".into(), color: Some("green"), copy: Some("7".into()) }]), "{seed:?}");
    // Ranking rows start with a gray place.
    assert_eq!(host.spans[5][0], Span::Text { text: "1. ".into(), color: Some("gray"), copy: None });
}

fn game_over_of(host: &FakeHost, player: u128) -> horizon_bingo::game_over::GameOverPacket {
    let payload = host
        .sent
        .iter()
        .rev()
        .find(|(to, path, _)| *to == player && path == "game_over_v2")
        .map(|(_, _, payload)| payload.clone())
        .expect("a game_over_v2 was sent");
    decode(&payload, horizon_bingo::game_over::GameOverPacket::read_v2).expect("game_over_v2 decodes")
}

#[test]
fn the_end_screen_opens_on_scores_for_a_draw_offers_the_vote_and_comes_back_on_rejoin() {
    use horizon_bingo::game_over::EndScreenTab;
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &[]);
    run(&mut session, &mut host, None, "end");
    // Two teams, none finished: not exactly one finisher, so the Scores tab (`GameOverPacket.defaultTab`).
    let packet = game_over_of(&host, ALICE);
    assert_eq!(packet.default_tab, EndScreenTab::Scores);
    assert!(packet.is_ready_vote_available, "the primary Return to Lobby button");
    assert_eq!(packet.seed, Some(7));
    assert!(!packet.is_update);
    assert!(packet.scores.iter().all(|row| row.duration.is_none()), "DNF rows carry no time");

    // A player who joins in Postgame gets the screen's data again, without it opening.
    host.sent.clear();
    session.on_join(&mut host, &player(CAROL, "Carol"));
    let again = game_over_of(&host, CAROL);
    assert!(again.is_update);
    assert_eq!(again.scores, packet.scores);
}

//! The panel the end screen shows after a game over: its heading is the end packet's title and its buttons are the vote
//! options of the ready update. Expected strings are the original's en_us.json, written out by hand.
//!
//! Expected lines are written out by hand: 20 ticks per second, so a 60 s time limit ends after 1200 ticks, and a
//! 100-tick round lasts 5 s.

use horizon_bingo::buf::decode;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::buf::encode;
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::nbt::Nbt;
use horizon_bingo::ready::SetReadyPacket;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::text::Text;

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


fn translation(text: &Text) -> (String, String) {
    let get = |key: &str| match text.0.get(key) {
        Some(Nbt::String(s)) => s.clone(),
        other => panic!("no string {key}: {other:?}"),
    };
    (get("translate"), get("fallback"))
}

fn last<T>(host: &FakeHost, player: u128, path_wanted: &str, read: impl FnOnce(&mut horizon_bingo::buf::Reader<'_>) -> horizon_bingo::buf::Result<T>) -> T {
    let payload = host
        .sent
        .iter()
        .rev()
        .find(|(to, path, _)| *to == player && path == path_wanted)
        .map(|(_, _, payload)| payload.clone())
        .unwrap_or_else(|| panic!("no {path_wanted} sent"));
    decode(&payload, read).expect("decodes")
}

fn pair(key: &str, fallback: &str) -> (String, String) {
    (key.to_string(), fallback.to_string())
}

#[test]
fn a_timer_end_shows_out_of_time_as_the_heading_and_next_round_as_the_primary_button() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["timelimit 1", "endwhen all_win"]);
    for _ in 0..1300 {
        session.tick(&mut host);
    }

    let over = last(&host, ALICE, "game_over_v2", GameOverPacket::read_v2);
    assert_eq!(translation(&over.title), pair("bingo.game.end.reason_timer", "Out of time!"));
    assert!(over.is_ready_vote_available, "the primary button is on offer");

    let ready = last(&host, ALICE, "ready_timer_update_v2", ReadyUpdatePacket::read_v2);
    let titles: Vec<_> = ready.vote_options.iter().map(|o| translation(&o.title)).collect();
    assert_eq!(
        titles,
        [
            pair("bingo.world.next_round", "Next Round"),
            // A timer end with no team finished: the time limit is grown, so Keep Playing names the extension.
            pair("bingo.world.extend_time_limit_by", "Extend time by %s"),
            pair("bingo.world.return_to_lobby", "Return to Lobby"),
        ]
    );
    // The primary button shows the pending vote's title: the default is the next round.
    assert_eq!(ready.vote_pending, Some(ready.vote_options[0].id));
    assert!(!ready.is_running && ready.vote_options.iter().all(|o| o.votes == 0 && !o.is_selected));
}

#[test]
fn a_stop_by_command_shows_who_stopped_it_and_still_offers_the_vote_options() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["endwhen all_win"]);
    run(&mut session, &mut host, Some(ALICE), "end");
    let over = last(&host, BOB, "game_over_v2", GameOverPacket::read_v2);
    assert_eq!(translation(&over.title), pair("bingo.game.end.reason_command", "Stopped by %s"));
    let ready = last(&host, BOB, "ready_timer_update_v2", ReadyUpdatePacket::read_v2);
    assert_eq!(ready.vote_options.len(), 3, "every game end sends the options, not only a timer's");
}

#[test]
fn choosing_an_option_counts_the_vote_and_marks_it_selected_for_that_player_only() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    started(&mut host, &mut session, &["endwhen all_win"]);
    run(&mut session, &mut host, None, "end");
    let ready = last(&host, ALICE, "ready_timer_update_v2", ReadyUpdatePacket::read_v2);
    let keep = ready.vote_options[1].id;

    let payload = encode(|w| SetReadyPacket { is_ready: true, option_id: Some(keep) }.write_v2(w));
    session.on_packet(&mut host, ALICE, "set_ready_v2", &payload);

    let alice = last(&host, ALICE, "ready_timer_update_v2", ReadyUpdatePacket::read_v2);
    assert_eq!(alice.vote_options.iter().map(|o| (o.votes, o.is_selected)).collect::<Vec<_>>(), [(0, false), (1, true), (0, false)]);
    let bob = last(&host, BOB, "ready_timer_update_v2", ReadyUpdatePacket::read_v2);
    assert_eq!(bob.vote_options.iter().map(|o| (o.votes, o.is_selected)).collect::<Vec<_>>(), [(0, false), (1, false), (0, false)]);
}

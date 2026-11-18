//! The lobby's ready screen, as the original's `ReadyService.getMessage` writes its title and subtitle: the hint to ready
//! up, the count of ready players while the timer runs, and the countdown. They travel on the `ready_timer_update_v2`
//! packet, which the client draws. Driven through a fake host that records every packet.
//!
//! Expected values are counted by hand: 20 ticks per second, a 600 s timer, and the share of one ready player out of two
//! players, which is half the timer (300 s). The lobby sends its packets every tenth tick.
use horizon_bingo::buf::decode;
use horizon_bingo::nbt::Nbt;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts;
use horizon_bingo::server::types::Player;
use horizon_bingo::text::Text;

const ALICE: u128 = 1;
const BOB: u128 = 2;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<(u128, String, Vec<u8>)>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push((player, path.to_string(), payload));
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

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

/// Alice and Bob online, both on a team, with the join packets drained.
fn lobby(host: &mut FakeHost, session: &mut Session) {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(session.command(host, Some(ALICE), "join red").starts_with("You joined"));
    assert!(session.command(host, Some(BOB), "join blue").starts_with("You joined"));
    host.sent.clear();
}

/// The last `ready_timer_update_v2` packet sent to a player.
fn last_packet(host: &FakeHost, player: u128) -> ReadyUpdatePacket {
    let payload = host
        .sent
        .iter()
        .rev()
        .find(|(to, path, _)| *to == player && path == "ready_timer_update_v2")
        .map(|(_, _, payload)| payload.clone())
        .expect("a ready packet was sent to the player");
    decode(&payload, ReadyUpdatePacket::read_v2).expect("ready_timer_update_v2 decodes")
}

/// The translation key of a translatable component.
fn key_of(text: &Text) -> String {
    match text.0.get("translate") {
        Some(Nbt::String(key)) => key.clone(),
        _ => panic!("not a translatable component"),
    }
}

/// The English a client shows without a translation: the fallback with its `%s` places filled from `with`.
fn english(text: &Text) -> String {
    let Some(Nbt::String(fallback)) = text.0.get("fallback") else {
        return text.as_plain().expect("a plain component").to_string();
    };
    let args: Vec<String> = match text.0.get("with") {
        Some(Nbt::List { items, .. }) => items
            .iter()
            .map(|arg| match arg {
                Nbt::String(s) => s.clone(),
                other => panic!("a plain argument, not {other:?}"),
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut args = args.into_iter();
    let mut parts = fallback.split("%s");
    let mut out = parts.next().unwrap_or_default().to_string();
    for part in parts {
        out.push_str(&args.next().expect("an argument for each %s"));
        out.push_str(part);
    }
    out
}

fn title(packet: &ReadyUpdatePacket) -> &Text {
    packet.title.as_ref().expect("the lobby sends a title")
}

fn subtitle(packet: &ReadyUpdatePacket) -> &Text {
    packet.subtitle.as_ref().expect("the lobby sends a subtitle")
}

#[test]
fn a_player_who_is_not_ready_is_told_how_to_ready_up_and_the_timer_waits() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    assert_eq!(session.command(&mut host, Some(ALICE), "ready"), "You are ready.");

    let bob = last_packet(&host, BOB);
    assert!(!bob.is_running && !bob.is_ready);
    assert_eq!(key_of(title(&bob)), "bingo.lobby.vote.ready_up_command");
    assert_eq!(english(title(&bob)), "type /bingo ready to speed up!");
    assert_eq!(key_of(subtitle(&bob)), "bingo.game.waiting_to_start");
    assert_eq!(english(subtitle(&bob)), "Waiting to start...");

    // Alice is ready, but no countdown runs yet: her title stays empty, the subtitle still says she waits.
    let alice = last_packet(&host, ALICE);
    assert!(alice.is_ready && !alice.is_running);
    assert_eq!(english(title(&alice)), "");
    assert_eq!(english(subtitle(&alice)), "Waiting to start...");
}

#[test]
fn once_a_player_is_ready_the_countdown_runs_and_the_ready_player_sees_the_count() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    session.command(&mut host, Some(ALICE), "ready");

    // The first tick starts the countdown; the tenth sends the packets. Nine ticks of 50 ms have run since the start.
    for _ in 0..10 {
        session.tick(&mut host);
    }
    let alice = last_packet(&host, ALICE);
    assert!(alice.is_running);
    assert_eq!(key_of(title(&alice)), "bingo.lobby.vote.players_ready");
    assert_eq!(english(title(&alice)), "1 of 2 ready!");
    assert_eq!(key_of(subtitle(&alice)), "bingo.lobby.vote.starting_time_remaining");
    // Half of 600 s is 300 s, less 450 ms: 299.55 s, shown as 4:59 (`%d:%02d`: the minutes are not padded).
    assert_eq!(english(subtitle(&alice)), "Starting in 4:59");

    let bob = last_packet(&host, BOB);
    assert_eq!(english(title(&bob)), "type /bingo ready to speed up!", "the hint stays for a player not ready");
    assert_eq!(english(subtitle(&bob)), "Starting in 4:59");
}

#[test]
fn the_lobby_clock_and_the_game_duration_read_as_the_original_writes_them() {
    // formatHHMMSS: hours only from one hour on.
    assert_eq!(texts::clock_text(299_550), "4:59");
    assert_eq!(texts::clock_text(3_661_000), "1:01:01");
    // formatString: days and hours only when there are any, minutes and seconds always.
    assert_eq!(texts::duration_text(5_000), "00m 05s");
    assert_eq!(texts::duration_text(3_723_000), "1h 02m 03s");
    assert_eq!(texts::duration_text(90_061_000), "1d 1h 01m 01s");
}

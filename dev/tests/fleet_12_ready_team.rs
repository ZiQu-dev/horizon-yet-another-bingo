//! Lobby ready-up rules and the spectators join, as the original's `ReadyService.setReady` and `JoinCommand` have them.
//! Expected values are counted by hand: the countdown is 600 s, shortened by the share of ready players (one of two
//! online ready gives 300 s).
use horizon_bingo::buf::decode;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

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

    fn command(&mut self, _line: &str) {}

    fn boss_bar(&mut self, _player: u128, _id: &str, _title: &str, _progress: f32, _visible: bool) {}
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

fn join(session: &mut Session, host: &mut FakeHost, uuid: u128, name: &str, team: &str) {
    let player = Player { uuid, name: name.to_string() };
    host.online.push(player.clone());
    session.on_join(host, &player);
    assert!(session.command(host, Some(uuid), &format!("join {team}")).starts_with("Moved player"));
}

#[test]
fn a_spectator_is_told_to_join_spectators_to_ready_up_and_the_join_makes_them_one() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    join(&mut session, &mut host, ALICE, "Alice", "red");
    join(&mut session, &mut host, BOB, "Bob", "red");

    assert_eq!(session.command(&mut host, Some(ALICE), "join spectators"), "You are now a spectator.");
    assert_eq!(
        session.command(&mut host, Some(ALICE), "ready"),
        "You cannot ready up without joining a team! (use '/join spectators' if you want to spectate)"
    );
}

#[test]
fn a_ready_press_that_cannot_start_the_countdown_is_refused() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    // Alice is alone on a team: the countdown needs two players online.
    join(&mut session, &mut host, ALICE, "Alice", "red");

    assert_eq!(
        session.command(&mut host, Some(ALICE), "ready"),
        "Cannot ready up - other players need to join a team!"
    );
}

#[test]
fn a_ready_press_with_an_unteamed_player_online_is_refused() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    join(&mut session, &mut host, ALICE, "Alice", "red");
    // Bob is online and on no team, so the start rules do not hold.
    host.online.push(Player { uuid: BOB, name: "Bob".to_string() });
    session.on_join(&mut host, &Player { uuid: BOB, name: "Bob".to_string() });

    assert_eq!(
        session.command(&mut host, Some(ALICE), "ready"),
        "Cannot ready up - other players need to join a team!"
    );
}

#[test]
fn a_ready_press_starts_the_countdown_when_the_start_rules_hold() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    join(&mut session, &mut host, ALICE, "Alice", "red");
    join(&mut session, &mut host, BOB, "Bob", "red");
    host.sent.clear();

    assert_eq!(session.command(&mut host, Some(ALICE), "ready"), "Alice is ready!");
    // One of two online is ready: (1 - 1/2) * 600 s = 300 000 ms, and no tick has passed yet.
    let (_, _, payload) = host
        .sent
        .iter()
        .find(|(player, path, _)| *player == BOB && path == "ready_timer_update_v2")
        .expect("Bob gets the ready packet");
    let packet: ReadyUpdatePacket = decode(payload, ReadyUpdatePacket::read_v2).expect("decodes");
    assert!(packet.is_running);
    assert_eq!(packet.remaining_millis, 300_000);
}

//! Game mode before play (spawn-15): a player who joins while the game is in PREGAME is put in adventure mode, so the
//! lobby cannot be built in. Expected values come from `PlayerController.updateGameMode` (PREGAME gives ADVENTURE).
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    commands: Vec<String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

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

fn options() -> SessionOptions {
    SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

#[test]
fn a_player_joining_before_play_is_in_adventure_mode() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    let alice = Player { uuid: ALICE, name: "Alice".to_string() };
    host.online = vec![alice.clone()];

    session.on_join(&mut host, &alice);

    assert!(host.commands.contains(&"gamemode adventure Alice".to_string()), "commands: {:?}", host.commands);
    assert!(!host.commands.iter().any(|c| c.starts_with("gamemode survival")), "commands: {:?}", host.commands);
}

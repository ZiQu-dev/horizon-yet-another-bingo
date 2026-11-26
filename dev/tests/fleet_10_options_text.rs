//! The options summary (`/bingo options`) names the end condition and the time limit only when they differ from the
//! defaults, as the original's summary does. Expected values are the original's lines: "End When" is hidden for
//! First Win, and "Time Limit" is `%dh %02dm` (90 minutes is "1h 30m"), shown only when a limit is set.
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

struct QuietHost;

impl ServerHost for QuietHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}
    fn tell(&mut self, _player: u128, _line: &str) {}
    fn broadcast(&mut self, _line: &str) {}
    fn players(&self) -> Vec<Player> {
        Vec::new()
    }
    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }
    fn command(&mut self, _line: &str) {}
}

fn session(time_limit_secs: Option<i32>) -> Session {
    Session::new(SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs: 0, time_limit_secs },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    })
}

#[test]
fn first_win_end_is_not_named_in_the_options_summary() {
    let mut host = QuietHost;
    let mut session = session(None);
    let text = session.command(&mut host, None, "options");
    assert!(!text.contains("End:"), "{text}");
    assert!(text.starts_with("Goal: 1 line(s). Mode: standard. PvP: on."), "{text}");
}

#[test]
fn a_set_time_limit_is_named_as_hours_and_minutes() {
    let mut host = QuietHost;
    let mut session = session(Some(90 * 60));
    let text = session.command(&mut host, None, "options");
    assert!(text.contains(" Time limit: 1h 30m."), "{text}");
}

#[test]
fn no_time_limit_is_not_named() {
    let mut host = QuietHost;
    let mut session = session(None);
    assert!(!session.command(&mut host, None, "options").contains("Time limit"));
}

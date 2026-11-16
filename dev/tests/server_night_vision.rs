//! The night vision effect of the server session (`NightVisionController.kt`): who gets an infinite `night_vision` effect
//! and when it is taken away. Expected values are read from the original's rules by hand: a team player gets it while the
//! round starts or plays with `nightVision` on, a spectator or a player on no team while it starts or plays (with the
//! player's setting on), and everyone after the game; an effect is only given once, and only taken away from a player
//! who has it and should not.
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::server::game::GameOptions;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<(u128, String, Vec<u8>)>,
    commands: Vec<String>,
}

impl FakeHost {
    fn take_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.commands)
    }

    /// The commands that change night vision, the others (gamemode, gamerule, give) aside.
    fn take_effects(&mut self) -> Vec<String> {
        self.take_commands().into_iter().filter(|c| c.starts_with("effect ")).collect()
    }
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

    fn command(&mut self, line: &str) {
        self.commands.push(line.to_string());
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(countdown_secs: i32) -> SessionOptions {
    SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Sets who is online, has each join, and puts each listed player on a team. Drains what was sent.
fn lobby(host: &mut FakeHost, session: &mut Session, online: &[(u128, &str)], teams: &[(u128, &str)]) {
    host.online = online.iter().map(|&(uuid, name)| player(uuid, name)).collect();
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    for &(uuid, team) in teams {
        let answer = run(session, host, Some(uuid), &format!("join {team}"));
        assert!(answer.starts_with("You joined"), "joining {team}: {answer}");
    }
    host.sent.clear();
    host.take_commands();
}

fn ticks(session: &mut Session, host: &mut FakeHost, count: u32) {
    for _ in 0..count {
        session.tick(host);
    }
}

#[test]
fn team_players_get_one_infinite_effect_when_the_option_is_on_and_no_more_after_that() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(0));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "nightvision on").contains("Night vision: on"));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Game started.");
    host.take_commands();

    ticks(&mut session, &mut host, 1);
    assert_eq!(
        host.take_effects(),
        ["effect give Alice night_vision infinite 0 true", "effect give Bob night_vision infinite 0 true"]
    );

    ticks(&mut session, &mut host, 40);
    assert!(host.take_effects().is_empty(), "the effect is not given again every tick");
}

#[test]
fn with_the_option_off_team_players_get_nothing_and_a_spectator_gets_the_effect_while_playing() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(0));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (CAROL, "Carol")], &[(ALICE, "red")]);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start ignore_warnings"), "Game started.");
    host.take_commands();

    ticks(&mut session, &mut host, 1);
    // Carol is on no team: a spectator, who keeps night vision while the game plays (`nightVisionInSpectator`).
    assert_eq!(host.take_effects(), ["effect give Carol night_vision infinite 0 true"]);
}

#[test]
fn a_player_who_turns_the_setting_off_loses_the_effect_on_the_next_tick_once() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(0));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "nightvision on");
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take_commands();
    ticks(&mut session, &mut host, 1);
    assert_eq!(host.take_effects(), ["effect give Alice night_vision infinite 0 true"]);

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs night_vision off"), "Set 'Night Vision' = Off");
    ticks(&mut session, &mut host, 1);
    assert_eq!(host.take_effects(), ["effect clear Alice night_vision"]);
    ticks(&mut session, &mut host, 5);
    assert!(host.take_effects().is_empty());
}

#[test]
fn the_effect_stays_after_the_game_and_is_taken_away_when_the_game_goes_back_to_the_lobby() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(0));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "nightvision on");
    run(&mut session, &mut host, Some(ALICE), "start");
    ticks(&mut session, &mut host, 1);
    host.take_commands();

    // After the game the original gives night vision to every player (`nightVisionInPostgame`): Alice already has it.
    run(&mut session, &mut host, None, "end");
    ticks(&mut session, &mut host, 1);
    assert!(host.take_effects().is_empty());

    // Back in the lobby nobody should have it, so Alice loses it.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote lobby"), "The vote is over: back to the lobby.");
    ticks(&mut session, &mut host, 1);
    assert_eq!(host.take_effects(), ["effect clear Alice night_vision"]);
}

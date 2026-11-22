//! The vanilla world reset flow (`horizon_bingo::server::world_reset`), run against a fake port that logs the calls it
//! gets. The order of the engine calls is the rule under test: players go to the lobby, the reset is asked only when
//! nobody is in a vanilla level, and nobody goes to the new overworld before `levels_reset_done`.
use std::collections::VecDeque;

use horizon_bingo::server::world_reset::{
    is_vanilla_dimension, Failure, Outcome, ResetRefusal, StartRefused, State, WorldPort, WorldReset, MAX_RESET_TICKS,
    MAX_RETRY_TICKS,
};

const SEED: u64 = 42;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    ResetVanilla(u64),
    TeleportToLobby,
    TeleportToOverworld,
}

/// A port whose answers the test sets. `refusals` are the next `reset_vanilla` answers; once they run out it succeeds.
struct Fake {
    lobby_open: bool,
    in_vanilla: bool,
    refusals: VecDeque<ResetRefusal>,
    log: Vec<Call>,
}

impl Fake {
    fn new() -> Self {
        Self { lobby_open: true, in_vanilla: false, refusals: VecDeque::new(), log: Vec::new() }
    }

    fn refusing(refusals: &[ResetRefusal]) -> Self {
        Self { refusals: refusals.iter().copied().collect(), ..Self::new() }
    }

    fn resets(&self) -> usize {
        self.log.iter().filter(|c| matches!(c, Call::ResetVanilla(_))).count()
    }

    fn overworld_teleports(&self) -> usize {
        self.log.iter().filter(|c| **c == Call::TeleportToOverworld).count()
    }
}

impl WorldPort for Fake {
    fn lobby_is_open(&mut self) -> bool {
        self.lobby_open
    }

    fn players_in_vanilla(&self) -> bool {
        self.in_vanilla
    }

    fn reset_vanilla(&mut self, seed: u64) -> Result<(), ResetRefusal> {
        self.log.push(Call::ResetVanilla(seed));
        match self.refusals.pop_front() {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    fn teleport_all_to_lobby(&mut self) {
        self.log.push(Call::TeleportToLobby);
    }

    fn teleport_all_to_overworld_spawn(&mut self) {
        self.log.push(Call::TeleportToOverworld);
    }
}

fn fixed_seed() -> impl FnMut() -> u64 {
    || SEED
}

#[test]
fn happy_path_sends_players_out_then_resets_then_sends_them_in() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert_eq!(port.log, vec![Call::TeleportToLobby]);

    // The first tick finds nobody in a vanilla level and asks for the reset with the injected seed.
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(flow.state(), State::Resetting);
    assert_eq!(port.log, vec![Call::TeleportToLobby, Call::ResetVanilla(SEED)]);

    // While the engine works, ticks ask nothing and move nobody.
    for _ in 0..50 {
        assert_eq!(flow.tick(&mut port), None);
    }
    assert_eq!(port.log.len(), 2);

    assert_eq!(flow.levels_reset_done(&mut port), Some(Outcome::Started));
    assert_eq!(flow.state(), State::Running);
    assert_eq!(
        port.log,
        vec![Call::TeleportToLobby, Call::ResetVanilla(SEED), Call::TeleportToOverworld]
    );
}

#[test]
fn start_is_refused_with_a_typed_error_when_the_lobby_is_closed() {
    let mut port = Fake::new();
    port.lobby_open = false;
    let mut flow = WorldReset::new();
    assert_eq!(flow.start(&mut port, &mut fixed_seed()), Err(StartRefused::LobbyClosed));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert!(port.log.is_empty());
    assert_eq!(flow.tick(&mut port), None);
    assert!(port.log.is_empty());
}

#[test]
fn a_second_start_during_a_game_is_refused() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert_eq!(flow.start(&mut port, &mut fixed_seed()), Err(StartRefused::AlreadyStarted));
    assert_eq!(port.log, vec![Call::TeleportToLobby]);
}

#[test]
fn reset_waits_while_a_player_is_in_a_vanilla_level_and_busy_or_inside_refusals_are_retried() {
    let mut port = Fake::refusing(&[ResetRefusal::Busy, ResetRefusal::PlayersInside]);
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();

    // A player still in vanilla: no reset is asked at all.
    port.in_vanilla = true;
    for _ in 0..5 {
        assert_eq!(flow.tick(&mut port), None);
    }
    assert_eq!(port.resets(), 0);

    // The player is out. The engine is busy, then a player is still inside, then it succeeds.
    port.in_vanilla = false;
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(flow.tick(&mut port), None);
    assert!(matches!(flow.state(), State::WaitingForPlayersOut { .. }));
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(flow.state(), State::Resetting);
    assert_eq!(port.resets(), 3);
    assert_eq!(port.overworld_teleports(), 0);
    assert_eq!(flow.levels_reset_done(&mut port), Some(Outcome::Started));
    assert_eq!(port.overworld_teleports(), 1);
}

#[test]
fn retry_limit_then_failure_sends_players_back_to_the_lobby() {
    // The engine is busy on every attempt. The first attempt is not a retry; then MAX_RETRY_TICKS retries are allowed.
    let mut port = Fake::refusing(&[ResetRefusal::Busy; MAX_RETRY_TICKS as usize + 2]);
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    for _ in 0..MAX_RETRY_TICKS {
        assert_eq!(flow.tick(&mut port), None);
        assert!(matches!(flow.state(), State::WaitingForPlayersOut { .. }));
    }
    assert_eq!(flow.tick(&mut port), Some(Outcome::Failed(Failure::RetriesExhausted)));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.resets(), MAX_RETRY_TICKS as usize + 1);
    assert_eq!(port.log.last(), Some(&Call::TeleportToLobby));
    assert_eq!(port.overworld_teleports(), 0);
    // The failed start leaves nothing running: the next tick does nothing.
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(port.resets(), MAX_RETRY_TICKS as usize + 1);
}

#[test]
fn a_refusal_that_waiting_cannot_cure_fails_at_once() {
    let mut port = Fake::refusing(&[ResetRefusal::NoHoldingLevel]);
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert_eq!(flow.tick(&mut port), Some(Outcome::Failed(Failure::Refused(ResetRefusal::NoHoldingLevel))));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.resets(), 1);
    assert_eq!(port.log.last(), Some(&Call::TeleportToLobby));
}

#[test]
fn nobody_goes_to_the_overworld_before_levels_reset_done() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();

    // Before a reset is asked, the done event does nothing and moves nobody.
    assert_eq!(flow.levels_reset_done(&mut port), None);
    assert!(port.log.is_empty());

    flow.start(&mut port, &mut fixed_seed()).unwrap();
    port.in_vanilla = true;
    for _ in 0..20 {
        flow.tick(&mut port);
    }
    port.in_vanilla = false;
    flow.tick(&mut port);
    assert_eq!(flow.state(), State::Resetting);
    for _ in 0..100 {
        flow.tick(&mut port);
    }
    assert_eq!(port.overworld_teleports(), 0);

    // A second done event after the start is ignored.
    assert_eq!(flow.levels_reset_done(&mut port), Some(Outcome::Started));
    assert_eq!(flow.levels_reset_done(&mut port), None);
    assert_eq!(port.overworld_teleports(), 1);
}

#[test]
fn ending_the_game_sends_players_back_to_the_lobby() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();
    assert!(!flow.return_to_lobby(&mut port));
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    flow.tick(&mut port);
    flow.levels_reset_done(&mut port);
    port.log.clear();

    assert!(flow.return_to_lobby(&mut port));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.log, vec![Call::TeleportToLobby]);
    // The lobby is open for the next start.
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert!(matches!(flow.state(), State::WaitingForPlayersOut { .. }));
}

#[test]
fn a_reset_the_engine_never_reports_fails_after_max_reset_ticks_and_players_go_to_the_lobby() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(flow.state(), State::Resetting);

    // The reset is still with the engine at the limit; the next tick gives the start up.
    for _ in 0..MAX_RESET_TICKS {
        assert_eq!(flow.tick(&mut port), None);
    }
    assert_eq!(flow.state(), State::Resetting);
    assert_eq!(flow.tick(&mut port), Some(Outcome::Failed(Failure::ResetTimedOut)));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.resets(), 1);
    assert_eq!(port.log.last(), Some(&Call::TeleportToLobby));
    assert_eq!(port.overworld_teleports(), 0);
    assert_eq!(flow.tick(&mut port), None);
}

#[test]
fn a_levels_reset_that_comes_after_the_timeout_is_ignored() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    flow.tick(&mut port);
    for _ in 0..=MAX_RESET_TICKS {
        flow.tick(&mut port);
    }
    assert_eq!(flow.state(), State::LobbyOnly);
    let log_len = port.log.len();

    assert_eq!(flow.levels_reset_done(&mut port), None);
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.overworld_teleports(), 0);
    assert_eq!(port.log.len(), log_len);
}

#[test]
fn only_the_three_vanilla_levels_count_as_vanilla() {
    assert!(is_vanilla_dimension("minecraft:overworld"));
    assert!(is_vanilla_dimension("minecraft:the_nether"));
    assert!(is_vanilla_dimension("minecraft:the_end"));
    assert!(!is_vanilla_dimension("bingo:lobby"));
    assert!(!is_vanilla_dimension("minecraft:overworld_caves"));
}

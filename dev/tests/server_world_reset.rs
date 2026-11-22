//! The vanilla world reset flow (`horizon_bingo::server::world_reset`), run against a fake port that logs the calls it
//! gets. The order of the engine calls is the rule under test: players go to the lobby, the reset is asked only when
//! nobody is in a vanilla level, nobody goes to the new overworld before `levels_reset_done`, and players go to its
//! spawn only once it is known. A refused move is tried again, and a start never reports success with a player unmoved.
use std::collections::{BTreeSet, VecDeque};

use horizon_bingo::server::world_reset::{
    gate_targets, is_vanilla_dimension, Failure, Outcome, ResetRefusal, StartRefused, State, WorldPort, WorldReset,
    MAX_RESET_TICKS, MAX_RETRY_TICKS, MAX_SPAWN_TICKS,
};

const SEED: u64 = 42;
const ALICE: u128 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    ResetVanilla(u64),
    TeleportToLobby,
    TeleportOneToLobby(u128),
    TeleportToStaging,
    TeleportToOverworld,
}

/// A port whose answers the test sets. `refusals` are the next `reset_vanilla` answers; once they run out it succeeds.
/// `lobby_moves_refused` and `spawn_moves_refused` count the moves that fail before they succeed.
struct Fake {
    lobby_open: bool,
    in_vanilla: bool,
    spawn_known: bool,
    lobby_moves_refused: u32,
    staging_moves_refused: u32,
    spawn_moves_refused: u32,
    refusals: VecDeque<ResetRefusal>,
    log: Vec<Call>,
}

impl Fake {
    fn new() -> Self {
        Self {
            lobby_open: true,
            in_vanilla: false,
            spawn_known: true,
            lobby_moves_refused: 0,
            staging_moves_refused: 0,
            spawn_moves_refused: 0,
            refusals: VecDeque::new(),
            log: Vec::new(),
        }
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

    fn staging_teleports(&self) -> usize {
        self.log.iter().filter(|c| **c == Call::TeleportToStaging).count()
    }

    fn lobby_teleports(&self) -> usize {
        self.log.iter().filter(|c| **c == Call::TeleportToLobby).count()
    }
}

impl WorldPort for Fake {
    fn lobby_is_open(&mut self) -> bool {
        self.lobby_open
    }

    fn vanilla_players(&self) -> Vec<u128> {
        if self.in_vanilla { vec![ALICE] } else { Vec::new() }
    }

    fn reset_vanilla(&mut self, seed: u64) -> Result<(), ResetRefusal> {
        self.log.push(Call::ResetVanilla(seed));
        match self.refusals.pop_front() {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    fn teleport_all_to_lobby(&mut self) -> bool {
        self.log.push(Call::TeleportToLobby);
        take(&mut self.lobby_moves_refused)
    }

    fn teleport_to_lobby(&mut self, player: u128) -> bool {
        self.log.push(Call::TeleportOneToLobby(player));
        true
    }

    fn teleport_all_to_staging(&mut self) -> bool {
        self.log.push(Call::TeleportToStaging);
        take(&mut self.staging_moves_refused)
    }

    fn overworld_spawn_known(&self) -> bool {
        self.spawn_known
    }

    fn teleport_all_to_overworld_spawn(&mut self) -> bool {
        self.log.push(Call::TeleportToOverworld);
        take(&mut self.spawn_moves_refused)
    }
}

/// Whether a refusal counter had a refusal to give, and uses one up: the move fails once per count.
fn take(refusals: &mut u32) -> bool {
    if *refusals > 0 {
        *refusals -= 1;
        false
    } else {
        true
    }
}

fn fixed_seed() -> impl FnMut() -> u64 {
    || SEED
}

/// A reset, asked and carried out, with the players in the lobby: the flow is with `levels_reset`.
fn resetting(port: &mut Fake) -> WorldReset {
    let mut flow = WorldReset::new();
    flow.start(port, &mut fixed_seed()).unwrap();
    assert_eq!(flow.tick(port), None);
    assert_eq!(flow.state(), State::Resetting);
    flow
}

#[test]
fn happy_path_sends_players_to_the_lobby_resets_then_stages_and_then_to_the_spawn() {
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

    // The reset is done: the players go to the staging place, not yet to the spawn. The spawn is known at once here, so
    // the next tick takes them there and the start is done.
    flow.levels_reset_done(&mut port);
    assert_eq!(port.log.last(), Some(&Call::TeleportToStaging));
    assert!(matches!(flow.state(), State::AwaitingSpawn { staged: true, .. }));
    assert_eq!(flow.tick(&mut port), Some(Outcome::Started));
    assert_eq!(flow.state(), State::Running);
    assert_eq!(
        port.log,
        vec![
            Call::TeleportToLobby,
            Call::ResetVanilla(SEED),
            Call::TeleportToStaging,
            Call::TeleportToOverworld,
        ]
    );
}

#[test]
fn the_players_wait_in_the_staging_place_while_the_spawn_is_unknown_then_go_to_it() {
    let mut port = Fake::new();
    port.spawn_known = false;
    let mut flow = resetting(&mut port);
    flow.levels_reset_done(&mut port);

    // Staging first: nobody goes to the spawn while it is unknown.
    assert_eq!(port.staging_teleports(), 1);
    assert_eq!(port.overworld_teleports(), 0);
    for _ in 0..30 {
        assert_eq!(flow.tick(&mut port), None);
        assert!(matches!(flow.state(), State::AwaitingSpawn { staged: true, .. }));
    }
    assert_eq!(port.overworld_teleports(), 0);
    assert_eq!(port.staging_teleports(), 1, "the staging is not asked again while everyone is there");

    // The world chooses its spawn: the next tick sends everyone there and the start is done.
    port.spawn_known = true;
    assert_eq!(flow.tick(&mut port), Some(Outcome::Started));
    assert_eq!(port.overworld_teleports(), 1);
    assert_eq!(flow.state(), State::Running);
}

#[test]
fn a_spawn_that_never_comes_fails_after_max_spawn_ticks_and_players_go_back_to_the_lobby() {
    let mut port = Fake::new();
    port.spawn_known = false;
    let mut flow = resetting(&mut port);
    flow.levels_reset_done(&mut port);

    for _ in 0..MAX_SPAWN_TICKS {
        assert_eq!(flow.tick(&mut port), None);
    }
    assert_eq!(flow.tick(&mut port), Some(Outcome::Failed(Failure::SpawnTimedOut)));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.overworld_teleports(), 0, "no player is sent to a spawn that was never known");
    assert_eq!(port.log.last(), Some(&Call::TeleportToLobby));
    assert_eq!(flow.tick(&mut port), None, "the failed start leaves nothing running");
}

#[test]
fn a_refused_move_to_the_staging_place_or_the_spawn_is_tried_again_and_success_waits_for_everyone() {
    let mut port = Fake::new();
    port.staging_moves_refused = 2;
    port.spawn_moves_refused = 1;
    let mut flow = resetting(&mut port);
    flow.levels_reset_done(&mut port);
    assert!(matches!(flow.state(), State::AwaitingSpawn { staged: false, .. }));
    assert_eq!(port.overworld_teleports(), 0);

    assert_eq!(flow.tick(&mut port), None, "the staging is refused again: still waiting");
    assert_eq!(flow.tick(&mut port), None, "staged now, but the spawn move is refused");
    assert_eq!(flow.tick(&mut port), Some(Outcome::Started));
    assert_eq!(port.staging_teleports(), 3);
    assert_eq!(port.overworld_teleports(), 2);
}

#[test]
fn a_refused_move_to_the_lobby_is_retried_and_the_reset_waits_for_it() {
    let mut port = Fake::new();
    port.lobby_moves_refused = 3;
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert!(matches!(flow.state(), State::WaitingForPlayersOut { gathered: false, .. }));

    // The start and the first two ticks are refused: no reset is asked while a player is not in the lobby.
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(port.resets(), 0);
    // The third tick's move works, and then the reset is asked.
    assert_eq!(flow.tick(&mut port), None);
    assert_eq!(port.resets(), 1);
    assert_eq!(flow.state(), State::Resetting);
    assert_eq!(port.lobby_teleports(), 4, "the start and three ticks, the last of them the move that worked");
}

#[test]
fn a_move_to_the_lobby_that_never_works_fails_the_start_after_the_retry_limit() {
    let mut port = Fake::new();
    port.lobby_moves_refused = u32::MAX;
    let mut flow = WorldReset::new();
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    for _ in 0..MAX_RETRY_TICKS {
        assert_eq!(flow.tick(&mut port), None);
    }
    assert_eq!(flow.tick(&mut port), Some(Outcome::Failed(Failure::RetriesExhausted)));
    assert_eq!(port.resets(), 0, "nothing is reset while a player was not moved");
    assert_eq!(flow.state(), State::LobbyOnly);
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
    flow.levels_reset_done(&mut port);
    assert_eq!(flow.tick(&mut port), Some(Outcome::Started));
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
fn nobody_goes_to_the_staging_place_before_levels_reset_done() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();

    // Before a reset is asked, the done event does nothing and moves nobody.
    flow.levels_reset_done(&mut port);
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
    assert_eq!(port.staging_teleports(), 0);
    assert_eq!(port.overworld_teleports(), 0);

    // A second done event after the staging is ignored.
    flow.levels_reset_done(&mut port);
    assert_eq!(flow.tick(&mut port), Some(Outcome::Started));
    flow.levels_reset_done(&mut port);
    assert_eq!(port.staging_teleports(), 1);
    assert_eq!(port.overworld_teleports(), 1);
}

#[test]
fn ending_the_game_sends_players_back_to_the_lobby_and_is_idempotent() {
    let mut port = Fake::new();
    let mut flow = WorldReset::new();
    assert!(!flow.return_to_lobby(&mut port));
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    flow.tick(&mut port);
    flow.levels_reset_done(&mut port);
    flow.tick(&mut port);
    assert_eq!(flow.state(), State::Running);
    port.log.clear();

    assert!(flow.return_to_lobby(&mut port));
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.log, vec![Call::TeleportToLobby]);
    // The second call, from another path of the same end, moves nobody again.
    assert!(!flow.return_to_lobby(&mut port));
    assert_eq!(port.log, vec![Call::TeleportToLobby]);
    // The lobby is open for the next start.
    flow.start(&mut port, &mut fixed_seed()).unwrap();
    assert!(matches!(flow.state(), State::WaitingForPlayersOut { .. }));
}

#[test]
fn a_reset_given_up_while_the_engine_works_sends_players_to_the_lobby_and_its_done_event_is_ignored() {
    let mut port = Fake::new();
    let mut flow = resetting(&mut port);
    assert!(flow.return_to_lobby(&mut port));
    assert_eq!(flow.state(), State::LobbyOnly);
    port.log.clear();

    flow.levels_reset_done(&mut port);
    assert_eq!(flow.state(), State::LobbyOnly);
    assert!(port.log.is_empty(), "a reset that ended with the game moves nobody");
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

    flow.levels_reset_done(&mut port);
    assert_eq!(flow.state(), State::LobbyOnly);
    assert_eq!(port.overworld_teleports(), 0);
    assert_eq!(port.log.len(), log_len);
}

#[test]
fn the_lobby_gate_moves_remembered_players_and_players_in_vanilla_but_not_during_a_start() {
    let remembered: BTreeSet<u128> = [7, 3].into_iter().collect();
    // Outside a start, the remembered players and the ones in vanilla, once each, in uuid order.
    assert_eq!(gate_targets(&remembered, &[3, 9], false), vec![3, 7, 9]);
    assert_eq!(gate_targets(&BTreeSet::new(), &[9], false), vec![9]);
    assert!(gate_targets(&BTreeSet::new(), &[], false).is_empty());
    // During a start the players in vanilla are on their way out or in the staging place: only the remembered move.
    assert_eq!(gate_targets(&remembered, &[9], true), vec![3, 7]);
}

#[test]
fn only_the_three_vanilla_levels_count_as_vanilla() {
    assert!(is_vanilla_dimension("minecraft:overworld"));
    assert!(is_vanilla_dimension("minecraft:the_nether"));
    assert!(is_vanilla_dimension("minecraft:the_end"));
    assert!(!is_vanilla_dimension("bingo:lobby"));
    assert!(!is_vanilla_dimension("minecraft:overworld_caves"));
}

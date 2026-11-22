//! Resetting the vanilla worlds when a game starts from the lobby, and sending the players back when it ends. The
//! original's rule: a start moves everyone from the lobby, resets the three vanilla worlds with a new seed, and puts
//! the players into the fresh overworld once the reset is done. The engine does the reset; this module only decides
//! the order. Everything the engine does goes through [`WorldPort`], and the engine's `levels_reset` hook arrives as
//! [`WorldReset::levels_reset_done`]. Each server tick calls [`WorldReset::tick`] once.
//!
//! The new overworld has no spawn until the engine has opened it, so after the reset the players wait in a staging
//! place in it (`AwaitingSpawn`) and go to the spawn once the world has chosen one. A move the engine refuses is tried
//! again on the next ticks; the start gives up after a bounded time, and it never reports `Started` while a player was
//! not moved.
use std::collections::BTreeSet;

/// Ticks a reset waits, after the first attempt, for players to leave the vanilla levels or for a busy engine, and for
/// a refused move to the lobby to succeed. At 20 server ticks per second this is 5 seconds.
pub const MAX_RETRY_TICKS: u32 = 100;

/// Ticks a reset may take with the engine, from the tick that asked for it, before the start is given up. The engine
/// fires no event when a reset fails, so this is the only way the flow learns of it. At 20 server ticks per second this
/// is 60 seconds.
pub const MAX_RESET_TICKS: u32 = 1200;

/// Ticks the players may wait in the staging place for the new overworld's spawn, counted from `levels_reset_done`. At
/// 20 server ticks per second this is 30 seconds.
pub const MAX_SPAWN_TICKS: u32 = 600;

/// Whether a dimension is one of the three vanilla levels a reset regenerates (the Overworld, the Nether, the End).
pub fn is_vanilla_dimension(id: &str) -> bool {
    matches!(id, "minecraft:overworld" | "minecraft:the_nether" | "minecraft:the_end")
}

/// The players the lobby gate sends to the lobby on this tick, in uuid order. `remembered` are the players who joined
/// before the lobby was open (or while a game was not running) and have not been moved yet. `in_vanilla` are the
/// players in a vanilla level. Outside a start, the gate moves both; during a start the players in vanilla are the
/// start's business (they are on their way out, or are in the staging place), so only the remembered ones are moved.
pub fn gate_targets(remembered: &BTreeSet<u128>, in_vanilla: &[u128], starting: bool) -> Vec<u128> {
    let mut targets: BTreeSet<u128> = remembered.clone();
    if !starting {
        targets.extend(in_vanilla.iter().copied());
    }
    targets.into_iter().collect()
}

/// Why the engine refused `reset_vanilla`, as the engine names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetRefusal {
    Unavailable,
    NotPermitted,
    PlayersInside,
    NoHoldingLevel,
    Busy,
}

/// What the flow needs from the engine and the server. A `bool` answer is `false` when a player was not moved; the
/// flow then tries again, and it counts as moved only once every player is where it was sent.
pub trait WorldPort {
    /// Whether the lobby level is open to start from.
    fn lobby_is_open(&mut self) -> bool;
    /// The players in a vanilla level right now.
    fn vanilla_players(&self) -> Vec<u128>;
    /// Whether any player is in a vanilla level right now.
    fn players_in_vanilla(&self) -> bool {
        !self.vanilla_players().is_empty()
    }
    /// Asks the engine to reset the three vanilla worlds with `seed`. Refused while a player is inside one.
    fn reset_vanilla(&mut self, seed: u64) -> Result<(), ResetRefusal>;
    /// Sends every player to the lobby level.
    fn teleport_all_to_lobby(&mut self) -> bool;
    /// Sends one player to the lobby level.
    fn teleport_to_lobby(&mut self, player: u128) -> bool;
    /// Sends every player to the staging place in the new overworld, where they wait for its spawn.
    fn teleport_all_to_staging(&mut self) -> bool;
    /// Whether the new overworld has chosen its spawn.
    fn overworld_spawn_known(&self) -> bool;
    /// Sends every player to the spawn of the new overworld. False, with the spawn not known, when nobody is moved.
    fn teleport_all_to_overworld_spawn(&mut self) -> bool;
}

/// Where the reset seed comes from. Tests pass a fixed value; the server passes a random one. A closure works too.
pub trait SeedSource {
    fn next_seed(&mut self) -> u64;
}

impl<F: FnMut() -> u64> SeedSource for F {
    fn next_seed(&mut self) -> u64 {
        self()
    }
}

/// Why a start was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRefused {
    LobbyClosed,
    AlreadyStarted,
}

/// Why a start did not reach play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The engine refused for a reason that waiting does not cure.
    Refused(ResetRefusal),
    /// Players stayed in a vanilla level, the engine stayed busy, or a player was not moved to the lobby, for more than
    /// [`MAX_RETRY_TICKS`] ticks.
    RetriesExhausted,
    /// The engine accepted the reset but `levels_reset` did not come within [`MAX_RESET_TICKS`] ticks.
    ResetTimedOut,
    /// The new overworld had no spawn, or a player could not be moved to it, within [`MAX_SPAWN_TICKS`] ticks.
    SpawnTimedOut,
}

/// How a start ended. Players are in the new overworld on `Started`, and back in the lobby on `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Started,
    Failed(Failure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    LobbyOnly,
    /// Players are sent to the lobby (`gathered` once every one is there); the reset waits until none is in a vanilla
    /// level.
    WaitingForPlayersOut { seed: u64, retries: u32, gathered: bool },
    /// The reset is with the engine; players wait for `levels_reset_done`.
    Resetting,
    /// The reset is done and the players wait in the staging place for the new overworld's spawn. `staged` is set once
    /// every player is there.
    AwaitingSpawn { waits: u32, staged: bool },
    Running,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldReset {
    state: State,
    /// Ticks since the reset was asked for, while it is with the engine.
    reset_ticks: u32,
}

impl WorldReset {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Starts a game from the lobby: draws the seed and sends everyone to the lobby. The reset itself is requested by
    /// later ticks, once no player is in a vanilla level.
    pub fn start(&mut self, port: &mut dyn WorldPort, seeds: &mut impl SeedSource) -> Result<(), StartRefused> {
        if self.state != State::LobbyOnly {
            return Err(StartRefused::AlreadyStarted);
        }
        if !port.lobby_is_open() {
            return Err(StartRefused::LobbyClosed);
        }
        let gathered = port.teleport_all_to_lobby();
        self.state = State::WaitingForPlayersOut { seed: seeds.next_seed(), retries: 0, gathered };
        Ok(())
    }

    /// One server tick. Each phase of a start acts on its tick; a move that was refused is tried again, and a phase
    /// that stays stuck past its limit ends the start: the players go back to the lobby. Returns the outcome only when
    /// the start ends here.
    pub fn tick(&mut self, port: &mut dyn WorldPort) -> Option<Outcome> {
        match self.state {
            State::Resetting => {
                self.reset_ticks += 1;
                if self.reset_ticks > MAX_RESET_TICKS {
                    return Some(self.fail(port, Failure::ResetTimedOut));
                }
                None
            }
            State::WaitingForPlayersOut { seed, retries, gathered } => self.tick_gathering(port, seed, retries, gathered),
            State::AwaitingSpawn { waits, staged } => self.tick_spawn(port, waits, staged),
            State::LobbyOnly | State::Running => None,
        }
    }

    /// The lobby phase: moves the players that were refused, then asks for the reset once none is in a vanilla level. A
    /// busy engine or players still inside are retried on later ticks.
    fn tick_gathering(&mut self, port: &mut dyn WorldPort, seed: u64, retries: u32, gathered: bool) -> Option<Outcome> {
        let gathered = gathered || port.teleport_all_to_lobby();
        if gathered && !port.players_in_vanilla() {
            match port.reset_vanilla(seed) {
                Ok(()) => {
                    self.state = State::Resetting;
                    self.reset_ticks = 0;
                    return None;
                }
                Err(ResetRefusal::Busy | ResetRefusal::PlayersInside) => {}
                Err(refusal) => return Some(self.fail(port, Failure::Refused(refusal))),
            }
        }
        let retries = retries + 1;
        if retries > MAX_RETRY_TICKS {
            return Some(self.fail(port, Failure::RetriesExhausted));
        }
        self.state = State::WaitingForPlayersOut { seed, retries, gathered };
        None
    }

    /// The spawn phase: the players wait in the staging place until the new overworld has its spawn, then go to it. The
    /// start succeeds only once every player is moved there.
    fn tick_spawn(&mut self, port: &mut dyn WorldPort, waits: u32, staged: bool) -> Option<Outcome> {
        let waits = waits + 1;
        if waits > MAX_SPAWN_TICKS {
            return Some(self.fail(port, Failure::SpawnTimedOut));
        }
        let staged = staged || port.teleport_all_to_staging();
        if staged && port.overworld_spawn_known() && port.teleport_all_to_overworld_spawn() {
            self.state = State::Running;
            return Some(Outcome::Started);
        }
        self.state = State::AwaitingSpawn { waits, staged };
        None
    }

    /// The engine's `levels_reset` hook: the players go to the staging place of the new overworld. Ignored unless a reset
    /// is with the engine (a start given up meanwhile, or one that never asked, ignores it).
    pub fn levels_reset_done(&mut self, port: &mut dyn WorldPort) {
        if self.state != State::Resetting {
            return;
        }
        let staged = port.teleport_all_to_staging();
        self.state = State::AwaitingSpawn { waits: 0, staged };
    }

    /// The game ended, was reset, or the lobby was entered again: the players go back to the lobby. Idempotent: false,
    /// with nothing moved, when no start or game is running. A reset still with the engine is given up here; its
    /// `levels_reset_done` then does nothing.
    pub fn return_to_lobby(&mut self, port: &mut dyn WorldPort) -> bool {
        if self.state == State::LobbyOnly {
            return false;
        }
        port.teleport_all_to_lobby();
        self.state = State::LobbyOnly;
        self.reset_ticks = 0;
        true
    }

    fn fail(&mut self, port: &mut dyn WorldPort, failure: Failure) -> Outcome {
        self.state = State::LobbyOnly;
        self.reset_ticks = 0;
        port.teleport_all_to_lobby();
        Outcome::Failed(failure)
    }
}

//! Resetting the vanilla worlds when a game starts from the lobby, and sending the players back when it ends. The
//! original's rule: a start moves everyone from the lobby, resets the three vanilla worlds with a new seed, and puts
//! the players into the fresh overworld once the reset is done. The engine does the reset; this module only decides
//! the order. Everything the engine does goes through [`WorldPort`], and the engine's `levels_reset` hook arrives as
//! [`WorldReset::levels_reset_done`]. Each server tick calls [`WorldReset::tick`] once.

/// Ticks a reset waits, after the first attempt, for players to leave the vanilla levels or for a busy engine. At 20
/// server ticks per second this is 5 seconds.
pub const MAX_RETRY_TICKS: u32 = 100;

/// Ticks a reset may take with the engine, from the tick that asked for it, before the start is given up. The engine
/// fires no event when a reset fails, so this is the only way the flow learns of it. At 20 server ticks per second this
/// is 60 seconds.
pub const MAX_RESET_TICKS: u32 = 1200;

/// Whether a dimension is one of the three vanilla levels a reset regenerates (the Overworld, the Nether, the End).
pub fn is_vanilla_dimension(id: &str) -> bool {
    matches!(id, "minecraft:overworld" | "minecraft:the_nether" | "minecraft:the_end")
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

/// What the flow needs from the engine and the server.
pub trait WorldPort {
    /// Whether the lobby level is open to start from.
    fn lobby_is_open(&mut self) -> bool;
    /// Whether any player is in a vanilla level right now.
    fn players_in_vanilla(&self) -> bool;
    /// Asks the engine to reset the three vanilla worlds with `seed`. Refused while a player is inside one.
    fn reset_vanilla(&mut self, seed: u64) -> Result<(), ResetRefusal>;
    /// Sends every player to the lobby level.
    fn teleport_all_to_lobby(&mut self);
    /// Sends every player to the spawn of the new overworld.
    fn teleport_all_to_overworld_spawn(&mut self);
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
    /// Players stayed in a vanilla level, or the engine stayed busy, for more than [`MAX_RETRY_TICKS`] ticks.
    RetriesExhausted,
    /// The engine accepted the reset but `levels_reset` did not come within [`MAX_RESET_TICKS`] ticks.
    ResetTimedOut,
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
    /// Players are sent to the lobby; the reset waits until none is in a vanilla level.
    WaitingForPlayersOut { seed: u64, retries: u32 },
    /// The reset is with the engine; players wait for `levels_reset_done`.
    Resetting,
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
        port.teleport_all_to_lobby();
        self.state = State::WaitingForPlayersOut { seed: seeds.next_seed(), retries: 0 };
        Ok(())
    }

    /// One server tick. While waiting, asks the engine for the reset once no player is in a vanilla level. A busy
    /// engine or players still inside are retried on later ticks; past [`MAX_RETRY_TICKS`] the start fails and the
    /// players go back to the lobby. Returns the outcome only when the start ends here.
    pub fn tick(&mut self, port: &mut dyn WorldPort) -> Option<Outcome> {
        if self.state == State::Resetting {
            self.reset_ticks += 1;
            if self.reset_ticks > MAX_RESET_TICKS {
                return Some(self.fail(port, Failure::ResetTimedOut));
            }
            return None;
        }
        let State::WaitingForPlayersOut { seed, retries } = self.state else {
            return None;
        };
        if !port.players_in_vanilla() {
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
        self.state = State::WaitingForPlayersOut { seed, retries };
        None
    }

    /// The engine's `levels_reset` hook: sends everyone into the new overworld and starts play. Ignored unless a reset
    /// is with the engine.
    pub fn levels_reset_done(&mut self, port: &mut dyn WorldPort) -> Option<Outcome> {
        if self.state != State::Resetting {
            return None;
        }
        port.teleport_all_to_overworld_spawn();
        self.state = State::Running;
        Some(Outcome::Started)
    }

    /// The game ended, or the lobby was entered again: players go back to the lobby. False while a game is not
    /// running or a reset is with the engine (the answer then comes with `levels_reset_done`).
    pub fn return_to_lobby(&mut self, port: &mut dyn WorldPort) -> bool {
        match self.state {
            State::LobbyOnly | State::Resetting => false,
            State::WaitingForPlayersOut { .. } | State::Running => {
                port.teleport_all_to_lobby();
                self.state = State::LobbyOnly;
                true
            }
        }
    }

    fn fail(&mut self, port: &mut dyn WorldPort, failure: Failure) -> Outcome {
        self.state = State::LobbyOnly;
        self.reset_ticks = 0;
        port.teleport_all_to_lobby();
        Outcome::Failed(failure)
    }
}

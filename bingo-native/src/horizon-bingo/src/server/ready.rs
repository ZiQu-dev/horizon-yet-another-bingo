//! Lobby ready timer: who is ready, when the countdown starts, how far it has run, and the `ready_timer_update_v2`
//! packet that shows it.
//!
//! Mirrors common/src/main/java/me/jfenn/bingo/common/ready/:
//! - `ReadyTimerState.kt`: the countdown formula `duration()` (line 46-50) and `remainingTime()` (line 61-66),
//!   `reset()` (line 73-79), and `cancelled`, which `reset()` clears.
//! - `ReadyService.kt`: the start rule in `tick()` (line 145-151), the stop rule when the condition fails (line 153-155),
//!   and `reset()` (line 157-160).
//! - `ReadyCommand.kt`: `/ready cancel` (line 58-71): it acts only while the timer runs, then sets the cancel flag.
//! - `ReadyController.kt`: `update()` (completion, packet fields) and the title and subtitle it sends.
//!
//! Time is counted in server ticks: one call to [`ReadyTimer::tick`] is one server tick, `TICK_MILLIS` (20 per second).
//! The countdown is `(1 - ready / online) * total - elapsed`, with the ratio in `f32` as in Kotlin, so a ratio
//! of one third gives the same truncated milliseconds as the mod.
//!
//! The start needs more than one player online, as the mod's `ReadyService.isConditionSatisfied` (line 74) has it:
//! `players.size > 1`. `min_players` defaults to 2. The other conditions (teams, options) belong to the caller and
//! arrive as the `eligible` argument.

use std::collections::HashSet;

use super::commands::VoteChoice;
use super::texts;
use crate::buf::encode;
use crate::game::{GameState, GameStatus};
use crate::ready_update::ReadyUpdatePacket;
use crate::text::Text;

/// One server tick in milliseconds (20 ticks per second).
pub const TICK_MILLIS: i64 = 50;

/// Settings of the ready timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyOptions {
    /// Length of the countdown with no one ready. `startWhenReadySeconds` in the mod, 600 by default.
    pub timer_secs: i32,
    /// Fewest players online for the timer to run. The mod's `players.size > 1` (`ReadyService.kt:74`), so 2: a lone
    /// player cannot start a countdown.
    pub min_players: usize,
    /// `startWhenReadyWaitsForFirstVote` (false by default): when true the countdown starts only once someone is ready.
    pub waits_for_first_vote: bool,
}

impl Default for ReadyOptions {
    fn default() -> Self {
        Self { timer_secs: 600, min_players: 2, waits_for_first_vote: false }
    }
}

/// The ready state of the lobby and the countdown.
#[derive(Debug, Clone)]
pub struct ReadyTimer {
    options: ReadyOptions,
    /// Players who pressed ready. May include players who are not online (see [`ReadyTimer::ready_count`]).
    ready: HashSet<u128>,
    running: bool,
    /// Milliseconds since the countdown started, counted per tick.
    elapsed_ms: i64,
    /// Set by [`ReadyTimer::cancel`]. Blocks the automatic start until someone is ready again.
    cancelled: bool,
}

impl ReadyTimer {
    pub fn new(options: ReadyOptions) -> Self {
        Self { options, ready: HashSet::new(), running: false, elapsed_ms: 0, cancelled: false }
    }

    /// Marks a player as ready or not. Setting a player ready clears a cancel, as `ReadyService.startTimer` does.
    pub fn set_ready(&mut self, player: u128, ready: bool) {
        if ready {
            self.ready.insert(player);
            self.cancelled = false;
        } else {
            self.ready.remove(&player);
        }
    }

    pub fn is_ready(&self, player: u128) -> bool {
        self.ready.contains(&player)
    }

    /// `ReadyService.setReady`: a ready press starts a countdown that is not running, when the start rules hold
    /// (`eligible`) and enough players are online. No tick is counted. Returns whether the countdown runs afterwards.
    pub fn start_on_ready(&mut self, online: &[u128], eligible: bool) -> bool {
        if !self.running && eligible && online.len() >= self.options.min_players {
            self.running = true;
            self.elapsed_ms = 0;
        }
        self.running
    }

    /// Forgets a player's readiness, for example when they leave the server.
    pub fn remove(&mut self, player: u128) {
        self.ready.remove(&player);
    }

    /// Resets the timer to its state before any countdown: nobody ready, not running, no cancel
    /// (`ReadyTimerState.reset()`).
    pub fn clear(&mut self) {
        self.reset();
    }

    /// Stops the timer and blocks the automatic start until someone is ready again. Like `/ready cancel`, it does
    /// nothing when the timer is not running, so no block is set in that case.
    pub fn cancel(&mut self) {
        if self.running {
            self.reset();
            self.cancelled = true;
        }
    }

    /// Number of players in `online` who are ready. Ready players who are offline are not counted.
    pub fn ready_count(&self, online: &[u128]) -> usize {
        online.iter().filter(|player| self.ready.contains(player)).count()
    }

    /// Advances the timer by one server tick. `online` lists the players on the server, `eligible` tells whether the
    /// caller's start conditions hold (for example, every player is on a team).
    ///
    /// Returns `true` exactly on the tick the countdown reaches zero. The timer then stops and resets, so the
    /// caller can start the game or the next vote.
    ///
    /// The timer runs when `eligible`, at least `min_players` are online, and, if `waits_for_first_vote`, someone online is ready. It stops
    /// (and resets) when `eligible` or the player count stops holding, as `ReadyService.tick` does.
    ///
    /// Tick timeline: the tick that starts the countdown sets `elapsed` to 0 and does not count. Each later tick
    /// adds one tick of time first, then checks whether the remaining time is at most zero.
    pub fn tick(&mut self, online: &[u128], eligible: bool) -> bool {
        let conditions_hold = eligible && online.len() >= self.options.min_players;

        if self.running {
            if !conditions_hold {
                // Deliberate: the completion check sits after the condition check, so a countdown that would end on
                // a tick where the conditions fail does not run the vote. The mod runs it in that case.
                self.reset();
                return false;
            }
            self.elapsed_ms += TICK_MILLIS;
            if self.remaining_ms(online) <= 0 {
                self.reset();
                return true;
            }
            return false;
        }

        if !self.cancelled && conditions_hold && (!self.options.waits_for_first_vote || self.ready_count(online) >= 1) {
            self.running = true;
            self.elapsed_ms = 0;
        }
        false
    }

    /// Builds the `ready_timer_update_v2` packet for one viewer (`ReadyController.update`).
    ///
    /// `title` is an empty component, because the translated line is up to the caller, and `subtitle` is `None`.
    /// `vote_options` is empty and `vote_pending` is `None`: the lobby has no vote.
    pub fn update_packet(
        &self,
        viewer: u128,
        online: &[u128],
        state_identifier: &str,
        status: GameStatus,
    ) -> ReadyUpdatePacket {
        let (remaining_millis, total_millis) = if self.running {
            (self.remaining_ms(online).max(0), self.total_ms())
        } else {
            (0, 0)
        };
        ReadyUpdatePacket {
            is_running: self.running,
            is_ready: self.is_ready(viewer),
            state: GameState { identifier: state_identifier.to_owned(), status },
            remaining_millis,
            total_millis,
            ready_players: self.ready_count(online) as i32,
            total_players: online.len() as i32,
            title: Some(Text::empty()),
            subtitle: None,
            can_send_ready: true,
            vote_options: Vec::new(),
            vote_pending: None,
        }
    }

    /// The two lobby lines of the sidebar (`ReadyService.getGlobalMessage`): `N of M ready!` while the timer runs, and the
    /// countdown (`Starting in ...`), or `Waiting to start...` while it does not.
    pub fn sidebar_lines(&self, online: &[u128]) -> (Option<texts::Line>, texts::Line) {
        let ready_up = self.running.then(|| texts::players_ready(self.ready_count(online), online.len()));
        let countdown = if self.running {
            texts::starting_in(&texts::clock_text(self.remaining_ms(online).max(0)))
        } else {
            texts::waiting_to_start()
        };
        (ready_up, countdown)
    }

    /// The packet for one viewer with the lines the original's lobby shows (`ReadyService.getMessage`):
    ///
    /// - the title: a player who is ready sees `N of M ready!` while the timer runs and nothing otherwise; any other
    ///   player sees the hint `type <ready_command> to speed up!`;
    /// - the subtitle: `Starting in H:MM:SS` while the timer runs, else `Waiting to start...`.
    pub fn lobby_packet(
        &self,
        viewer: u128,
        online: &[u128],
        state_identifier: &str,
        status: GameStatus,
        ready_command: &str,
    ) -> ReadyUpdatePacket {
        let mut packet = self.update_packet(viewer, online, state_identifier, status);
        let title = if self.is_ready(viewer) {
            self.running.then(|| texts::players_ready(self.ready_count(online), online.len()))
        } else {
            Some(texts::ready_up_command(ready_command))
        };
        let subtitle = if self.running {
            texts::starting_in(&texts::clock_text(self.remaining_ms(online).max(0)))
        } else {
            texts::waiting_to_start()
        };
        packet.title = Some(title.map_or_else(Text::empty, |line| line.text()));
        packet.subtitle = Some(subtitle.text());
        packet
    }

    fn total_ms(&self) -> i64 {
        i64::from(self.options.timer_secs) * 1_000
    }

    /// `ReadyTimerState.remainingTime()`: the duration for the current ready share, minus the elapsed time.
    /// The share is `(1 - ready / max(online, 1)) * total`, truncated to whole milliseconds.
    fn remaining_ms(&self, online: &[u128]) -> i64 {
        let ready = self.ready_count(online) as f32;
        let ratio = ready / (online.len().max(1) as f32);
        let duration = ((1.0_f32 - ratio) * self.total_ms() as f32) as i64;
        duration - self.elapsed_ms
    }

    fn reset(&mut self) {
        self.ready.clear();
        self.running = false;
        self.elapsed_ms = 0;
        self.cancelled = false;
    }
}

/// `VoteType.id` of each postgame vote. The original draws them at random on every start; the clients only echo them, so
/// fixed ones do the same.
pub fn vote_id(choice: VoteChoice) -> u128 {
    match choice {
        VoteChoice::NextRound => 0x7f3a_91c2_0b54_4d1e_8a6f_2c0d_5e91_b301,
        VoteChoice::KeepPlaying => 0x7f3a_91c2_0b54_4d1e_8a6f_2c0d_5e91_b302,
        VoteChoice::ReturnToLobby => 0x7f3a_91c2_0b54_4d1e_8a6f_2c0d_5e91_b303,
    }
}

/// The vote an option id of `set_ready_v2` stands for.
pub fn vote_for_id(id: u128) -> Option<VoteChoice> {
    [VoteChoice::NextRound, VoteChoice::KeepPlaying, VoteChoice::ReturnToLobby]
        .into_iter()
        .find(|&choice| vote_id(choice) == id)
}

/// The postgame `ready_timer_update_v2` for one viewer (`ReadyController.createVoteOptions`, `currentVote`): the options
/// Next Round, Keep Playing (titled `keep_playing`, the resume text of the game) and Return to Lobby with their votes,
/// the default vote pending (the original's `AUTO` picks the next round), and no timer. `votes` are the players' votes
/// so far. The sneak-ready key stays off (`can_send_ready` false): this port has no postgame countdown for it to speed up.
pub fn postgame_packet(
    viewer: u128,
    votes: &std::collections::BTreeMap<u128, VoteChoice>,
    online: usize,
    keep_playing: texts::Line,
) -> ReadyUpdatePacket {
    let option = |choice: VoteChoice, line: texts::Line| crate::ready_update::VoteOption {
        id: vote_id(choice),
        title: line.text(),
        votes: votes.values().filter(|&&vote| vote == choice).count() as i32,
        is_selected: votes.get(&viewer) == Some(&choice),
    };
    ReadyUpdatePacket {
        is_running: false,
        is_ready: votes.contains_key(&viewer),
        state: GameState { identifier: "postgame".to_owned(), status: GameStatus::Postgame },
        remaining_millis: 0,
        total_millis: 0,
        ready_players: votes.len() as i32,
        total_players: online as i32,
        title: Some(Text::empty()),
        subtitle: None,
        can_send_ready: false,
        vote_options: vec![
            option(VoteChoice::NextRound, texts::next_round()),
            option(VoteChoice::KeepPlaying, keep_playing),
            option(VoteChoice::ReturnToLobby, texts::return_to_lobby()),
        ],
        vote_pending: Some(vote_id(VoteChoice::NextRound)),
    }
}

/// Encodes the packet on channel `ready_timer_update_v2` (the path without namespace, as `wire.rs` returns it).
pub fn encode_update_v2(packet: &ReadyUpdatePacket) -> (&'static str, Vec<u8>) {
    let bytes = encode(|w| packet.write_v2(w).expect("ready_timer_update_v2: a text component fails to write"));
    ("ready_timer_update_v2", bytes)
}

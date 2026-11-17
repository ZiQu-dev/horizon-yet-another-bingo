//! Game state machine and game timer, server side: Pregame, Starting (countdown), Playing (with an optional time limit)
//! and Postgame. Pure logic: the caller sends the returned events to the players.
//!
//! Mirrors GameService.kt (start, end, reset), CountdownService.kt and CountdownController.kt (the countdown; the
//! original sends a final 0 when the game starts, so this does too), and TimerCheck.kt and TimerPacketController.kt
//! (the time limit and the timer packets, the first one sent when play starts).
use crate::game::GameStatus;

/// Server ticks per second. One call to [`Game::tick`] is one server tick.
const TICKS_PER_SECOND: u64 = 20;
const MS_PER_TICK: i64 = 1000 / TICKS_PER_SECOND as i64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOptions {
    /// Seconds from start to the game beginning. 0 starts play at once. Negative values count as 0.
    pub countdown_secs: i32,
    /// Game time limit in seconds, or `None` for no limit. Negative values count as 0.
    pub time_limit_secs: Option<i32>,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self { countdown_secs: 3, time_limit_secs: None }
    }
}

/// Something the players must be told.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    StatusChanged(GameStatus),
    /// Seconds left before play begins. Sent at start, then once per second, and 0 as play begins.
    Countdown { seconds: i32 },
    /// Seconds left in the time limit. Sent as play begins, then once per second, and 0 at the end.
    Timer { seconds_remaining: i32 },
    /// The game is over. `winner` is `None` when the time limit ran out or nobody was named.
    Ended { winner: Option<String> },
    /// The game went back to Pregame.
    Reset,
    /// Postgame went back to Playing with the same card and timer (`/bingo resume`).
    Resumed,
}

/// The timer state a snapshot keeps: the phase, the ticks of the countdown and of play, and the time limit as set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSnapshot {
    pub status: GameStatus,
    pub countdown_ticks: u64,
    pub playing_ticks: u64,
    pub time_limit_secs: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct Game {
    options: GameOptions,
    status: GameStatus,
    /// Ticks since `start`, while Starting.
    countdown_ticks: u64,
    /// Ticks since play began. Stays at its final value in Postgame, so the elapsed time is frozen.
    playing_ticks: u64,
}

impl Game {
    pub fn new(options: GameOptions) -> Self {
        Self { options, status: GameStatus::Pregame, countdown_ticks: 0, playing_ticks: 0 }
    }

    /// Sets or removes the time limit (counted from the start of play); takes effect from the next tick.
    pub fn set_time_limit(&mut self, secs: Option<i32>) {
        self.options.time_limit_secs = secs;
    }

    pub fn status(&self) -> GameStatus {
        self.status
    }

    /// The timer state, for a snapshot. See [`Game::restore`].
    pub fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            status: self.status,
            countdown_ticks: self.countdown_ticks,
            playing_ticks: self.playing_ticks,
            time_limit_secs: self.options.time_limit_secs,
        }
    }

    /// Puts the timer back to a snapshot. No events are returned: the players learn the state when they join. The
    /// elapsed time is the saved one, not the wall clock.
    pub fn restore(&mut self, snapshot: GameSnapshot) {
        self.status = snapshot.status;
        self.countdown_ticks = snapshot.countdown_ticks;
        self.playing_ticks = snapshot.playing_ticks;
        self.options.time_limit_secs = snapshot.time_limit_secs;
    }

    /// Pregame to Starting. With a zero countdown, play begins at once.
    pub fn start(&mut self) -> Result<Vec<Event>, &'static str> {
        if self.status != GameStatus::Pregame {
            return Err("start: the game is not in Pregame");
        }
        self.status = GameStatus::Starting;
        self.countdown_ticks = 0;
        let mut events = vec![
            Event::StatusChanged(GameStatus::Starting),
            Event::Countdown { seconds: self.countdown_secs() },
        ];
        if self.countdown_secs() == 0 {
            self.begin_playing(&mut events);
        }
        Ok(events)
    }

    /// Playing to Postgame. The timer stops at this point.
    pub fn end(&mut self, winner: Option<String>) -> Result<Vec<Event>, &'static str> {
        if self.status != GameStatus::Playing {
            return Err("end: the game is not Playing");
        }
        Ok(self.finish(winner))
    }

    /// Postgame to Playing again, keeping the elapsed time. If the time limit has run out, it is moved to `extend_secs`
    /// from now (`extendRoundTimeLimitByMinutes` of the original), so the game does not end on the next tick.
    pub fn resume(&mut self, extend_secs: i32) -> Result<Vec<Event>, &'static str> {
        if self.status != GameStatus::Postgame {
            return Err("resume: the game is not in Postgame");
        }
        let elapsed_secs = (self.elapsed_ms() / 1000) as i32;
        if let Some(limit) = self.time_limit_secs() {
            if limit <= elapsed_secs {
                self.options.time_limit_secs = Some(elapsed_secs.saturating_add(extend_secs.max(1)));
            }
        }
        self.status = GameStatus::Playing;
        let mut events = vec![Event::Resumed];
        if let Some(seconds_remaining) = self.remaining_ms().map(|ms| (ms / 1000) as i32) {
            events.push(Event::Timer { seconds_remaining });
        }
        Ok(events)
    }

    /// Any state to Pregame. The elapsed time goes back to zero.
    pub fn reset(&mut self) -> Vec<Event> {
        self.status = GameStatus::Pregame;
        self.countdown_ticks = 0;
        self.playing_ticks = 0;
        vec![Event::StatusChanged(GameStatus::Pregame), Event::Reset]
    }

    /// Advances one server tick. Pregame and Postgame do nothing.
    pub fn tick(&mut self) -> Vec<Event> {
        match self.status {
            GameStatus::Pregame | GameStatus::Postgame => Vec::new(),
            GameStatus::Starting => self.tick_starting(),
            GameStatus::Playing => self.tick_playing(),
        }
    }

    /// Time spent playing, in milliseconds. Zero before play begins, frozen after it ends.
    pub fn elapsed_ms(&self) -> i64 {
        self.playing_ticks as i64 * MS_PER_TICK
    }

    /// Time left in the time limit, in milliseconds, never negative. `None` without a limit.
    pub fn remaining_ms(&self) -> Option<i64> {
        self.time_limit_secs().map(|secs| (secs as i64 * 1000 - self.elapsed_ms()).max(0))
    }

    fn countdown_secs(&self) -> i32 {
        self.options.countdown_secs.max(0)
    }

    /// The time limit in seconds, if one is set (`timelimit`); read by the sidebar.
    pub fn time_limit_secs(&self) -> Option<i32> {
        self.options.time_limit_secs.map(|secs| secs.max(0))
    }

    fn tick_starting(&mut self) -> Vec<Event> {
        self.countdown_ticks += 1;
        // One Countdown event per full second: at ticks 20, 40, ... after start.
        if self.countdown_ticks % TICKS_PER_SECOND != 0 {
            return Vec::new();
        }
        let elapsed_secs = (self.countdown_ticks / TICKS_PER_SECOND) as i32;
        let seconds = self.countdown_secs() - elapsed_secs;
        let mut events = vec![Event::Countdown { seconds }];
        if seconds == 0 {
            self.begin_playing(&mut events);
        }
        events
    }

    fn tick_playing(&mut self) -> Vec<Event> {
        self.playing_ticks += 1;
        let ticks = self.playing_ticks;
        let mut events = Vec::new();
        if let Some(limit_secs) = self.time_limit_secs() {
            let limit_ticks = limit_secs as u64 * TICKS_PER_SECOND;
            if ticks % TICKS_PER_SECOND == 0 && ticks <= limit_ticks {
                let seconds_remaining = ((limit_ticks - ticks) / TICKS_PER_SECOND) as i32;
                events.push(Event::Timer { seconds_remaining });
            }
            if ticks >= limit_ticks {
                events.extend(self.finish(None));
            }
        }
        events
    }

    /// Enters Playing with a fresh timer. The first Timer event for the limit goes out here, as the original does.
    fn begin_playing(&mut self, events: &mut Vec<Event>) {
        self.status = GameStatus::Playing;
        self.playing_ticks = 0;
        events.push(Event::StatusChanged(GameStatus::Playing));
        if let Some(seconds_remaining) = self.time_limit_secs() {
            events.push(Event::Timer { seconds_remaining });
        }
    }

    fn finish(&mut self, winner: Option<String>) -> Vec<Event> {
        self.status = GameStatus::Postgame;
        vec![Event::StatusChanged(GameStatus::Postgame), Event::Ended { winner }]
    }
}

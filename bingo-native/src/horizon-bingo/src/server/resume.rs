//! What Keep Playing (`/bingo resume` and the postgame vote) does to an ended game, and the button text that names it
//! (`GameResumeService.getResumeInfo` and `getResumeDescription`). The goal increase of a winning card is not ported:
//! the port keeps one goal for every team.

use super::texts::{self, duration_text, Line};

/// The original's `GameResumeInfo` choices this port carries out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    /// Play on as before (no team has finished and the time limit is not near).
    KeepPlaying,
    /// No team has finished and the time limit is nearly over: the limit grows by the extension.
    IncreaseTimeLimit,
    /// A team has won but some teams are still playing: the game ends only when every team has won.
    WaitUntilAllWin,
    /// Every team has finished: the game goes on with no card limit.
    InfiniteMode,
}

/// What the ended game looks like, from the state the choice reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Situation {
    /// No team has finished its card (`canResumeAllCards`).
    pub no_team_finished: bool,
    /// Some team has not finished its card.
    pub unfinished_teams: bool,
    /// A time limit is set and under 30 seconds away, or already run out (`isNearTimeLimit`).
    pub near_time_limit: bool,
}

/// The choice for a situation, in the original's order.
pub fn choose(situation: Situation) -> Resume {
    if situation.no_team_finished {
        if situation.near_time_limit {
            Resume::IncreaseTimeLimit
        } else {
            Resume::KeepPlaying
        }
    } else if situation.unfinished_teams {
        Resume::WaitUntilAllWin
    } else {
        Resume::InfiniteMode
    }
}

/// Whether a time limit is near its end, given the time left in milliseconds (`None` without a limit).
pub fn near_time_limit(remaining_ms: Option<i64>) -> bool {
    remaining_ms.is_some_and(|ms| ms < 30_000)
}

/// The Keep Playing button's text for a choice (`getResumeDescription`). `extend_secs` is the time the limit grows by.
pub fn description(resume: Resume, extend_secs: i32) -> Line {
    match resume {
        Resume::KeepPlaying => texts::keep_playing(),
        Resume::IncreaseTimeLimit => texts::extend_time_limit_by(&duration_text(extend_secs.max(0) as i64 * 1000)),
        Resume::WaitUntilAllWin => texts::extend_game_to_all_win(&texts::all_teams_win().render()),
        Resume::InfiniteMode => texts::extend_game_to_infinite(),
    }
}

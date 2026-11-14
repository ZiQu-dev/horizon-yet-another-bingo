//! Scoring for one card in the default mode: goal "1 line", win condition "1 card".
//!
//! Sources in orig/common/src/main/java/me/jfenn/bingo/common/:
//! - square found: the item is held, `held[item] >= 1` (task specification; the original marks an objective as
//!   achieved by a player in card/objective/ItemObjectiveManager.kt and ScoredItemCheck.kt, `updateScores`).
//! - lines: 5 rows, 5 columns, 2 diagonals; a line counts when all 5 squares are achieved (card/BingoCard.kt:62-86,
//!   `lines()` and `countLines`).
//! - items: number of achieved squares (card/BingoCard.kt:93-96, `countItems`).
//! - default options: `BingoCardOptions.goal = BingoGoal.Lines(1)` (options/BingoCardOptions.kt:20) and
//!   `BingoOptions.winCondition = BingoWinCondition.Cards(1)` (options/BingoOptions.kt:21).
//! - card completed: `team.score.lines >= goal.lines` (scoring/ScoredItemCheck.kt:186-190), then
//!   `team.countCards() >= requiredCards` ends the game for the team (ScoredItemCheck.kt:355-363).
//!
//! Goals (`Goal`), card modes (`Mode`) and end rules (`EndWhen`) are the options of the round. The default
//! (`Lines(1)`, `Standard`, `FirstWin`) is what `team_score` and `has_won` compute; `goal_reached`,
//! `team_score_goal`, `score_inventory_mode`, `claim_lockout` and `game_should_end` take the round's options.
use std::collections::HashMap;

use super::options::{EndWhen, Goal, Mode};
use super::types::{Card, TeamProgress, CARD_SQUARES};
use crate::score_update::TeamScore;

/// The 12 lines as square indices (index = row * 5 + col): rows 0-4, columns 0-4, main diagonal, anti-diagonal.
const LINES: [[usize; 5]; 12] = [
    [0, 1, 2, 3, 4],
    [5, 6, 7, 8, 9],
    [10, 11, 12, 13, 14],
    [15, 16, 17, 18, 19],
    [20, 21, 22, 23, 24],
    [0, 5, 10, 15, 20],
    [1, 6, 11, 16, 21],
    [2, 7, 12, 17, 22],
    [3, 8, 13, 18, 23],
    [4, 9, 14, 19, 24],
    [0, 6, 12, 18, 24],
    [4, 8, 12, 16, 20],
];

/// Marks which squares are in `found`. Duplicates count once; indices outside the card are ignored.
fn squares_found(found: &[usize]) -> [bool; CARD_SQUARES] {
    let mut have = [false; CARD_SQUARES];
    for &index in found {
        if index < CARD_SQUARES {
            have[index] = true;
        }
    }
    have
}

/// Adds every square whose item is held (count >= 1) and not yet found. Returns the newly found indices in card order.
/// Found squares stay found when the item leaves the inventory.
pub fn score_inventory(card: &Card, progress: &mut TeamProgress, held: &HashMap<String, u32>) -> Vec<usize> {
    let mut newly_found = Vec::new();
    for (index, square) in card.squares.iter().enumerate() {
        if progress.found.contains(&index) {
            continue;
        }
        if held.get(&square.item).copied().unwrap_or(0) >= 1 {
            progress.found.push(index);
            newly_found.push(index);
        }
    }
    newly_found
}

/// Number of the 12 lines whose 5 squares are all in `found`.
pub fn count_lines(found: &[usize]) -> i32 {
    let have = squares_found(found);
    LINES.iter().filter(|line| line.iter().all(|&index| have[index])).count() as i32
}

/// Score of the team on this card: items found, lines, and 1 card once it has a line (the default goal `Lines(1)`).
pub fn team_score(progress: &TeamProgress) -> TeamScore {
    let have = squares_found(&progress.found);
    let items = have.iter().filter(|&&found| found).count();
    let lines = count_lines(&progress.found);
    let cards = if lines >= 1 { 1 } else { 0 };
    TeamScore {
        items: items as i32,
        lines,
        cards,
    }
}

/// Default win rule: the team has a line on the card (`team.score.lines >= goal.lines` with `Lines(1)`,
/// ScoredItemCheck.kt:186-190, and `Cards(1)` ends the game for the team, ScoredItemCheck.kt:355-363).
pub fn has_won(progress: &TeamProgress) -> bool {
    team_score(progress).cards >= 1
}

/// Number of distinct squares in `found` that are on the card.
fn items_found(found: &[usize]) -> usize {
    squares_found(found).iter().filter(|&&found| found).count()
}

/// Whether the team has reached `goal`. `Lines(n)`: at least `n` lines. `Items(n)`: at least `n` squares.
/// `FullCard`: all 25 squares. `Lines(0)` and `Items(0)` are false: a goal of nothing would be reached before
/// the first square, so it is treated as no goal.
pub fn goal_reached(progress: &TeamProgress, goal: Goal) -> bool {
    match goal {
        Goal::Lines(0) | Goal::Items(0) => false,
        Goal::Lines(n) => count_lines(&progress.found) as u32 >= n,
        Goal::Items(n) => items_found(&progress.found) as u32 >= n,
        Goal::FullCard => items_found(&progress.found) == CARD_SQUARES,
    }
}

/// Score of the team for the round's `goal`: as `team_score`, but `cards` is 1 once `goal_reached`.
pub fn team_score_goal(progress: &TeamProgress, goal: Goal) -> TeamScore {
    let score = team_score(progress);
    let cards = if goal_reached(progress, goal) { 1 } else { 0 };
    TeamScore { cards, ..score }
}

/// Whether the square at `index` is on the card and its item is held (count >= 1).
fn square_held(card: &Card, index: usize, held: &HashMap<String, u32>) -> bool {
    card.squares
        .get(index)
        .is_some_and(|square| held.get(&square.item).copied().unwrap_or(0) >= 1)
}

/// Scores the card in `mode` and returns `(newly found, lost)`, both as square indices.
/// - `Standard` and `Lockout`: as `score_inventory`; nothing is lost. (Lockout ownership is `claim_lockout`.)
/// - `Inventory`: a square counts only while its item is held. Newly held squares are added in card order;
///   found squares whose item is no longer held are removed from `progress.found` and returned in their found
///   order (an index that is not on the card is dropped too, since its item cannot be held).
pub fn score_inventory_mode(
    card: &Card,
    progress: &mut TeamProgress,
    held: &HashMap<String, u32>,
    mode: Mode,
) -> (Vec<usize>, Vec<usize>) {
    match mode {
        Mode::Standard | Mode::Lockout => (score_inventory(card, progress, held), Vec::new()),
        Mode::Inventory => {
            let mut lost = Vec::new();
            progress.found.retain(|&index| {
                let keep = square_held(card, index, held);
                if !keep {
                    lost.push(index);
                }
                keep
            });

            let mut newly_found = Vec::new();
            for index in 0..card.squares.len() {
                if !progress.found.contains(&index) && square_held(card, index, held) {
                    progress.found.push(index);
                    newly_found.push(index);
                }
            }
            (newly_found, lost)
        }
    }
}

/// Lockout: gives each square in `gained` that has no owner to `team`, and returns those squares in the order given.
/// A square the same team already owns is skipped (not returned); a square owned by another team is refused.
pub fn claim_lockout(owners: &mut HashMap<usize, String>, team: &str, gained: &[usize]) -> Vec<usize> {
    let mut claimed = Vec::new();
    for &index in gained {
        // An owned square is skipped whether the owner is `team` (already its) or another team (refused).
        if !owners.contains_key(&index) {
            owners.insert(index, team.to_string());
            claimed.push(index);
        }
    }
    claimed
}

/// Whether the round ends after the current state: `reached` teams have reached the goal out of `total_teams`.
/// `Never`: never. `FirstWin`: one team. `TeamsWin(n)`: `n` teams (`n = 0` never ends the round, since `n >= 1`
/// is required). `AllWin`: every team, and only when there is at least one team.
pub fn game_should_end(end_when: EndWhen, teams_reached: usize, total_teams: usize) -> bool {
    match end_when {
        EndWhen::Never => false,
        EndWhen::FirstWin => teams_reached >= 1,
        EndWhen::TeamsWin(n) => n >= 1 && teams_reached >= n as usize,
        EndWhen::AllWin => total_teams > 0 && teams_reached >= total_teams,
    }
}

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
//! Deviation: only the default goal `Lines(1)` is implemented, so a card is complete (`cards = 1`) once it has one line.
use std::collections::HashMap;

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

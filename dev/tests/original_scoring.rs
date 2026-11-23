//! Scoring rules checked against the original mod (Kotlin, orig/common/src/main/java/me/jfenn/bingo/common/scoring/).
//! The original was not run here: every expected value is traced by hand from the cited file:line, and the
//! expectations do not come from the port's code. Each test that the port fails is `#[ignore]`d with the difference.
use std::collections::HashMap;

use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::options::{EndWhen, Goal};
use horizon_bingo::server::scoring::{game_should_end, goal_reached, score_inventory, team_score};
use horizon_bingo::server::types::{Card, Objective, TeamProgress};

/// Card where square `i` needs the item `item{i}` (index = row * 5 + col).
fn card() -> Card {
    Card {
        squares: (0..25)
            .map(|i| Objective { item: format!("item{i}"), tier: TierLabel::A })
            .collect(),
    }
}

fn progress(found: &[usize]) -> TeamProgress {
    TeamProgress { found: found.to_vec() }
}

fn held(indices: &[usize]) -> HashMap<String, u32> {
    indices.iter().map(|i| (format!("item{i}"), 1)).collect()
}

// hand-traced from ScoredItemCheck.kt:186-190: `team.score.items >= goal.items` (Items) and
// `team.score.lines >= goal.lines` (Lines). Expected values counted by hand from the card layout.
#[test]
fn items_goal_is_reached_at_exactly_its_item_count() {
    // three squares found: Items(3) reached, Items(4) not
    let p = progress(&[0, 1, 2]);
    assert!(goal_reached(&p, Goal::Items(3)));
    assert!(!goal_reached(&p, Goal::Items(4)));
}

// hand-traced from ScoredItemCheck.kt:186-190 and BingoCard.kt:62-86 (countLines). Row 0 and column 0 share
// square 0: 9 squares make 2 lines, so Lines(2) is reached and Lines(3) is not.
#[test]
fn lines_goal_counts_lines_not_squares() {
    let nine = progress(&[0, 1, 2, 3, 4, 5, 10, 15, 20]);
    assert!(goal_reached(&nine, Goal::Lines(2)));
    assert!(!goal_reached(&nine, Goal::Lines(3)));

    // five squares in one row are one line: Lines(2) is not reached
    let row = progress(&[0, 1, 2, 3, 4]);
    assert!(!goal_reached(&row, Goal::Lines(2)));
}

// hand-traced from ScoredItemCheck.kt:186-190: `team.score.lines >= goal.lines` with goal 0 and lines 0 is true,
// so the original card is complete from the first tick. The port never reaches Lines(0).
#[test]
fn lines_zero_goal_is_reached_at_once() {
    assert!(goal_reached(&progress(&[]), Goal::Lines(0)));
}

// hand-traced from ScoredItemCheck.kt:187-189: `team.score.items >= goal.items` with 0 >= 0 is true.
#[test]
fn items_zero_goal_is_reached_at_once() {
    assert!(goal_reached(&progress(&[]), Goal::Items(0)));
}

// hand-traced from ScoredItemCheck.kt:430-437: `teams.count { it.isFinished() } >= endWhen.teams` is 0 >= 0 with
// no team finished, so TeamsWin(0) ends the game at once. The port's game_should_end requires n >= 1.
#[test]
fn teams_win_zero_ends_at_once() {
    assert!(game_should_end(EndWhen::TeamsWin(0), 0, 2));
}

// hand-traced from ScoredItemCheck.kt:430-437: `teams.all { it.isFinished() }` over an empty list is true, so
// AllWin ends the game when there are no teams. The port's game_should_end requires total > 0.
#[test]
fn all_win_with_no_teams_ends_in_the_original() {
    assert!(game_should_end(EndWhen::AllWin, 0, 0));
}

// hand-traced from ScoredItemCheck.kt:430-437 (FirstWin) and BingoCard.kt:93-96 (countItems): FirstWin ends when one team has finished, and the
// score counts only squares whose item is held in the default (Standard) mode: 2 squares, no line.
#[test]
fn first_win_ends_with_one_finished_team_and_held_squares_score_as_items() {
    assert!(game_should_end(EndWhen::FirstWin, 1, 3));
    assert!(!game_should_end(EndWhen::FirstWin, 0, 3));

    let mut p = progress(&[]);
    score_inventory(&card(), &mut p, &held(&[3, 7]));
    let score = team_score(&p);
    assert_eq!((score.items, score.lines, score.cards), (2, 0, 0));
}

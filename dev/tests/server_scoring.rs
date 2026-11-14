//! Scoring of one card in the default mode. Expected values are counted by hand from the square layout
//! (index = row * 5 + col) and do not come from the code under test.
use std::collections::HashMap;

use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::score_update::TeamScore;
use horizon_bingo::server::scoring::{count_lines, has_won, score_inventory, team_score};
use horizon_bingo::server::types::{Card, Objective, TeamProgress};

/// Card where square `i` needs the item `item{i}`.
fn card() -> Card {
    Card {
        squares: (0..25)
            .map(|i| Objective { item: format!("item{i}"), tier: TierLabel::A })
            .collect(),
    }
}

/// Inventory holding one of each item for the given square indices.
fn held(indices: &[usize]) -> HashMap<String, u32> {
    indices.iter().map(|i| (format!("item{i}"), 1)).collect()
}

fn progress(found: &[usize]) -> TeamProgress {
    TeamProgress { found: found.to_vec() }
}

fn score(items: i32, lines: i32, cards: i32) -> TeamScore {
    TeamScore { items, lines, cards }
}

#[test]
fn each_row_is_one_line() {
    assert_eq!(count_lines(&[0, 1, 2, 3, 4]), 1);
    assert_eq!(count_lines(&[5, 6, 7, 8, 9]), 1);
    assert_eq!(count_lines(&[10, 11, 12, 13, 14]), 1);
    assert_eq!(count_lines(&[15, 16, 17, 18, 19]), 1);
    assert_eq!(count_lines(&[20, 21, 22, 23, 24]), 1);
}

#[test]
fn each_column_is_one_line() {
    assert_eq!(count_lines(&[0, 5, 10, 15, 20]), 1);
    assert_eq!(count_lines(&[1, 6, 11, 16, 21]), 1);
    assert_eq!(count_lines(&[2, 7, 12, 17, 22]), 1);
    assert_eq!(count_lines(&[3, 8, 13, 18, 23]), 1);
    assert_eq!(count_lines(&[4, 9, 14, 19, 24]), 1);
}

#[test]
fn each_diagonal_alone_is_one_line() {
    assert_eq!(count_lines(&[0, 6, 12, 18, 24]), 1);
    assert_eq!(count_lines(&[4, 8, 12, 16, 20]), 1);
}

#[test]
fn both_diagonals_are_two_lines() {
    // 9 squares: the two diagonals share the centre (12).
    assert_eq!(count_lines(&[0, 6, 12, 18, 24, 4, 8, 16, 20]), 2);
}

#[test]
fn row_and_column_crossing_is_two_lines() {
    // Row 0 and column 0 share square 0: 9 squares, 2 lines.
    assert_eq!(count_lines(&[0, 1, 2, 3, 4, 5, 10, 15, 20]), 2);
}

#[test]
fn four_of_a_row_is_not_a_line() {
    assert_eq!(count_lines(&[0, 1, 2, 3]), 0);
    assert_eq!(count_lines(&[0, 5, 10, 15]), 0);
}

#[test]
fn empty_progress_scores_zero() {
    assert_eq!(count_lines(&[]), 0);
    assert_eq!(team_score(&progress(&[])), score(0, 0, 0));
    assert!(!has_won(&progress(&[])));
}

#[test]
fn full_card_is_twelve_lines_and_one_card() {
    let all: Vec<usize> = (0..25).collect();
    assert_eq!(count_lines(&all), 12);
    assert_eq!(team_score(&progress(&all)), score(25, 12, 1));
}

#[test]
fn twenty_four_squares_still_one_card_by_the_default_goal() {
    // Missing square 12 (centre): row 2 and column 2 are broken, so the rows 0, 1, 3, 4 and the columns 0, 1, 3, 4
    // are full (8 lines); both diagonals pass through 12.
    let found: Vec<usize> = (0..25).filter(|&i| i != 12).collect();
    assert_eq!(team_score(&progress(&found)), score(24, 8, 1));
}

#[test]
fn one_line_completes_the_card_and_four_squares_do_not() {
    assert_eq!(team_score(&progress(&[0, 1, 2, 3, 4])).cards, 1);
    assert_eq!(team_score(&progress(&[0, 1, 2, 3])).cards, 0);
}

#[test]
fn a_held_item_with_count_zero_is_not_found() {
    let mut p = progress(&[]);
    let mut inventory = HashMap::new();
    inventory.insert("item3".to_string(), 0);
    assert_eq!(score_inventory(&card(), &mut p, &inventory), Vec::<usize>::new());
    assert!(p.found.is_empty());
}

#[test]
fn empty_inventory_finds_nothing() {
    let mut p = progress(&[]);
    assert!(score_inventory(&card(), &mut p, &HashMap::new()).is_empty());
    assert!(p.found.is_empty());
}

#[test]
fn newly_found_indices_come_in_card_order() {
    let mut p = progress(&[]);
    let returned = score_inventory(&card(), &mut p, &held(&[20, 3, 7]));
    assert_eq!(returned, vec![3, 7, 20]);
    assert_eq!(p.found, vec![3, 7, 20]);
}

#[test]
fn already_found_squares_are_not_returned_again() {
    let mut p = progress(&[7]);
    let returned = score_inventory(&card(), &mut p, &held(&[3, 7]));
    assert_eq!(returned, vec![3]);
    assert_eq!(p.found, vec![7, 3]);
}

#[test]
fn found_square_stays_found_after_item_leaves_inventory() {
    let mut p = progress(&[]);
    assert_eq!(score_inventory(&card(), &mut p, &held(&[5])), vec![5]);

    assert!(score_inventory(&card(), &mut p, &HashMap::new()).is_empty());
    assert_eq!(p.found, vec![5]);
    assert_eq!(team_score(&p), score(1, 0, 0));
}

#[test]
fn one_item_on_two_squares_finds_both() {
    let mut card = card();
    card.squares[9].item = "item0".to_string();
    let mut p = progress(&[]);
    let inventory: HashMap<String, u32> = [("item0".to_string(), 1)].into_iter().collect();
    assert_eq!(score_inventory(&card, &mut p, &inventory), vec![0, 9]);
}

#[test]
fn duplicate_indices_count_once() {
    // Row 0 with square 0 listed three times: still one line and five items.
    assert_eq!(count_lines(&[0, 0, 0, 1, 2, 3, 4]), 1);
    assert_eq!(team_score(&progress(&[0, 0, 1])), score(2, 0, 0));
    assert_eq!(team_score(&progress(&[0, 0, 0, 0, 0])), score(1, 0, 0));
}

#[test]
fn indices_outside_the_card_are_ignored() {
    assert_eq!(team_score(&progress(&[0, 25, 99])), score(1, 0, 0));
    assert_eq!(count_lines(&[25, 26, 27, 28, 29]), 0);
}

#[test]
fn won_with_a_single_line() {
    assert!(has_won(&progress(&[0, 1, 2, 3, 4])));
    assert!(has_won(&progress(&[4, 8, 12, 16, 20])));
}

#[test]
fn not_won_with_scattered_squares() {
    assert!(!has_won(&progress(&[0, 1, 2, 3])));
    assert!(!has_won(&progress(&[0, 6, 12, 18])));
}

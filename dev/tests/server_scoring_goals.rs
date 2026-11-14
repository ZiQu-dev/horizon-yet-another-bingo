//! Goals, card modes, lockout and end rules of the round. Expected values are counted by hand from the square layout
//! (index = row * 5 + col) and the rules, and do not come from the code under test.
use std::collections::HashMap;

use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::score_update::TeamScore;
use horizon_bingo::server::options::{EndWhen, Goal, Mode};
use horizon_bingo::server::scoring::{claim_lockout, game_should_end, goal_reached, score_inventory_mode, team_score_goal};
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
fn first_row_is_one_line_and_five_items() {
    let row = progress(&[0, 1, 2, 3, 4]);
    assert!(goal_reached(&row, Goal::Lines(1)));
    assert!(!goal_reached(&row, Goal::Lines(2)));
    assert!(goal_reached(&row, Goal::Items(5)));
    assert!(!goal_reached(&row, Goal::Items(6)));
    assert!(!goal_reached(&row, Goal::FullCard));
}

#[test]
fn full_card_reaches_every_goal() {
    let all = progress(&(0..25).collect::<Vec<_>>());
    assert!(goal_reached(&all, Goal::FullCard));
    assert!(goal_reached(&all, Goal::Items(25)));
    assert!(!goal_reached(&all, Goal::Items(26)));
    // Twelve lines: 5 rows, 5 columns, 2 diagonals.
    assert!(goal_reached(&all, Goal::Lines(12)));
    assert!(!goal_reached(&all, Goal::Lines(13)));
}

#[test]
fn zero_goals_are_never_reached() {
    let all = progress(&(0..25).collect::<Vec<_>>());
    assert!(!goal_reached(&all, Goal::Lines(0)));
    assert!(!goal_reached(&all, Goal::Items(0)));
    assert!(!goal_reached(&progress(&[]), Goal::Items(0)));
}

#[test]
fn team_score_goal_sets_cards_when_goal_is_reached() {
    let row = progress(&[0, 1, 2, 3, 4]);
    assert_eq!(team_score_goal(&row, Goal::Lines(1)), score(5, 1, 1));
    assert_eq!(team_score_goal(&row, Goal::Lines(2)), score(5, 1, 0));
    assert_eq!(team_score_goal(&row, Goal::Items(5)), score(5, 1, 1));
    assert_eq!(team_score_goal(&row, Goal::Items(6)), score(5, 1, 0));
    assert_eq!(team_score_goal(&row, Goal::FullCard), score(5, 1, 0));
}

#[test]
fn inventory_mode_counts_only_held_items() {
    let mut p = progress(&[4]);
    // Square 4 is found but not held any more, so it is lost; square 1 is newly held.
    let (new, lost) = score_inventory_mode(&card(), &mut p, &held(&[1]), Mode::Inventory);
    assert_eq!(new, vec![1]);
    assert_eq!(lost, vec![4]);
    assert_eq!(p.found, vec![1]);
}

#[test]
fn inventory_mode_loses_square_when_item_leaves_inventory() {
    let mut p = progress(&[]);
    // Find squares 2 and 7 together, in card order.
    let (new, lost) = score_inventory_mode(&card(), &mut p, &held(&[7, 2]), Mode::Inventory);
    assert_eq!(new, vec![2, 7]);
    assert!(lost.is_empty());

    // Item 7 leaves the inventory: square 7 is lost, square 2 stays.
    let (new, lost) = score_inventory_mode(&card(), &mut p, &held(&[2]), Mode::Inventory);
    assert!(new.is_empty());
    assert_eq!(lost, vec![7]);
    assert_eq!(p.found, vec![2]);
}

#[test]
fn standard_and_lockout_never_lose_squares() {
    for mode in [Mode::Standard, Mode::Lockout] {
        let mut p = progress(&[7]);
        let (new, lost) = score_inventory_mode(&card(), &mut p, &HashMap::new(), mode);
        assert!(new.is_empty());
        assert!(lost.is_empty());
        assert_eq!(p.found, vec![7]);
    }
}

#[test]
fn standard_mode_finds_like_score_inventory() {
    let mut p = progress(&[]);
    let (new, lost) = score_inventory_mode(&card(), &mut p, &held(&[20, 3]), Mode::Standard);
    assert_eq!(new, vec![3, 20]);
    assert!(lost.is_empty());
    assert_eq!(p.found, vec![3, 20]);
}

#[test]
fn lockout_gives_free_squares_and_refuses_taken_ones() {
    let mut owners: HashMap<usize, String> = HashMap::new();
    assert_eq!(claim_lockout(&mut owners, "red", &[0, 1, 2]), vec![0, 1, 2]);

    // Blue tries 0, 1, 2 again and gets only the new square 3.
    assert_eq!(claim_lockout(&mut owners, "blue", &[0, 1, 2, 3]), vec![3]);
    assert_eq!(owners.get(&0).map(String::as_str), Some("red"));
    assert_eq!(owners.get(&3).map(String::as_str), Some("blue"));
}

#[test]
fn lockout_skips_squares_the_same_team_already_owns() {
    let mut owners: HashMap<usize, String> = HashMap::new();
    claim_lockout(&mut owners, "red", &[5]);
    assert!(claim_lockout(&mut owners, "red", &[5]).is_empty());
}

#[test]
fn lockout_returns_squares_in_given_order_and_each_once() {
    let mut owners: HashMap<usize, String> = HashMap::new();
    assert_eq!(claim_lockout(&mut owners, "red", &[9, 4, 9]), vec![9, 4]);
}

#[test]
fn game_should_end_table() {
    let cases: [(EndWhen, usize, usize, bool); 22] = [
        (EndWhen::Never, 0, 1, false),
        (EndWhen::Never, 5, 5, false),
        (EndWhen::FirstWin, 0, 3, false),
        (EndWhen::FirstWin, 1, 3, true),
        (EndWhen::FirstWin, 3, 3, true),
        (EndWhen::TeamsWin(1), 0, 2, false),
        (EndWhen::TeamsWin(1), 1, 2, true),
        (EndWhen::TeamsWin(2), 1, 3, false),
        (EndWhen::TeamsWin(2), 2, 3, true),
        (EndWhen::TeamsWin(3), 2, 3, false),
        (EndWhen::TeamsWin(3), 3, 3, true),
        (EndWhen::TeamsWin(0), 0, 2, false),
        (EndWhen::TeamsWin(0), 2, 2, false),
        (EndWhen::AllWin, 0, 0, false),
        (EndWhen::AllWin, 1, 2, false),
        (EndWhen::AllWin, 2, 2, true),
        (EndWhen::AllWin, 1, 1, true),
        (EndWhen::AllWin, 3, 2, true),
        (EndWhen::TeamsWin(1), 0, 0, false),
        (EndWhen::FirstWin, 0, 0, false),
        (EndWhen::AllWin, 0, 1, false),
        (EndWhen::TeamsWin(4), 3, 4, false),
    ];
    for (end_when, reached, total, expected) in cases {
        assert_eq!(
            game_should_end(end_when, reached, total),
            expected,
            "{end_when:?} reached={reached} total={total}"
        );
    }
}

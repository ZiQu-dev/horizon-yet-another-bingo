//! Multi-card play (`play_to`), the cards a team completes, when a round ends, pending cards, leaders, and the stalemate
//! detector with its behaviours. Expected values are counted by hand from the original's rules (ScoredItemCheck.kt,
//! ScoreService.kt, StalemateBehavior.kt, BingoWinCondition.kt) and the 5x5 layout (index = row * 5 + col; lines are
//! rows 5r..5r+4, columns c, c+5, .., c+20, the diagonals 0,6,12,18,24 and 4,8,12,16,20). None of them is computed by
//! the code under test.
use std::cell::Cell;
use std::collections::HashMap;

use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::score_update::TeamScore;
use horizon_bingo::server::multicard::{
    complete_card, finish_card_on_goal, leading_team, reached_goal, reroll_card, round_ends, settle_pending,
    skip_others_on_card, stalemate_action, stalemate_reason, team_score, CardState, Completion, CompletionOutcome,
    EndCause, Rules, StalemateAction, StalemateBehavior, StalemateReason, TeamRound, WinCondition,
    REROLL_NOTICE_BEHAVIOR,
};
use horizon_bingo::server::options::{EndWhen, Goal, Mode};
use horizon_bingo::server::types::{Card, Objective, TeamProgress};
use serde_json::json;

const GONE: &str = "bingo_gone";

/// Card where square `i` needs the item `item{i}`.
fn card() -> Card {
    Card {
        squares: (0..25)
            .map(|i| Objective { item: format!("item{i}"), tier: TierLabel::A })
            .collect(),
    }
}

fn found(squares: &[usize]) -> TeamProgress {
    TeamProgress { found: squares.to_vec() }
}

/// A team in `state` with `squares` found on its current card.
fn team(key: &str, state: CardState, squares: &[usize]) -> TeamRound {
    let mut t = TeamRound::new(key, state);
    t.progress = found(squares);
    t
}

/// A team that has already completed the cards in `wins` (each a winning completion at time 1).
fn with_wins(mut t: TeamRound, wins: &[usize]) -> TeamRound {
    for &card_index in wins {
        t.completed.push(horizon_bingo::server::multicard::CardCompletion {
            card_index,
            is_winner: true,
            stalemate: None,
            completed_at: 1,
            score: TeamScore::default(),
        });
    }
    t
}

fn rules(goal: Goal, mode: Mode, end_when: EndWhen, win: WinCondition) -> Rules {
    Rules { goal, mode, end_when, win }
}

/// Standard mode, one line, first win, one card: the default round.
fn default_rules() -> Rules {
    rules(Goal::Lines(1), Mode::Standard, EndWhen::FirstWin, WinCondition::Cards(1))
}

fn lockout_rules(goal: Goal, end_when: EndWhen) -> Rules {
    rules(goal, Mode::Lockout, end_when, WinCondition::Cards(1))
}

fn completion(card_index: usize, is_winner: bool, completed_at: u64) -> Completion {
    Completion { card_index, is_winner, stalemate: None, completed_at, score: TeamScore::default() }
}

/// Owners of squares `from..=to` (inclusive) for each `(from, to, team)` part.
fn owners(parts: &[(usize, usize, &str)]) -> HashMap<usize, String> {
    let mut map = HashMap::new();
    for &(from, to, team) in parts {
        for square in from..=to {
            map.insert(square, team.to_string());
        }
    }
    map
}

fn no_card() -> usize {
    panic!("no new card should be made")
}

fn score(items: i32, lines: i32, cards: i32) -> TeamScore {
    TeamScore { items, lines, cards }
}

fn row0() -> Vec<usize> {
    vec![0, 1, 2, 3, 4]
}

// ---------------------------------------------------------------------------------------------------------------
// Options and JSON

#[test]
fn default_win_condition_is_one_card() {
    assert_eq!(WinCondition::default(), WinCondition::Cards(1));
    assert_eq!(WinCondition::default().required_cards(), Some(1));
}

#[test]
fn default_stalemate_behaviour_is_end_game() {
    assert_eq!(StalemateBehavior::default(), StalemateBehavior::EndGame);
}

#[test]
fn required_cards_follow_the_win_condition() {
    assert_eq!(WinCondition::Cards(3).required_cards(), Some(3));
    assert_eq!(WinCondition::Infinite.required_cards(), None);
    // ReplaceGoals requires 1 card (ScoredItemCheck.kt, `ReplaceGoals -> 1`).
    assert_eq!(WinCondition::ReplaceGoals.required_cards(), Some(1));
}

#[test]
fn win_condition_json_has_the_horizon_shape() {
    assert_eq!(WinCondition::Cards(4).to_json(), json!({ "type": "cards", "n": 4 }));
    assert_eq!(WinCondition::Infinite.to_json(), json!({ "type": "infinite", "n": 0 }));
    assert_eq!(WinCondition::ReplaceGoals.to_json(), json!({ "type": "replace_goals", "n": 0 }));
}

#[test]
fn win_condition_json_round_trips() {
    for win in [WinCondition::Cards(1), WinCondition::Cards(25), WinCondition::Infinite, WinCondition::ReplaceGoals] {
        assert_eq!(WinCondition::from_json(&win.to_json()), Some(win));
    }
}

#[test]
fn win_condition_from_json_rejects_bad_values() {
    assert_eq!(WinCondition::from_json(&json!({ "type": "cards", "n": 0 })), None, "play_to cards needs at least 1");
    assert_eq!(WinCondition::from_json(&json!({ "type": "cards" })), None);
    assert_eq!(WinCondition::from_json(&json!({ "type": "cards", "n": "2" })), None);
    assert_eq!(WinCondition::from_json(&json!({ "type": "cards", "n": -1 })), None);
    assert_eq!(WinCondition::from_json(&json!({ "type": "cards", "n": 4_294_967_296u64 })), None);
    assert_eq!(WinCondition::from_json(&json!({ "type": "teams", "n": 2 })), None);
    assert_eq!(WinCondition::from_json(&json!("cards")), None);
}

#[test]
fn stalemate_behaviour_json_uses_the_lower_case_enum_names() {
    assert_eq!(StalemateBehavior::EndGame.to_json(), json!("end_game"));
    assert_eq!(StalemateBehavior::RerollCard.to_json(), json!("reroll_card"));
    assert_eq!(StalemateBehavior::Nothing.to_json(), json!("nothing"));
    for b in [StalemateBehavior::EndGame, StalemateBehavior::RerollCard, StalemateBehavior::Nothing] {
        assert_eq!(StalemateBehavior::from_json(&b.to_json()), Some(b));
    }
    // The command word is do_nothing, the enum is NOTHING: the file form is the enum name.
    assert_eq!(StalemateBehavior::from_json(&json!("do_nothing")), None);
    assert_eq!(StalemateBehavior::from_json(&json!(1)), None);
}

// ---------------------------------------------------------------------------------------------------------------
// Scores and goals

#[test]
fn lines_are_counted_only_for_a_lines_goal() {
    let row = found(&row0());
    assert_eq!(team_score(Goal::Lines(1), &row, 0), score(5, 1, 0));
    assert_eq!(team_score(Goal::Items(5), &row, 0), score(5, 0, 0));
    assert_eq!(team_score(Goal::FullCard, &row, 0), score(5, 0, 0));
}

#[test]
fn score_carries_the_card_count_it_is_given() {
    assert_eq!(team_score(Goal::Lines(1), &found(&row0()), 2), score(5, 1, 2));
}

#[test]
fn repeated_and_off_card_squares_count_once_or_not_at_all() {
    assert_eq!(team_score(Goal::Items(1), &found(&[0, 0, 0, 30, 99]), 0), score(1, 0, 0));
    assert_eq!(team_score(Goal::Lines(1), &found(&[0, 0, 0, 30, 99]), 0), score(1, 0, 0));
}

#[test]
fn two_full_columns_are_two_lines() {
    // Column 0 is 0,5,10,15,20 and column 1 is 1,6,11,16,21: no row or diagonal is complete.
    let progress = found(&[0, 5, 10, 15, 20, 1, 6, 11, 16, 21]);
    assert_eq!(team_score(Goal::Lines(2), &progress, 0), score(10, 2, 0));
}

#[test]
fn goals_are_reached_by_their_numbers() {
    let row = found(&row0());
    assert!(reached_goal(Goal::Lines(1), &row));
    assert!(!reached_goal(Goal::Lines(2), &row));
    assert!(reached_goal(Goal::Items(5), &row));
    assert!(!reached_goal(Goal::Items(6), &row));
    assert!(!reached_goal(Goal::FullCard, &row));

    let all = found(&(0..25).collect::<Vec<_>>());
    assert!(reached_goal(Goal::FullCard, &all));
    assert!(reached_goal(Goal::Items(25), &all));
    // A full card has 12 lines: 5 rows, 5 columns, 2 diagonals.
    assert!(reached_goal(Goal::Lines(12), &all));
    assert!(!reached_goal(Goal::Lines(13), &all));
}

#[test]
fn a_goal_of_zero_is_reached_at_once_as_the_original_compares() {
    // ScoredItemCheck compares `score.lines >= goal.lines` with no check for 0.
    assert!(reached_goal(Goal::Lines(0), &found(&[])));
    assert!(reached_goal(Goal::Items(0), &found(&[])));
}

// ---------------------------------------------------------------------------------------------------------------------
// Completing cards and the win condition

#[test]
fn one_card_to_win_finishes_the_team() {
    let rules = default_rules();
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    let outcome = complete_card(&mut t, &rules, 10, Some(1), completion(0, true, 100));
    assert_eq!(outcome, CompletionOutcome::Finished);
    assert_eq!(t.state, CardState::Finished { completed_at: 100 });
    assert_eq!(t.count_cards(), 1);
    assert!(t.is_finished());
}

#[test]
fn cards_three_needs_three_wins() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(3));
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    assert_eq!(complete_card(&mut t, &rules, 10, Some(1), completion(0, true, 10)), CompletionOutcome::Pending(Some(1)));
    assert_eq!(t.count_cards(), 1);
    assert_eq!(complete_card(&mut t, &rules, 10, Some(2), completion(1, true, 20)), CompletionOutcome::Pending(Some(2)));
    assert_eq!(t.count_cards(), 2);
    assert_eq!(complete_card(&mut t, &rules, 10, None, completion(2, true, 300)), CompletionOutcome::Finished);
    assert_eq!(t.count_cards(), 3);
    assert_eq!(t.state, CardState::Finished { completed_at: 300 });
}

#[test]
fn a_skipped_card_does_not_count_towards_the_win() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(2));
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    assert_eq!(complete_card(&mut t, &rules, 10, Some(1), completion(0, false, 10)), CompletionOutcome::Pending(Some(1)));
    assert_eq!(t.count_cards(), 0, "a skip is not a win");
    assert_eq!(complete_card(&mut t, &rules, 10, None, completion(1, true, 20)), CompletionOutcome::Pending(None));
    assert_eq!(t.count_cards(), 1);
    assert_eq!(complete_card(&mut t, &rules, 10, None, completion(2, true, 30)), CompletionOutcome::Finished);
    assert_eq!(t.count_cards(), 2);
    assert_eq!(t.completed.len(), 3, "the skip is still in the history");
}

#[test]
fn a_second_completion_of_a_card_keeps_the_first_record() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(2));
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    complete_card(&mut t, &rules, 10, None, completion(0, true, 10));
    complete_card(&mut t, &rules, 10, None, completion(0, false, 20));
    assert_eq!(t.completed.len(), 1);
    assert!(t.completed[0].is_winner, "the first record (a win) is kept");
    assert_eq!(t.completed[0].completed_at, 10);
    assert_eq!(t.count_cards(), 1);
}

#[test]
fn infinite_cards_never_finishes_the_team() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Infinite);
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    for card_index in 0..10 {
        let outcome = complete_card(&mut t, &rules, 10, Some(card_index + 1), completion(card_index, true, 5));
        assert_eq!(outcome, CompletionOutcome::Pending(Some(card_index + 1)));
    }
    assert_eq!(t.count_cards(), 10);
    assert!(!t.is_finished());
}

#[test]
fn replace_goals_finishes_on_the_first_card() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::ReplaceGoals);
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    assert_eq!(complete_card(&mut t, &rules, 10, None, completion(0, true, 7)), CompletionOutcome::Finished);
    assert_eq!(t.state, CardState::Finished { completed_at: 7 });
}

#[test]
fn a_card_completed_within_three_ticks_ends_the_game() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(2));
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    // Ticks 3 or fewer: the original ends the game with an error (ScoredItemCheck.kt, `card.ticks <= 3`).
    assert_eq!(complete_card(&mut t, &rules, 3, Some(1), completion(0, true, 9)), CompletionOutcome::ImmediateEnd);
    assert_eq!(t.state, CardState::Playing(0), "the state does not change");
    assert_eq!(t.count_cards(), 1, "the completion is still recorded");

    let mut u = TeamRound::new("bingo_b", CardState::Playing(0));
    assert_eq!(complete_card(&mut u, &rules, 4, Some(1), completion(0, true, 9)), CompletionOutcome::Pending(Some(1)));
}

#[test]
fn a_winning_completion_finishes_even_at_zero_ticks() {
    // The finish comes before the tick guard in the original.
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    assert_eq!(complete_card(&mut t, &default_rules(), 0, None, completion(0, true, 1)), CompletionOutcome::Finished);
}

#[test]
fn the_pending_next_card_is_kept() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(2));
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    complete_card(&mut t, &rules, 50, Some(7), completion(0, false, 1));
    assert_eq!(t.state, CardState::Pending(Some(7)));
    assert_eq!(t.current_card(), None);
}

// ---------------------------------------------------------------------------------------------------------------------
// The goal reached on a card, and the other teams

#[test]
fn the_first_team_in_order_wins_and_the_others_are_skipped_on_first_win() {
    let rules = default_rules();
    let mut teams = vec![
        team("bingo_a", CardState::Playing(0), &row0()),
        team("bingo_b", CardState::Playing(0), &row0()),
        team("bingo_c", CardState::Playing(0), &[]),
    ];
    let finish = finish_card_on_goal(&mut teams, &rules, 0, 10, Some(1), 40).expect("a team reached the goal");
    assert_eq!(finish.winner, 0);
    assert_eq!(finish.outcome, CompletionOutcome::Finished, "Cards(1) is one win");
    assert_eq!(teams[0].state, CardState::Finished { completed_at: 40 });
    // FirstWin skips every other team still on the card, though B reached the goal too.
    assert_eq!(
        finish.skipped,
        vec![(1, CompletionOutcome::Pending(Some(1))), (2, CompletionOutcome::Pending(Some(1)))]
    );
    assert_eq!(teams[1].state, CardState::Pending(Some(1)));
    assert!(!teams[1].completion_for(0).unwrap().is_winner);
}

#[test]
fn never_leaves_the_other_teams_on_the_card() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(1));
    let mut teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &row0())];
    let finish = finish_card_on_goal(&mut teams, &rules, 0, 10, None, 40).unwrap();
    assert!(finish.skipped.is_empty());
    assert_eq!(teams[1].state, CardState::Playing(0));
}

#[test]
fn all_win_leaves_the_other_teams_on_the_card() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::AllWin, WinCondition::Cards(1));
    let mut teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let finish = finish_card_on_goal(&mut teams, &rules, 0, 10, None, 40).unwrap();
    assert!(finish.skipped.is_empty());
    assert_eq!(teams[1].state, CardState::Playing(0));
}

#[test]
fn teams_win_skips_only_once_enough_teams_completed_the_card() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::TeamsWin(2), WinCondition::Cards(1));
    let mut teams = vec![
        team("bingo_a", CardState::Playing(0), &row0()),
        team("bingo_b", CardState::Playing(0), &row0()),
        team("bingo_c", CardState::Playing(0), &[]),
    ];
    // First completion: 1 team completed the card, below 2, so nobody is skipped.
    let first = finish_card_on_goal(&mut teams, &rules, 0, 10, None, 40).unwrap();
    assert_eq!(first.winner, 0);
    assert!(first.skipped.is_empty());
    assert_eq!(teams[2].state, CardState::Playing(0));

    // B completes the card too: 2 teams have, so C is moved on.
    let second = finish_card_on_goal(&mut teams, &rules, 0, 10, None, 41).unwrap();
    assert_eq!(second.winner, 1);
    assert_eq!(second.skipped, vec![(2, CompletionOutcome::Pending(None))]);
}

#[test]
fn the_winner_score_is_the_one_it_had_before_the_card() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(2));
    let mut teams = vec![team("bingo_a", CardState::Playing(0), &row0())];

    finish_card_on_goal(&mut teams, &rules, 0, 10, Some(1), 40).unwrap();
    assert_eq!(teams[0].completed[0].score, score(5, 1, 0), "no card won before this one");

    // Move to card 1 and complete it: the score now counts the first card.
    settle_pending(&mut teams, 2, no_card);
    teams[0].progress = found(&row0());
    let second = finish_card_on_goal(&mut teams, &rules, 1, 10, None, 90).unwrap();
    assert_eq!(second.outcome, CompletionOutcome::Finished);
    assert_eq!(teams[0].completed[1].score, score(5, 1, 1));
}

#[test]
fn no_team_at_the_goal_gives_no_finish() {
    let rules = default_rules();
    let mut teams = vec![team("bingo_a", CardState::Playing(0), &[0, 1, 2, 3]), team("bingo_b", CardState::Playing(0), &[])];
    assert_eq!(finish_card_on_goal(&mut teams, &rules, 0, 10, None, 40), None);
    assert_eq!(teams[0].state, CardState::Playing(0));
    assert!(teams[0].completed.is_empty());
}

#[test]
fn only_a_team_playing_this_card_can_win_it() {
    let rules = default_rules();
    let mut teams = vec![
        team("bingo_a", CardState::Playing(1), &row0()),
        team("bingo_b", CardState::Finished { completed_at: 5 }, &row0()),
    ];
    assert_eq!(finish_card_on_goal(&mut teams, &rules, 0, 10, None, 40), None);
}

#[test]
fn an_immediate_completion_is_reported_from_the_finish() {
    // Cards(2): the win is not the last card, so the tick guard applies.
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::Never, WinCondition::Cards(2));
    let mut teams = vec![team("bingo_a", CardState::Playing(0), &row0())];
    let finish = finish_card_on_goal(&mut teams, &rules, 0, 2, None, 40).unwrap();
    assert_eq!(finish.outcome, CompletionOutcome::ImmediateEnd);
}

#[test]
fn skip_rules_per_end_rule() {
    assert!(skip_others_on_card(EndWhen::FirstWin, 1));
    assert!(!skip_others_on_card(EndWhen::AllWin, 9));
    assert!(!skip_others_on_card(EndWhen::Never, 9));
    assert!(!skip_others_on_card(EndWhen::TeamsWin(2), 1));
    assert!(skip_others_on_card(EndWhen::TeamsWin(2), 2));
    assert!(skip_others_on_card(EndWhen::TeamsWin(2), 3));
}

#[test]
fn reroll_moves_every_team_on_the_card_on_without_a_win() {
    let rules = default_rules();
    let mut teams = vec![
        team("bingo_a", CardState::Playing(0), &row0()),
        team("bingo_b", CardState::Playing(0), &[]),
        team("bingo_c", CardState::Playing(1), &[]),
    ];
    let outcomes = reroll_card(&mut teams, &rules, 0, 10, Some(1), 77);
    assert_eq!(outcomes, vec![(0, CompletionOutcome::Pending(Some(1))), (1, CompletionOutcome::Pending(Some(1)))]);
    assert_eq!(teams[2].state, CardState::Playing(1), "a team on another card is untouched");
    assert_eq!(teams[0].count_cards(), 0);
    assert_eq!(teams[0].completion_for(0).unwrap().completed_at, 77);
    assert!(!teams[0].completion_for(0).unwrap().is_winner);
}

// ---------------------------------------------------------------------------------------------------------------------
// Round end, pending cards and the leader

#[test]
fn never_never_ends_the_round() {
    let teams = vec![TeamRound::new("bingo_a", CardState::Finished { completed_at: 1 }), TeamRound::new("bingo_b", CardState::Playing(0))];
    assert!(!round_ends(EndWhen::Never, &teams));
}

#[test]
fn first_win_ends_the_round_at_the_first_finished_team() {
    let playing = vec![TeamRound::new("bingo_a", CardState::Playing(0)), TeamRound::new("bingo_b", CardState::Playing(0))];
    assert!(!round_ends(EndWhen::FirstWin, &playing));
    let one = vec![TeamRound::new("bingo_a", CardState::Finished { completed_at: 1 }), TeamRound::new("bingo_b", CardState::Playing(0))];
    assert!(round_ends(EndWhen::FirstWin, &one));
}

#[test]
fn teams_win_counts_finished_teams() {
    let fin = |n: usize| -> Vec<TeamRound> {
        (0..3)
            .map(|i| {
                let state = if i < n { CardState::Finished { completed_at: 1 } } else { CardState::Playing(0) };
                TeamRound::new(&format!("bingo_{i}"), state)
            })
            .collect()
    };
    assert!(!round_ends(EndWhen::TeamsWin(2), &fin(1)));
    assert!(round_ends(EndWhen::TeamsWin(2), &fin(2)));
    assert!(round_ends(EndWhen::TeamsWin(2), &fin(3)));
}

#[test]
fn all_win_needs_every_team_and_is_true_for_none() {
    let fin = |n: usize| -> Vec<TeamRound> {
        (0..3)
            .map(|i| {
                let state = if i < n { CardState::Finished { completed_at: 1 } } else { CardState::Playing(0) };
                TeamRound::new(&format!("bingo_{i}"), state)
            })
            .collect()
    };
    assert!(!round_ends(EndWhen::AllWin, &fin(2)));
    assert!(round_ends(EndWhen::AllWin, &fin(3)));
    // The original's `teams.all { isFinished }` is true for no teams.
    assert!(round_ends(EndWhen::AllWin, &[]));
}

#[test]
fn a_team_short_of_its_cards_does_not_end_a_first_win_round() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::FirstWin, WinCondition::Cards(2));
    let mut t = TeamRound::new("bingo_a", CardState::Playing(0));
    complete_card(&mut t, &rules, 10, None, completion(0, true, 10));
    assert!(!round_ends(EndWhen::FirstWin, std::slice::from_ref(&t)));
    complete_card(&mut t, &rules, 10, None, completion(1, true, 20));
    assert!(round_ends(EndWhen::FirstWin, std::slice::from_ref(&t)));
}

#[test]
fn a_pending_team_on_an_existing_card_plays_it_with_fresh_progress() {
    let mut teams = vec![team("bingo_a", CardState::Pending(Some(1)), &row0())];
    assert!(!settle_pending(&mut teams, 2, no_card));
    assert_eq!(teams[0].state, CardState::Playing(1));
    assert!(teams[0].progress.found.is_empty());
}

#[test]
fn pending_teams_without_a_card_share_one_made_card() {
    let made = Cell::new(0);
    let mut teams = vec![
        team("bingo_a", CardState::Pending(None), &[]),
        team("bingo_b", CardState::Pending(Some(3)), &[]),
        team("bingo_c", CardState::Playing(0), &[]),
    ];
    let changed = settle_pending(&mut teams, 2, || {
        made.set(made.get() + 1);
        2
    });
    assert!(changed);
    assert_eq!(made.get(), 1, "one card for both pending teams");
    assert_eq!(teams[0].state, CardState::Playing(2));
    assert_eq!(teams[1].state, CardState::Playing(2));
    assert_eq!(teams[2].state, CardState::Playing(0));
}

#[test]
fn nobody_pending_makes_no_card() {
    let mut teams = vec![TeamRound::new("bingo_a", CardState::Playing(0)), TeamRound::new("bingo_b", CardState::Finished { completed_at: 1 })];
    assert!(!settle_pending(&mut teams, 1, no_card));
    assert_eq!(teams[0].state, CardState::Playing(0));
}

#[test]
fn a_next_card_past_the_list_makes_a_card() {
    let mut teams = vec![team("bingo_a", CardState::Pending(Some(5)), &[])];
    assert!(settle_pending(&mut teams, 2, || 2));
    assert_eq!(teams[0].state, CardState::Playing(2));
}

#[test]
fn leader_is_the_earlier_finisher() {
    let teams = vec![
        TeamRound::new("bingo_a", CardState::Finished { completed_at: 100 }),
        TeamRound::new("bingo_b", CardState::Finished { completed_at: 50 }),
        team("bingo_c", CardState::Playing(0), &row0()),
    ];
    assert_eq!(leading_team(&teams, Goal::Lines(1), false), Some(1));
}

#[test]
fn leader_is_decided_by_cards_before_lines_and_items() {
    let teams = vec![
        team("bingo_b", CardState::Playing(0), &(0..25).collect::<Vec<_>>()),
        with_wins(team("bingo_a", CardState::Playing(1), &[]), &[0]),
    ];
    // B has the full card but no card won; A has one card won.
    assert_eq!(leading_team(&teams, Goal::Lines(1), false), Some(1));
}

#[test]
fn lines_then_items_break_a_tie_on_lines() {
    let teams = vec![
        team("bingo_b", CardState::Playing(0), &row0()),
        team("bingo_a", CardState::Playing(0), &[0, 1, 2, 3, 4, 5]),
    ];
    // Both have one line; A has 6 items to B's 5.
    assert_eq!(leading_team(&teams, Goal::Lines(1), false), Some(1));
}

#[test]
fn the_items_goal_compares_items_and_the_lines_goal_compares_lines() {
    let b = team("bingo_b", CardState::Playing(0), &row0());
    let a = team("bingo_a", CardState::Playing(0), &[0, 2, 4, 6, 8, 10]);
    let teams = vec![b, a];
    // Items goal: A has 6 items, B has 5 (B's line does not count).
    assert_eq!(leading_team(&teams, Goal::Items(6), false), Some(1));
    // Lines goal: B has a line, A has none.
    assert_eq!(leading_team(&teams, Goal::Lines(1), false), Some(0));
}

#[test]
fn a_tie_at_the_top_has_no_leader_unless_ties_are_allowed() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &[]), team("bingo_b", CardState::Playing(0), &[])];
    assert_eq!(leading_team(&teams, Goal::Lines(1), false), None);
    assert_eq!(leading_team(&teams, Goal::Lines(1), true), Some(0));
}

#[test]
fn a_later_team_that_beats_a_tie_is_the_leader() {
    let teams = vec![
        team("bingo_a", CardState::Playing(0), &[]),
        team("bingo_b", CardState::Playing(0), &[]),
        team("bingo_c", CardState::Playing(0), &row0()),
    ];
    assert_eq!(leading_team(&teams, Goal::Lines(1), false), Some(2));
}

// ---------------------------------------------------------------------------------------------------------------------
// The stalemate detector

fn stalemate(teams: &[TeamRound], owners: &HashMap<usize, String>, rules: &Rules) -> Option<StalemateReason> {
    stalemate_reason(&card(), 0, teams, owners, rules, true)
}

fn win(key: &str) -> StalemateReason {
    StalemateReason::WinByStalemate(key.to_string())
}

#[test]
fn standard_mode_with_two_open_teams_has_no_stalemate() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &[]), team("bingo_b", CardState::Playing(0), &[])];
    assert_eq!(stalemate(&teams, &HashMap::new(), &default_rules()), None);
}

#[test]
fn a_leader_is_not_a_stalemate_while_the_other_team_can_still_beat_it() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    assert_eq!(stalemate(&teams, &HashMap::new(), &default_rules()), None);
}

#[test]
fn a_single_team_is_never_a_stalemate() {
    let rules = default_rules();
    let open = vec![team("bingo_a", CardState::Playing(0), &row0())];
    assert_eq!(stalemate(&open, &HashMap::new(), &rules), None);
    // Lockout: A holds the row, squares 5..23 belong to a team that left, 24 is open. A still has its line: no stalemate.
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    assert_eq!(stalemate(&open, &owners, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)), None);
}

#[test]
fn lockout_row_keeps_the_other_team_alive() {
    // A owns row 0 and B can still take the open squares 5..24 (four rows, the columns are cut by A's squares).
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a")]);
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)), None);
}

#[test]
fn lockout_hands_the_card_to_the_leader_when_nobody_can_catch_up() {
    // Squares 5..23 belong to a team that left and 0..4 to A, so 24 is the only open square: B can take it (one item, no
    // line) but cannot tie A's line and 5 items. The card is not full, so it is a stalemate that A wins.
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    assert_eq!(
        stalemate(&teams, &owners, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)),
        Some(win("bingo_a"))
    );
}

#[test]
fn a_stalemate_without_a_leader_on_the_card_is_a_plain_stalemate() {
    // The round's leader is a finished team on another card, so the card has no WinByStalemate.
    let teams = vec![
        TeamRound::new("bingo_f", CardState::Finished { completed_at: 50 }),
        team("bingo_a", CardState::Playing(0), &row0()),
        team("bingo_b", CardState::Playing(0), &[]),
    ];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)), Some(StalemateReason::Stalemate));
}

#[test]
fn an_impossible_goal_is_reported_when_no_team_can_reach_it() {
    // Standard mode: every square is open, so a team can reach 12 lines at most. Lines(13) is unreachable.
    let teams = vec![team("bingo_a", CardState::Playing(0), &[]), team("bingo_b", CardState::Playing(0), &[])];
    assert_eq!(
        stalemate(&teams, &HashMap::new(), &rules(Goal::Lines(13), Mode::Standard, EndWhen::FirstWin, WinCondition::Cards(1))),
        Some(StalemateReason::ImpossibleGoal)
    );
}

#[test]
fn an_impossible_goal_wins_over_a_win_by_stalemate() {
    // Only squares 0..4 (A's) and 24 (open) are not held. A can reach 6 items (the row and 24), B only 24.
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    // Items(6) is still reachable by A: a stalemate that A wins.
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(Goal::Items(6), EndWhen::FirstWin)), Some(win("bingo_a")));
    // Items(7): nobody can reach it, and an impossible goal wins over the stalemate.
    assert_eq!(
        stalemate(&teams, &owners, &lockout_rules(Goal::Items(7), EndWhen::FirstWin)),
        Some(StalemateReason::ImpossibleGoal)
    );
}

#[test]
fn a_full_card_goal_in_lockout_is_not_impossible_with_several_teams() {
    // Squares 20..23 are open, 24 and 5..19 belong to a team that left, 0..4 to A. B can reach 4 items, A's 5 lead.
    // Full card, lockout, two teams: the goal is not called impossible, so A wins the card by stalemate.
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 19, GONE), (24, 24, GONE)]);
    assert_eq!(
        stalemate(&teams, &owners, &lockout_rules(Goal::FullCard, EndWhen::FirstWin)),
        Some(win("bingo_a"))
    );
    // The same board with one team: the exception does not apply, the full card is out of reach.
    let alone = vec![team("bingo_a", CardState::Playing(0), &row0())];
    assert_eq!(
        stalemate(&alone, &owners, &lockout_rules(Goal::FullCard, EndWhen::FirstWin)),
        Some(StalemateReason::ImpossibleGoal)
    );
}

#[test]
fn teams_win_and_all_win_need_that_many_teams_that_can_reach_the_goal() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    let goal = Goal::Lines(1);
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(goal, EndWhen::Never)), Some(win("bingo_a")));
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(goal, EndWhen::FirstWin)), Some(win("bingo_a")));
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(goal, EndWhen::TeamsWin(1))), Some(win("bingo_a")));
    // Two teams must reach the goal, but B cannot.
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(goal, EndWhen::TeamsWin(2))), Some(StalemateReason::ImpossibleGoal));
    assert_eq!(stalemate(&teams, &owners, &lockout_rules(goal, EndWhen::AllWin)), Some(StalemateReason::ImpossibleGoal));
}

#[test]
fn replace_goals_has_no_stalemate() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    let rules = rules(Goal::Lines(1), Mode::Lockout, EndWhen::FirstWin, WinCondition::ReplaceGoals);
    assert_eq!(stalemate(&teams, &owners, &rules), None);
}

#[test]
fn no_stalemate_when_the_game_is_not_playing_or_no_team_plays_the_card() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    let rules = lockout_rules(Goal::Lines(1), EndWhen::FirstWin);
    assert_eq!(stalemate_reason(&card(), 0, &teams, &owners, &rules, false), None);

    let elsewhere = vec![team("bingo_a", CardState::Playing(1), &row0()), team("bingo_b", CardState::Playing(1), &[])];
    assert_eq!(stalemate(&elsewhere, &owners, &rules), None);
}

#[test]
fn a_full_lockout_card_with_one_team_is_a_win_for_it() {
    let alone = vec![team("bingo_a", CardState::Playing(0), &[])];
    let all_gone = owners(&[(0, 24, GONE)]);
    assert_eq!(
        stalemate(&alone, &all_gone, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)),
        Some(StalemateReason::WinByLockout("bingo_a".to_string()))
    );
}

#[test]
fn a_full_lockout_card_with_a_tie_is_a_plain_lockout() {
    let teams = vec![team("bingo_a", CardState::Playing(0), &[]), team("bingo_b", CardState::Playing(0), &[])];
    let all_gone = owners(&[(0, 24, GONE)]);
    assert_eq!(stalemate(&teams, &all_gone, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)), Some(StalemateReason::Lockout));
}

#[test]
fn a_full_lockout_card_with_a_leader_is_a_win_by_lockout() {
    // A holds 0..12 (two lines, 13 items), B holds 13..24 (two lines, 12 items): A leads on items.
    let teams = vec![team("bingo_a", CardState::Playing(0), &(0..=12).collect::<Vec<_>>()), team("bingo_b", CardState::Playing(0), &(13..=24).collect::<Vec<_>>())];
    let owners = owners(&[(0, 12, "bingo_a"), (13, 24, "bingo_b")]);
    assert_eq!(
        stalemate(&teams, &owners, &lockout_rules(Goal::Lines(1), EndWhen::FirstWin)),
        Some(StalemateReason::WinByLockout("bingo_a".to_string()))
    );
}

#[test]
fn inventory_mode_never_locks_a_square() {
    // The same board as the lockout hand-over: in inventory mode B can still take every square.
    let teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];
    let owners = owners(&[(0, 4, "bingo_a"), (5, 23, GONE)]);
    let rules = rules(Goal::Lines(1), Mode::Inventory, EndWhen::FirstWin, WinCondition::Cards(1));
    assert_eq!(stalemate(&teams, &owners, &rules), None);
}

// ---------------------------------------------------------------------------------------------------------------------
// Stalemate behaviour

#[test]
fn no_reason_does_nothing_under_any_behaviour() {
    for behaviour in [StalemateBehavior::EndGame, StalemateBehavior::RerollCard, StalemateBehavior::Nothing] {
        assert_eq!(stalemate_action(None, behaviour), StalemateAction::Nothing);
    }
}

#[test]
fn a_win_completes_the_card_under_every_behaviour() {
    let reason = win("bingo_a");
    for behaviour in [StalemateBehavior::EndGame, StalemateBehavior::RerollCard, StalemateBehavior::Nothing] {
        assert_eq!(
            stalemate_action(Some(&reason), behaviour),
            StalemateAction::WinCard { team: "bingo_a".to_string(), reason: reason.clone() }
        );
    }
    let by_lockout = StalemateReason::WinByLockout("bingo_b".to_string());
    assert_eq!(
        stalemate_action(Some(&by_lockout), StalemateBehavior::Nothing),
        StalemateAction::WinCard { team: "bingo_b".to_string(), reason: by_lockout.clone() }
    );
}

#[test]
fn end_game_ends_the_game_with_the_cause() {
    assert_eq!(stalemate_action(Some(&StalemateReason::Stalemate), StalemateBehavior::EndGame), StalemateAction::EndGame(EndCause::Stalemate));
    assert_eq!(stalemate_action(Some(&StalemateReason::Lockout), StalemateBehavior::EndGame), StalemateAction::EndGame(EndCause::Lockout));
    assert_eq!(
        stalemate_action(Some(&StalemateReason::ImpossibleGoal), StalemateBehavior::EndGame),
        StalemateAction::EndGame(EndCause::ImpossibleGoal)
    );
}

#[test]
fn reroll_and_nothing_for_a_stalemate_without_a_winner() {
    assert_eq!(stalemate_action(Some(&StalemateReason::Stalemate), StalemateBehavior::RerollCard), StalemateAction::RerollCard);
    assert_eq!(stalemate_action(Some(&StalemateReason::Lockout), StalemateBehavior::RerollCard), StalemateAction::RerollCard);
    assert_eq!(stalemate_action(Some(&StalemateReason::ImpossibleGoal), StalemateBehavior::RerollCard), StalemateAction::RerollCard);
    assert_eq!(stalemate_action(Some(&StalemateReason::Stalemate), StalemateBehavior::Nothing), StalemateAction::Nothing);
    assert_eq!(stalemate_action(Some(&StalemateReason::ImpossibleGoal), StalemateBehavior::Nothing), StalemateAction::Nothing);
}

#[test]
fn the_reroll_notice_names_end_game_whatever_the_setting() {
    // StalemateBehavior.END_GAME.string(text) is hard-coded in ScoredItemCheck.kt.
    assert_eq!(REROLL_NOTICE_BEHAVIOR, StalemateBehavior::EndGame);
}

#[test]
fn stalemate_reason_reports_its_winner_only_for_a_win() {
    assert_eq!(win("bingo_a").winner(), Some(&"bingo_a".to_string()));
    assert_eq!(StalemateReason::Stalemate.winner(), None);
    assert_eq!(StalemateReason::ImpossibleGoal.winner(), None);
}

// ---------------------------------------------------------------------------------------------------------------------
// A round of two cards, end to end

#[test]
fn a_two_card_round_ends_when_the_first_team_has_both_cards() {
    let rules = rules(Goal::Lines(1), Mode::Standard, EndWhen::FirstWin, WinCondition::Cards(2));
    let mut teams = vec![team("bingo_a", CardState::Playing(0), &row0()), team("bingo_b", CardState::Playing(0), &[])];

    // Card 0: A completes its line first. Under FirstWin, B is moved on from card 0 without a win.
    let first = finish_card_on_goal(&mut teams, &rules, 0, 10, Some(1), 40).unwrap();
    assert_eq!(first.outcome, CompletionOutcome::Pending(Some(1)));
    assert_eq!(first.skipped, vec![(1, CompletionOutcome::Pending(Some(1)))]);
    assert!(!round_ends(EndWhen::FirstWin, &teams), "one card of two is not the win");

    // Both teams play card 1; A completes its second line and finishes the round.
    assert!(!settle_pending(&mut teams, 2, no_card));
    assert_eq!(teams[0].state, CardState::Playing(1));
    assert_eq!(teams[1].state, CardState::Playing(1));
    teams[0].progress = found(&row0());
    let second = finish_card_on_goal(&mut teams, &rules, 1, 10, None, 90).unwrap();
    assert_eq!(second.outcome, CompletionOutcome::Finished);
    assert_eq!(second.skipped, vec![(1, CompletionOutcome::Pending(None))]);
    assert_eq!(teams[0].count_cards(), 2);
    assert!(round_ends(EndWhen::FirstWin, &teams));
}

#[test]
fn the_stalemate_winner_is_the_round_leader_not_the_card_leader() {
    // Card 1: X has already won card 0 (one card), Y has a line here. X leads the round on cards, so X is the leader
    // (`getLeading(state)`), even though Y is the top team on this card. X can only take square 24, so Y holds the rest.
    let rules = rules(Goal::Lines(1), Mode::Lockout, EndWhen::FirstWin, WinCondition::Cards(2));
    let teams = vec![
        with_wins(team("bingo_x", CardState::Playing(1), &[]), &[0]),
        team("bingo_y", CardState::Playing(1), &row0()),
    ];
    let owners = owners(&[(0, 4, "bingo_y"), (5, 23, GONE)]);
    assert_eq!(
        stalemate_reason(&card(), 1, &teams, &owners, &rules, true),
        Some(win("bingo_x"))
    );
}

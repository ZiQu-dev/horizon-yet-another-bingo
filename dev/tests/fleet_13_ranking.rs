//! The ranking of the sidebar keeps the viewer's own team in the top eight (`ScoreboardController.getTeamScores`): a team
//! ninth in the ranking takes the eighth row, under its own place number, and the eighth team is left out. Expected lines
//! are written by hand from the original's template and its `Standing` formatting (`➤ ` marker, `N. ` place, the name).
use horizon_bingo::server::options::{Goal, Mode};
use horizon_bingo::server::sidebar::{english_lines, rows_for, Clock, Facts, Features, Phase, Standing};

fn team(number: i32) -> Standing {
    Standing {
        key: format!("bingo_t{number}"),
        name: format!("T{number}"),
        color: "red".to_string(),
        symbol: None,
        player: None,
        // Nine teams, the first with the most items, so the team number is its place.
        items: 100 - 10 * number,
        lines: 0,
        cards: 0,
        finished: None,
    }
}

fn live_round() -> Facts {
    Facts {
        phase: Phase::Live,
        mode: Mode::Standard,
        hidden_items: false,
        consume_items: false,
        replace_goals: false,
        features: Features::default(),
        goal: Goal::Lines(1),
        infinite: false,
        more_cards: None,
        multi_card: false,
        clock: Clock::Elapsed(0),
        teams: (1..=9).map(team).collect(),
        ready_up: None,
        countdown: None,
        end: None,
    }
}

#[test]
fn a_viewer_whose_team_is_ninth_sees_it_in_the_eighth_row_under_its_own_place() {
    let lines = english_lines(&rows_for(&live_round(), Some("bingo_t9")));
    // Four settings lines, the blank, then the ranking of eight rows.
    assert_eq!(lines.len(), 13);
    assert_eq!(lines[12], "\u{27A4} 9. T9");
    assert!(lines.iter().all(|line| !line.contains("8. T8")), "the eighth team is left out: {lines:?}");
    assert!(lines.contains(&"7. T7".to_string()));
}

#[test]
fn a_viewer_whose_team_is_already_in_the_top_eight_keeps_the_ranking_as_it_is() {
    let lines = english_lines(&rows_for(&live_round(), Some("bingo_t2")));
    assert_eq!(lines[5..13].to_vec(), (1..=8).map(|n| if n == 2 { "\u{27A4} 2. T2".to_string() } else { format!("{n}. T{n}") }).collect::<Vec<_>>());
}

#[test]
fn a_viewer_on_no_team_sees_the_first_eight_teams_only() {
    let lines = english_lines(&rows_for(&live_round(), None));
    assert_eq!(lines[12], "8. T8");
    assert!(lines.iter().all(|line| !line.contains("T9")));
}

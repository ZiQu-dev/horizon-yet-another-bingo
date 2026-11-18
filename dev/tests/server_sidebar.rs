//! The sidebar (`sidebar.rs`, the original's `ScoreboardController` and its `scoreboard*.json` templates, shown as one
//! sidebar for everyone): the lines of each phase, their text and colours, the score column, and the console commands
//! that change the server's sidebar, which must send only what changed. Expected values are written by hand from the
//! original's lang strings (`en_us.json`) and templates.
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::options::{Goal, Mode};
use horizon_bingo::server::sidebar::{
    component_json, english, end_message, rows, Clock, End, Facts, Features, Part, Phase, Row, Shown, Standing, MAX_ROWS,
};
use horizon_bingo::server::texts;
use horizon_bingo::server::types::Player;

#[derive(Default)]
struct FakeHost {
    commands: Vec<String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}
    fn tell(&mut self, _player: u128, _line: &str) {}
    fn broadcast(&mut self, _line: &str) {}
    fn players(&self) -> Vec<Player> {
        Vec::new()
    }
    fn item_numeric_id(&self, _item: &str) -> Option<i32> {
        None
    }
    fn command(&mut self, line: &str) {
        self.commands.push(line.to_string());
    }
}

fn team(key: &str, name: &str, color: &str, symbol: &str, items: i32, lines: i32) -> Standing {
    Standing {
        key: key.to_string(),
        name: name.to_string(),
        color: color.to_string(),
        symbol: (!symbol.is_empty()).then(|| symbol.to_string()),
        player: None,
        items,
        lines,
        cards: 0,
        finished: None,
    }
}

/// The facts of a pregame round with the original's defaults: standard mode, one line, PvP on, no time limit.
fn pregame() -> Facts {
    Facts {
        phase: Phase::Lobby,
        mode: Mode::Standard,
        hidden_items: false,
        consume_items: false,
        replace_goals: false,
        features: Features { pvp: true, ..Features::default() },
        goal: Goal::Lines(1),
        infinite: false,
        more_cards: None,
        multi_card: false,
        clock: Clock::NoLimit,
        teams: Vec::new(),
        ready_up: None,
        countdown: Some(texts::waiting_to_start()),
        end: None,
    }
}

fn live(teams: Vec<Standing>) -> Facts {
    Facts { phase: Phase::Live, clock: Clock::Elapsed(0), teams, countdown: None, ..pregame() }
}

fn text_of(facts: &Facts) -> Vec<String> {
    rows(facts).iter().map(|row| english(&row.text)).collect()
}

/// The score column of each row: `None` where the row shows none.
fn scores_of(facts: &Facts) -> Vec<Option<String>> {
    rows(facts).iter().map(|row| row.score.as_ref().map(|score| english(std::slice::from_ref(score)))).collect()
}

#[test]
fn the_pregame_shows_the_settings_the_time_and_the_waiting_line() {
    assert_eq!(
        text_of(&pregame()),
        ["Mode: Standard", "Features: \u{1F5E1}", "Goal: 1 line", "Time: 00m 00s", "", "Waiting to start..."]
    );
}

#[test]
fn the_pregame_with_a_time_limit_and_a_running_ready_timer_shows_both_lines() {
    let facts = Facts {
        clock: Clock::Limit(600_000),
        ready_up: Some(texts::players_ready(2, 3)),
        countdown: Some(texts::starting_in("1:30")),
        ..pregame()
    };
    assert_eq!(
        text_of(&facts),
        ["Mode: Standard", "Features: \u{1F5E1}", "Goal: 1 line", "Time left: 10m 00s", "", "2 of 3 ready!", "Starting in 1:30"]
    );
}

#[test]
fn the_mode_shows_icons_in_the_originals_order_and_standard_when_there_are_none() {
    let lockout = Facts { mode: Mode::Lockout, ..pregame() };
    assert_eq!(text_of(&lockout)[0], "Mode: \u{1F512}");
    let inventory = Facts { mode: Mode::Inventory, ..pregame() };
    assert_eq!(text_of(&inventory)[0], "Mode: \u{1F392}");
    let hidden_consume_replace = Facts { hidden_items: true, consume_items: true, replace_goals: true, ..pregame() };
    assert_eq!(text_of(&hidden_consume_replace)[0], "Mode: \u{2753} \u{2B1A} \u{2612}");
}

#[test]
fn the_features_show_their_icons_or_none() {
    let all = Facts {
        features: Features { elytra: true, night_vision: true, keep_inventory: true, pvp: true },
        ..pregame()
    };
    assert_eq!(text_of(&all)[1], "Features: \u{1F987} \u{1F441} \u{1F392} \u{1F5E1}");
    let none = Facts { features: Features::default(), ..pregame() };
    assert_eq!(text_of(&none)[1], "Features: None");
}

#[test]
fn the_goal_reads_as_the_original_says_it() {
    let goal = |goal: Goal, more: Option<i32>, infinite: bool| {
        text_of(&Facts { goal, more_cards: more, infinite, ..pregame() })[2].clone()
    };
    assert_eq!(goal(Goal::Lines(1), None, false), "Goal: 1 line");
    assert_eq!(goal(Goal::Lines(3), None, false), "Goal: 3 lines");
    assert_eq!(goal(Goal::Items(1), None, false), "Goal: 1 item");
    assert_eq!(goal(Goal::Items(5), None, false), "Goal: 5 items");
    assert_eq!(goal(Goal::FullCard, None, false), "Goal: Full Card");
    assert_eq!(goal(Goal::Lines(1), Some(2), false), "Goal: 1 line +2", "the cards still to play follow the goal");
    assert_eq!(goal(Goal::Lines(1), None, true), "Goal: 1 line + \u{221E}");
}

#[test]
fn the_goal_and_mode_values_are_yellow_and_the_labels_are_not() {
    let json = component_json(&rows(&pregame())[0].text);
    assert_eq!(
        json,
        r#"{"translate":"bingo.scoreboard.mode","fallback":"Mode: %s","with":[{"translate":"bingo.options.mode_standard","fallback":"Standard","color":"yellow"}]}"#
    );
}

#[test]
fn the_clock_reads_the_time_left_or_the_time_played_as_the_original_formats_it() {
    let left = |clock: Clock| text_of(&Facts { clock, ..live(Vec::new()) })[3].clone();
    assert_eq!(left(Clock::Left(299_000)), "Time left: 04m 59s");
    assert_eq!(left(Clock::Left(0)), "Time left: (Game Over)");
    assert_eq!(left(Clock::Elapsed(3_725_000)), "Time: 1h 02m 05s");
    assert_eq!(left(Clock::Elapsed(30_000)), "Time: 00m 30s");
}

#[test]
fn the_time_left_turns_red_on_even_seconds_in_the_last_thirty() {
    let red = |ms: i64| component_json(&rows(&Facts { clock: Clock::Left(ms), ..live(Vec::new()) })[3].text);
    assert!(red(30_000).contains(r#""color":"red""#), "30 s, even: red");
    assert!(red(29_000).contains(r#""color":"yellow""#), "29 s, odd: yellow");
    assert!(red(31_000).contains(r#""color":"yellow""#), "over 30 s: yellow");
}

#[test]
fn a_game_with_one_team_shows_the_teams_items_and_lines_in_its_colour() {
    let facts = live(vec![team("bingo_red", "Red", "red", "\u{1F525}", 3, 1)]);
    assert_eq!(
        text_of(&facts),
        ["Mode: Standard", "Features: \u{1F5E1}", "Goal: 1 line", "Time: 00m 00s", "", "Items scored: 3", "Lines scored: 1"]
    );
    let json = component_json(&rows(&facts)[5].text);
    assert!(json.contains(r#""color":"red""#), "the number takes the team's colour: {json}");
    assert!(scores_of(&facts).iter().all(Option::is_none), "a one-team sidebar shows no score column");
}

#[test]
fn a_multi_card_game_with_one_team_also_shows_the_cards() {
    let mut team = team("bingo_red", "Red", "red", "\u{1F525}", 3, 1);
    team.cards = 1;
    let facts = Facts { multi_card: true, ..live(vec![team]) };
    assert_eq!(text_of(&facts)[5..], ["Items scored: 3", "Lines scored: 1", "Cards scored: 1"]);
}

#[test]
fn a_game_with_several_teams_ranks_them_with_the_score_on_the_right() {
    let mut red = team("bingo_red", "Red", "red", "\u{1F525}", 4, 1);
    red.finished = Some(90_000);
    let blue = team("bingo_blue", "Blue", "blue", "\u{1F30A}", 0, 2);
    let green = team("bingo_green", "Green", "green", "\u{1F342}", 5, 0);
    let facts = live(vec![red, blue, green]);
    assert_eq!(
        text_of(&facts),
        [
            "Mode: Standard",
            "Features: \u{1F5E1}",
            "Goal: 1 line",
            "Time: 00m 00s",
            "",
            "1. \u{1F525} Red",
            "2. \u{1F30A} Blue",
            "3. \u{1F342} Green",
        ]
    );
    // A finished team shows its time; the others their lines (on a lines goal) or else their items.
    assert_eq!(
        scores_of(&facts)[5..],
        [Some("1:30".to_string()), Some("2 lines".to_string()), Some("5 items".to_string())]
    );
}

#[test]
fn teams_with_the_same_score_share_a_place_and_the_next_place_counts_one_on() {
    let aqua = team("bingo_aqua", "Aqua", "aqua", "\u{1F344}", 2, 0);
    let pink = team("bingo_pink", "Pink", "light_purple", "\u{1F33A}", 2, 0);
    let yellow = team("bingo_yellow", "Yellow", "yellow", "\u{1F41D}", 1, 0);
    let shown = text_of(&live(vec![aqua, pink, yellow]));
    assert_eq!(shown[5..], ["1. \u{1F344} Aqua", "1. \u{1F33A} Pink", "2. \u{1F41D} Yellow"]);
}

#[test]
fn a_one_player_team_is_named_by_its_player() {
    let mut red = team("bingo_red", "Red", "red", "\u{1F525}", 1, 0);
    red.player = Some("Alex".to_string());
    let blue = team("bingo_blue", "Blue", "blue", "\u{1F30A}", 0, 0);
    assert_eq!(text_of(&live(vec![red, blue]))[5..], ["1. Alex", "2. \u{1F30A} Blue"]);
}

#[test]
fn the_ranking_shows_at_most_eight_teams() {
    let teams: Vec<Standing> = (1..=10).map(|n| team(&format!("bingo_t{n}"), &format!("T{n}"), "gray", "", 10 - n, 0)).collect();
    let shown = text_of(&live(teams));
    assert_eq!(shown.len(), 5 + 8, "four header lines, a blank line, then eight teams");
    assert_eq!(shown.last().map(String::as_str), Some("8. T8"));
}

#[test]
fn the_end_shows_the_reason_the_winner_and_the_team_scores_of_one_team() {
    let red = team("bingo_red", "Red", "red", "\u{1F525}", 5, 1);
    let end = End {
        reason: texts::bingo_reason(),
        message: end_message(std::slice::from_ref(&red), Some("bingo_red"), true),
    };
    let facts = Facts { phase: Phase::Over, end: Some(end), teams: vec![red], ..pregame() };
    assert_eq!(
        text_of(&facts),
        ["BINGO!", "\u{1F525} Red completed their card!", "", "Items scored: 5", "Lines scored: 1"]
    );
}

#[test]
fn the_end_with_no_winner_names_the_leader_by_lines_then_draws_or_nothing() {
    let red = team("bingo_red", "Red", "red", "\u{1F525}", 3, 2);
    let blue = team("bingo_blue", "Blue", "blue", "\u{1F30A}", 9, 1);
    let teams = vec![red.clone(), blue.clone()];
    let with_leader = End { reason: texts::out_of_time(), message: end_message(&teams, None, true) };
    let facts = Facts { phase: Phase::Over, end: Some(with_leader), teams: teams.clone(), ..pregame() };
    assert_eq!(
        text_of(&facts),
        ["Out of time!", "\u{1F525} Red had the most lines!", "", "1. \u{1F525} Red", "2. \u{1F30A} Blue"]
    );

    let tied = vec![team("bingo_red", "Red", "red", "\u{1F525}", 2, 0), team("bingo_blue", "Blue", "blue", "\u{1F30A}", 2, 0)];
    let draw = End { reason: texts::out_of_time(), message: end_message(&tied, None, true) };
    let facts = Facts { phase: Phase::Over, end: Some(draw.clone()), teams: tied.clone(), ..pregame() };
    assert_eq!(text_of(&facts)[1], "The game is a draw!");
    let no_draw = End { reason: texts::out_of_time(), message: end_message(&tied, None, false) };
    assert_eq!(end_message(&tied, None, false), None, "no draw when the first win does not end the game");
    assert_eq!(no_draw.message, None);
}

#[test]
fn the_sidebar_never_has_more_rows_than_the_room_it_has() {
    // Twenty teams: the ranking is cut to eight, so the sidebar has 4 + 1 + 8 lines, within the room of 15.
    let teams: Vec<Standing> = (0..20).map(|n| team(&format!("bingo_x{n}"), "X", "gray", "", 1, 0)).collect();
    let facts = live(teams);
    assert_eq!(rows(&facts).len(), 13);
    assert!(rows(&facts).len() <= MAX_ROWS);
}

#[test]
fn a_new_sidebar_is_added_then_each_row_is_written_with_its_score() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    let ranked = Row { text: vec![Part::text("1. Red")], score: Some(Part::text("2 lines")) };
    shown.update(&mut host, Some(vec![Row { text: vec![Part::text("Mode: x")], score: None }, ranked]));
    assert_eq!(
        host.commands,
        [
            r#"scoreboard objectives add bingo_sidebar dummy {"translate":"bingo.scoreboard.name","fallback":"Minecraft Bingo"}"#,
            "scoreboard objectives modify bingo_sidebar numberformat blank",
            "scoreboard objectives setdisplay sidebar bingo_sidebar",
            "scoreboard players set bingo_row00 bingo_sidebar 15",
            r#"scoreboard players display name bingo_row00 bingo_sidebar {"text":"Mode: x"}"#,
            "scoreboard players set bingo_row01 bingo_sidebar 14",
            r#"scoreboard players display name bingo_row01 bingo_sidebar {"text":"1. Red"}"#,
            r#"scoreboard players display numberformat bingo_row01 bingo_sidebar fixed {"text":"2 lines"}"#,
        ]
    );
}

#[test]
fn only_the_rows_that_changed_are_sent_again() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    let row = |text: &str| Row { text: vec![Part::text(text)], score: None };
    shown.update(&mut host, Some(vec![row("A"), row("B")]));
    host.commands.clear();

    shown.update(&mut host, Some(vec![row("A"), row("B")]));
    assert!(host.commands.is_empty(), "the same lines send nothing");

    shown.update(&mut host, Some(vec![row("A"), row("C")]));
    assert_eq!(host.commands, [r#"scoreboard players display name bingo_row01 bingo_sidebar {"text":"C"}"#]);
    host.commands.clear();

    shown.update(&mut host, Some(vec![row("A")]));
    assert_eq!(host.commands, ["scoreboard players reset bingo_row01 bingo_sidebar"]);
}

#[test]
fn a_changed_score_alone_is_sent_as_the_number_format_and_a_removed_one_is_cleared() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    let with = |score: Option<&str>| Row { text: vec![Part::text("1. Red")], score: score.map(Part::text) };
    shown.update(&mut host, Some(vec![with(Some("1 line"))]));
    host.commands.clear();

    shown.update(&mut host, Some(vec![with(Some("2 lines"))]));
    assert_eq!(
        host.commands,
        [r#"scoreboard players display numberformat bingo_row00 bingo_sidebar fixed {"text":"2 lines"}"#]
    );
    host.commands.clear();

    shown.update(&mut host, Some(vec![with(None)]));
    assert_eq!(host.commands, ["scoreboard players display numberformat bingo_row00 bingo_sidebar"]);
}

#[test]
fn a_text_is_quoted_as_json_so_quotes_and_backslashes_cannot_break_the_command() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec![Row { text: vec![Part::text(r#"Say "hi" \ x"#)], score: None }]));
    assert_eq!(
        host.commands.last().map(String::as_str),
        Some(r#"scoreboard players display name bingo_row00 bingo_sidebar {"text":"Say \"hi\" \\ x"}"#)
    );
}

#[test]
fn a_blank_row_is_a_space_so_its_name_is_never_empty() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec![Row::default()]));
    assert_eq!(host.commands.last().map(String::as_str), Some(r#"scoreboard players display name bingo_row00 bingo_sidebar {"text":" "}"#));
}

#[test]
fn taking_the_sidebar_away_clears_it_once() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec![Row::default()]));
    host.commands.clear();

    shown.update(&mut host, None);
    assert_eq!(host.commands, ["scoreboard objectives setdisplay sidebar", "scoreboard objectives remove bingo_sidebar"]);
    host.commands.clear();

    shown.update(&mut host, None);
    assert!(host.commands.is_empty(), "nothing shown, nothing to take away");
}

#[test]
fn a_sidebar_with_more_rows_than_the_room_sends_only_the_room() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    let rows: Vec<Row> = (0..20).map(|n| Row { text: vec![Part::text(format!("row {n}"))], score: None }).collect();
    shown.update(&mut host, Some(rows));
    assert!(host.commands.iter().any(|line| line.contains("bingo_row14")));
    assert!(!host.commands.iter().any(|line| line.contains("bingo_row15")));
}

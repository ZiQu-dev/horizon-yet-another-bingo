//! The sidebar (`sidebar.rs`, the original's `ScoreboardController` and its `scoreboard*.json` templates, shown as one
//! sidebar for everyone): the lines of each phase, the clock, the ranking, and the console commands that change the
//! server's sidebar, which must send only what changed. Expected values are written by hand.
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::options::{Goal, Mode};
use horizon_bingo::server::sidebar::{format_clock, goal_label, rows, Clock, Facts, Phase, Shown, Standing, MAX_ROWS};
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

fn standing(name: &str, lines: i32, items: i32) -> Standing {
    Standing { name: name.to_string(), lines, items }
}

fn live(clock: Clock, ranking: Vec<Standing>) -> Facts {
    Facts { phase: Phase::Live, mode: Mode::Standard, goal: Goal::Lines(1), clock, teams: Vec::new(), ranking }
}

#[test]
fn the_clock_shows_hours_minutes_and_seconds_and_a_negative_time_as_zero() {
    assert_eq!(format_clock(0), "00:00:00");
    assert_eq!(format_clock(59_999), "00:00:59");
    assert_eq!(format_clock(3_725_000), "01:02:05");
    assert_eq!(format_clock(-5), "00:00:00");
}

#[test]
fn a_goal_reads_as_the_original_says_it() {
    assert_eq!(goal_label(Goal::Lines(1)), "1 line");
    assert_eq!(goal_label(Goal::Lines(3)), "3 lines");
    assert_eq!(goal_label(Goal::Items(1)), "1 item");
    assert_eq!(goal_label(Goal::Items(5)), "5 items");
    assert_eq!(goal_label(Goal::FullCard), "full card");
}

#[test]
fn the_lobby_shows_the_settings_the_time_limit_and_the_teams() {
    let facts = Facts {
        phase: Phase::Lobby,
        mode: Mode::Lockout,
        goal: Goal::Items(5),
        clock: Clock::Limit(10),
        teams: vec![("Red".to_string(), 2), ("Blue".to_string(), 1)],
        ranking: Vec::new(),
    };
    assert_eq!(
        rows(&facts),
        ["Mode: Lockout", "Goal: 5 items", "Time limit: 10 min", "", "Red: 2 players", "Blue: 1 player"]
    );
}

#[test]
fn the_lobby_without_a_limit_says_so() {
    let facts = Facts { phase: Phase::Lobby, mode: Mode::Standard, goal: Goal::Lines(1), clock: Clock::NoLimit, teams: Vec::new(), ranking: Vec::new() };
    assert_eq!(rows(&facts), ["Mode: Standard", "Goal: 1 line", "Time limit: none", ""]);
}

#[test]
fn a_game_with_a_limit_shows_the_time_left_and_the_top_teams() {
    let facts = live(Clock::Left(299_000), vec![standing("Red", 3, 7), standing("Blue", 0, 2)]);
    assert_eq!(
        rows(&facts),
        ["Mode: Standard", "Goal: 1 line", "Time left: 00:04:59", "", "1. Red: 3 lines, 7 items", "2. Blue: 0 lines, 2 items"]
    );
}

#[test]
fn a_game_without_a_limit_shows_the_time_played() {
    let facts = live(Clock::Elapsed(3_725_000), Vec::new());
    assert_eq!(rows(&facts), ["Mode: Standard", "Goal: 1 line", "Time: 01:02:05", ""]);
}

#[test]
fn the_ranking_uses_the_singular_for_one() {
    let shown = rows(&live(Clock::Elapsed(0), vec![standing("Red", 1, 1), standing("Blue", 2, 0)]));
    assert_eq!(&shown[4..], ["1. Red: 1 line, 1 item", "2. Blue: 2 lines, 0 items"]);
}

#[test]
fn the_ranking_shows_at_most_eight_teams() {
    let ranking: Vec<Standing> = (1..=10).map(|n| standing(&format!("T{n}"), 0, n)).collect();
    let shown = rows(&live(Clock::Elapsed(0), ranking));
    assert_eq!(shown.len(), 4 + 8);
    assert_eq!(shown.last().map(String::as_str), Some("8. T8: 0 lines, 8 items"));
}

#[test]
fn the_end_shows_game_over_the_duration_and_the_ranking() {
    let facts = Facts {
        phase: Phase::Over,
        mode: Mode::Standard,
        goal: Goal::Lines(1),
        clock: Clock::Elapsed(9_000),
        teams: Vec::new(),
        ranking: vec![standing("Blue", 1, 5)],
    };
    assert_eq!(rows(&facts), ["Game over!", "Time: 00:00:09", "", "1. Blue: 1 line, 5 items"]);
}

#[test]
fn the_sidebar_never_has_more_rows_than_the_room_it_has() {
    let teams: Vec<(String, usize)> = (0..20).map(|n| (format!("Team{n}"), 1)).collect();
    let facts = Facts { phase: Phase::Lobby, mode: Mode::Standard, goal: Goal::Lines(1), clock: Clock::NoLimit, teams, ranking: Vec::new() };
    assert_eq!(rows(&facts).len(), MAX_ROWS);
}

#[test]
fn a_new_sidebar_is_added_then_each_row_is_written_with_its_score() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec!["Mode: Standard".to_string(), String::new()]));
    assert_eq!(
        host.commands,
        [
            "scoreboard objectives add bingo_sidebar dummy \"Minecraft Bingo\"",
            "scoreboard objectives modify bingo_sidebar numberformat blank",
            "scoreboard objectives setdisplay sidebar bingo_sidebar",
            "scoreboard players set bingo_row00 bingo_sidebar 15",
            "scoreboard players display name bingo_row00 bingo_sidebar \"Mode: Standard\"",
            "scoreboard players set bingo_row01 bingo_sidebar 14",
            "scoreboard players display name bingo_row01 bingo_sidebar \" \"",
        ]
    );
}

#[test]
fn only_the_rows_that_changed_are_sent_again() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec!["A".to_string(), "B".to_string()]));
    host.commands.clear();

    shown.update(&mut host, Some(vec!["A".to_string(), "B".to_string()]));
    assert!(host.commands.is_empty(), "the same lines send nothing");

    shown.update(&mut host, Some(vec!["A".to_string(), "C".to_string()]));
    assert_eq!(host.commands, ["scoreboard players display name bingo_row01 bingo_sidebar \"C\""]);
    host.commands.clear();

    shown.update(&mut host, Some(vec!["A".to_string()]));
    assert_eq!(host.commands, ["scoreboard players reset bingo_row01 bingo_sidebar"]);
    host.commands.clear();

    shown.update(&mut host, Some(vec!["A".to_string(), "D".to_string(), "E".to_string()]));
    assert_eq!(
        host.commands,
        [
            "scoreboard players set bingo_row01 bingo_sidebar 14",
            "scoreboard players display name bingo_row01 bingo_sidebar \"D\"",
            "scoreboard players set bingo_row02 bingo_sidebar 13",
            "scoreboard players display name bingo_row02 bingo_sidebar \"E\"",
        ]
    );
}

#[test]
fn a_row_text_is_quoted_so_quotes_and_backslashes_cannot_break_the_command() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec![r#"Say "hi" \ x"#.to_string()]));
    assert_eq!(host.commands.last().map(String::as_str), Some(r#"scoreboard players display name bingo_row00 bingo_sidebar "Say \"hi\" \\ x""#));
}

#[test]
fn taking_the_sidebar_away_clears_it_once() {
    let mut host = FakeHost::default();
    let mut shown = Shown::new();
    shown.update(&mut host, Some(vec!["A".to_string()]));
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
    let rows: Vec<String> = (0..20).map(|n| format!("row {n}")).collect();
    shown.update(&mut host, Some(rows));
    assert!(host.commands.iter().any(|line| line.contains("bingo_row14")));
    assert!(!host.commands.iter().any(|line| line.contains("bingo_row15")));
}

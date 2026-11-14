//! Chat command parsing. Expected values are written by hand from the spec of the original mod's commands.
use horizon_bingo::server::commands::{parse, requires_op, usage, Command};

#[test]
fn every_command_parses_to_its_variant() {
    assert_eq!(parse("help"), Ok(Command::Help));
    assert_eq!(parse("status"), Ok(Command::Status));
    assert_eq!(parse("start"), Ok(Command::Start { ignore_warnings: false }));
    assert_eq!(parse("start ignore_warnings"), Ok(Command::Start { ignore_warnings: true }));
    assert_eq!(parse("end"), Ok(Command::End));
    assert_eq!(parse("reset"), Ok(Command::Reset));
    assert_eq!(parse("shuffleteams 3"), Ok(Command::ShuffleTeams(3)));
    assert_eq!(parse("join red"), Ok(Command::Join("red".to_string())));
    assert_eq!(parse("spectator"), Ok(Command::Spectator));
    assert_eq!(parse("ready"), Ok(Command::Ready));
    assert_eq!(parse("ready cancel"), Ok(Command::ReadyCancel));
    assert_eq!(parse("timelimit 30"), Ok(Command::TimeLimit(Some(30))));
    assert_eq!(parse("timelimit off"), Ok(Command::TimeLimit(None)));
    assert_eq!(parse("card"), Ok(Command::Card));
}

#[test]
fn empty_and_blank_lines_are_help() {
    assert_eq!(parse(""), Ok(Command::Help));
    assert_eq!(parse("   "), Ok(Command::Help));
    assert_eq!(parse("\t"), Ok(Command::Help));
}

#[test]
fn extra_spaces_and_tabs_are_ignored() {
    assert_eq!(parse("  start   ignore_warnings  "), Ok(Command::Start { ignore_warnings: true }));
    assert_eq!(parse("shuffleteams\t 4 "), Ok(Command::ShuffleTeams(4)));
    assert_eq!(parse("join   bingo_red"), Ok(Command::Join("bingo_red".to_string())));
    assert_eq!(parse("timelimit  \t 5"), Ok(Command::TimeLimit(Some(5))));
    assert_eq!(parse("  ready   cancel "), Ok(Command::ReadyCancel));
}

#[test]
fn command_and_keywords_ignore_case_but_team_names_do_not() {
    assert_eq!(parse("START IGNORE_WARNINGS"), Ok(Command::Start { ignore_warnings: true }));
    assert_eq!(parse("Shuffleteams 2"), Ok(Command::ShuffleTeams(2)));
    assert_eq!(parse("Timelimit OFF"), Ok(Command::TimeLimit(None)));
    assert_eq!(parse("Ready CANCEL"), Ok(Command::ReadyCancel));
    assert_eq!(parse("STATUS"), Ok(Command::Status));
    assert_eq!(parse("JOIN RED"), Ok(Command::Join("RED".to_string())));
    assert_eq!(parse("join Red"), Ok(Command::Join("Red".to_string())));
    assert_eq!(parse("join bingo_red"), Ok(Command::Join("bingo_red".to_string())));
}

#[test]
fn shuffleteams_accepts_one_and_rejects_zero_and_below() {
    assert_eq!(parse("shuffleteams 1"), Ok(Command::ShuffleTeams(1)));
    assert!(parse("shuffleteams 0").unwrap_err().starts_with("shuffleteams needs a whole number of at least 1"));
    assert!(parse("shuffleteams -1").unwrap_err().starts_with("shuffleteams needs"));
}

#[test]
fn shuffleteams_rejects_non_numbers_fractions_and_overflow() {
    assert!(parse("shuffleteams two").unwrap_err().starts_with("shuffleteams needs"));
    assert!(parse("shuffleteams 2.5").unwrap_err().starts_with("shuffleteams needs"));
    assert!(parse("shuffleteams 99999999999999999999999999").unwrap_err().starts_with("shuffleteams needs"));
}

#[test]
fn shuffleteams_without_count_shows_usage() {
    assert!(parse("shuffleteams").unwrap_err().starts_with("Usage: shuffleteams <n>"));
}

#[test]
fn timelimit_accepts_one_minute_and_rejects_zero_and_below() {
    assert_eq!(parse("timelimit 1"), Ok(Command::TimeLimit(Some(1))));
    assert!(parse("timelimit 0").unwrap_err().starts_with("timelimit needs a number of minutes of at least 1"));
    assert!(parse("timelimit -5").unwrap_err().starts_with("timelimit needs"));
}

#[test]
fn timelimit_rejects_words_fractions_and_near_misses_of_off() {
    assert!(parse("timelimit abc").unwrap_err().starts_with("timelimit needs"));
    assert!(parse("timelimit 1.5").unwrap_err().starts_with("timelimit needs"));
    assert!(parse("timelimit offf").unwrap_err().starts_with("timelimit needs"));
    assert!(parse("timelimit 4294967296").unwrap_err().starts_with("timelimit needs"));
}

#[test]
fn timelimit_without_value_shows_usage() {
    assert!(parse("timelimit").unwrap_err().starts_with("Usage: timelimit <minutes|off>"));
}

#[test]
fn join_without_team_shows_usage_and_extra_words_are_rejected() {
    assert!(parse("join").unwrap_err().starts_with("Usage: join <team>"));
    assert!(parse("join red blue").unwrap_err().starts_with("Too many arguments for 'join'"));
}

#[test]
fn start_rejects_unknown_option_and_extra_words() {
    assert!(parse("start now").unwrap_err().starts_with("Unknown option 'now' for start"));
    assert!(parse("start ignore_warnings_").unwrap_err().starts_with("Unknown option 'ignore_warnings_' for start"));
    assert!(parse("start ignore_warnings extra").unwrap_err().starts_with("Too many arguments for 'start'"));
}

#[test]
fn ready_rejects_unknown_option_and_extra_words() {
    assert!(parse("ready later").unwrap_err().starts_with("Unknown option 'later' for ready"));
    assert!(parse("ready cancel now").unwrap_err().starts_with("Too many arguments for 'ready'"));
}

#[test]
fn commands_without_arguments_reject_any_word() {
    for line in ["help x", "status all", "end now", "reset now", "spectator red", "card set"] {
        let name = line.split(' ').next().unwrap();
        assert_eq!(parse(line), Err(format!("'{name}' takes no arguments.")), "line: {line}");
    }
}

#[test]
fn unknown_command_lists_the_allowed_ones() {
    for line in ["foo", "stat", "startt", "endgame", "bingo"] {
        let err = parse(line).unwrap_err();
        assert!(err.starts_with("Unknown command '"), "line: {line}, got: {err}");
        assert!(err.contains(&usage()), "line: {line}, got: {err}");
    }
}

#[test]
fn requires_op_for_each_variant() {
    let table = [
        (Command::Help, false),
        (Command::Status, false),
        (Command::Start { ignore_warnings: false }, true),
        (Command::Start { ignore_warnings: true }, true),
        (Command::End, true),
        (Command::Reset, true),
        (Command::ShuffleTeams(2), true),
        (Command::Join("red".to_string()), false),
        (Command::Spectator, false),
        (Command::Ready, false),
        (Command::ReadyCancel, true),
        (Command::TimeLimit(Some(5)), true),
        (Command::TimeLimit(None), true),
        (Command::Card, false),
    ];
    for (command, expected) in table {
        assert_eq!(requires_op(&command), expected, "command: {command:?}");
    }
}

#[test]
fn requires_op_follows_the_parsed_line() {
    assert!(requires_op(&parse("start").unwrap()));
    assert!(requires_op(&parse("ready cancel").unwrap()));
    assert!(!requires_op(&parse("ready").unwrap()));
    assert!(!requires_op(&parse("join red").unwrap()));
}

#[test]
fn usage_names_every_command_and_each_one_parses() {
    let text = usage();
    assert!(!text.contains('\n'));
    let examples = [
        ("start", "start"),
        ("end", "end"),
        ("reset", "reset"),
        ("shuffleteams <n>", "shuffleteams 1"),
        ("join <team>", "join red"),
        ("spectator", "spectator"),
        ("ready [cancel]", "ready"),
        ("timelimit <minutes|off>", "timelimit off"),
        ("card", "card"),
        ("status", "status"),
    ];
    for (shown, line) in examples {
        assert!(text.contains(shown), "usage lacks '{shown}': {text}");
        assert!(parse(line).is_ok(), "usage example '{line}' does not parse");
    }
}

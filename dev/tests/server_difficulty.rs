//! `bingo difficulty`, `reroll`, `teams`, `autoshuffle`: the parsing, the operator rule, the usage text and the saved
//! form of the new round options. Expected counts are the original's (TierLabel.DIFFICULTY_PRESETS, BingoConfig).
use horizon_bingo::server::commands::{parse, requires_op, usage, Command};
use horizon_bingo::server::options::{Options, Toggle};
use horizon_bingo::server::store::{options_from_json, options_to_json};
use serde_json::Value;

#[test]
fn each_preset_name_gives_the_original_counts_in_any_case() {
    assert_eq!(parse("difficulty easy"), Ok(Command::Difficulty([0, 0, 0, 9, 16])));
    assert_eq!(parse("difficulty Medium"), Ok(Command::Difficulty([0, 0, 3, 10, 12])));
    assert_eq!(parse("difficulty hard"), Ok(Command::Difficulty([0, 1, 5, 10, 9])));
    assert_eq!(parse("difficulty EXTREME"), Ok(Command::Difficulty([0, 5, 7, 7, 6])));
    assert_eq!(parse("difficulty impossible"), Ok(Command::Difficulty([5, 7, 6, 5, 2])));
}

#[test]
fn five_counts_summing_to_25_are_a_custom_difficulty() {
    assert_eq!(parse("difficulty 0 0 0 9 16"), Ok(Command::Difficulty([0, 0, 0, 9, 16])));
    assert_eq!(parse("difficulty 25 0 0 0 0"), Ok(Command::Difficulty([25, 0, 0, 0, 0])));
    assert_eq!(parse("difficulty 1 2 3 4 15"), Ok(Command::Difficulty([1, 2, 3, 4, 15])));
}

#[test]
fn custom_counts_must_add_up_to_25_and_each_be_0_to_25() {
    assert_eq!(parse("difficulty 0 0 0 9 15").unwrap_err(), "difficulty counts must add up to 25 (the squares of a card), got 24");
    assert_eq!(parse("difficulty 0 0 0 9 17").unwrap_err(), "difficulty counts must add up to 25 (the squares of a card), got 26");
    assert_eq!(parse("difficulty 0 0 0 9 26").unwrap_err(), "difficulty D needs a whole number from 0 to 25, got '26'");
    assert_eq!(parse("difficulty -1 0 0 9 17").unwrap_err(), "difficulty S needs a whole number from 0 to 25, got '-1'");
    assert_eq!(parse("difficulty 0 x 0 9 16").unwrap_err(), "difficulty A needs a whole number from 0 to 25, got 'x'");
}

#[test]
fn wrong_number_of_arguments_or_unknown_name_is_an_error() {
    assert!(parse("difficulty").unwrap_err().starts_with("Usage: difficulty"));
    assert!(parse("difficulty hardest").unwrap_err().starts_with("Unknown difficulty 'hardest'."));
    assert_eq!(
        parse("difficulty 1 2 3").unwrap_err(),
        "difficulty needs one preset name or five counts, got 3 arguments. Usage: difficulty easy|medium|hard|extreme|impossible, or difficulty <S> <A> <B> <C> <D>"
    );
}

#[test]
fn reroll_takes_no_arguments_and_teams_takes_1_to_8() {
    assert_eq!(parse("reroll"), Ok(Command::Reroll));
    assert_eq!(parse("reroll now").unwrap_err(), "'reroll' takes no arguments.");
    assert_eq!(parse("teams 1"), Ok(Command::TeamCount(1)));
    assert_eq!(parse("teams 8"), Ok(Command::TeamCount(8)));
    assert_eq!(parse("teams 0").unwrap_err(), "teams needs a whole number of teams from 1 to 8, got '0'");
    assert_eq!(parse("teams 9").unwrap_err(), "teams needs a whole number of teams from 1 to 8, got '9'");
    assert_eq!(parse("teams two").unwrap_err(), "teams needs a whole number of teams from 1 to 8, got 'two'");
    assert_eq!(parse("teams").unwrap_err(), "Usage: teams <n> (1 to 8)");
}

#[test]
fn autoshuffle_is_an_on_off_toggle() {
    assert_eq!(parse("autoshuffle on"), Ok(Command::Toggle(Toggle::AutoShuffle, true)));
    assert_eq!(parse("autoshuffle OFF"), Ok(Command::Toggle(Toggle::AutoShuffle, false)));
    assert_eq!(parse("autoshuffle maybe").unwrap_err(), "autoshuffle needs on or off, got 'maybe'");
    assert_eq!(parse("autoshuffle").unwrap_err(), "Usage: autoshuffle on|off");
}

#[test]
fn the_new_commands_need_an_operator_and_are_in_the_usage_text() {
    for line in ["difficulty hard", "difficulty 0 0 0 9 16", "reroll", "teams 4", "autoshuffle on"] {
        assert!(requires_op(&parse(line).unwrap()), "{line} needs an operator");
    }
    let text = usage();
    assert!(text.contains("difficulty easy|medium|hard|extreme|impossible"), "{text}");
    assert!(text.contains("difficulty <S> <A> <B> <C> <D>"), "{text}");
    assert!(text.contains("reroll"), "{text}");
    assert!(text.contains("teams <n>"), "{text}");
    assert!(text.contains("autoshuffle on|off"), "{text}");
}

fn keys(json: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(json).expect("options are JSON");
    value.as_object().expect("an object").keys().cloned().collect()
}

#[test]
fn default_options_write_none_of_the_new_keys() {
    let text = options_to_json(&Options::default());
    let keys = keys(&text);
    for key in ["tierCounts", "teamAutoShuffle", "teamCount"] {
        assert!(!keys.iter().any(|k| k == key), "{key} must be absent by default: {text}");
    }
}

#[test]
fn non_default_difficulty_and_team_settings_are_written_and_read_back() {
    let options = Options {
        tier_counts: [5, 7, 6, 5, 2],
        team_auto_shuffle: true,
        team_count: 5,
        ..Options::default()
    };
    let text = options_to_json(&options);
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["tierCounts"], serde_json::json!([5, 7, 6, 5, 2]));
    assert_eq!(value["teamAutoShuffle"], Value::Bool(true));
    assert_eq!(value["teamCount"], Value::from(5));
    assert_eq!(options_from_json(&text), options);
}

#[test]
fn saved_tier_counts_that_are_not_25_or_out_of_range_fall_back_to_easy() {
    let base = |extra: &str| format!("{{{extra}}}");
    let easy = Options::default().tier_counts;
    assert_eq!(options_from_json(&base(r#""tierCounts": [0,0,0,9,15]"#)).tier_counts, easy);
    assert_eq!(options_from_json(&base(r#""tierCounts": [0,0,0,9]"#)).tier_counts, easy);
    assert_eq!(options_from_json(&base(r#""tierCounts": [0,0,0,9,26]"#)).tier_counts, easy);
    assert_eq!(options_from_json(&base(r#""tierCounts": [0,0,3,10,12]"#)).tier_counts, [0, 0, 3, 10, 12]);
}

#[test]
fn saved_team_count_outside_1_to_8_falls_back_to_three() {
    assert_eq!(options_from_json(r#"{"teamCount": 9}"#).team_count, 3);
    assert_eq!(options_from_json(r#"{"teamCount": 0}"#).team_count, 3);
    assert_eq!(options_from_json(r#"{"teamCount": 8}"#).team_count, 8);
}

#[test]
fn toggle_autoshuffle_sets_the_option() {
    let mut options = Options::default();
    options.set_toggle(Toggle::AutoShuffle, true);
    assert!(options.team_auto_shuffle);
    options.set_toggle(Toggle::AutoShuffle, false);
    assert!(!options.team_auto_shuffle);
}

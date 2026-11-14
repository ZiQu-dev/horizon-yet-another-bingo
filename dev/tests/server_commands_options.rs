//! Chat command parsing of the round options and votes. Expected values are written by hand from the command list.
use horizon_bingo::server::commands::{parse, requires_op, usage, Command, VoteChoice};
use horizon_bingo::server::options::{EndWhen, Goal, Mode, Toggle};

#[test]
fn goal_commands_parse_to_their_variant() {
    assert_eq!(parse("goal lines 3"), Ok(Command::Goal(Goal::Lines(3))));
    assert_eq!(parse("goal items 12"), Ok(Command::Goal(Goal::Items(12))));
    assert_eq!(parse("goal full_card"), Ok(Command::Goal(Goal::FullCard)));
}

#[test]
fn goal_counts_accept_one_and_reject_zero() {
    assert_eq!(parse("goal lines 1"), Ok(Command::Goal(Goal::Lines(1))));
    assert_eq!(parse("goal items 1"), Ok(Command::Goal(Goal::Items(1))));
    assert_eq!(parse("goal lines 0").unwrap_err(), "goal lines needs a number of lines of at least 1, got '0'");
    assert_eq!(parse("goal items 0").unwrap_err(), "goal items needs a number of items of at least 1, got '0'");
}

#[test]
fn goal_counts_reject_non_numbers_negatives_and_overflow() {
    assert!(parse("goal lines two").unwrap_err().starts_with("goal lines needs"));
    assert!(parse("goal items -1").unwrap_err().starts_with("goal items needs"));
    assert!(parse("goal items 2.5").unwrap_err().starts_with("goal items needs"));
    assert!(parse("goal lines 4294967296").unwrap_err().starts_with("goal lines needs"));
}

#[test]
fn goal_without_argument_or_kind_shows_usage() {
    assert_eq!(parse("goal").unwrap_err(), "Usage: goal lines <n>|items <n>|full_card");
    assert_eq!(parse("goal lines").unwrap_err(), "Usage: goal lines <n>");
    assert_eq!(parse("goal items").unwrap_err(), "Usage: goal items <n>");
}

#[test]
fn goal_rejects_unknown_kind_and_extra_words() {
    assert_eq!(parse("goal row 3").unwrap_err(), "Unknown goal 'row'. Usage: goal lines <n>|items <n>|full_card");
    assert_eq!(parse("goal lines 3 4").unwrap_err(), "Too many arguments for 'goal lines'. Usage: goal lines <n>");
    assert_eq!(parse("goal full_card 25").unwrap_err(), "Too many arguments for 'goal full_card'. Usage: goal full_card");
}

#[test]
fn mode_commands_parse_to_their_variant() {
    assert_eq!(parse("mode standard"), Ok(Command::Mode(Mode::Standard)));
    assert_eq!(parse("mode inventory"), Ok(Command::Mode(Mode::Inventory)));
    assert_eq!(parse("mode lockout"), Ok(Command::Mode(Mode::Lockout)));
}

#[test]
fn mode_rejects_unknown_words_and_missing_or_extra_arguments() {
    assert_eq!(parse("mode").unwrap_err(), "Usage: mode standard|inventory|lockout");
    assert_eq!(parse("mode hidden").unwrap_err(), "Unknown mode 'hidden'. Usage: mode standard|inventory|lockout");
    assert_eq!(parse("mode lockout now").unwrap_err(), "Too many arguments for 'mode'. Usage: mode standard|inventory|lockout");
}

#[test]
fn endwhen_commands_parse_to_their_variant() {
    assert_eq!(parse("endwhen never"), Ok(Command::EndWhen(EndWhen::Never)));
    assert_eq!(parse("endwhen first_win"), Ok(Command::EndWhen(EndWhen::FirstWin)));
    assert_eq!(parse("endwhen all_win"), Ok(Command::EndWhen(EndWhen::AllWin)));
    assert_eq!(parse("endwhen teams_win 2"), Ok(Command::EndWhen(EndWhen::TeamsWin(2))));
}

#[test]
fn endwhen_teams_win_accepts_one_and_rejects_zero() {
    assert_eq!(parse("endwhen teams_win 1"), Ok(Command::EndWhen(EndWhen::TeamsWin(1))));
    assert_eq!(
        parse("endwhen teams_win 0").unwrap_err(),
        "endwhen teams_win needs a number of teams of at least 1, got '0'"
    );
    assert!(parse("endwhen teams_win many").unwrap_err().starts_with("endwhen teams_win needs"));
}

#[test]
fn endwhen_teams_win_without_count_shows_usage() {
    assert_eq!(parse("endwhen teams_win").unwrap_err(), "Usage: endwhen teams_win <n>");
    assert_eq!(parse("endwhen").unwrap_err(), "Usage: endwhen never|first_win|all_win|teams_win <n>");
}

#[test]
fn endwhen_rejects_unknown_words_and_extra_arguments() {
    assert_eq!(
        parse("endwhen sometimes").unwrap_err(),
        "Unknown endwhen 'sometimes'. Usage: endwhen never|first_win|all_win|teams_win <n>"
    );
    assert_eq!(
        parse("endwhen never 1").unwrap_err(),
        "Unknown endwhen 'never'. Usage: endwhen never|first_win|all_win|teams_win <n>"
    );
    assert_eq!(
        parse("endwhen teams_win 2 3").unwrap_err(),
        "Too many arguments for 'endwhen'. Usage: endwhen never|first_win|all_win|teams_win <n>"
    );
}

#[test]
fn toggles_parse_with_on_and_off() {
    assert_eq!(parse("pvp on"), Ok(Command::Toggle(Toggle::Pvp, true)));
    assert_eq!(parse("pvp off"), Ok(Command::Toggle(Toggle::Pvp, false)));
    assert_eq!(parse("keepinventory on"), Ok(Command::Toggle(Toggle::KeepInventory, true)));
    assert_eq!(parse("keepinventory off"), Ok(Command::Toggle(Toggle::KeepInventory, false)));
    assert_eq!(parse("elytra on"), Ok(Command::Toggle(Toggle::Elytra, true)));
    assert_eq!(parse("elytra off"), Ok(Command::Toggle(Toggle::Elytra, false)));
    assert_eq!(parse("nightvision on"), Ok(Command::Toggle(Toggle::NightVision, true)));
    assert_eq!(parse("nightvision off"), Ok(Command::Toggle(Toggle::NightVision, false)));
}

#[test]
fn toggles_accept_true_and_false_as_on_and_off() {
    assert_eq!(parse("pvp true"), Ok(Command::Toggle(Toggle::Pvp, true)));
    assert_eq!(parse("pvp false"), Ok(Command::Toggle(Toggle::Pvp, false)));
    assert_eq!(parse("keepinventory true"), Ok(Command::Toggle(Toggle::KeepInventory, true)));
    assert_eq!(parse("elytra false"), Ok(Command::Toggle(Toggle::Elytra, false)));
    assert_eq!(parse("nightvision TRUE"), Ok(Command::Toggle(Toggle::NightVision, true)));
}

#[test]
fn toggle_words_ignore_case() {
    assert_eq!(parse("PVP OFF"), Ok(Command::Toggle(Toggle::Pvp, false)));
    assert_eq!(parse("KeepInventory On"), Ok(Command::Toggle(Toggle::KeepInventory, true)));
    assert_eq!(parse("Elytra oFf"), Ok(Command::Toggle(Toggle::Elytra, false)));
    assert_eq!(parse("NIGHTVISION ON"), Ok(Command::Toggle(Toggle::NightVision, true)));
}

#[test]
fn toggle_rejects_other_values_and_wrong_argument_counts() {
    assert_eq!(parse("pvp").unwrap_err(), "Usage: pvp on|off");
    assert_eq!(parse("pvp maybe").unwrap_err(), "pvp needs on or off, got 'maybe'");
    assert_eq!(parse("pvp o").unwrap_err(), "pvp needs on or off, got 'o'");
    assert_eq!(parse("pvp 1").unwrap_err(), "pvp needs on or off, got '1'");
    assert_eq!(parse("keepinventory").unwrap_err(), "Usage: keepinventory on|off");
    assert_eq!(parse("elytra on off").unwrap_err(), "Too many arguments for 'elytra'. Usage: elytra on|off");
    assert_eq!(parse("nightvision").unwrap_err(), "Usage: nightvision on|off");
}

#[test]
fn spawndistance_accepts_the_range_bounds() {
    assert_eq!(parse("spawndistance 1"), Ok(Command::SpawnDistance(1)));
    assert_eq!(parse("spawndistance 25"), Ok(Command::SpawnDistance(25)));
    assert_eq!(parse("spawndistance 1000"), Ok(Command::SpawnDistance(1000)));
}

#[test]
fn spawndistance_rejects_zero_and_above_the_maximum() {
    assert_eq!(
        parse("spawndistance 0").unwrap_err(),
        "spawndistance needs a whole number of chunks from 1 to 1000, got '0'"
    );
    assert_eq!(
        parse("spawndistance 1001").unwrap_err(),
        "spawndistance needs a whole number of chunks from 1 to 1000, got '1001'"
    );
    assert!(parse("spawndistance -3").unwrap_err().starts_with("spawndistance needs"));
    assert!(parse("spawndistance far").unwrap_err().starts_with("spawndistance needs"));
    assert!(parse("spawndistance 1.5").unwrap_err().starts_with("spawndistance needs"));
}

#[test]
fn spawndistance_without_or_with_extra_arguments_shows_usage() {
    assert_eq!(parse("spawndistance").unwrap_err(), "Usage: spawndistance <chunks> (1 to 1000)");
    assert_eq!(
        parse("spawndistance 5 6").unwrap_err(),
        "Too many arguments for 'spawndistance'. Usage: spawndistance <chunks> (1 to 1000)"
    );
}

#[test]
fn options_and_resume_take_no_arguments() {
    assert_eq!(parse("options"), Ok(Command::Options));
    assert_eq!(parse("OPTIONS"), Ok(Command::Options));
    assert_eq!(parse("resume"), Ok(Command::Resume));
    assert_eq!(parse("Resume"), Ok(Command::Resume));
    assert_eq!(parse("options all").unwrap_err(), "'options' takes no arguments.");
    assert_eq!(parse("resume now").unwrap_err(), "'resume' takes no arguments.");
}

#[test]
fn vote_parses_each_choice() {
    assert_eq!(parse("vote next"), Ok(Command::Vote(VoteChoice::NextRound)));
    assert_eq!(parse("vote keep"), Ok(Command::Vote(VoteChoice::KeepPlaying)));
    assert_eq!(parse("vote lobby"), Ok(Command::Vote(VoteChoice::ReturnToLobby)));
}

#[test]
fn vote_words_ignore_case_and_reject_others() {
    assert_eq!(parse("VOTE Next"), Ok(Command::Vote(VoteChoice::NextRound)));
    assert_eq!(parse("vote LOBBY"), Ok(Command::Vote(VoteChoice::ReturnToLobby)));
    assert_eq!(parse("vote").unwrap_err(), "Usage: vote next|keep|lobby");
    assert_eq!(parse("vote later").unwrap_err(), "Unknown vote 'later'. Usage: vote next|keep|lobby");
    assert_eq!(parse("vote next keep").unwrap_err(), "Too many arguments for 'vote'. Usage: vote next|keep|lobby");
}

#[test]
fn requires_op_is_true_for_round_settings_and_resume() {
    let operator_only = [
        Command::Goal(Goal::Lines(1)),
        Command::Goal(Goal::Items(5)),
        Command::Goal(Goal::FullCard),
        Command::Mode(Mode::Standard),
        Command::EndWhen(EndWhen::Never),
        Command::EndWhen(EndWhen::TeamsWin(2)),
        Command::Toggle(Toggle::Pvp, false),
        Command::Toggle(Toggle::KeepInventory, true),
        Command::Toggle(Toggle::Elytra, true),
        Command::Toggle(Toggle::NightVision, false),
        Command::SpawnDistance(25),
        Command::Resume,
    ];
    for command in operator_only {
        assert!(requires_op(&command), "command: {command:?}");
    }
}

#[test]
fn requires_op_is_false_for_options_and_vote() {
    let open = [
        Command::Options,
        Command::Vote(VoteChoice::NextRound),
        Command::Vote(VoteChoice::KeepPlaying),
        Command::Vote(VoteChoice::ReturnToLobby),
    ];
    for command in open {
        assert!(!requires_op(&command), "command: {command:?}");
    }
}

#[test]
fn usage_lists_the_new_commands_and_each_example_parses() {
    let text = usage();
    assert!(!text.contains('\n'));
    let examples = [
        ("goal lines <n>|items <n>|full_card", "goal full_card"),
        ("mode standard|inventory|lockout", "mode inventory"),
        ("endwhen never|first_win|all_win|teams_win <n>", "endwhen teams_win 2"),
        ("pvp on|off", "pvp on"),
        ("keepinventory on|off", "keepinventory off"),
        ("elytra on|off", "elytra on"),
        ("nightvision on|off", "nightvision off"),
        ("spawndistance <chunks>", "spawndistance 25"),
        ("options", "options"),
        ("vote next|keep|lobby", "vote lobby"),
        ("resume", "resume"),
    ];
    for (shown, line) in examples {
        assert!(text.contains(shown), "usage lacks '{shown}': {text}");
        assert!(parse(line).is_ok(), "usage example '{line}' does not parse");
    }
}

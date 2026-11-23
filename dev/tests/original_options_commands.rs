//! Round options and their chat commands against the original (Kotlin, `common/options/`, `common/commands/`).
//! Expected values are hand-traced from the original's source, never from the port. Tests that show a real
//! difference from the original are `#[ignore]`d with the difference in the reason.
use horizon_bingo::server::commands::{parse, requires_op, Command};
use horizon_bingo::server::multicard::{StalemateBehavior, WinCondition};
use horizon_bingo::server::options::{EndWhen, Goal, Mode, Options, Toggle};

// ---------------------------------------------------------------------------------------------
// Defaults of the options (options/BingoOptions.kt, options/BingoCardOptions.kt)
// ---------------------------------------------------------------------------------------------

#[test]
fn default_goal_is_one_line() {
    // hand-traced from options/BingoCardOptions.kt: goal = BingoGoal.Lines(1)
    assert_eq!(Options::default().goal, Goal::Lines(1));
}

#[test]
fn default_card_modes_are_standard() {
    // hand-traced from options/BingoCardOptions.kt: isLockoutMode, isInventoryMode, isHiddenItemsMode,
    // isConsumeItemsMode all false, so formatGameMode falls back to OptionsModeStandard
    let options = Options::default();
    assert_eq!(options.mode, Mode::Standard);
    assert!(!options.hidden_consume.hidden_items);
    assert!(!options.hidden_consume.consume_items);
}

#[test]
fn default_end_when_is_first_win() {
    // hand-traced from options/BingoOptions.kt: endGameWhen = EndWhen.FirstWin
    assert_eq!(Options::default().end_when, EndWhen::FirstWin);
}

#[test]
fn default_win_condition_is_one_card() {
    // hand-traced from options/BingoOptions.kt: winCondition = BingoWinCondition.Cards(1)
    assert_eq!(Options::default().win_condition, WinCondition::Cards(1));
}

#[test]
fn default_stalemate_behaviour_ends_the_game() {
    // hand-traced from options/BingoOptions.kt: stalemateBehavior = StalemateBehavior.END_GAME
    assert_eq!(Options::default().stalemate, StalemateBehavior::EndGame);
}

#[test]
fn default_feature_toggles_match_the_original() {
    // hand-traced from options/BingoOptions.kt: isElytra false, isNightVision false,
    // isPvpEnabled true, isKeepInventory false
    let options = Options::default();
    assert!(!options.elytra);
    assert!(!options.night_vision);
    assert!(options.pvp);
    assert!(!options.keep_inventory);
}

#[test]
fn default_auto_shuffle_is_off_with_three_teams() {
    // hand-traced from options/BingoOptions.kt: teamAutoShuffle = false, teamCount = 3
    let options = Options::default();
    assert!(!options.team_auto_shuffle);
    assert_eq!(options.team_count, 3);
}

#[test]
fn default_spawn_distance_is_25_chunks() {
    // hand-traced from options/BingoOptions.kt: spawnDistance = 25
    assert_eq!(Options::default().spawn_distance, 25);
}

#[test]
fn default_item_distribution_is_the_easy_preset() {
    // hand-traced from card/tierlist/TierLabel.kt: DIFFICULTY_EASY = listOf(0, 0, 0, 9, 16)
    assert_eq!(Options::default().tier_counts, [0, 0, 0, 9, 16]);
}

// ---------------------------------------------------------------------------------------------
// Permissions (Permission.kt: CONFIGURE_GAME defaults to OPERATORS)
// ---------------------------------------------------------------------------------------------

#[test]
fn round_settings_need_operator_rights() {
    // hand-traced from Permission.kt: configure.game is PermissionDefault.OPERATORS, and every
    // round-setting command (goal, mode, options, timelimit) requires hasPermission(CONFIGURE_GAME)
    assert!(requires_op(&Command::Goal(Goal::Lines(1))));
    assert!(requires_op(&Command::Mode(Mode::Lockout)));
    assert!(requires_op(&Command::EndWhen(EndWhen::Never)));
    assert!(requires_op(&Command::PlayTo(WinCondition::Cards(1))));
    assert!(requires_op(&Command::Stalemate(StalemateBehavior::EndGame)));
    assert!(requires_op(&Command::Toggle(Toggle::Pvp, false)));
    assert!(requires_op(&Command::SpawnDistance(25)));
    assert!(requires_op(&Command::TimeLimit(Some(10))));
}

// ---------------------------------------------------------------------------------------------
// Allowed values and ranges of the commands (commands/BingoOptionsCommands.kt)
// ---------------------------------------------------------------------------------------------

#[test]
fn goal_full_card_is_all_25_items() {
    // hand-traced from commands/BingoOptionsCommands.kt: full_card -> BingoGoal.Items(BingoGoal.MAX_ITEMS = 25)
    assert_eq!(parse("goal full_card"), Ok(Command::Goal(Goal::FullCard)));
}

#[test]
fn goal_full_card_lines_is_the_maximum_line_count() {
    // hand-traced from commands/BingoOptionsCommands.kt: full_card lines -> Lines(MAX_LINES = 11)
    assert_eq!(parse("goal full_card lines"), Ok(Command::Goal(Goal::Lines(11))));
}

#[test]
fn playto_cards_takes_a_count_of_at_least_one() {
    // hand-traced from commands/BingoOptionsCommands.kt: play_to cards integer(min = 1)
    assert_eq!(parse("playto cards 2"), Ok(Command::PlayTo(WinCondition::Cards(2))));
    assert!(parse("playto cards 0").is_err());
}

#[test]
fn endwhen_teams_win_takes_at_least_two_teams() {
    // hand-traced from commands/BingoOptionsCommands.kt: end_when teams_win integer(min = 2)
    assert_eq!(parse("endwhen teams_win 2"), Ok(Command::EndWhen(EndWhen::TeamsWin(2))));
}

#[test]
fn endwhen_teams_win_rejects_one_team() {
    assert!(parse("endwhen teams_win 1").is_err());
}

#[test]
fn timelimit_takes_at_least_one_minute_or_off() {
    // hand-traced from commands/BingoOptionsCommands.kt: timelimit integer(min = 1), literal off
    assert_eq!(parse("timelimit 15"), Ok(Command::TimeLimit(Some(15))));
    assert_eq!(parse("timelimit off"), Ok(Command::TimeLimit(None)));
    assert!(parse("timelimit 0").is_err());
}

#[test]
fn spawn_distance_accepts_zero_chunks() {
    // hand-traced from commands/BingoOptionsCommands.kt: spawn_distance integer("chunks", min = 0)
    assert_eq!(parse("spawndistance 0"), Ok(Command::SpawnDistance(0)));
}

#[test]
fn spawn_distance_accepts_large_values() {
    // hand-traced from commands/BingoOptionsCommands.kt: spawn_distance integer("chunks", min = 0), no maximum
    assert_eq!(parse("spawndistance 1001"), Ok(Command::SpawnDistance(1001)));
}

#[test]
fn playto_infinite_cards_keyword_matches_the_original() {
    // hand-traced from commands/BingoOptionsCommands.kt: options play_to infinite_cards -> BingoWinCondition.Infinite
    assert_eq!(parse("playto infinite_cards"), Ok(Command::PlayTo(WinCondition::Infinite)));
}

#[test]
fn mode_lockout_accepts_an_on_off_toggle() {
    // hand-traced from commands/BingoOptionsCommands.kt: mode lockout executesToggle { ... } (boolean "enabled")
    assert!(parse("mode lockout on").is_ok());
}

#[test]
fn preview_card_option_exists() {
    // hand-traced from commands/BingoOptionsCommands.kt: options preview_card executesToggle (pregame only)
    assert!(parse("options preview_card on").is_ok());
}

#[test]
fn end_when_lives_under_options_in_the_original() {
    // hand-traced from commands/BingoOptionsCommands.kt: literal("options") { literal("end_when") { ... } }
    assert_eq!(parse("options end_when never"), Ok(Command::EndWhen(EndWhen::Never)));
}

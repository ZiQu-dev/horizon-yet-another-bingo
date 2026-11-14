//! Chat command parsing for the bingo game. Pure: the input is the text after the `bingo` word, and the caller decides
//! who may run the result (see [`requires_op`]). Command and keyword words ignore case; team names are kept as typed.

use super::options::{EndWhen, Goal, Mode, Toggle};

/// One parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Status,
    Start { ignore_warnings: bool },
    End,
    Reset,
    ShuffleTeams(usize),
    Join(String),
    Spectator,
    Ready,
    ReadyCancel,
    /// Limit in minutes, or `None` for `off`.
    TimeLimit(Option<u32>),
    Card,
    /// `goal lines|items <n>` or `goal full_card`.
    Goal(Goal),
    /// `mode standard|inventory|lockout`.
    Mode(Mode),
    /// `endwhen never|first_win|all_win` or `endwhen teams_win <n>`.
    EndWhen(EndWhen),
    /// `pvp`, `keepinventory`, `elytra` or `nightvision`, with `on` (true) or `off` (false).
    Toggle(Toggle, bool),
    /// Distance between team spawns in chunks, 1 to 1000.
    SpawnDistance(u32),
    /// Show the current round options.
    Options,
    /// A player's vote at the end of a round.
    Vote(VoteChoice),
    /// Extends a round that has reached its end.
    Resume,
    /// Your own game statistics.
    Stats,
}

/// The answer to the end-of-round vote: play another round, keep playing, or return to the lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteChoice {
    NextRound,
    KeepPlaying,
    ReturnToLobby,
}

const USAGE: &str = "start [ignore_warnings] | end | reset | shuffleteams <n> | join <team> | spectator | ready [cancel] | timelimit <minutes|off> | card | status | goal lines <n>|items <n>|full_card | mode standard|inventory|lockout | endwhen never|first_win|all_win|teams_win <n> | pvp on|off | keepinventory on|off | elytra on|off | nightvision on|off | spawndistance <chunks> | options | vote next|keep|lobby | resume | stats";

const SPAWN_DISTANCE_MAX: u32 = 1000;

/// The list of commands, for replies to a bad command.
pub fn usage() -> String {
    USAGE.to_string()
}

/// Parses one command line (the text after `bingo`). An empty line is `Help`.
pub fn parse(line: &str) -> Result<Command, String> {
    let mut words = line.split_whitespace();
    let Some(name) = words.next() else {
        return Ok(Command::Help);
    };
    let args: Vec<&str> = words.collect();
    match name.to_lowercase().as_str() {
        "help" => no_args(name, &args, Command::Help),
        "status" => no_args(name, &args, Command::Status),
        "end" => no_args(name, &args, Command::End),
        "reset" => no_args(name, &args, Command::Reset),
        "spectator" => no_args(name, &args, Command::Spectator),
        "card" => no_args(name, &args, Command::Card),
        "stats" => no_args(name, &args, Command::Stats),
        "options" => no_args(name, &args, Command::Options),
        "resume" => no_args(name, &args, Command::Resume),
        "start" => match args.as_slice() {
            [] => Ok(Command::Start { ignore_warnings: false }),
            [option] if option.eq_ignore_ascii_case("ignore_warnings") => Ok(Command::Start { ignore_warnings: true }),
            [option] => Err(format!("Unknown option '{option}' for start. Usage: start [ignore_warnings]")),
            _ => Err("Too many arguments for 'start'. Usage: start [ignore_warnings]".to_string()),
        },
        "ready" => match args.as_slice() {
            [] => Ok(Command::Ready),
            [option] if option.eq_ignore_ascii_case("cancel") => Ok(Command::ReadyCancel),
            [option] => Err(format!("Unknown option '{option}' for ready. Usage: ready [cancel]")),
            _ => Err("Too many arguments for 'ready'. Usage: ready [cancel]".to_string()),
        },
        "shuffleteams" => match args.as_slice() {
            [] => Err("Usage: shuffleteams <n>".to_string()),
            [count] => match count.parse::<usize>() {
                Ok(teams) if teams >= 1 => Ok(Command::ShuffleTeams(teams)),
                _ => Err(format!("shuffleteams needs a whole number of at least 1, got '{count}'")),
            },
            _ => Err("Too many arguments for 'shuffleteams'. Usage: shuffleteams <n>".to_string()),
        },
        "join" => match args.as_slice() {
            [] => Err("Usage: join <team>".to_string()),
            [team] => Ok(Command::Join(team.to_string())),
            _ => Err("Too many arguments for 'join'. Usage: join <team>".to_string()),
        },
        "timelimit" => match args.as_slice() {
            [] => Err("Usage: timelimit <minutes|off>".to_string()),
            [value] if value.eq_ignore_ascii_case("off") => Ok(Command::TimeLimit(None)),
            [value] => match value.parse::<u32>() {
                Ok(minutes) if minutes >= 1 => Ok(Command::TimeLimit(Some(minutes))),
                _ => Err(format!("timelimit needs a number of minutes of at least 1, or 'off', got '{value}'")),
            },
            _ => Err("Too many arguments for 'timelimit'. Usage: timelimit <minutes|off>".to_string()),
        },
        "goal" => parse_goal(&args),
        "mode" => match args.as_slice() {
            [] => Err("Usage: mode standard|inventory|lockout".to_string()),
            [value] => match value.to_lowercase().as_str() {
                "standard" => Ok(Command::Mode(Mode::Standard)),
                "inventory" => Ok(Command::Mode(Mode::Inventory)),
                "lockout" => Ok(Command::Mode(Mode::Lockout)),
                _ => Err(format!("Unknown mode '{value}'. Usage: mode standard|inventory|lockout")),
            },
            _ => Err("Too many arguments for 'mode'. Usage: mode standard|inventory|lockout".to_string()),
        },
        "endwhen" => parse_end_when(&args),
        "pvp" => parse_toggle(name, Toggle::Pvp, &args),
        "keepinventory" => parse_toggle(name, Toggle::KeepInventory, &args),
        "elytra" => parse_toggle(name, Toggle::Elytra, &args),
        "nightvision" => parse_toggle(name, Toggle::NightVision, &args),
        "spawndistance" => match args.as_slice() {
            [] => Err(format!("Usage: spawndistance <chunks> (1 to {SPAWN_DISTANCE_MAX})")),
            [value] => match value.parse::<u32>() {
                Ok(chunks) if (1..=SPAWN_DISTANCE_MAX).contains(&chunks) => Ok(Command::SpawnDistance(chunks)),
                _ => Err(format!("spawndistance needs a whole number of chunks from 1 to {SPAWN_DISTANCE_MAX}, got '{value}'")),
            },
            _ => Err(format!("Too many arguments for 'spawndistance'. Usage: spawndistance <chunks> (1 to {SPAWN_DISTANCE_MAX})")),
        },
        "vote" => match args.as_slice() {
            [] => Err("Usage: vote next|keep|lobby".to_string()),
            [value] => match value.to_lowercase().as_str() {
                "next" => Ok(Command::Vote(VoteChoice::NextRound)),
                "keep" => Ok(Command::Vote(VoteChoice::KeepPlaying)),
                "lobby" => Ok(Command::Vote(VoteChoice::ReturnToLobby)),
                _ => Err(format!("Unknown vote '{value}'. Usage: vote next|keep|lobby")),
            },
            _ => Err("Too many arguments for 'vote'. Usage: vote next|keep|lobby".to_string()),
        },
        _ => Err(format!("Unknown command '{name}'. Allowed: {}", usage())),
    }
}

/// Whether the command needs operator rights. Start, end, reset, shuffleteams, ready cancel, timelimit, the round
/// settings (goal, mode, endwhen, toggles, spawndistance) and resume do; the rest are open to every player.
pub fn requires_op(command: &Command) -> bool {
    match command {
        Command::Start { .. }
        | Command::End
        | Command::Reset
        | Command::ShuffleTeams(_)
        | Command::ReadyCancel
        | Command::TimeLimit(_)
        | Command::Goal(_)
        | Command::Mode(_)
        | Command::EndWhen(_)
        | Command::Toggle(..)
        | Command::SpawnDistance(_)
        | Command::Resume => true,
        Command::Help
        | Command::Status
        | Command::Join(_)
        | Command::Spectator
        | Command::Ready
        | Command::Card
        | Command::Options
        | Command::Vote(_)
        | Command::Stats => false,
    }
}

fn no_args(name: &str, args: &[&str], command: Command) -> Result<Command, String> {
    if args.is_empty() {
        Ok(command)
    } else {
        Err(format!("'{name}' takes no arguments."))
    }
}

fn parse_goal(args: &[&str]) -> Result<Command, String> {
    const GOAL_USAGE: &str = "Usage: goal lines <n>|items <n>|full_card";
    match args {
        [] => Err(GOAL_USAGE.to_string()),
        [kind, value] if kind.eq_ignore_ascii_case("lines") => {
            Ok(Command::Goal(Goal::Lines(positive_count("goal lines", value, "a number of lines")?)))
        }
        [kind, value] if kind.eq_ignore_ascii_case("items") => {
            Ok(Command::Goal(Goal::Items(positive_count("goal items", value, "a number of items")?)))
        }
        [kind] if kind.eq_ignore_ascii_case("full_card") => Ok(Command::Goal(Goal::FullCard)),
        [kind] if kind.eq_ignore_ascii_case("lines") || kind.eq_ignore_ascii_case("items") => {
            Err(format!("Usage: goal {} <n>", kind.to_lowercase()))
        }
        [kind, ..] if kind.eq_ignore_ascii_case("full_card") => {
            Err("Too many arguments for 'goal full_card'. Usage: goal full_card".to_string())
        }
        [kind, ..] if kind.eq_ignore_ascii_case("lines") || kind.eq_ignore_ascii_case("items") => {
            Err(format!("Too many arguments for 'goal {}'. Usage: goal {} <n>", kind.to_lowercase(), kind.to_lowercase()))
        }
        [kind, ..] => Err(format!("Unknown goal '{kind}'. {GOAL_USAGE}")),
    }
}

fn parse_end_when(args: &[&str]) -> Result<Command, String> {
    const END_WHEN_USAGE: &str = "Usage: endwhen never|first_win|all_win|teams_win <n>";
    match args {
        [] => Err(END_WHEN_USAGE.to_string()),
        [value] => match value.to_lowercase().as_str() {
            "never" => Ok(Command::EndWhen(EndWhen::Never)),
            "first_win" => Ok(Command::EndWhen(EndWhen::FirstWin)),
            "all_win" => Ok(Command::EndWhen(EndWhen::AllWin)),
            "teams_win" => Err("Usage: endwhen teams_win <n>".to_string()),
            _ => Err(format!("Unknown endwhen '{value}'. {END_WHEN_USAGE}")),
        },
        [kind, value] if kind.eq_ignore_ascii_case("teams_win") => {
            Ok(Command::EndWhen(EndWhen::TeamsWin(positive_count("endwhen teams_win", value, "a number of teams")?)))
        }
        [kind, _] => Err(format!("Unknown endwhen '{kind}'. {END_WHEN_USAGE}")),
        _ => Err(format!("Too many arguments for 'endwhen'. {END_WHEN_USAGE}")),
    }
}

fn parse_toggle(name: &str, toggle: Toggle, args: &[&str]) -> Result<Command, String> {
    match args {
        [] => Err(format!("Usage: {name} on|off")),
        [value] => match value.to_lowercase().as_str() {
            "on" | "true" => Ok(Command::Toggle(toggle, true)),
            "off" | "false" => Ok(Command::Toggle(toggle, false)),
            _ => Err(format!("{name} needs on or off, got '{value}'")),
        },
        _ => Err(format!("Too many arguments for '{name}'. Usage: {name} on|off")),
    }
}

/// A whole number of at least 1 for a count argument; `what` names it in the error.
fn positive_count(command: &str, value: &str, what: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(count) if count >= 1 => Ok(count),
        _ => Err(format!("{command} needs {what} of at least 1, got '{value}'")),
    }
}

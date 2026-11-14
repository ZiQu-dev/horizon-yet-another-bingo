//! Chat command parsing for the bingo game. Pure: the input is the text after the `bingo` word, and the caller decides
//! who may run the result (see [`requires_op`]). Command and keyword words ignore case; team names are kept as typed.

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
}

const USAGE: &str = "start [ignore_warnings] | end | reset | shuffleteams <n> | join <team> | spectator | ready [cancel] | timelimit <minutes|off> | card | status";

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
        _ => Err(format!("Unknown command '{name}'. Allowed: {}", usage())),
    }
}

/// Whether the command needs operator rights. Start, end, reset, shuffleteams, ready cancel and timelimit do; the rest
/// are open to every player.
pub fn requires_op(command: &Command) -> bool {
    match command {
        Command::Start { .. }
        | Command::End
        | Command::Reset
        | Command::ShuffleTeams(_)
        | Command::ReadyCancel
        | Command::TimeLimit(_) => true,
        Command::Help
        | Command::Status
        | Command::Join(_)
        | Command::Spectator
        | Command::Ready
        | Command::Card => false,
    }
}

fn no_args(name: &str, args: &[&str], command: Command) -> Result<Command, String> {
    if args.is_empty() {
        Ok(command)
    } else {
        Err(format!("'{name}' takes no arguments."))
    }
}

//! The server's chat lines in one place, in English. A line a client can translate is a [`Line`]: a translation key,
//! its English template (`%s` takes the next argument, `%%` is a percent sign) and the arguments. The adapter sends it
//! as a translatable component with the English as its fallback, so a client that has no translation for the key shows
//! the English, as vanilla does. Replies to a command are plain English for now.
//!
//! The keys are `bingo.native.*`, so the original mod's own `bingo.*` keys keep their meaning.

/// A line with a translation key: the English template and the arguments that fill its `%s` places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub key: &'static str,
    pub template: &'static str,
    pub args: Vec<String>,
}

impl Line {
    pub fn new(key: &'static str, template: &'static str, args: &[&str]) -> Self {
        Self { key, template, args: args.iter().map(|arg| arg.to_string()).collect() }
    }

    /// The English line: each `%s` takes the next argument (left as `%s` when there is none), `%%` is `%`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut args = self.args.iter();
        let mut chars = self.template.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.peek() {
                Some('%') => {
                    chars.next();
                    out.push('%');
                }
                Some('s') => {
                    chars.next();
                    match args.next() {
                        Some(arg) => out.push_str(arg),
                        None => out.push_str("%s"),
                    }
                }
                _ => out.push('%'),
            }
        }
        out
    }
}

/// `%s reached the goal!`, for a team that reached the goal before the game ends.
pub fn team_reached_goal(team: &str) -> Line {
    Line::new("bingo.native.team_reached_goal", "%s reached the goal!", &[team])
}

/// `%s won the game!`
pub fn team_won_game(team: &str) -> Line {
    Line::new("bingo.native.team_won_game", "%s won the game!", &[team])
}

pub fn game_ended() -> Line {
    Line::new("bingo.native.game_ended", "The game has ended.", &[])
}

pub fn game_continues() -> Line {
    Line::new("bingo.native.game_continues", "The game continues.", &[])
}

pub fn stalemate_no_team_can_win() -> Line {
    Line::new("bingo.native.stalemate_no_team_can_win", "A stalemate: no team can win the card, so the game ends.", &[])
}

pub fn stalemate_lockout() -> Line {
    Line::new(
        "bingo.native.stalemate_lockout",
        "A stalemate: every square is claimed and no team leads, so the game ends.",
        &[],
    )
}

pub fn stalemate_impossible_goal() -> Line {
    Line::new("bingo.native.stalemate_impossible_goal", "A stalemate: no team can reach the goal, so the game ends.", &[])
}

impl Line {
    /// The line as a component (`{translate, fallback, with}`): the key is looked up in the player's language, the
    /// English template is the fallback, and the arguments fill its `%s` places, as vanilla's translatable text does.
    pub fn text(&self) -> crate::text::Text {
        use crate::nbt::Nbt;
        crate::text::Text(Nbt::compound(vec![
            ("translate", Nbt::string(self.key)),
            ("fallback", Nbt::string(self.template)),
            ("with", Nbt::list(self.args.iter().map(|arg| Nbt::string(arg)).collect())),
        ]))
    }
}

// The lobby's ready screen (`ReadyService.getMessage`). The keys are the original's where its English is the same.

/// `%s of %s ready!`: shown to a player who is ready while the timer runs.
pub fn players_ready(ready: usize, total: usize) -> Line {
    let (ready, total) = (ready.to_string(), total.to_string());
    Line::new("bingo.lobby.vote.players_ready", "%s of %s ready!", &[ready.as_str(), total.as_str()])
}

/// `type %s to speed up!`: the hint to a player who is not ready yet, with the command to type.
pub fn ready_up_command(command: &str) -> Line {
    Line::new("bingo.lobby.vote.ready_up_command", "type %s to speed up!", &[command])
}

/// `Starting in %s`: the countdown while the timer runs, with the time as `clock_text` writes it.
pub fn starting_in(clock: &str) -> Line {
    Line::new("bingo.lobby.vote.starting_time_remaining", "Starting in %s", &[clock])
}

/// `Waiting to start...`: the countdown line while the timer does not run.
pub fn waiting_to_start() -> Line {
    Line::new("bingo.game.waiting_to_start", "Waiting to start...", &[])
}

// The start (`GameService.start`): the title every player gets, and the banner in chat.

/// `BINGO`, the title at the start, in the team's colour.
pub fn start_title() -> Line {
    Line::new("bingo.game.announce.bingo", "BINGO", &[])
}

/// `Game is starting!`, the subtitle at the start.
pub fn start_subtitle() -> Line {
    Line::new("bingo.game.announce.starting", "Game is starting!", &[])
}

/// `BINGO Start!`, the chat banner before the options of the game.
pub fn start_banner() -> Line {
    Line::new("bingo.game.start", "BINGO Start!", &[])
}

// The end of the game (`GameOverController`): the reason line, the duration, and the ranking of the teams.

/// `%s completed their card!`, the reason when a team won.
pub fn completed_card(team: &str) -> Line {
    Line::new("bingo.game.end.completed_card", "%s completed their card!", &[team])
}

/// `Out of time!`, the reason when the time limit ran out.
pub fn out_of_time() -> Line {
    Line::new("bingo.game.end.reason_timer", "Out of time!", &[])
}

/// `Stalemate!`, the reason when a stalemate ended the game.
pub fn stalemate_reason() -> Line {
    Line::new("bingo.game.end.reason_stalemate", "Stalemate!", &[])
}

/// `Lockout!`, the reason when lockout ended the game with no team able to reach the goal.
pub fn lockout_reason() -> Line {
    Line::new("bingo.game.end.reason_lockout", "Lockout!", &[])
}

/// `Soft-locked!`, the reason when the goal cannot be reached any more.
pub fn soft_locked_reason() -> Line {
    Line::new("bingo.game.end.reason_impossible_goal", "Soft-locked!", &[])
}

/// `Game stopped.`, the reason when `bingo end` stopped the game. The original names who stopped it; this does not know.
pub fn stopped_reason() -> Line {
    Line::new("bingo.native.game_stopped", "Game stopped.", &[])
}

/// `Game duration: %s`.
pub fn game_duration(clock: &str) -> Line {
    Line::new("bingo.game.end.duration", "Game duration: %s", &[clock])
}

/// The rule of dashes that frames the start and the end (the original's `game_start.json` and `game_end.json`).
pub fn separator() -> String {
    "┄".repeat(40)
}

/// The time as the original's `formatString` writes it: days, hours (when any), then minutes and seconds with two
/// digits, for example `1h 02m 03s`.
pub fn duration_text(millis: i64) -> String {
    let secs = millis.max(0) / 1000;
    let (days, hours) = (secs / 86_400, secs % 86_400 / 3_600);
    let (minutes, seconds) = (secs % 3_600 / 60, secs % 60);
    let mut out = String::new();
    if days > 0 {
        out.push_str(&format!("{days}d "));
    }
    if hours > 0 {
        out.push_str(&format!("{hours}h "));
    }
    out.push_str(&format!("{minutes:02}m {seconds:02}s"));
    out
}

/// The time as the lobby's countdown writes it (`formatHHMMSS`): `H:MM:SS` from an hour on, else `MM:SS`.
pub fn clock_text(millis: i64) -> String {
    let secs = millis.max(0) / 1000;
    let (hours, minutes, seconds) = (secs / 3_600, secs % 3_600 / 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// `1 line` or `2 lines`, `1 item` or `3 items`, `1 card` or `2 cards`.
pub fn count_text(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// A team's score as the ranking writes it: `2 cards, 1 line, 3 items` (the parts it has, or `0`).
pub fn score_text(score: crate::score_update::TeamScore) -> String {
    let mut parts = Vec::new();
    if score.cards > 0 {
        parts.push(count_text(score.cards as u32, "card", "cards"));
    }
    if score.lines > 0 {
        parts.push(count_text(score.lines as u32, "line", "lines"));
    }
    if score.items > 0 {
        parts.push(count_text(score.items as u32, "item", "items"));
    }
    if parts.is_empty() {
        return "0".to_string();
    }
    parts.join(", ")
}

/// One row of the ranking in the end's chat: `1. Red - 2 lines, 3 items - 1m 30s`.
pub fn score_row(place: usize, team: &str, score: &str, duration: &str) -> String {
    format!("{place}. {team} - {score} - {duration}")
}

// The start warnings (`GameService.start`) are replies to the command that starts the game, so they are plain English.

/// The lockout warning: lockout with a lines goal and more than one team may stall, so the start waits for a choice.
pub fn lockout_with_lines_goal(goal: &str) -> String {
    format!(
        "Lockout is enabled, but the card goal is {goal} - this might cause a stalemate. You may want this instead: stalemate do_nothing. Use start ignore_warnings to start anyway."
    )
}

/// The warning that the goal is met by the card before play: the start is refused even with `ignore_warnings`, as in
/// the original.
pub fn goal_already_met(goal: &str, completed: &str) -> String {
    format!("The goal is set to {goal}, but the card already has {completed} completed!")
}

// Replies to `bingo kit`: plain English.

pub const KIT_USAGE: &str = "Usage: kit [add <item> [count] | remove <item> | clear]";
pub const KIT_ADD_USAGE: &str = "Usage: kit add <item> [count]";
pub const KIT_EMPTY: &str = "Kit: none. Team players get nothing at the start.";
pub const KIT_ONLY_BEFORE_START: &str = "The kit can be changed only before the game starts.";
pub const KIT_CLEARED: &str = "Kit cleared.";

pub fn unknown_item(name: &str) -> String {
    format!("Unknown item '{name}'. Use an item id such as diamond or minecraft:diamond.")
}

pub fn kit_list(items: &str) -> String {
    format!("Kit: {items}. Team players get these at the start of each round.")
}

pub fn kit_full(max: usize) -> String {
    format!("The kit has {max} items already. Remove one first.")
}

pub fn kit_added(item: &str, count: u32) -> String {
    format!("Kit: {count} {item} set.")
}

pub fn kit_removed(item: &str) -> String {
    format!("Kit: {item} removed.")
}

pub fn not_in_kit(item: &str) -> String {
    format!("{item} is not in the kit.")
}

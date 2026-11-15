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

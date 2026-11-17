//! Team chat of the original (`ChatMessageController`, `ChatMessageService.sendTeamMessage`): while a game is Starting or
//! Playing, a player's chat line goes only to their team, and a player on no team (a spectator, or one not yet on a team)
//! only to the other players on no team. `g`, `global` and `all` send a line to everyone (`ChatCommand`). Pure: the caller
//! gives the roster and does the sending.

use super::teams;

/// The label of the spectators' chat, in brackets (`bingo.command.teammsg.spectators`).
pub const SPECTATORS_LABEL: &str = "Spectators";
/// The commands that send to everyone, from the original's `globalCommandAliases`.
pub const GLOBAL_COMMANDS: [&str; 3] = ["g", "global", "all"];

/// Who gets a team chat line, and the line they see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routed {
    pub recipients: Vec<u128>,
    pub line: String,
}

/// The line of a team chat message: `[Red] <Alice> hello`, the team's display name in brackets.
pub fn route(sender_name: &str, sender_team: Option<&str>, online: &[(u128, Option<&str>)], message: &str) -> Routed {
    let (label, recipients): (String, Vec<u128>) = match sender_team {
        Some(key) => (
            teams::preset(key).map_or(key, |preset| preset.name).to_string(),
            online.iter().filter(|(_, team)| *team == Some(key)).map(|(uuid, _)| *uuid).collect(),
        ),
        None => (
            SPECTATORS_LABEL.to_string(),
            online.iter().filter(|(_, team)| team.is_none()).map(|(uuid, _)| *uuid).collect(),
        ),
    };
    Routed { recipients, line: format!("[{label}] <{sender_name}> {message}") }
}

/// The text of a `g`, `global` or `all` command (the line the player typed, without the slash), or `None` for any other
/// command or an empty text.
pub fn global_text(command: &str) -> Option<&str> {
    let mut words = command.splitn(2, char::is_whitespace);
    if !GLOBAL_COMMANDS.contains(&words.next()?) {
        return None;
    }
    let text = words.next()?.trim();
    (!text.is_empty()).then_some(text)
}

/// The line a global message shows everyone: `<Alice> hello`.
pub fn global_line(sender_name: &str, text: &str) -> String {
    format!("<{sender_name}> {text}")
}

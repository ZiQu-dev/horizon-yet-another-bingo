//! The sidebar of the original's `ScoreboardController`, shown as one sidebar for everyone on the server. The original
//! builds a separate one per team, from the templates `scoreboard.json`, `scoreboard_pregame.json` and
//! `scoreboard_postgame.json`; Horizon's host has no per-player sidebar, so this sends the spectators' version to all:
//! the mode, goal, time and the ranking of the teams (or the teams and their players before the game).
//!
//! Pure: [`rows`] makes the lines from the game's facts, and [`Shown`] turns each change of the lines into console
//! commands (`scoreboard ...`), sending only what changed. Every row is a fake score holder with a display name, so the
//! sidebar shows the text, not the holder.

use super::host::ServerHost;
use super::options::{Goal, Mode};

/// The objective the sidebar lives in. Its holders are `bingo_row00` ... `bingo_row14`.
pub const OBJECTIVE: &str = "bingo_sidebar";
/// The title (`bingo.scoreboard.name`).
pub const TITLE: &str = "Minecraft Bingo";
/// Rows a sidebar shows at most (the sidebar's room).
pub const MAX_ROWS: usize = 15;
/// Teams in the ranking, as the original shows at most 8.
pub const RANKING_ROWS: usize = 8;

/// The game's phase, as the sidebar shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Pregame: the settings and the teams.
    Lobby,
    /// Starting or Playing: the ranking.
    Live,
    /// Postgame: the end and the ranking.
    Over,
}

/// What the clock line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// No time limit: the time played so far, in milliseconds.
    Elapsed(i64),
    /// A time limit: the time left, in milliseconds.
    Left(i64),
    /// A time limit set before the game, in minutes.
    Limit(u32),
    /// No time limit set before the game.
    NoLimit,
}

/// One team's place in the ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub name: String,
    pub lines: i32,
    pub items: i32,
}

/// What the sidebar is made from. `teams` is the lobby's teams with their player counts; `ranking` is best first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub phase: Phase,
    pub mode: Mode,
    pub goal: Goal,
    pub clock: Clock,
    pub teams: Vec<(String, usize)>,
    pub ranking: Vec<Standing>,
}

/// `HH:MM:SS` of a duration in milliseconds (negative counts as zero).
pub fn format_clock(millis: i64) -> String {
    let secs = millis.max(0) / 1000;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// The mode's name on the sidebar.
pub fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Standard => "Standard",
        Mode::Inventory => "Inventory",
        Mode::Lockout => "Lockout",
    }
}

/// The goal's name on the sidebar: `1 line`, `5 items`, `full card`.
pub fn goal_label(goal: Goal) -> String {
    match goal {
        Goal::Lines(1) => "1 line".to_string(),
        Goal::Lines(n) => format!("{n} lines"),
        Goal::Items(1) => "1 item".to_string(),
        Goal::Items(n) => format!("{n} items"),
        Goal::FullCard => "full card".to_string(),
    }
}

fn clock_line(clock: Clock) -> String {
    match clock {
        Clock::Elapsed(ms) => format!("Time: {}", format_clock(ms)),
        Clock::Left(ms) => format!("Time left: {}", format_clock(ms)),
        Clock::Limit(minutes) => format!("Time limit: {minutes} min"),
        Clock::NoLimit => "Time limit: none".to_string(),
    }
}

/// The sidebar's lines, top first, at most [`MAX_ROWS`].
pub fn rows(facts: &Facts) -> Vec<String> {
    let mut rows = match facts.phase {
        Phase::Lobby => {
            let mut rows = vec![
                format!("Mode: {}", mode_label(facts.mode)),
                format!("Goal: {}", goal_label(facts.goal)),
                clock_line(facts.clock),
                String::new(),
            ];
            rows.extend(facts.teams.iter().map(|(name, players)| {
                format!("{name}: {players} {}", if *players == 1 { "player" } else { "players" })
            }));
            rows
        }
        Phase::Live => {
            let mut rows = vec![
                format!("Mode: {}", mode_label(facts.mode)),
                format!("Goal: {}", goal_label(facts.goal)),
                clock_line(facts.clock),
                String::new(),
            ];
            rows.extend(ranking_rows(&facts.ranking));
            rows
        }
        Phase::Over => {
            let mut rows = vec!["Game over!".to_string(), clock_line(facts.clock), String::new()];
            rows.extend(ranking_rows(&facts.ranking));
            rows
        }
    };
    rows.truncate(MAX_ROWS);
    rows
}

fn ranking_rows(ranking: &[Standing]) -> Vec<String> {
    ranking
        .iter()
        .take(RANKING_ROWS)
        .enumerate()
        .map(|(index, standing)| {
            format!(
                "{}. {}: {} {}, {} {}",
                index + 1,
                standing.name,
                standing.lines,
                if standing.lines == 1 { "line" } else { "lines" },
                standing.items,
                if standing.items == 1 { "item" } else { "items" },
            )
        })
        .collect()
}

/// The score holder of row `index`. A player name cannot have this form, so it is never a real player.
fn holder(index: usize) -> String {
    format!("bingo_row{index:02}")
}

/// A text as a quoted string argument of a command: `"` and `\` escaped. A blank row shows a space, so the display
/// name is never empty.
fn quoted(text: &str) -> String {
    let text = if text.is_empty() { " " } else { text };
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The sidebar as shown on the server, so each change is sent as commands once: the lines last sent.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Shown {
    active: bool,
    rows: Vec<String>,
}

impl Shown {
    pub const fn new() -> Self {
        Self { active: false, rows: Vec::new() }
    }

    /// Makes the server's sidebar show `rows` (`None` takes it away). Only the rows that changed are sent: a new sidebar
    /// is added and shown first. The score of row `i` is `MAX_ROWS - i`, so the top row is the highest and the scores do
    /// not move when the rows change.
    pub fn update(&mut self, host: &mut dyn ServerHost, rows: Option<Vec<String>>) {
        let Some(mut rows) = rows else {
            if self.active {
                host.command("scoreboard objectives setdisplay sidebar");
                host.command(&format!("scoreboard objectives remove {OBJECTIVE}"));
            }
            self.active = false;
            self.rows.clear();
            return;
        };
        rows.truncate(MAX_ROWS);

        if !self.active {
            host.command(&format!("scoreboard objectives add {OBJECTIVE} dummy {}", quoted(TITLE)));
            host.command(&format!("scoreboard objectives modify {OBJECTIVE} numberformat blank"));
            host.command(&format!("scoreboard objectives setdisplay sidebar {OBJECTIVE}"));
            self.active = true;
            self.rows.clear();
        }

        for (index, row) in rows.iter().enumerate() {
            let holder = holder(index);
            if self.rows.get(index) != Some(row) {
                if index >= self.rows.len() {
                    host.command(&format!("scoreboard players set {holder} {OBJECTIVE} {}", MAX_ROWS - index));
                }
                host.command(&format!("scoreboard players display name {holder} {OBJECTIVE} {}", quoted(row)));
            }
        }
        for index in rows.len()..self.rows.len() {
            host.command(&format!("scoreboard players reset {} {OBJECTIVE}", holder(index)));
        }
        self.rows = rows;
    }
}

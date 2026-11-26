//! The sidebar of the original's `ScoreboardController`. The original builds one per team, from the templates
//! `scoreboard.json`, `scoreboard_pregame.json` and `scoreboard_postgame.json`, and sends each team's sidebar to its
//! members only. Horizon's mod API has no per-player sidebar, so the server still shows one sidebar to everyone, and
//! [`Shown`] sends it (see `PORT-GAP` in [`rows_for`]).
//!
//! Pure: [`rows_for`] makes the lines of one team's view from the game's facts, and [`rows_for_viewer`] adds the
//! per-player visibility ([`shows`]); both make components the client translates (the original's keys, with its English
//! as the fallback). [`Shown`] turns each change of the lines into console commands (`scoreboard ...`), sending only what
//! changed. Every row is a fake score holder: its display name is the line, its score (the number on the right) is the
//! ranking's score text, and the objective's number format hides the rest.

use std::cmp::Reverse;

use super::host::ServerHost;
use super::options::{Goal, Mode};
use super::texts::{self, Line};

/// The objective the sidebar lives in. Its holders are `bingo_row00` ... `bingo_row14`.
pub const OBJECTIVE: &str = "bingo_sidebar";
/// The title's translation key and its English (`bingo.scoreboard.name`).
pub const TITLE_KEY: &str = "bingo.scoreboard.name";
pub const TITLE: &str = "Minecraft Bingo";
/// Rows a sidebar shows at most (the sidebar's room).
pub const MAX_ROWS: usize = 15;
/// Teams in the ranking, as the original shows at most 8.
pub const RANKING_ROWS: usize = 8;

/// A piece of a line, sent as a text component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Literal text, with its colour (a vanilla formatting name in lower case) and bold.
    Text { text: String, color: Option<String>, bold: bool },
    /// A translated line: its key, the English it falls back to (`%s` takes the next of `with`), its colour.
    Translate { key: String, fallback: String, color: Option<String>, with: Vec<Part> },
    /// Pieces that share one colour (the original's `formatted(...)` on a component with children).
    Group { color: Option<String>, parts: Vec<Part> },
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Part::Text { text: text.into(), color: None, bold: false }
    }

    pub fn translate(key: &str, fallback: &str, with: Vec<Part>) -> Self {
        Part::Translate { key: key.to_string(), fallback: fallback.to_string(), color: None, with }
    }

    /// A translation line of the texts module, with its arguments as plain text.
    pub fn of_line(line: &Line) -> Self {
        Part::Translate {
            key: line.key.to_string(),
            fallback: line.template.to_string(),
            color: None,
            with: line.args.iter().map(Part::text).collect(),
        }
    }

    /// The part in `color`, unless it has one already (`Style.withColor` as the original's `formatted` sets it).
    fn colored(self, color: &str) -> Self {
        match self {
            Part::Text { text, color: None, bold } => Part::Text { text, color: Some(color.to_string()), bold },
            Part::Translate { key, fallback, color: None, with } => {
                Part::Translate { key, fallback, color: Some(color.to_string()), with }
            }
            Part::Group { color: None, parts } => Part::Group { color: Some(color.to_string()), parts },
            other => other,
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, Part::Text { text, .. } if text.is_empty())
    }
}

/// One line of the sidebar: its text, and the score the line shows on its right (`None` hides it).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    pub text: Vec<Part>,
    pub score: Option<Part>,
}

impl Row {
    fn of(text: Vec<Part>) -> Self {
        Row { text, score: None }
    }

    fn blank() -> Self {
        Row::default()
    }

    fn is_blank(&self) -> bool {
        self.text.iter().all(Part::is_empty)
    }
}

/// The game's phase, as the sidebar shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Pregame: the settings, the time and the ready-up lines (`scoreboard_pregame.json`).
    Lobby,
    /// Starting or Playing: the settings, the time and the ranking (`scoreboard.json`).
    Live,
    /// Postgame: the end and the ranking (`scoreboard_postgame.json`).
    Over,
}

/// What the clock line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// No time limit: the time played so far, in milliseconds (`bingo.scoreboard.time`).
    Elapsed(i64),
    /// A time limit: the time left, in milliseconds (`bingo.scoreboard.time_left`).
    Left(i64),
    /// A time limit set before the game, in milliseconds: shown as the time left.
    Limit(i64),
    /// No time limit set before the game: the time is zero.
    NoLimit,
}

/// The round's features (`BingoOptions`: elytra, night vision, keep inventory, PvP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Features {
    pub elytra: bool,
    pub night_vision: bool,
    pub keep_inventory: bool,
    pub pvp: bool,
}

/// One team's place in the game: what its line shows and how the ranking orders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The team's key (`bingo_red`).
    pub key: String,
    /// The team's English name (`Red`); its translation key is `bingo.team.<name in lower case>`.
    pub name: String,
    /// The team's colour as a vanilla formatting name in lower case (`red`).
    pub color: String,
    /// The symbol shown before the team's name (`🔥`), when the team has one.
    pub symbol: Option<String>,
    /// The name of the team's only player, shown in place of the team's name. The original also draws the player's head,
    /// which this does not (PORT-GAP in [`Standing::name_part`]).
    pub player: Option<String>,
    pub items: i32,
    pub lines: i32,
    pub cards: i32,
    /// Milliseconds from the start to the team's finish, `None` while it has not finished.
    pub finished: Option<i64>,
}

impl Standing {
    /// The team's name as `getName(playerName = true, symbol = true)` writes it: the symbol and the name, in the team's
    /// colour.
    pub fn name_part(&self) -> Part {
        let mut parts = Vec::new();
        match &self.player {
            Some(player) => parts.push(Part::text(player.as_str())),
            None => {
                // PORT-GAP: the original puts the player's head before a one-player team's name; a sidebar line cannot
                // draw it here, so the player's name stands alone.
                if let Some(symbol) = &self.symbol {
                    parts.push(Part::text(format!("{symbol} ")));
                }
                parts.push(Part::translate(&format!("bingo.team.{}", self.name.to_lowercase()), &self.name, Vec::new()));
            }
        }
        Part::Group { color: Some(self.color.clone()), parts }
    }
}

/// The round's end, as `GameOverService` gives it to the sidebar: the reason, then the message about the winner or the
/// leader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct End {
    pub reason: Line,
    pub message: Option<Message>,
}

/// A yellow message about a team, `%s` being the team's name (`None` for a message with no team).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub key: &'static str,
    pub fallback: &'static str,
    pub team: Option<Standing>,
}

impl Message {
    fn part(&self) -> Part {
        let with = self.team.iter().map(Standing::name_part).collect();
        Part::Translate {
            key: self.key.to_string(),
            fallback: self.fallback.to_string(),
            color: Some("yellow".to_string()),
            with,
        }
    }
}

/// What the sidebar is made from. `teams` are the teams that play, in preset order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub phase: Phase,
    pub mode: Mode,
    pub hidden_items: bool,
    pub consume_items: bool,
    /// The win condition replaces goals (`☒` in the mode, and no full card).
    pub replace_goals: bool,
    pub features: Features,
    pub goal: Goal,
    /// The win condition is infinite (`+ ∞` after the goal).
    pub infinite: bool,
    /// Cards after this one, shown as ` +N` after the goal (the original's `remainingCards`).
    pub more_cards: Option<i32>,
    /// The win condition plays more than one card: the goal lists cards, and the team lines count them.
    pub multi_card: bool,
    pub clock: Clock,
    pub teams: Vec<Standing>,
    /// Pregame: `N of M ready!`, while the ready timer runs.
    pub ready_up: Option<Line>,
    /// Pregame: `Starting in ...` while the ready timer runs, else `Waiting to start...`.
    pub countdown: Option<Line>,
    /// Postgame: the end of the game.
    pub end: Option<End>,
}

const ICON_LOCKOUT: &str = "\u{1F512}";
const ICON_INVENTORY: &str = "\u{1F392}";
const ICON_HIDDEN: &str = "\u{2753}";
const ICON_CONSUME: &str = "\u{2B1A}";
const ICON_REPLACE: &str = "\u{2612}";
const ICON_ELYTRA: &str = "\u{1F987}";
const ICON_NIGHT_VISION: &str = "\u{1F441}";
const ICON_KEEP_INVENTORY: &str = "\u{1F392}";
const ICON_PVP: &str = "\u{1F5E1}";

/// What the original reads of one player to decide their sidebar: `ScoreboardService.shouldDisplayScoreboard`, and the
/// player filter of `ScoreboardController` (a player gets the sidebar of their own team, or of no team).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Viewer {
    /// The key of the player's team (`bingo_red`); `None` for a player on no team (`teamService.getPlayerTeam`).
    pub team: Option<String>,
    /// `PlayerSettings.scoreboard`.
    pub scoreboard: bool,
    /// `PlayerSettings.scoreboardAutoHide`.
    pub auto_hide: bool,
    /// `CardViewService.supportsCardHud`: the player's client runs the card HUD, so the auto-hide never hides it.
    pub card_hud: bool,
    /// The player is not in survival, or holds a map item in either hand: the auto-hide's test for a player without the HUD.
    pub holds_map_or_not_survival: bool,
}

/// Whether the original shows the sidebar to a player. A player on no team sees it only in the lobby world
/// (`state.isLobbyMode`); the port has no lobby world, so callers pass `false`, the default of `BingoState.isLobbyMode`.
pub fn shows(viewer: &Viewer, lobby_mode: bool) -> bool {
    if !lobby_mode && viewer.team.is_none() {
        return false;
    }
    if !viewer.scoreboard {
        return false;
    }
    if !viewer.auto_hide || viewer.card_hud {
        return true;
    }
    viewer.holds_map_or_not_survival
}

/// The sidebar of one player: their team's lines (see [`rows_for`]), or `None` when the sidebar is not shown to them.
pub fn rows_for_viewer(facts: &Facts, viewer: &Viewer, lobby_mode: bool) -> Option<Vec<Row>> {
    shows(viewer, lobby_mode).then(|| rows_for(facts, viewer.team.as_deref()))
}

/// The lines of the sidebar of a player on no team, top first, at most [`MAX_ROWS`]. The title is the objective's own, so
/// it is not here. This is what the shared sidebar shows everyone, until a per-player one is possible (see `PORT-GAP` in
/// [`rows_for`]).
pub fn rows(facts: &Facts) -> Vec<Row> {
    rows_for(facts, None)
}

/// The lines of the sidebar of a player on `team` (`None`: on no team), top first, at most [`MAX_ROWS`].
///
/// For a member of `team`, the `Team: <name>` line comes before the game (when several teams play), and in the ranking
/// their own row has the `➤` marker in their team's colour, white, while the other teams' rows are gray. A player on no
/// team sees the ranking in white, with no marker.
///
/// PORT-GAP: the original sends each team its own sidebar, which only that team's players are sent. Horizon's mod API has
/// no per-player sidebar (the engine's `native_sidebar` is not exposed to mods), so the shared sidebar (`rows`) is the
/// view of a player on no team, shown to everyone: the members of a team see no `➤` and no `Team:` line.
pub fn rows_for(facts: &Facts, team: Option<&str>) -> Vec<Row> {
    let rows = match facts.phase {
        Phase::Lobby => {
            let mut rows = header(facts);
            rows.push(Row::blank());
            rows.extend(team_line(facts, team));
            rows.extend(facts.ready_up.iter().chain(facts.countdown.iter()).map(|line| Row::of(vec![Part::of_line(line)])));
            rows
        }
        Phase::Live => {
            let mut rows = header(facts);
            rows.push(Row::blank());
            rows.extend(scores(facts, team));
            rows
        }
        Phase::Over => {
            let mut rows = Vec::new();
            if let Some(end) = &facts.end {
                rows.push(Row::of(vec![Part::of_line(&end.reason)]));
                rows.extend(end.message.iter().map(|message| Row::of(vec![message.part()])));
            }
            rows.push(Row::blank());
            rows.extend(scores(facts, team));
            rows
        }
    };
    let mut rows = tidy(rows);
    rows.truncate(MAX_ROWS);
    rows
}

/// The mode, features, goal and time lines (`%mode%`, `%features%`, `%goal%`, `%time%`), with the values in yellow.
fn header(facts: &Facts) -> Vec<Row> {
    let mode = match mode_icons(facts) {
        icons if icons.is_empty() => Part::translate("bingo.options.mode_standard", "Standard", Vec::new()),
        icons => Part::text(icons.join(" ")),
    };
    let features = match feature_icons(facts.features) {
        icons if icons.is_empty() => Part::translate("bingo.options.features.none", "None", Vec::new()),
        icons => Part::text(icons.join(" ")),
    };
    vec![
        Row::of(vec![Part::translate("bingo.scoreboard.mode", "Mode: %s", vec![yellow(mode)])]),
        Row::of(vec![Part::translate("bingo.scoreboard.features", "Features: %s", vec![yellow(features)])]),
        Row::of(vec![Part::translate("bingo.scoreboard.goal", "Goal: %s", vec![yellow(goal_part(facts))])]),
        Row::of(vec![clock_part(facts.clock)]),
    ]
}

/// The mode's icons in the original's order (`formatGameModeIcons`), plus `☒` for replaced goals.
fn mode_icons(facts: &Facts) -> Vec<&'static str> {
    let mut icons = Vec::new();
    if facts.mode == Mode::Lockout {
        icons.push(ICON_LOCKOUT);
    }
    if facts.mode == Mode::Inventory {
        icons.push(ICON_INVENTORY);
    }
    if facts.hidden_items {
        icons.push(ICON_HIDDEN);
    }
    if facts.consume_items {
        icons.push(ICON_CONSUME);
    }
    if facts.replace_goals {
        icons.push(ICON_REPLACE);
    }
    icons
}

/// The features' icons in the original's order (`formatFeaturesIcons`).
fn feature_icons(features: Features) -> Vec<&'static str> {
    let mut icons = Vec::new();
    if features.elytra {
        icons.push(ICON_ELYTRA);
    }
    if features.night_vision {
        icons.push(ICON_NIGHT_VISION);
    }
    if features.keep_inventory {
        icons.push(ICON_KEEP_INVENTORY);
    }
    if features.pvp {
        icons.push(ICON_PVP);
    }
    icons
}

/// The goal (`formatCardGoals`): `Full Card` or a count of items or lines, then ` +N` for the cards still to play, and
/// ` + ∞` for an infinite game.
fn goal_part(facts: &Facts) -> Part {
    // `BingoGoal.isFullCard`, and no full card when goals are replaced (`supportsFullCard`).
    let full_card = !facts.replace_goals
        && match facts.goal {
            Goal::FullCard => true,
            Goal::Lines(lines) => lines >= 11,
            Goal::Items(items) => items >= 25,
        };
    let mut parts = vec![if full_card {
        Part::translate("bingo.goal.full_card", "Full Card", Vec::new())
    } else {
        match facts.goal {
            Goal::Lines(lines) => count_part(lines as i32, "line"),
            Goal::Items(items) => count_part(items as i32, "item"),
            Goal::FullCard => count_part(25, "item"),
        }
    }];
    if let Some(more) = facts.more_cards {
        parts.push(Part::Text { text: format!(" +{more}"), color: None, bold: true });
    }
    if facts.infinite {
        parts.push(Part::text(" + ∞"));
    }
    Part::Group { color: Some("yellow".to_string()), parts }
}

/// A count of items, lines or cards as the lang file writes it: `1 line`, `4 lines` (`bingo.game.line_count_N`).
fn count_part(count: i32, kind: &str) -> Part {
    if (1..=9).contains(&count) {
        let fallback = if count == 1 { format!("1 {kind}") } else { format!("{count} {kind}s") };
        Part::translate(&format!("bingo.game.{kind}_count_{count}"), &fallback, Vec::new())
    } else {
        Part::translate(&format!("bingo.game.{kind}_count"), &format!("%s {kind}s"), vec![Part::text(count.to_string())])
    }
}

/// The clock line: the time left (`Time left: %s`) or the time played (`Time: %s`), in yellow. The time left turns red
/// every other second in the last 30 (`formatTimeRemaining`), and reads `(Game Over)` at zero.
fn clock_part(clock: Clock) -> Part {
    match clock {
        Clock::Limit(ms) => Part::translate(
            "bingo.scoreboard.time_left",
            "Time left: %s",
            vec![Part::text(texts::duration_text(ms)).colored("yellow")],
        ),
        Clock::Left(ms) if ms <= 0 => Part::translate(
            "bingo.scoreboard.time_left",
            "Time left: %s",
            vec![Part::translate("bingo.timer.game_over", "(Game Over)", Vec::new())],
        ),
        Clock::Left(ms) => {
            let seconds = ms / 1000;
            let color = if seconds <= 30 && seconds % 2 == 0 { "red" } else { "yellow" };
            Part::translate("bingo.scoreboard.time_left", "Time left: %s", vec![Part::text(texts::duration_text(ms)).colored(color)])
        }
        Clock::Elapsed(ms) => Part::translate(
            "bingo.scoreboard.time",
            "Time: %s",
            vec![Part::text(texts::duration_text(ms)).colored("yellow")],
        ),
        Clock::NoLimit => Part::translate(
            "bingo.scoreboard.time",
            "Time: %s",
            vec![Part::text(texts::duration_text(0)).colored("yellow")],
        ),
    }
}

/// The `Team: <name>` line (`%team%`, `getTeamName`): the viewer's team, when more than one team plays. None otherwise.
fn team_line(facts: &Facts, team: Option<&str>) -> Option<Row> {
    if facts.teams.len() <= 1 {
        return None;
    }
    let standing = facts.teams.iter().find(|standing| Some(standing.key.as_str()) == team)?;
    Some(Row::of(vec![Part::translate("bingo.scoreboard.team", "Team: %s", vec![standing.name_part()])]))
}

/// The team lines of a game with one team (the team's own view: `Items scored`, `Lines scored` and `Cards scored`, in
/// the team's colour), or the ranking of several teams, or nothing; then the blank line before the bottom of the list.
/// The team lines are the same for a viewer on that team and for one on no team: with one team, the leader is that team.
fn scores(facts: &Facts, team: Option<&str>) -> Vec<Row> {
    match facts.teams.as_slice() {
        [] => vec![Row::blank()],
        [team] => {
            let number = |n: i32| Part::text(n.to_string()).colored(&team.color);
            let mut rows = vec![
                Row::of(vec![Part::translate("bingo.scoreboard.items_scored", "Items scored: %s", vec![number(team.items)])]),
                Row::of(vec![Part::translate("bingo.scoreboard.lines_scored", "Lines scored: %s", vec![number(team.lines)])]),
            ];
            if facts.multi_card {
                rows.push(Row::of(vec![Part::translate("bingo.scoreboard.cards_scored", "Cards scored: %s", vec![number(team.cards)])]));
            }
            rows.push(Row::blank());
            rows
        }
        teams => top_ranking(ranked(teams), team)
            .into_iter()
            .map(|(index, standing)| {
                // `getTeamScores`: the viewer's own row is white with the marker in the team's colour, the other teams'
                // rows are gray, and a player on no team sees every row white.
                let own = team == Some(standing.key.as_str());
                let formatting = if own || team.is_none() { "white" } else { "gray" };
                let mut parts = Vec::new();
                if own {
                    parts.push(Part::Text { text: "\u{27A4} ".to_string(), color: Some(standing.color.clone()), bold: false });
                }
                parts.push(Part::text(format!("{}. ", index + 1)));
                parts.push(standing.name_part());
                Row {
                    text: vec![Part::Group { color: Some(formatting.to_string()), parts }],
                    score: Some(score_part(standing, facts.goal).colored(formatting)),
                }
            })
            .collect(),
    }
}

/// The ranking's rows: the first [`RANKING_ROWS`] teams, and the viewer's own team in place of the last one when it is not
/// among them (`ScoreboardController.getTeamScores`). The own team keeps its place number.
fn top_ranking<'a>(order: Vec<(usize, &'a Standing)>, team: Option<&str>) -> Vec<(usize, &'a Standing)> {
    let own = team.and_then(|key| order.iter().find(|(_, standing)| standing.key == key).copied());
    let mut top: Vec<(usize, &'a Standing)> = order.iter().take(RANKING_ROWS).copied().collect();
    if let Some((place, standing)) = own {
        if top.iter().all(|(_, shown)| shown.key != standing.key) {
            top.pop();
            top.push((place, standing));
        }
    }
    top
}

/// The score on a ranking line: the time a finished team took, else its cards, lines (for a lines goal) or items.
fn score_part(team: &Standing, goal: Goal) -> Part {
    if let Some(ms) = team.finished {
        Part::text(texts::clock_text(ms))
    } else if team.cards > 0 {
        count_part(team.cards, "card")
    } else if team.lines > 0 && matches!(goal, Goal::Lines(_)) {
        count_part(team.lines, "line")
    } else {
        count_part(team.items, "item")
    }
}

/// The teams in the ranking order of `ScoreRankingService`, each with its place: the teams that finished first, then by
/// cards, lines and items. Teams with the same score and finish share a place.
pub fn ranked(teams: &[Standing]) -> Vec<(usize, &Standing)> {
    let mut order: Vec<&Standing> = teams.iter().collect();
    order.sort_by_key(|team| (team.finished.unwrap_or(i64::MAX), Reverse(team.cards), Reverse(team.lines), Reverse(team.items)));
    let mut place = 0;
    let mut ranked = Vec::with_capacity(order.len());
    for (i, team) in order.iter().enumerate() {
        if i > 0 && !same_place(order[i - 1], team) {
            place += 1;
        }
        ranked.push((place, *team));
    }
    ranked
}

fn same_place(a: &Standing, b: &Standing) -> bool {
    a.finished == b.finished && (a.items, a.lines, a.cards) == (b.items, b.lines, b.cards)
}

/// The leading team by the first `depth` of cards, lines and items (`ScoreService.getLeadingByCards` and the like), after
/// the finish: `None` when two teams tie for it.
fn leader(teams: &[Standing], depth: u8) -> Option<&Standing> {
    let key = |team: &Standing| {
        (
            team.finished.unwrap_or(i64::MAX),
            Reverse(team.cards),
            Reverse(if depth >= 2 { team.lines } else { 0 }),
            Reverse(if depth >= 3 { team.items } else { 0 }),
        )
    };
    let best = teams.iter().map(key).min()?;
    let mut tied = teams.iter().filter(|team| key(team) == best);
    let first = tied.next()?;
    tied.next().is_none().then_some(first)
}

/// The message at the end (`GameOverService.getMessage`): the team that completed its card, else the leading team when
/// there is more than one, else a draw when the first win ends the game.
pub fn end_message(teams: &[Standing], winner: Option<&str>, first_win: bool) -> Option<Message> {
    if let Some(team) = winner.and_then(|key| teams.iter().find(|team| team.key == key)) {
        return Some(Message {
            key: "bingo.game.end.completed_card",
            fallback: "%s completed their card!",
            team: Some(team.clone()),
        });
    }
    if teams.len() <= 1 {
        return None;
    }
    let leading = [
        (1, "bingo.game.end.won_with_more_cards", "%s had the most cards!"),
        (2, "bingo.game.end.won_with_more_lines", "%s had the most lines!"),
        (3, "bingo.game.end.won_with_more_items", "%s had the most items!"),
    ];
    for (depth, key, fallback) in leading {
        if let Some(team) = leader(teams, depth) {
            return Some(Message { key, fallback, team: Some(team.clone()) });
        }
    }
    first_win.then_some(Message { key: "bingo.game.end.draw", fallback: "The game is a draw!", team: None })
}

/// Drops a blank line that follows a blank line, and the blank lines at the bottom (`ScoreboardController`).
fn tidy(rows: Vec<Row>) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for row in rows {
        if row.is_blank() && out.last().is_some_and(Row::is_blank) {
            continue;
        }
        out.push(row);
    }
    while out.last().is_some_and(Row::is_blank) {
        out.pop();
    }
    out
}

fn yellow(part: Part) -> Part {
    part.colored("yellow")
}

/// The English of a line: each part's fallback with its arguments, as the client shows it when it has no translation.
pub fn english(parts: &[Part]) -> String {
    parts.iter().map(english_part).collect()
}

/// The English of each line of the sidebar (see [`english`]).
pub fn english_lines(rows: &[Row]) -> Vec<String> {
    rows.iter().map(|row| english(&row.text)).collect()
}

fn english_part(part: &Part) -> String {
    match part {
        Part::Text { text, .. } => text.clone(),
        Part::Group { parts, .. } => english(parts),
        Part::Translate { fallback, with, .. } => {
            let mut args = with.iter().map(|arg| english_part(arg));
            let mut out = String::new();
            let mut chars = fallback.chars().peekable();
            while let Some(c) = chars.next() {
                match (c, chars.peek()) {
                    ('%', Some('%')) => {
                        chars.next();
                        out.push('%');
                    }
                    ('%', Some('s')) => {
                        chars.next();
                        out.push_str(&args.next().unwrap_or_default());
                    }
                    _ => out.push(c),
                }
            }
            out
        }
    }
}

/// A text component as JSON, for a console command.
pub fn component_json(parts: &[Part]) -> String {
    match parts {
        [] => r#"{"text":" "}"#.to_string(),
        [only] => part_json(only),
        many => {
            let extra: Vec<String> = many.iter().map(part_json).collect();
            format!(r#"{{"text":"","extra":[{}]}}"#, extra.join(","))
        }
    }
}

fn part_json(part: &Part) -> String {
    match part {
        Part::Text { text, color, bold } => {
            let mut fields = vec![format!(r#""text":{}"#, json_string(text))];
            push_style(&mut fields, color.as_deref(), *bold);
            format!("{{{}}}", fields.join(","))
        }
        Part::Translate { key, fallback, color, with } => {
            let mut fields = vec![format!(r#""translate":{}"#, json_string(key)), format!(r#""fallback":{}"#, json_string(fallback))];
            if !with.is_empty() {
                let args: Vec<String> = with.iter().map(part_json).collect();
                fields.push(format!(r#""with":[{}]"#, args.join(",")));
            }
            push_style(&mut fields, color.as_deref(), false);
            format!("{{{}}}", fields.join(","))
        }
        Part::Group { color, parts } => {
            let extra: Vec<String> = parts.iter().map(part_json).collect();
            let mut fields = vec![r#""text":"""#.to_string(), format!(r#""extra":[{}]"#, extra.join(","))];
            push_style(&mut fields, color.as_deref(), false);
            format!("{{{}}}", fields.join(","))
        }
    }
}

fn push_style(fields: &mut Vec<String>, color: Option<&str>, bold: bool) {
    if let Some(color) = color {
        fields.push(format!(r#""color":"{color}""#));
    }
    if bold {
        fields.push(r#""bold":true"#.to_string());
    }
}

/// A string as JSON: quotes, backslashes and control characters escaped.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The score holder of row `index`. A player name cannot have this form, so it is never a real player.
fn holder(index: usize) -> String {
    format!("bingo_row{index:02}")
}

/// The sidebar as shown on the server, so each change is sent as commands once: the lines last sent.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Shown {
    active: bool,
    /// Whether the objective an earlier run left in the world's save has been removed.
    swept: bool,
    rows: Vec<Row>,
}

impl Shown {
    pub const fn new() -> Self {
        Self { active: false, swept: false, rows: Vec::new() }
    }

    /// Makes the server's sidebar show `rows` (`None` takes it away). Only what changed is sent: a new sidebar is added and
    /// shown first; a row's text is sent when it changes, and its score when the score does. The score of row `i` is
    /// `MAX_ROWS - i`, so the top row is the highest and the scores do not move when the rows change.
    ///
    /// The objective is saved in the world, so a server that starts on a world an earlier run left behind still holds that
    /// run's rows. The first update removes the objective (removing it also clears its display slot) before anything else,
    /// so no row this state did not make survives: it would otherwise show an old header next to the new lines.
    pub fn update(&mut self, host: &mut dyn ServerHost, rows: Option<Vec<Row>>) {
        if !self.swept {
            host.command(&format!("scoreboard objectives remove {OBJECTIVE}"));
            self.swept = true;
        }
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
            host.command(&format!(
                "scoreboard objectives add {OBJECTIVE} dummy {{\"translate\":\"{TITLE_KEY}\",\"fallback\":\"{TITLE}\"}}"
            ));
            host.command(&format!("scoreboard objectives modify {OBJECTIVE} numberformat blank"));
            host.command(&format!("scoreboard objectives setdisplay sidebar {OBJECTIVE}"));
            self.active = true;
            self.rows.clear();
        }

        for (index, row) in rows.iter().enumerate() {
            let holder = holder(index);
            let old = self.rows.get(index);
            if old == Some(row) {
                continue;
            }
            if old.is_none() {
                host.command(&format!("scoreboard players set {holder} {OBJECTIVE} {}", MAX_ROWS - index));
            }
            if old.map(|old| &old.text) != Some(&row.text) {
                host.command(&format!(
                    "scoreboard players display name {holder} {OBJECTIVE} {}",
                    component_json(&row.text)
                ));
            }
            if old.map(|old| &old.score).unwrap_or(&None) != &row.score {
                match &row.score {
                    Some(score) => host.command(&format!(
                        "scoreboard players display numberformat {holder} {OBJECTIVE} fixed {}",
                        component_json(std::slice::from_ref(score))
                    )),
                    // Cleared, the holder takes the objective's format, which is blank.
                    None => host.command(&format!("scoreboard players display numberformat {holder} {OBJECTIVE}")),
                }
            }
        }
        for index in rows.len()..self.rows.len() {
            host.command(&format!("scoreboard players reset {} {OBJECTIVE}", holder(index)));
        }
        self.rows = rows;
    }
}

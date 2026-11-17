//! The end-of-game scores list and the game info panel, ported from `hud/screen/BingoEndScoresWidget.kt` and
//! `hud/screen/GameInfoWidget.kt`. Pure: the caller gives the packet, the cards (for each team's players), the
//! language lookup, a text-width function, the widget's box and the wall clock; the result is draw commands and
//! hit regions in the widget's own pixels (its content origin is the top left of the scores list).
//!
//! The Kotlin `IScrollableWidget` overrides `extractContents` without a scroll offset, so the list is drawn at fixed
//! positions. Clipping to the box is the host's job (`AbstractTextAreaWidget`).
//!
//! Team names keep their colours: each row's team text is drawn run by run (`Row::team_runs`), and a player's head in
//! a team name is an 8 x 8 `Sprite` where its two spaces are. Not drawn here, on purpose: the tab row itself
//! (`TabsWidget`), the player head icon of a tooltip line (the host draws it; `Tooltip` is a command for the host's
//! `drawTooltip`), and the tooltip box itself. Medals, places beyond `#n`, and a highlight of the viewer's team do not
//! exist in these two Kotlin files, so they are not here either.

use crate::card_draw::truncate_to_width;
use crate::card_images::face_sprite;
use crate::display_cards::PlayerProfile;
use crate::game_over::{EndScreenTab, GameOverPacket};
use crate::game_status::{GameStatusPacket, JavaDuration};
use crate::state::Cards;
use crate::text_plain::{format_template, runs, Lang, StyledRun};

/// `Font.lineHeight` of the 26.3 font (`FontImpl.getTextHeight`).
const TEXT_HEIGHT: i32 = 9;
/// `entryHeight = client.font.getTextHeight() + 8`.
pub const ENTRY_HEIGHT: i32 = TEXT_HEIGHT + 8;
/// `yOffset = entryHeight/2 - getTextHeight()/2`: where the text sits inside a row.
const Y_OFFSET: i32 = ENTRY_HEIGHT / 2 - TEXT_HEIGHT / 2;
/// `teamCol = 4`.
pub const TEAM_COL: i32 = 4;
/// `IScrollableWidget.padding` default.
pub const DEFAULT_PADDING: i32 = 4;

/// `Formatting.GRAY` (0xAAAAAA) and `Formatting.YELLOW` (0xFFFF55), with alpha.
const GRAY: u32 = 0xFF_AAAAAA;
const YELLOW: u32 = 0xFF_FFFF55;
/// `0xFF_FFFFFF`: the colour a widget draws unstyled text in.
const WHITE: u32 = 0xFF_FFFFFF;

/// `GameInfoWidget`: line height is the text height + 2, the box is 8 px of frame on each side.
const LINE_STEP: i32 = TEXT_HEIGHT + 2;
const INFO_FRAME: i32 = 8;

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const NANOS_PER_MINUTE: i128 = 60 * NANOS_PER_SECOND;
const NANOS_PER_HOUR: i128 = 60 * NANOS_PER_MINUTE;
const NANOS_PER_DAY: i128 = 24 * NANOS_PER_HOUR;

/// A command the host draws. The `card_draw::Draw` conventions hold (pixels, ARGB, top left origin), and it wraps
/// `Draw` for the two things `Draw` cannot say.
// TODO(integration): `Draw::Text` has no shadow flag and no obfuscation; `Text` below carries both. `NineSlice` is
// `drawNinePatch` (the `bingo/game_container` frame): add it to `Draw`, or the host maps this variant.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenDraw {
    /// A command `card_draw` already has.
    Base(crate::card_draw::Draw),
    /// A texture of the atlas, `w` by `h` pixels at (x, y): a player's face in a team name (`face_sprite`).
    Sprite { name: String, x: i32, y: i32, w: i32, h: i32 },
    /// Text in one colour. `obfuscated` text keeps its string; the host draws random glyphs of the same width.
    Text { x: i32, y: i32, text: String, argb: u32, shadow: bool, obfuscated: bool },
    /// `drawNinePatch`: a texture cut into nine pieces, the borders `slice` pixels wide.
    NineSlice { name: String, x: i32, y: i32, w: i32, h: i32, slice: i32, tex_w: i32, tex_h: i32 },
    /// `drawTooltip` at the mouse position. Each line is a player's head (when `head` is set) then the name.
    Tooltip { x: i32, y: i32, lines: Vec<TooltipLine> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipLine {
    /// The player's uuid for the head icon; `None` for the team-name line.
    pub head: Option<u128>,
    pub text: String,
}

/// A rectangle in the widget's content pixels, and what a click there means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Hovering the team cell of row `row` shows its players. Clicking does nothing in the original.
    TeamPlayers { row: usize },
}

/// Everything `BingoEndScoresWidget.resize` reads.
pub struct ScoresInput<'a> {
    pub game_over: &'a GameOverPacket,
    /// For each team's players (`gameOver.cards.find { it.teamKey == score.key }?.display?.players`).
    pub cards: &'a Cards,
    pub lang: Lang<'a>,
    /// `Font.getTextWidth` of a flattened string.
    pub text_width: &'a dyn Fn(&str) -> i32,
    /// `IScrollableWidget.width`.
    pub width: i32,
    /// `IScrollableWidget.padding`.
    pub padding: i32,
}

/// A stretch of a team name in one colour, measured at layout time. Adjacent stretches of one colour are one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamRun {
    pub text: String,
    /// The colour with alpha: the row's white when the name sets none, else `0xFF_000000 | rgb`.
    pub argb: u32,
    /// A player's head: drawn as its 8 x 8 face where the run starts, and the run keeps its width (its two spaces).
    pub head: Option<u128>,
    pub width: i32,
}

/// One row of the list, as `ScoreEntry` holds it after `resize`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// `#n name`, cut to fit before the score column.
    pub team: String,
    /// The width of `team` after the cut: the hover range ends here.
    pub team_width: i32,
    /// `team` in its colours, cut to the same characters as `team`.
    pub team_runs: Vec<TeamRun>,
    pub score: String,
    pub duration: String,
    /// `teamTooltip`: `None` when the team has no card shown; an empty list is a tooltip with no lines.
    pub tooltip: Option<Vec<TooltipLine>>,
}

/// The laid-out list: the column positions, the titles and the rows.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoresLayout {
    pub score_col: i32,
    pub duration_col: i32,
    pub titles: [String; 3],
    pub rows: Vec<Row>,
    /// `gameOver.packet.endedAt`, as epoch milliseconds.
    pub ended_at_ms: i64,
}

fn tr(lang: Lang<'_>, key: &str, fallback: &str) -> String {
    lang(key).unwrap_or_else(|| fallback.to_string())
}

/// `TextProvider.itemCount` and its kin: `<key>_<n>` when the language has it (1 to 9), else `%s` filled with n.
fn count_text(lang: Lang<'_>, base: &str, n: i32, plural: &str) -> String {
    if (1..=9).contains(&n) {
        if let Some(s) = lang(&format!("{base}_{n}")) {
            return s;
        }
    }
    format_template(&tr(lang, base, plural), &[n.to_string()])
}

/// `TeamScore.formatText`: cards, lines, items (only the non-zero ones), joined by ", ", or "0".
pub fn score_text(lang: Lang<'_>, cards: i32, lines: i32, items: i32) -> String {
    let mut parts = Vec::new();
    if cards > 0 {
        parts.push(count_text(lang, "bingo.game.card_count", cards, "%s cards"));
    }
    if lines > 0 {
        parts.push(count_text(lang, "bingo.game.line_count", lines, "%s lines"));
    }
    if items > 0 {
        parts.push(count_text(lang, "bingo.game.item_count", items, "%s items"));
    }
    if parts.is_empty() {
        "0".to_string()
    } else {
        parts.join(", ")
    }
}

/// `BingoEndScoresWidget.resize`: the rows, the three columns and the cut of the team names.
pub fn layout(input: &ScoresInput<'_>) -> ScoresLayout {
    let lang = input.lang;
    let width_of = input.text_width;
    let titles = [
        tr(lang, "bingo.game.end.scores.team", "Team"),
        tr(lang, "bingo.game.end.scores.score", "Score"),
        tr(lang, "bingo.game.end.scores.time", "Time"),
    ];
    let dnf = tr(lang, "bingo.game.end.scores.dnf", "DNF");

    struct Pending {
        team: String,
        team_runs: Vec<StyledRun>,
        score: String,
        duration: String,
        tooltip: Option<Vec<TooltipLine>>,
    }
    let pending: Vec<Pending> = input
        .game_over
        .scores
        .iter()
        .map(|s| {
            let name_runs = runs(&s.name, lang);
            let name: String = name_runs.iter().map(|r| r.text.as_str()).collect();
            let players: Option<&[PlayerProfile]> =
                input.cards.get(&Some(s.key.clone())).map(|c| c.display.players.as_slice());
            let tooltip = players.map(|ps| {
                let mut lines: Vec<TooltipLine> =
                    ps.iter().map(|p| TooltipLine { head: Some(p.uuid), text: p.name.clone() }).collect();
                // if there's >1 player, include the team name in the tooltip
                if ps.len() > 1 {
                    lines.insert(0, TooltipLine { head: None, text: name.clone() });
                }
                lines
            });
            let duration = s
                .duration
                .as_deref()
                .and_then(parse_java_duration)
                .map(format_hhmmss)
                // an unreadable duration is shown as DNF too: the original's parser would drop the packet
                .unwrap_or_else(|| dnf.clone());
            // The `#n ` is unstyled, so it takes the row's colour; the name keeps its own runs.
            let mut team_runs = vec![StyledRun { text: format!("#{} ", s.index + 1), rgb: None, head: None }];
            team_runs.extend(name_runs);
            Pending {
                team: team_runs.iter().map(|r| r.text.as_str()).collect(),
                team_runs,
                score: score_text(lang, s.score.cards, s.score.lines, s.score.items),
                duration,
                tooltip,
            }
        })
        .collect();

    let team_max = pending.iter().map(|p| width_of(&p.team)).chain([width_of(&titles[0])]).max().unwrap_or(0);
    let score_max = pending.iter().map(|p| width_of(&p.score)).chain([width_of(&titles[1])]).max().unwrap_or(0);
    let duration_max = pending.iter().map(|p| width_of(&p.duration)).chain([width_of(&titles[2])]).max().unwrap_or(0);

    let content_width = input.width - input.padding * 2;
    let duration_col = content_width - duration_max - 4;
    let score_max_x = duration_col - score_max - 4;
    let score_min_x = TEAM_COL + team_max;
    let score_col = ((score_min_x + score_max_x) / 2).min(score_max_x);

    let rows = pending
        .into_iter()
        .map(|p| {
            let team = truncate_to_width(&p.team, score_col - TEAM_COL, width_of);
            let team_runs = team_runs_of(&p.team_runs, team.chars().count(), width_of);
            Row {
                team_width: width_of(&team),
                team,
                team_runs,
                score: p.score,
                duration: p.duration,
                tooltip: p.tooltip,
            }
        })
        .collect();

    ScoresLayout {
        score_col,
        duration_col,
        titles,
        rows,
        ended_at_ms: input.game_over.ended_at_millis,
    }
}

/// The first `chars` characters of a team name's runs, one `TeamRun` per colour. Adjacent runs of one colour merge,
/// and a head never merges. A head is one piece: when the cut falls inside its two spaces the head is not drawn and
/// the name ends before it. Each piece is measured with `width_of`.
fn team_runs_of(runs: &[StyledRun], chars: usize, width_of: &dyn Fn(&str) -> i32) -> Vec<TeamRun> {
    let mut left = chars;
    let mut pieces: Vec<(String, u32, Option<u128>)> = Vec::new();
    for r in runs {
        if left == 0 {
            break;
        }
        if r.head.is_some() && r.text.chars().count() > left {
            break;
        }
        let text: String = r.text.chars().take(left).collect();
        left -= text.chars().count();
        if text.is_empty() {
            continue;
        }
        let argb = r.rgb.map_or(WHITE, |c| 0xFF_000000 | c);
        match pieces.last_mut() {
            Some((last, last_argb, None)) if *last_argb == argb && r.head.is_none() => last.push_str(&text),
            _ => pieces.push((text, argb, r.head)),
        }
    }
    pieces.into_iter().map(|(text, argb, head)| TeamRun { width: width_of(&text), text, argb, head }).collect()
}

impl ScoresLayout {
    /// `measureContentHeight`: a title row and one row per score.
    pub fn content_height(&self) -> i32 {
        ENTRY_HEIGHT * (self.rows.len() as i32 + 1)
    }

    /// `isTextHovered`, with the original's inclusive ranges: x from the team column to the text's width (not
    /// `teamCol + width`), y over the row's height plus one pixel.
    fn hovered(&self, row: usize, mouse_x: i32, mouse_y: i32) -> bool {
        let r = &self.rows[row];
        let top = (row as i32 + 1) * ENTRY_HEIGHT;
        (TEAM_COL..=r.team_width).contains(&mouse_x) && (top..=top + ENTRY_HEIGHT).contains(&mouse_y)
    }

    /// `renderContents`. `now_epoch_ms` is the wall clock (`Instant.now()`); the mouse is in content pixels.
    /// Row `i` is revealed once `i + 1` whole seconds have passed since the game ended.
    pub fn draw(&self, now_epoch_ms: i64, mouse_x: i32, mouse_y: i32) -> Vec<ScreenDraw> {
        let text = |x: i32, y: i32, s: &str, argb: u32, obfuscated: bool| ScreenDraw::Text {
            x,
            y,
            text: s.to_string(),
            argb,
            shadow: true,
            obfuscated,
        };
        let mut out = Vec::new();
        let cols = [TEAM_COL, self.score_col, self.duration_col];
        for (title, x) in self.titles.iter().zip(cols) {
            out.push(text(x, Y_OFFSET, title, GRAY, false));
        }

        // `(now - endedAt).seconds`: whole seconds, rounded down.
        let seconds_elapsed = (now_epoch_ms - self.ended_at_ms).div_euclid(1000);
        for (i, row) in self.rows.iter().enumerate() {
            let obfuscated = seconds_elapsed < i as i64 + 1;
            let y = (i as i32 + 1) * ENTRY_HEIGHT + Y_OFFSET;
            let mut x = TEAM_COL;
            for run in &row.team_runs {
                match run.head {
                    // The face is 8 x 8 with its top at the text's top; the run's width is its two spaces.
                    Some(uuid) => out.push(ScreenDraw::Sprite { name: face_sprite(uuid), x, y, w: 8, h: 8 }),
                    None => out.push(text(x, y, &run.text, run.argb, obfuscated)),
                }
                x += run.width;
            }
            out.push(text(self.score_col, y, &row.score, WHITE, obfuscated));
            out.push(text(self.duration_col, y, &row.duration, WHITE, obfuscated));
            if let Some(tooltip) = &row.tooltip {
                if self.hovered(i, mouse_x, mouse_y) {
                    out.push(ScreenDraw::Tooltip { x: mouse_x, y: mouse_y, lines: tooltip.clone() });
                }
            }
        }
        out
    }

    /// The team cells that have a tooltip, as the rectangles `hovered` accepts (both ends inclusive, so the width
    /// and height get one pixel more than the span).
    pub fn hits(&self) -> Vec<Hit> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.tooltip.is_some())
            .map(|(i, r)| Hit {
                x: TEAM_COL,
                y: (i as i32 + 1) * ENTRY_HEIGHT,
                w: (r.team_width - TEAM_COL + 1).max(0),
                h: ENTRY_HEIGHT + 1,
                action: Action::TeamPlayers { row: i },
            })
            .collect()
    }
}

/// `Duration.parse` for the text `Duration.toString` writes (`PnDTnHnMn.nS`, each part may carry its own sign),
/// in nanoseconds. `None` when it is not that form.
pub fn parse_java_duration(s: &str) -> Option<i128> {
    let rest = s.strip_prefix('P')?;
    let (date, time) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut total = parse_parts(date, &[('D', NANOS_PER_DAY)])?;
    if let Some(t) = time {
        total += parse_parts(t, &[('H', NANOS_PER_HOUR), ('M', NANOS_PER_MINUTE), ('S', NANOS_PER_SECOND)])?;
    }
    Some(total)
}

fn parse_parts(mut s: &str, units: &[(char, i128)]) -> Option<i128> {
    let mut total = 0i128;
    while !s.is_empty() {
        let end = s.find(|c: char| c.is_ascii_alphabetic())?;
        let (number, rest) = s.split_at(end);
        let unit = rest.chars().next()?;
        let scale = units.iter().find(|(u, _)| *u == unit)?.1;
        total += decimal_times(number, scale)?;
        s = &rest[1..];
    }
    Some(total)
}

/// A decimal such as `-1.5` multiplied by a unit, exactly.
fn decimal_times(number: &str, scale: i128) -> Option<i128> {
    let (negative, body) = match number.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, number.strip_prefix('+').unwrap_or(number)),
    };
    let (whole, frac) = body.split_once('.').unwrap_or((body, ""));
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if frac.len() > 18 || !whole.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole: i128 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let mut frac_value = 0i128;
    let mut denominator = 1i128;
    for c in frac.chars() {
        frac_value = frac_value * 10 + i128::from(c.to_digit(10)?);
        denominator *= 10;
    }
    let value = whole * scale + frac_value * scale / denominator;
    Some(if negative { -value } else { value })
}

/// `JavaDuration` in nanoseconds.
pub fn nanos_of(d: JavaDuration) -> i128 {
    i128::from(d.seconds) * NANOS_PER_SECOND + i128::from(d.nanos)
}

/// `Duration.formatHHMMSS`: `H:MM:SS` when there are hours, else `M:SS`; a leading `-` when negative.
pub fn format_hhmmss(nanos: i128) -> String {
    let sign = if nanos < 0 { "-" } else { "" };
    let secs = nanos.abs() / NANOS_PER_SECOND;
    let (hours, minutes, seconds) = (secs / 3600, secs / 60 % 60, secs % 60);
    if hours > 0 {
        format!("{sign}{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{sign}{minutes}:{seconds:02}")
    }
}

/// `Duration.formatString`: `3d 2h 01m 32s`, the days and hours left out when they are 0, the sign when negative.
pub fn format_string(nanos: i128) -> String {
    let sign = if nanos < 0 { "-" } else { "" };
    let secs = nanos.abs() / NANOS_PER_SECOND;
    let (days, hours, minutes, seconds) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let mut out = sign.to_string();
    if days > 0 {
        out.push_str(&format!("{days}d "));
    }
    if hours > 0 {
        out.push_str(&format!("{hours}h "));
    }
    out.push_str(&format!("{minutes:02}m {seconds:02}s"));
    out
}

/// One coloured piece of a status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub argb: u32,
}

/// A line of the game info panel: runs drawn one after another, the whole line centred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub runs: Vec<Run>,
}

impl StatusLine {
    fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// `MessageHistoryWidget.resize` (the feeder of `GameInfoWidget.text`): the time line, then "waiting to start".
/// `status` is `None` when no `game_status` has arrived (the Kotlin `GameStatusPacket.DEFAULT`).
pub fn status_lines(status: Option<&GameStatusPacket>, lang: Lang<'_>) -> Vec<StatusLine> {
    let mut lines = Vec::new();
    if let Some(status) = status {
        let timed = status
            .remaining_duration
            .map(|d| ("bingo.scoreboard.time_left", "Time left: %s", d))
            .or_else(|| status.ingame_duration.map(|d| ("bingo.scoreboard.time", "Time: %s", d)));
        if let Some((key, fallback, d)) = timed {
            lines.push(labelled(&tr(lang, key, fallback), &format_string(nanos_of(d))));
        }
        if status.ingame_duration.is_none() {
            lines.push(StatusLine {
                runs: vec![Run {
                    text: tr(lang, "bingo.game.waiting_to_start", "Waiting to start..."),
                    argb: GRAY,
                }],
            });
        }
    }
    lines
}

/// A template with one `%s`: the text around it in white, the value in yellow.
fn labelled(template: &str, value: &str) -> StatusLine {
    let (before, after) = template.split_once("%s").unwrap_or((template, ""));
    let mut runs = Vec::new();
    for (text, argb) in [(before, WHITE), (value, YELLOW), (after, WHITE)] {
        if !text.is_empty() {
            runs.push(Run { text: text.to_string(), argb });
        }
    }
    StatusLine { runs }
}

/// `GameInfoWidget`: the frame and the centred lines. `height` is `lines * (text height + 2) + 16`.
pub struct GameInfo {
    pub width: i32,
    pub height: i32,
    /// Each line with the width of its whole text and of each run, measured once (`resize`).
    measured: Vec<(StatusLine, i32, Vec<i32>)>,
}

impl GameInfo {
    /// `size.x = width`, then `resize()`.
    pub fn new(lines: Vec<StatusLine>, width: i32, width_of: &dyn Fn(&str) -> i32) -> Self {
        let height = lines.len() as i32 * LINE_STEP + INFO_FRAME * 2;
        let measured = lines
            .into_iter()
            .map(|l| {
                let total = width_of(&l.text());
                let runs = l.runs.iter().map(|r| width_of(&r.text)).collect();
                (l, total, runs)
            })
            .collect();
        GameInfo { width, height, measured }
    }

    /// `render`: the nine-patch frame at the origin, then each line centred, the first at y = 9.
    pub fn draw(&self) -> Vec<ScreenDraw> {
        let mut out = vec![ScreenDraw::NineSlice {
            name: "bingo/game_container".to_string(),
            x: 0,
            y: 0,
            w: self.width,
            h: self.height,
            slice: 8,
            tex_w: 200,
            tex_h: 40,
        }];
        let mut title_y = 9;
        for (line, line_width, run_widths) in &self.measured {
            let mut x = (self.width - line_width) / 2;
            for (run, w) in line.runs.iter().zip(run_widths) {
                out.push(ScreenDraw::Text {
                    x,
                    y: title_y,
                    text: run.text.clone(),
                    argb: run.argb,
                    shadow: false,
                    obfuscated: false,
                });
                x += w;
            }
            title_y += LINE_STEP;
        }
        out
    }
}

/// The tabs of the end screen (`TabsWidget` is the host's): their labels, the one open at first, and whether the
/// row is shown at all (`BingoHudScreen.isShowingTabs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tabs {
    pub labels: [String; 2],
    pub current: usize,
    pub shown: bool,
}

/// `isShowingTabs`: shown when more than one team has a duration, or the open tab or the packet's default tab is
/// not Cards. `current` is the tab open now (at first, the packet's default).
pub fn tabs(packet: &GameOverPacket, current: EndScreenTab, lang: Lang<'_>) -> Tabs {
    Tabs {
        labels: [
            tr(lang, "bingo.game.end.tab.cards", "Cards"),
            tr(lang, "bingo.game.end.tab.scores", "Scores"),
        ],
        current: match current {
            EndScreenTab::Cards => 0,
            EndScreenTab::Scores => 1,
        },
        shown: packet.scores.len() > 1 || current != EndScreenTab::Cards || packet.default_tab != EndScreenTab::Cards,
    }
}

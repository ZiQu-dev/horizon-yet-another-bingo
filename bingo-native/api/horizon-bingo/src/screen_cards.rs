//! The card screen's row of cards and the score message history, ported from the original client's
//! `BingoCardsWidget.kt` (card positions and z order, scrolling by wheel, keys and the scroll bar, card selection,
//! tile hover and tile tooltips) and `MessageHistoryWidget.kt` (the game-over title or the status box, then the past
//! messages, newest first).
//!
//! Pure, like `card_draw` and `message_draw`: nothing is drawn or clicked here. A frame returns the draw commands in
//! paint order, the hit regions a click can land on (topmost first), the tile tooltip to draw last, and the scroll
//! state for the next frame. Mouse positions are GUI pixels. A widget's `x` and `y` are its centre, as in the original.
//!
//! Hit regions are half-open rectangles `[x, x + w) x [y, y + h)`. The original's `a..b` ranges are inclusive, so an
//! inclusive end is written as one more pixel (the card is 123 pixels wide for hover, though it draws 122).
//!
//! Not drawn yet, each marked TODO(integration) where it happens: the card's scale and opacity (`Draw` has neither;
//! the entrance scale is applied to positions and sizes), the rotated "winner" label, the player list tooltip, the
//! thumbnails of a tile tooltip, text shadows and text scale under `Draw::Text`, the nine-slice of the game-over box
//! (a `ScreenDraw` command here), and the message icons that message_draw keeps private.

use crate::card_draw::{draw_card, CardView, Draw, ItemNames, TeamColors, CARD_HEIGHT, CARD_WIDTH};
use crate::card_tile::CardTile;
use crate::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use crate::game_message::MessageType;
use crate::game_over::GameOverPacket;
use crate::message_draw::ITEM_HEIGHT;
use crate::state::{ClientCard, ScoreMessage, TeamKey, TILES_PER_CARD};
use crate::text_plain::{flatten, runs, Lang, StyledRun};

/// `MessageHistoryWidget.marginX` and `marginY`.
const MARGIN_X: i32 = 14;
const MARGIN_Y: i32 = 12;
/// `Font.lineHeight` of the 26.3 font, and the spacing of the lines of a text block (`textHeight + 2`).
const TEXT_HEIGHT: i32 = 9;
const LINE_STEP: i32 = TEXT_HEIGHT + 2;
/// `ClientCardRenderer.TOOLTIP_MAX_WIDTH`: lore lines wrap at this width (except the last line).
const TOOLTIP_MAX_WIDTH: i32 = 200;
/// The focus outline of a card, `ClientCardRenderer.cardTextureOutline` (the colours' default).
pub const OUTLINE_SPRITE: &str = "bingo/outline";
/// `BingoCardsWidget` colours: the scroll bar when hovered, while dragged, and idle; the track's alpha is 80.
const BAR_HOVER: u32 = 0xb3b3c9;
const BAR_DRAG: u32 = 0xffffff;
const BAR_IDLE: u32 = 0x6b6b85;
const TRACK_ALPHA: i32 = 80;
const WHITE: u32 = 0xffff_ffff;
/// `CardTileRenderer` and `BingoMessageRenderer`: the message's icon and text offsets, at scale 1.
const MESSAGE_ICON_X: i32 = 8;
const MESSAGE_TEXT_X: i32 = 28;
/// `BingoMessageRenderer.drawMessage`: the message is moved 6 pixels left in the history.
const HISTORY_OFFSET_X: i32 = -6;
/// `MessageHistoryWidget` wraps its messages at the widget width less 4 pixels.
const HISTORY_INSET: i32 = 4;

/// One card of the row: its team, the card, its team colours and the team's name, already cut to fit.
pub struct CardSlot<'a> {
    pub team: TeamKey,
    pub card: &'a ClientCard,
    pub colors: &'a TeamColors,
    pub team_name: Option<String>,
}

/// The widget's rectangle: `x`, `y` are its centre, `width` and `height` its size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub width: i32,
    pub height: i32,
}

/// The entrance animation of the card screen (`interpolateKey`, `interpolateFrom*`, `interpolateDuration`). The
/// card of `team` slides and scales from its `from` values; every card is drawn at `from_scale` at first.
#[derive(Debug, Clone, PartialEq)]
pub struct Entrance {
    pub team: TeamKey,
    pub from_x: f32,
    pub from_y: f32,
    pub from_scale: f32,
    pub duration_ms: u64,
}

/// What the frame needs from the host for one frame.
pub struct CardsInput<'a> {
    pub area: Area,
    pub slots: &'a [CardSlot<'a>],
    /// `winner`: the team whose card is marked as the winner, and where the row starts scrolled.
    pub winner: Option<&'a str>,
    pub entrance: Option<Entrance>,
    /// Milliseconds since the entrance started (`Interpolate` reads the host's clock).
    pub elapsed_ms: u64,
    pub now_ms: u64,
    pub opacity: f32,
    /// `onViewClick != null`: clicking a card selects it, and the hovered card is outlined.
    pub selectable: bool,
    pub mouse: (i32, i32),
    pub show_outlines: bool,
    pub team_outline: &'a dyn Fn(&str) -> u32,
    pub items: &'a dyn ItemNames,
    /// Which dynamic picture sprites exist (see `card_draw::CardView::images`).
    pub images: Option<&'a dyn Fn(&str) -> bool>,
    pub lang: Lang<'a>,
    pub text_width: &'a dyn Fn(&str) -> i32,
}

/// The state the widget keeps between frames: the drawn scroll position `scroll_x` (0 to 1), the position it moves
/// towards, and a drag that is running.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardsScroll {
    pub scroll_x: f32,
    pub target_x: f32,
    pub drag: Option<Drag>,
}

/// A drag: where it started, the scroll it started from, and whether it grabbed the scroll bar (else it pans).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub start: (f64, f64),
    pub initial_scroll: f32,
    pub on_bar: bool,
}

impl CardsScroll {
    pub fn starting_at(scroll: f32) -> Self {
        CardsScroll { scroll_x: scroll, target_x: scroll, drag: None }
    }
}

/// What a click or hover can land on.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// The tile `index` (0 to 24) of the card of `team`: `BingoCardsWidget.onTileClick`.
    Tile { team: TeamKey, index: usize },
    /// The card of `team` was clicked and no tile took the click: `onViewClick`.
    SelectCard(TeamKey),
    /// The scroll bar, where a drag can start.
    ScrollBar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub action: Action,
}

impl Hit {
    pub fn contains(&self, mouse: (i32, i32)) -> bool {
        mouse.0 >= self.x && mouse.0 < self.x + self.w && mouse.1 >= self.y && mouse.1 < self.y + self.h
    }
}

/// `ClientCardRenderer.drawTooltip`: the lines of the tile under the mouse, drawn with the vanilla tooltip.
#[derive(Debug, Clone, PartialEq)]
pub struct TileTooltip {
    pub team: TeamKey,
    pub index: usize,
    pub lines: Vec<String>,
    pub mouse: (i32, i32),
}

/// Where one card's commands are in `CardsFrame::draws` and `CardsFrame::labels`, as half-open ranges of indices.
#[derive(Debug, Clone, PartialEq)]
pub struct CardSpan {
    pub team: TeamKey,
    pub draws: (usize, usize),
    pub labels: (usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardsFrame {
    pub draws: Vec<Draw>,
    /// Turned text over the cards (the winner's label). Each one belongs to its card's span and is painted right after
    /// that card's draws, so a nearer card covers it (`screen_controller::card_commands` interleaves them).
    pub labels: Vec<ScreenDraw>,
    pub hits: Vec<Hit>,
    pub tooltip: Option<TileTooltip>,
    /// The scroll state after this frame's step; keep it for the next frame.
    pub scroll: CardsScroll,
    /// One span per card, in the order the cards were painted. The scroll bar is in neither span.
    pub card_spans: Vec<CardSpan>,
    /// The index in `card_spans` of the card sliding in from the HUD, while the entrance runs. None when there is no
    /// entrance or it is done, so every card is then an ordinary card.
    pub entrance_card: Option<usize>,
}

/// `BingoCardsWidget.initialScollX`: the winner's place in the row, else the start.
pub fn initial_scroll(slots: &[CardSlot<'_>], winner: Option<&str>) -> f32 {
    let Some(winner) = winner else { return 0.0 };
    let Some(index) = slots.iter().position(|slot| slot.team.as_deref() == Some(winner)) else { return 0.0 };
    // With one card, the original divides by zero and the render resets the scroll to 0.
    if slots.len() <= 1 {
        0.0
    } else {
        index as f32 / (slots.len() - 1) as f32
    }
}

/// `Math.round`: halves go up, unlike `f32::round`.
fn round_half_up(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// `cardsAreaWidth`: six tenths of the widget.
fn cards_area_width(area: &Area) -> i32 {
    area.width * 6 / 10
}

/// `cardsTotalWidth` of `count` cards.
fn cards_total_width(count: usize) -> i32 {
    count as i32 * CARD_WIDTH
}

/// `barFilledWidth`: the thumb is the share of the row that is visible.
fn bar_filled_width(area_w: i32, total: i32) -> i32 {
    area_w * area_w / total.max(1)
}

/// `isInBounds` on the widget's x range, both ends inclusive.
fn bounds_x(area: &Area) -> (i32, i32) {
    let half = area.width / 2;
    ((area.x - half as f32) as i32, (area.x + half as f32) as i32)
}

/// `isInBounds` on the widget's y range, both ends inclusive.
fn bounds_y(area: &Area) -> (i32, i32) {
    let half = area.height / 2;
    ((area.y - half as f32) as i32, (area.y + half as f32) as i32)
}

fn in_bounds(area: &Area, mouse: (i32, i32)) -> bool {
    let (x0, x1) = bounds_x(area);
    let (y0, y1) = bounds_y(area);
    mouse.0 >= x0 && mouse.0 <= x1 && mouse.1 >= y0 && mouse.1 <= y1
}

/// `barY` of the scroll bar: 3 pixels under the widget's middle line.
fn bar_y(area: &Area) -> i32 {
    area.y as i32 + area.height / 2 + 3
}

/// `isInScrollBarBounds`: the bar's 2 pixels plus 2 of padding above and below, both ends inclusive.
fn in_scroll_bar(area: &Area, mouse: (i32, i32)) -> bool {
    let (x0, x1) = bounds_x(area);
    let y = bar_y(area);
    mouse.0 >= x0 && mouse.0 <= x1 && mouse.1 >= y - 2 && mouse.1 <= y + 2 + 2
}

/// `mouseScrolled`: a wheel turn over the widget moves the target by a step of half a card per turn, shared
/// between the cards. Returns whether the widget took the event.
pub fn wheel(scroll: &mut CardsScroll, area: &Area, count: usize, mouse: (f64, f64), amount: f64) -> bool {
    if !in_bounds(area, (mouse.0 as i32, mouse.1 as i32)) {
        return false;
    }
    let step = amount as f32 * -0.5 / count.max(1) as f32;
    scroll.target_x = (scroll.target_x + step).clamp(0.0, 1.0);
    true
}

/// `onMouseDragged`: a drag that starts on the scroll bar moves the thumb with the mouse; one that starts elsewhere in
/// the widget pans the row. Returns false when the press was outside the widget and nothing is dragged.
pub fn drag(scroll: &mut CardsScroll, area: &Area, count: usize, mouse: (f64, f64)) -> bool {
    let mouse_i = (mouse.0 as i32, mouse.1 as i32);
    if scroll.drag.is_none() {
        let on_bar = if in_scroll_bar(area, mouse_i) {
            true
        } else if in_bounds(area, mouse_i) {
            false
        } else {
            return false;
        };
        scroll.drag = Some(Drag { start: mouse, initial_scroll: scroll.scroll_x, on_bar });
    }
    let Some(drag) = scroll.drag else { return false };

    let area_w = cards_area_width(area);
    let total = cards_total_width(count);
    if drag.on_bar {
        let filled = bar_filled_width(area_w, total);
        let moved = (mouse.0 - drag.start.0) as f32 / (area_w - filled).max(1) as f32;
        let value = (moved + drag.initial_scroll).clamp(0.0, 1.0);
        scroll.target_x = value;
        scroll.scroll_x = value;
    } else {
        let moved = (drag.start.0 - mouse.0) as f32 / (total - area_w).max(1) as f32;
        scroll.target_x = (moved + drag.initial_scroll).clamp(0.0, 1.0);
    }
    true
}

/// `onMouseReleased`: ends the drag. Returns whether there was one.
pub fn release(scroll: &mut CardsScroll) -> bool {
    scroll.drag.take().is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
}

/// `onKeyPressed`: an arrow key moves the selection one card along the row. Returns whether the key was taken.
pub fn key(scroll: &mut CardsScroll, count: usize, key: Key) -> bool {
    if count == 0 {
        return false;
    }
    if count == 1 {
        // The original divides by zero here; the render then resets the scroll to 0.
        scroll.target_x = 0.0;
        return true;
    }
    let last = (count - 1) as f32;
    let current = round_half_up(scroll.scroll_x * last).clamp(0, count as i32 - 1);
    let next = match key {
        Key::Left => current - 1,
        Key::Right => current + 1,
    };
    scroll.target_x = (next as f32 / last).clamp(0.0, 1.0);
    true
}

/// The per-frame step of the scroll: the drawn position eases a quarter of the way to the target. A non-finite value
/// is reset to 0, as the original does.
pub fn step(scroll: &mut CardsScroll) {
    scroll.scroll_x = (scroll.scroll_x * 3.0 + scroll.target_x) / 4.0;
    if !scroll.target_x.is_finite() || !scroll.scroll_x.is_finite() {
        scroll.target_x = 0.0;
        scroll.scroll_x = 0.0;
    }
}

/// `Interpolate` with `EXP_OUT`: `1 - 2^(-10 p)` of the clamped progress. A zero duration counts as finished.
fn entrance_eased(duration_ms: u64, elapsed_ms: u64) -> f32 {
    let progress = if duration_ms == 0 { 1.0 } else { (elapsed_ms as f32 / duration_ms as f32).clamp(0.0, 1.0) };
    1.0 - 2f32.powf(-10.0 * progress)
}

/// A card as drawn: its position, and its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placed {
    index: usize,
    x: f32,
    y: f32,
    scale: f32,
}

/// Moves the commands of a card drawn at (0, 0) to (x, y) at `scale`, as the original's `matrices.translate` and
/// `scale` do. Glyphs are not scaled (`Draw::Text` has no scale); fills keep their edges rounded.
fn place(draws: Vec<Draw>, x: f32, y: f32, scale: f32) -> Vec<Draw> {
    draws
        .into_iter()
        .map(|d| match d {
            Draw::Sprite { name, x: dx, y: dy, w, h } => {
                Draw::Sprite { name, x: x + dx * scale, y: y + dy * scale, w: w * scale, h: h * scale }
            }
            Draw::Fill { x: dx, y: dy, w, h, argb } => {
                let (x1, y1) = ((x + dx as f32 * scale).round() as i32, (y + dy as f32 * scale).round() as i32);
                let (x2, y2) = ((x + (dx + w) as f32 * scale).round() as i32, (y + (dy + h) as f32 * scale).round() as i32);
                Draw::Fill { x: x1, y: y1, w: x2 - x1, h: y2 - y1, argb }
            }
            Draw::Text { x: dx, y: dy, text, argb } => Draw::Text {
                x: (x + dx as f32 * scale).round() as i32,
                y: (y + dy as f32 * scale).round() as i32,
                text,
                argb,
            },
            Draw::Item { name, x: dx, y: dy, size } => {
                Draw::Item { name, x: x + dx * scale, y: y + dy * scale, size: size * scale }
            }
            Draw::Count { text, right, y: dy } => Draw::Count {
                text,
                right: (x + right as f32 * scale).round() as i32,
                y: (y + dy as f32 * scale).round() as i32,
            },
        })
        .collect()
}

/// `ClientCardRenderer.getItem`: the tile under the mouse in a card at (x, y), or None. The card's x and y are
/// truncated first, as the original's `getItem(x.toInt(), y.toInt(), ...)` calls do.
fn tile_at(x: f32, y: f32, scale: f32, mouse: (i32, i32)) -> Option<usize> {
    let start_x = (x as i32) as f32 + 11.0 * scale;
    let start_y = (y as i32) as f32 + 21.0 * scale;
    let dx = mouse.0 as f32 - start_x;
    let dy = mouse.1 as f32 - start_y;
    if dx < 0.0 || dy < 0.0 {
        return None;
    }
    let tx = (dx / (20.0 * scale)) as i32;
    let ty = (dy / (20.0 * scale)) as i32;
    if !(0..=4).contains(&tx) || !(0..=4).contains(&ty) {
        return None;
    }
    Some((ty * 5 + tx) as usize)
}

/// The hit rectangle of tile `index` of a card, on the same grid as `tile_at`.
fn tile_rect(x: f32, y: f32, scale: f32, index: usize) -> Hit {
    let (tx, ty) = ((index % 5) as f32, (index / 5) as f32);
    let x0 = (x as i32) as f32 + 11.0 * scale + 20.0 * scale * tx;
    let y0 = (y as i32) as f32 + 21.0 * scale + 20.0 * scale * ty;
    let (x1, y1) = (x0 + 20.0 * scale, y0 + 20.0 * scale);
    let (left, top) = (x0.ceil() as i32, y0.ceil() as i32);
    Hit {
        x: left,
        y: top,
        w: x1.ceil() as i32 - left,
        h: y1.ceil() as i32 - top,
        action: Action::Tile { team: None, index },
    }
}

/// `BingoCardsWidget.render` for the scroll bar: a track, the thumb, and the rest of the track.
fn scroll_bar(out: &mut Vec<Draw>, area: &Area, total: i32, scroll: &CardsScroll, opacity: f32, mouse: (i32, i32)) {
    let area_w = cards_area_width(area);
    let bar_x = area.x as i32 - area_w / 2;
    let y = bar_y(area);
    let filled = bar_filled_width(area_w, total);
    let offset_max = area_w - filled;
    let offset = round_half_up(scroll.scroll_x * offset_max as f32);
    let alpha = round_half_up(opacity * 255.0).clamp(0, 255) as u32;

    // `setAlpha(0, colorAlpha * 80 / 255)`: black at 80/255 of the bar's alpha.
    let track = ((alpha * TRACK_ALPHA as u32 / 255) << 24) & 0xff00_0000;
    let color = if scroll.drag.is_none() && in_scroll_bar(area, mouse) {
        BAR_HOVER
    } else if scroll.drag.is_some_and(|d| d.on_bar) {
        BAR_DRAG
    } else {
        BAR_IDLE
    };
    let thumb = (alpha << 24) | (color & 0x00ff_ffff);
    let mut fill = |x1: i32, x2: i32, argb: u32| {
        out.push(Draw::Fill { x: x1, y, w: x2 - x1, h: 2, argb });
    };
    fill(bar_x, bar_x + offset, track);
    fill(bar_x + offset, bar_x + offset + filled, thumb);
    fill(bar_x + offset + filled, bar_x + area_w, track);
}

/// The tooltip lines of a tile (`drawTooltip`): the name, then the lore, each line but the last wrapped at 200 px.
pub fn tile_tooltip_lines(tile: &CardTile, lang: Lang<'_>, text_width: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(name) = &tile.name {
        lines.push(flatten(name, lang));
    }
    for (i, lore) in tile.lore.iter().enumerate() {
        let flat = flatten(lore, lang);
        // The last lore line (the tile's own info) is never wrapped.
        if i + 1 >= tile.lore.len() {
            lines.push(flat);
        } else {
            lines.extend(wrap_lines(&flat, TOOLTIP_MAX_WIDTH, text_width));
        }
    }
    lines
}

/// `Font.wrapLines`, approximated: greedy word wrap at spaces, a newline always breaks, and a word wider than the
/// line is cut by characters. TODO(integration): Minecraft's `StringSplitter` also keeps the style and the spaces at
/// a break; this drops the trailing space of each line.
pub fn wrap_lines(text: &str, max_width: i32, width_of: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if width_of(&candidate) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let (cut, rest) = cut_word(word, max_width, width_of);
            lines.extend(cut);
            line = rest;
        }
        lines.push(line);
    }
    lines
}

/// The part of [`wrap_lines`] that cuts a word too wide for a line by characters: the full lines cut from its start,
/// and the rest, which the caller goes on with. A word that fits, or has at most one character, is the rest whole.
///
/// Each line is found by measuring the rest's prefixes, shortest first, up to the first that is too wide (as
/// `Font.substrByWidth` does), so a word costs about its length times a line's length. The rest fits when no prefix
/// of it is too wide, which the same scan finds; so the rest is never measured whole, and a long word is not
/// re-measured from its start on every line.
fn cut_word(word: &str, max_width: i32, width_of: &dyn Fn(&str) -> i32) -> (Vec<String>, String) {
    // `cuts[k]` is the byte offset of character `k`, and `cuts[n]` the word's length.
    let cuts: Vec<usize> = word.char_indices().map(|(i, _)| i).chain(std::iter::once(word.len())).collect();
    let n = cuts.len() - 1;
    let mut lines = Vec::new();
    // The first character of the rest.
    let mut start = 0usize;
    while n - start > 1 {
        // The characters before the first prefix that is too wide, or `None` when no prefix is: the rest fits.
        let Some(taken) =(start + 1..=n).find_map(|end| {
            (width_of(&word[cuts[start]..cuts[end]]) > max_width).then_some(end - 1 - start)
        }) else {
            break;
        };
        // A character wider than the line is cut alone, as an empty line, and the cut still moves past it.
        lines.push(word[cuts[start]..cuts[start + taken]].to_string());
        start += taken.max(1);
    }
    (lines, word[cuts[start]..].to_string())
}

/// The commands of the whole card row, the hit regions, and the tooltip, for one frame.
pub fn frame(input: &CardsInput<'_>, scroll: &CardsScroll) -> CardsFrame {
    let mut next = *scroll;
    step(&mut next);
    let mut out = CardsFrame {
        draws: Vec::new(),
        labels: Vec::new(),
        hits: Vec::new(),
        tooltip: None,
        scroll: next,
        card_spans: Vec::new(),
        entrance_card: None,
    };
    let count = input.slots.len();
    if count == 0 {
        return out;
    }
    let area = input.area;
    let area_w = cards_area_width(&area);
    let total = cards_total_width(count);

    // viewsWithPositions: the row is wider than the area by `width_diff` either way, and the scroll slides it.
    let width_diff = (total - area_w).max(0) / 2;
    let scroll_center = (next.scroll_x - 0.5) * -2.0;
    let width_offset = scroll_center * width_diff as f32;
    let screen_x = area.x - (total / 2) as f32;
    let screen_y = area.y - (CARD_HEIGHT / 2) as f32;

    // Z order: the cards farthest from the middle are painted first; the one nearest the middle is on top.
    let distance = |i: usize| {
        let x_off = i as f32 * CARD_WIDTH as f32 + width_offset + (CARD_WIDTH / 2) as f32;
        (screen_x + round_half_up(x_off) as f32 - area.x).abs()
    };
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&a, &b| distance(b).total_cmp(&distance(a)));

    // The card's scale and the entrance position, shared by every card of the row.
    let (eased, scale) = match &input.entrance {
        Some(en) => {
            let e = entrance_eased(en.duration_ms, input.elapsed_ms);
            (e, en.from_scale * (1.0 - e) + e)
        }
        None => (1.0, 1.0),
    };
    let x_min = area.x - (area_w / 2) as f32;
    let x_max = area.x + (area_w / 2) as f32 - CARD_WIDTH as f32;

    let mut placed = Vec::with_capacity(count);
    for &index in &order {
        let x_off = index as f32 * CARD_WIDTH as f32 + width_offset;
        let x = screen_x + x_off;
        let y = screen_y;
        // A card pushed past the area's edge is held just outside it, 4 pixels further per card from the edge.
        let x_coerced = if x < x_min {
            x.max(x_min - 4.0 * (count - index - 1) as f32)
        } else if x > x_max {
            x.min(x_max + 4.0 * index as f32)
        } else {
            x
        };
        let (cur_x, cur_y) = match &input.entrance {
            Some(en) if en.team == input.slots[index].team => (
                en.from_x * (1.0 - eased) + x_coerced * eased,
                en.from_y * (1.0 - eased) + y * eased,
            ),
            _ => (x_coerced, y),
        };
        placed.push(Placed { index, x: cur_x, y: cur_y, scale });
    }

    // getHoveredView: the topmost card whose box (both edges inclusive) holds the mouse.
    let hovered = placed
        .iter()
        .filter(|p| {
            let (x0, y0) = (p.x as i32, p.y as i32);
            input.mouse.0 >= x0 && input.mouse.0 <= x0 + CARD_WIDTH && input.mouse.1 >= y0 && input.mouse.1 <= y0 + CARD_HEIGHT
        })
        .map(|p| p.index)
        .next_back();
    let entrance_done = input.entrance.as_ref().is_none_or(|en| input.elapsed_ms > en.duration_ms);
    // `interpolateKey` is set once and never cleared (`BingoHudScreen.kt:123`), so that card keeps full opacity for the
    // whole life of the screen, not only while it slides.
    let sliding_team: Option<TeamKey> = input.entrance.as_ref().map(|en| en.team.clone());

    for p in &placed {
        let slot = &input.slots[p.index];
        let draws_from = out.draws.len();
        let labels_from = out.labels.len();
        if sliding_team.as_ref() == Some(&slot.team) {
            out.entrance_card = Some(out.card_spans.len());
        }
        let view = CardView {
            card: slot.card,
            colors: slot.colors,
            now_ms: input.now_ms,
            show_outlines: input.show_outlines,
            team_outline: input.team_outline,
            items: input.items,
            images: input.images,
            team_name: slot.team_name.clone(),
            is_gui: true,
        };
        out.draws.extend(place(draw_card(&view), p.x, p.y, p.scale));

        let is_hovered = hovered == Some(p.index);
        // `isFocused`: the hovered card of a row of two or more, when it can be selected.
        if is_hovered && count > 1 && input.selectable {
            out.draws.extend(place(
                vec![Draw::Sprite { name: OUTLINE_SPRITE.into(), x: 0.0, y: 0.0, w: CARD_WIDTH as f32, h: CARD_HEIGHT as f32 }],
                p.x,
                p.y,
                p.scale,
            ));
        }
        // The winner's label: `translate(90, -4)` in the card's frame, turned 0.25 rad, in the cycling win colour.
        if slot.team.is_some() && slot.team.as_deref() == input.winner {
            let text = (input.lang)("bingo.card.winner").unwrap_or_else(|| "WINNER!".into());
            let colour = crate::screen_main::WIN_COLORS[((input.now_ms / 1000) % crate::screen_main::WIN_COLORS.len() as u64) as usize];
            out.labels.push(ScreenDraw::Label {
                text,
                anchor: (p.x + 90.0 * p.scale, p.y - 4.0 * p.scale),
                offset: (0.0, 0.0),
                scale: p.scale,
                rotation: 0.25,
                argb: colour,
                shadow: true,
            });
        }
        // TODO(integration): the team's player list is a tooltip over the card's title (`ClientCardRenderer`).
        let is_mouse_over = entrance_done && is_hovered && input.opacity > 0.5;
        if is_mouse_over {
            if let Some(index) = tile_at(p.x, p.y, p.scale, input.mouse) {
                if let Some(tile) = slot.card.tiles.get(index) {
                    let lines = tile_tooltip_lines(tile, input.lang, input.text_width);
                    if !lines.is_empty() {
                        out.tooltip = Some(TileTooltip { team: slot.team.clone(), index, lines, mouse: input.mouse });
                    }
                }
            }
        }
        out.card_spans.push(CardSpan {
            team: slot.team.clone(),
            draws: (draws_from, out.draws.len()),
            labels: (labels_from, out.labels.len()),
        });
    }

    if total > area_w {
        scroll_bar(&mut out.draws, &area, total, &next, input.opacity, input.mouse);
    }

    // Hit regions, topmost first: each card's tiles, then the card itself, then the cards under it.
    let (bx0, bx1) = bounds_x(&area);
    out.hits.push(Hit {
        x: bx0,
        y: bar_y(&area) - 2,
        w: bx1 - bx0 + 1,
        h: 7,
        action: Action::ScrollBar,
    });
    for p in placed.iter().rev() {
        let slot = &input.slots[p.index];
        for index in 0..TILES_PER_CARD.min(slot.card.tiles.len()) {
            let mut hit = tile_rect(p.x, p.y, p.scale, index);
            hit.action = Action::Tile { team: slot.team.clone(), index };
            out.hits.push(hit);
        }
        if input.selectable {
            out.hits.push(Hit {
                x: p.x as i32,
                y: p.y as i32,
                w: CARD_WIDTH + 1,
                h: CARD_HEIGHT + 1,
                action: Action::SelectCard(slot.team.clone()),
            });
        }
    }
    out
}

/// Something the history draws: a plain command, a nine-slice box, or text drawn at a scale.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenDraw {
    Plain(Draw),
    /// `drawNinePatch` of `bingo/game_container`: a `w` x `h` box whose corners are `slice` pixels.
    NinePatch { name: String, x: i32, y: i32, w: i32, h: i32, slice: i32 },
    /// Text drawn under `matrices.scale(scale)`: its glyphs are `scale` times larger, its position is `scale` times
    /// `(x, y)`. TODO(integration): the text needs a scale in `Draw::Text`.
    ScaledText { x: f32, y: f32, scale: f32, text: String, argb: u32 },
    /// Text in the frame of `translate(anchor); rotate(rotation); scale(scale)`, its top left at `offset` there.
    Label { text: String, anchor: (f32, f32), offset: (f32, f32), scale: f32, rotation: f32, argb: u32, shadow: bool },
}

/// What the history reads from the widget and the state.
pub struct HistoryView<'a> {
    pub width: i32,
    /// The widget's game-over packet (`gameOver`): it draws the title box.
    pub game_over: Option<&'a GameOverPacket>,
    /// `state.gameOver != null`: it changes the measured height, as in the original.
    pub state_has_game_over: bool,
    /// The status lines of `GameInfoWidget` (the time left or elapsed, and "waiting to start"), already formatted.
    /// TODO(integration): the host formats them from the game status with the language's keys.
    /// Each a list of runs (text, colour) drawn one after another and centred as a whole.
    pub status_lines: Vec<Vec<(String, u32)>>,
    /// `state.pastMessages`, oldest first.
    pub messages: &'a [ScoreMessage],
    pub items: &'a dyn ItemNames,
    pub lang: Lang<'a>,
    pub text_width: &'a dyn Fn(&str) -> i32,
}

/// The history's layout: what to draw, and how tall the scrolled content is.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    pub draws: Vec<ScreenDraw>,
    pub content_height: i32,
}

/// A message wrapped to the history's width (`getMessageWrapped`).
#[derive(Debug, Clone, PartialEq)]
pub struct WrappedMessage {
    pub lines: Vec<String>,
    /// The same lines as runs of one colour each (a head is a run of two spaces with its uuid).
    pub runs: Vec<Vec<StyledRun>>,
    pub height: i32,
}

/// The runs of each wrapped line: the line's characters taken in order from the runs of the whole text, skipping what
/// the wrap dropped (the spaces it broke at).
fn runs_of_lines(lines: &[String], all: &[StyledRun]) -> Vec<Vec<StyledRun>> {
    // One entry per character of the flat text: its colour and head.
    let chars: Vec<(char, Option<u32>, Option<u128>)> =
        all.iter().flat_map(|r| r.text.chars().map(move |c| (c, r.rgb, r.head))).collect();
    let mut at = 0usize;
    let mut out = Vec::new();
    for line in lines {
        let mut runs: Vec<StyledRun> = Vec::new();
        for c in line.chars() {
            while at < chars.len() && chars[at].0 != c {
                at += 1;
            }
            let Some(&(_, rgb, head)) = chars.get(at) else { break };
            at += 1;
            match runs.last_mut() {
                Some(last) if last.rgb == rgb && last.head == head => last.text.push(c),
                _ => runs.push(StyledRun { text: c.to_string(), rgb, head }),
            }
        }
        out.push(runs);
    }
    out
}

/// `BingoMessageRenderer.getMessageWrapped` for the history: `width` is the widget's width less 4 (the caller's
/// `width - 4`), and a message wider than `width - 28` wraps there. A message is at least one item high.
pub fn wrap_message(message: &ScoreMessage, width: i32, lang: Lang<'_>, text_width: &dyn Fn(&str) -> i32) -> WrappedMessage {
    let all = runs(&message.packet.message, lang);
    let flat: String = all.iter().map(|r| r.text.as_str()).collect();
    let wrap_width = width - 28;
    let lines = if text_width(&flat) > wrap_width { wrap_lines(&flat, wrap_width, text_width) } else { vec![flat] };
    let runs = runs_of_lines(&lines, &all);
    let count = lines.len() as i32;
    let text_height = count * TEXT_HEIGHT + (count - 1) * 2;
    WrappedMessage { lines, runs, height: (text_height + 6).max(ITEM_HEIGHT) }
}

/// The height of a title or a status box's line block, and the status box's height (`GameInfoWidget.size.y`).
fn status_box_height(lines: usize) -> i32 {
    lines as i32 * LINE_STEP + 8 + 8
}

/// `MessageHistoryWidget`: the game-over title or the status box, then the messages, newest first.
pub fn history(view: &HistoryView<'_>) -> History {
    let width = view.width;
    let wrap_text = |text: &str, max: i32| -> Vec<String> {
        if (view.text_width)(text) > max {
            wrap_lines(text, max, view.text_width)
        } else {
            vec![text.to_string()]
        }
    };

    // resize(): the title is wrapped to half the width (it is drawn at 2x), the subtitle to the full width.
    let (title_lines, subtitle_lines, subtitle_runs) = match view.game_over {
        Some(game_over) => {
            let all = runs(&game_over.subtitle, view.lang);
            let flat: String = all.iter().map(|r| r.text.as_str()).collect();
            let lines = wrap_text(&flat, width - MARGIN_X * 2);
            let subtitle_runs = runs_of_lines(&lines, &all);
            (wrap_text(&flatten(&game_over.title, view.lang), (width - MARGIN_X * 2) / 2), lines, subtitle_runs)
        }
        None => (Vec::new(), Vec::new(), Vec::new()),
    };
    let title_height = title_lines.len() as i32 * LINE_STEP * 2
        + subtitle_lines.len() as i32 * LINE_STEP
        + 4
        + 12
        + 8;
    let status_height = status_box_height(view.status_lines.len());

    let messages: Vec<WrappedMessage> =
        view.messages.iter().map(|m| wrap_message(m, width - HISTORY_INSET, view.lang, view.text_width)).collect();

    // measureContentHeight
    let game_over_height = if view.state_has_game_over { title_height + 8 } else { status_height + 8 };
    let messages_height: i32 = messages.iter().map(|m| m.height).sum();
    let content_height = MARGIN_Y + game_over_height + messages_height + MARGIN_Y;

    // renderContents
    let mut draws = Vec::new();
    let mut draw_y = MARGIN_Y;
    if let Some(_game_over) = view.game_over {
        draws.push(ScreenDraw::NinePatch {
            name: "bingo/game_container".into(),
            x: 0,
            y: draw_y,
            w: width,
            h: title_height,
            slice: 8,
        });
        let mut title_y = 12;
        for line in &title_lines {
            // The title is drawn at 2x: its position is halved, then doubled again by the scale.
            draws.push(ScreenDraw::ScaledText {
                x: (MARGIN_X / 2) as f32,
                y: (draw_y + title_y) as f32 / 2.0,
                scale: 2.0,
                text: line.clone(),
                argb: WHITE,
            });
            title_y += LINE_STEP * 2;
        }
        title_y += 4;
        for line in &subtitle_runs {
            let mut x = MARGIN_X;
            for run in line {
                let w = (view.text_width)(&run.text);
                match run.head {
                    Some(uuid) => draws.push(ScreenDraw::Plain(Draw::Sprite {
                        name: crate::card_images::face_sprite(uuid),
                        x: x as f32,
                        y: (draw_y + title_y) as f32,
                        w: 8.0,
                        h: 8.0,
                    })),
                    None => draws.push(ScreenDraw::Plain(Draw::Text {
                        x,
                        y: draw_y + title_y,
                        text: run.text.clone(),
                        argb: run.rgb.map_or(WHITE, |c| 0xff00_0000 | c),
                    })),
                }
                x += w;
            }
            title_y += LINE_STEP;
        }
        let _ = &subtitle_lines;
        draw_y += title_height + 8;
    } else if !view.status_lines.is_empty() {
        draws.push(ScreenDraw::NinePatch {
            name: "bingo/game_container".into(),
            x: 0,
            y: draw_y,
            w: width,
            h: status_height,
            slice: 8,
        });
        let mut line_y = 9;
        for line in &view.status_lines {
            let line_width: i32 = line.iter().map(|(t, _)| (view.text_width)(t)).sum();
            let mut x = (width - line_width) / 2;
            for (text, argb) in line {
                draws.push(ScreenDraw::Plain(Draw::Text { x, y: draw_y + line_y, text: text.clone(), argb: *argb }));
                x += (view.text_width)(text);
            }
            line_y += LINE_STEP;
        }
        draw_y += status_height + 8;
    }

    // `messages.reversed()`: the newest message is at the top.
    for (message, wrapped) in view.messages.iter().zip(&messages).rev() {
        draw_history_message(&mut draws, message, wrapped, draw_y, view);
        draw_y += wrapped.height;
    }

    History { draws: draws.into_iter().collect(), content_height }
}

/// `BingoMessageRenderer.drawMessage` at visibility 1 and not left of the card, moved 6 px left by the history.
fn draw_history_message(
    out: &mut Vec<ScreenDraw>,
    message: &ScoreMessage,
    wrapped: &WrappedMessage,
    draw_y: i32,
    view: &HistoryView<'_>,
) {
    let height = wrapped.height;
    let item_x = HISTORY_OFFSET_X + MESSAGE_ICON_X;
    let item_y = draw_y + height / 2 - 8;
    let packet = &message.packet;
    if packet.image.item.is_some() || packet.image.texture.is_some() {
        icon_tile(out, view.items, &packet.image, &packet.image_list, item_x as f32, item_y as f32);
    } else {
        let name = match packet.message_type {
            MessageType::LeadingTeam => "bingo/message_leading",
            MessageType::LineScored | MessageType::CardCompleted => "bingo/message_check",
            MessageType::ItemScored => "bingo/message_item_hidden",
        };
        out.push(ScreenDraw::Plain(Draw::Sprite {
            name: name.into(),
            x: item_x as f32,
            y: item_y as f32,
            w: 16.0,
            h: 16.0,
        }));
    }

    // The lines are centred on the icon's middle: `height/2 - (lines * 9 + (lines - 1) * 2) / 2`.
    let count = wrapped.lines.len() as i32;
    let text_y = height / 2 - (count * TEXT_HEIGHT + (count - 1) * 2) / 2;
    let text_x = HISTORY_OFFSET_X + MESSAGE_TEXT_X;
    for (index, line) in wrapped.runs.iter().enumerate() {
        let y = draw_y + text_y + LINE_STEP * index as i32;
        let mut x = text_x;
        // A line with no runs (an empty message) is still one piece of text, as it was drawn before.
        if line.is_empty() {
            out.push(ScreenDraw::Plain(Draw::Text { x, y, text: String::new(), argb: WHITE }));
        }
        for run in line {
            let w = (view.text_width)(&run.text);
            match run.head {
                Some(uuid) => out.push(ScreenDraw::Plain(Draw::Sprite {
                    name: crate::card_images::face_sprite(uuid),
                    x: x as f32,
                    y: y as f32,
                    w: 8.0,
                    h: 8.0,
                })),
                None => out.push(ScreenDraw::Plain(Draw::Text {
                    x,
                    y,
                    text: run.text.clone(),
                    argb: run.rgb.map_or(WHITE, |c| 0xff00_0000 | c),
                })),
            }
            x += w;
        }
    }
}

/// `CardTileRenderer.renderTile` for a message icon at scale 1: a single item, or the one-of and all-of stacks.
// TODO(integration): this repeats `message_draw::tile_icon` and `card_draw::tile_image`, which are private. Make one
// of them `pub(crate)` and call it here; the offsets are the same.
fn icon_tile(out: &mut Vec<ScreenDraw>, items: &dyn ItemNames, image: &CardTileImage, list: &[CardTileImage], x: f32, y: f32) {
    let one = |out: &mut Vec<ScreenDraw>, img: &CardTileImage, x: f32, y: f32, size: f32| {
        if let Some(stack) = &img.item {
            if let Some(name) = items.item_name(stack.item) {
                out.push(ScreenDraw::Plain(Draw::Item { name, x, y, size }));
            }
        }
    };
    if list.len() > 1 && image.item_style == ItemStyle::OneOf {
        for i in (0..3).rev() {
            if let Some(img) = list.get(i) {
                one(out, img, x + 2.0 * i as f32, y + 2.0 * i as f32, 12.0);
            }
        }
    } else if list.len() > 1 && image.item_style == ItemStyle::AllOf {
        for i in 0..4 {
            if let Some(img) = list.get(i) {
                one(out, img, x + (i % 2) as f32 * 6.75, y + (i / 2) as f32 * 6.75, 9.0);
            }
        }
    } else {
        one(out, image, x, y, 16.0);
    }
    let name = match (&image.decoration_texture, image.decoration) {
        (Some(custom), _) => Some(custom.rsplit(':').next().unwrap_or(custom).to_string()),
        (None, Decoration::Advancement) => Some("bingo/image_frame_advancement".to_string()),
        (None, Decoration::Forbidden) => Some("bingo/image_frame_forbidden".to_string()),
        (None, Decoration::FreeSpace) => Some("bingo/image_frame_free_space".to_string()),
        (None, Decoration::MultiItem | Decoration::None) => None,
    };
    if let Some(name) = name {
        out.push(ScreenDraw::Plain(Draw::Sprite { name, x: x - 2.0, y: y - 2.0, w: 20.0, h: 20.0 }));
    }
}

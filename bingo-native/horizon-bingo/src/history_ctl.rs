//! The message history in the card screen's sidebar, with the scroll it keeps between frames. It ports the part of
//! `BingoHudScreen` that feeds `MessageHistoryWidget` (`messagesWidget`, inside a `ScrollableWidget`): the widget's
//! rows are rebuilt from the HUD state on every `HudStateChangedEvent`, and its scroll survives those rebuilds.
//!
//! [`HistoryCtl::frame`] builds the rows with `screen_cards::history` from `hud.past_messages`, the game's status lines
//! (`screen_end::status_lines`, the feeder of `GameInfoWidget`) and the game-over box. It clamps the scroll to the
//! content and moves the draws to the area's top left less the scroll. The scroll rules are vanilla's
//! `AbstractScrollArea`, as Horizon's `ui::Scroll` ports them: a wheel step is `ScrollableWidget.scrollRate()` (9)
//! pixels, the thumb travels the area less its own height, and a held thumb moves the content by
//! `max scroll / travel` for each pixel the pointer moves.
//!
//! Nothing is drawn or clicked here. Hit tests are the caller's to route: `contains`, `wheel`, `drag_start`, `drag`
//! and `release` take the pointer in GUI pixels. The history has no hover state and no clicks of its own.
//!
//! Not done, each marked TODO(integration) where it happens: the content is clipped to the area (vanilla's scissor,
//! and `UiDraw` has no clip), the scroll bar's sprites are vanilla's and not in the mod's atlas, and the message text
//! is drawn without its shadow (`screen_cards` has no shadow flag for history text).

use crate::card_draw::ItemNames;
use crate::game_message::GameMessagePacket;
use crate::game_over::GameOverPacket;
use crate::screen_cards::{history, History, HistoryView};
use crate::screen_end::status_lines;
use crate::state::HudState;
use crate::text_plain::{flatten, runs, Lang, StyledRun};
use crate::ui_draw::UiDraw;

/// `ScrollableWidget.scrollRate()`: the content moves this many pixels per wheel step.
pub const WHEEL_RATE: f32 = 9.0;
/// The width of the scroll bar's column, at the area's right edge (`AbstractScrollArea.scrollbarWidth()`).
pub const SCROLLBAR_WIDTH: i32 = 6;
/// The thumb's smallest height, and the largest is the area less 8 (`AbstractScrollArea.scrollbarHeight`).
const THUMB_MIN: i32 = 32;
const THUMB_INSET: i32 = 8;
// TODO(integration): vanilla's scroll bar sprites. The mod's atlas does not hold them, so `ui_draw::run` skips them
// until the host looks them up in the vanilla atlas.
const SCROLLER_BACKGROUND: &str = "gui/sprites/widget/scroller_background";
const SCROLLER: &str = "gui/sprites/widget/scroller";

/// A rectangle in GUI pixels: `x`, `y` at its top left, `w` and `h` its size.
pub type Area = (i32, i32, i32, i32);

/// What one frame draws: the rows and the scroll bar in paint order, and the content's height (for the caller's
/// own bookkeeping; the scroll is already clamped to it).
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryFrame {
    pub draws: Vec<UiDraw>,
    pub content_height: i32,
}

/// The history's scroll between frames: `scroll` in pixels from the top, `held` the pointer's last y while the scroll
/// bar is dragged, and the last frame's area and content height, which the events between frames use.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistoryCtl {
    scroll: f32,
    held: Option<f32>,
    area: Option<Area>,
    content_height: i32,
    /// The last build, reused by the next frame while its inputs are unchanged.
    cache: Option<Cached>,
}

/// What a build reads, resolved the way the build reads it: the width, the status lines (which carry the running
/// time), the game over's title and subtitle, and each message's runs. The language enters through these, so a new
/// language is a new key. The rest of the message and the game over are compared as they are, in [`Cached::matches`].
#[derive(Debug, Clone, PartialEq)]
struct Resolved {
    width: i32,
    status: Vec<Vec<(String, u32)>>,
    game_over: Option<(String, Vec<StyledRun>)>,
    messages: Vec<Vec<StyledRun>>,
}

impl Resolved {
    fn new(hud: &HudState, width: i32, lang: Lang<'_>) -> Self {
        // `GameInfoWidget.text`: each status line is a few coloured runs (the label white, the duration yellow).
        let status = status_lines(hud.game_status.as_ref(), lang)
            .iter()
            .map(|line| line.runs.iter().map(|run| (run.text.clone(), run.argb)).collect())
            .collect();
        let game_over = hud.game_over.as_ref().map(|g| (flatten(&g.title, lang), runs(&g.subtitle, lang)));
        let messages = hud.past_messages.iter().map(|m| runs(&m.packet.message, lang)).collect();
        Self { width, status, game_over, messages }
    }
}

/// A build and the inputs it was made from.
#[derive(Debug, Clone, PartialEq)]
struct Cached {
    resolved: Resolved,
    game_over: Option<GameOverPacket>,
    /// The messages' packets: what the rows draw (icons, kinds, text) besides their runs.
    packets: Vec<GameMessagePacket>,
    built: History,
}

impl Cached {
    fn matches(&self, hud: &HudState, resolved: &Resolved) -> bool {
        self.resolved == *resolved
            && self.game_over.as_ref() == hud.game_over.as_ref()
            && self.packets.len() == hud.past_messages.len()
            && self.packets.iter().zip(&hud.past_messages).all(|(p, m)| *p == m.packet)
    }
}

impl HistoryCtl {
    pub fn new() -> Self {
        Self::default()
    }

    /// The scroll position in pixels, as the next frame will draw it before clamping.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// One frame: the rows built from `hud`, scrolled inside `area`. The build is reused while what it reads is
    /// unchanged (see [`Resolved`]); the status lines change every second, so that rebuilds it once a second.
    /// `mouse` is accepted for the screen's signature; the history draws nothing that depends on the pointer.
    pub fn frame(
        &mut self,
        hud: &HudState,
        area: Area,
        _mouse: (i32, i32),
        items: &dyn ItemNames,
        lang: Lang<'_>,
        width_of: &dyn Fn(&str) -> i32,
    ) -> HistoryFrame {
        let (x, y, w, h) = area;
        let resolved = Resolved::new(hud, w, lang);
        if !self.cache.as_ref().is_some_and(|c| c.matches(hud, &resolved)) {
            let view = HistoryView {
                width: w,
                game_over: hud.game_over.as_ref(),
                state_has_game_over: hud.game_over.is_some(),
                status_lines: resolved.status.clone(),
                messages: &hud.past_messages,
                items,
                lang,
                text_width: width_of,
            };
            self.cache = Some(Cached {
                game_over: hud.game_over.clone(),
                packets: hud.past_messages.iter().map(|m| m.packet.clone()).collect(),
                built: history(&view),
                resolved,
            });
        }
        let built = &self.cache.as_ref().expect("built above").built;

        self.area = Some(area);
        self.content_height = built.content_height;
        self.scroll = self.scroll.clamp(0.0, max_scroll(h, built.content_height));

        // `translate(x, y - scroll)`: the scroll is whole pixels once it is drawn.
        let (dx, dy) = (x, y - round_half_up(self.scroll));
        let mut draws: Vec<UiDraw> = built.draws.iter().map(|d| shift(UiDraw::from(d), dx, dy)).collect();

        if let Some((thumb_y, thumb_h)) = thumb(y, h, built.content_height, self.scroll) {
            let bar_x = bar_x(area);
            draws.push(UiDraw::Sprite {
                name: SCROLLER_BACKGROUND.into(),
                x: bar_x as f32,
                y: y as f32,
                w: SCROLLBAR_WIDTH as f32,
                h: h as f32,
            });
            draws.push(UiDraw::Sprite {
                name: SCROLLER.into(),
                x: bar_x as f32,
                y: thumb_y as f32,
                w: SCROLLBAR_WIDTH as f32,
                h: thumb_h as f32,
            });
        }
        HistoryFrame { draws, content_height: built.content_height }
    }

    /// Whether the pointer is over the history's area as it was last drawn (`isMouseOver`: the right and bottom edges
    /// are outside). False before the first frame.
    pub fn contains(&self, mouse: (i32, i32)) -> bool {
        self.area.is_some_and(|area| inside(area, mouse))
    }

    /// `mouseScrolled`: a wheel turn over the area moves the content by `WHEEL_RATE` per step (a positive `amount`
    /// scrolls up). Returns whether the area took the turn. The content height is the last frame's.
    pub fn wheel(&mut self, mouse: (i32, i32), amount: f64, area: Area) -> bool {
        if !inside(area, mouse) {
            return false;
        }
        let scroll = self.scroll - amount as f32 * WHEEL_RATE;
        self.scroll = scroll.clamp(0.0, max_scroll(area.3, self.content_height));
        true
    }

    /// `mouseClicked` on the scroll bar: a press on its column, over the area, while there is something to scroll,
    /// holds the thumb. Returns whether it did.
    pub fn drag_start(&mut self, mouse: (i32, i32), area: Area) -> bool {
        let (_, y, _, h) = area;
        let bar_x = bar_x(area);
        let on_bar = mouse.0 >= bar_x
            && mouse.0 < bar_x + SCROLLBAR_WIDTH
            && mouse.1 >= y
            && mouse.1 < y + h;
        let scrollable = self.content_height > h;
        self.held = (on_bar && scrollable).then_some(mouse.1 as f32);
        self.held.is_some()
    }

    /// `mouseDragged` while the thumb is held: above the area the content goes to its top, below it to its end,
    /// else it moves `max / travel` pixels per pixel of the pointer (at least one). Returns whether a thumb was held.
    pub fn drag(&mut self, mouse: (i32, i32), area: Area) -> bool {
        let Some(last) = self.held else {
            return false;
        };
        let (_, top, _, h) = area;
        let bottom = top + h;
        let max = max_scroll(h, self.content_height);
        let y = mouse.1 as f32;
        if mouse.1 < top {
            self.scroll = 0.0;
        } else if mouse.1 > bottom {
            self.scroll = max;
        } else {
            let travel = (h - thumb_height(h, self.content_height)) as f32;
            let per_pixel = (max.max(1.0) / travel).max(1.0);
            self.scroll += (y - last) * per_pixel;
        }
        self.scroll = self.scroll.clamp(0.0, max);
        self.held = Some(y);
        true
    }

    /// `mouseReleased`: lets go of the thumb. Returns whether one was held.
    pub fn release(&mut self) -> bool {
        self.held.take().is_some()
    }
}

/// `Math.max(0, content - h)`: how far the content scrolls.
fn max_scroll(h: i32, content: i32) -> f32 {
    (content - h).max(0) as f32
}

/// The scroll bar's column: the area's right edge.
fn bar_x(area: Area) -> i32 {
    area.0 + area.2 - SCROLLBAR_WIDTH
}

fn inside((x, y, w, h): Area, (mx, my): (i32, i32)) -> bool {
    mx >= x && mx < x + w && my >= y && my < y + h
}

/// `(int) (h*h / content)`, held between the thumb's bounds (Java's `Mth.clamp`: the minimum wins below it).
fn thumb_height(h: i32, content: i32) -> i32 {
    let raw = h * h / content.max(1);
    if raw < THUMB_MIN {
        THUMB_MIN
    } else {
        raw.min(h - THUMB_INSET)
    }
}

/// The thumb's top and height, or `None` when the content fits (`scrollbarVisible` is false).
fn thumb(top: i32, h: i32, content: i32, scroll: f32) -> Option<(i32, i32)> {
    if content <= h {
        return None;
    }
    let bar = thumb_height(h, content);
    let travel = (h - bar) as f32;
    let offset = (scroll / max_scroll(h, content) * travel).floor() as i32;
    Some((top + offset, bar))
}

fn round_half_up(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// Moves a draw by whole pixels.
fn shift(draw: UiDraw, dx: i32, dy: i32) -> UiDraw {
    let (fx, fy) = (dx as f32, dy as f32);
    match draw {
        UiDraw::Sprite { name, x, y, w, h } => UiDraw::Sprite { name, x: x + fx, y: y + fy, w, h },
        UiDraw::Fill { x, y, w, h, argb } => UiDraw::Fill { x: x + fx, y: y + fy, w, h, argb },
        UiDraw::Text { x, y, text, argb, shadow, obfuscated } => {
            UiDraw::Text { x: x + fx, y: y + fy, text, argb, shadow, obfuscated }
        }
        UiDraw::Item { name, x, y, size } => UiDraw::Item { name, x: x + fx, y: y + fy, size },
        UiDraw::Count { text, right, y } => UiDraw::Count { text, right: right + dx, y: y + dy },
        UiDraw::TextXf { text, anchor, offset, scale, rotation, argb, shadow } => {
            UiDraw::TextXf { text, anchor: (anchor.0 + fx, anchor.1 + fy), offset, scale, rotation, argb, shadow }
        }
        UiDraw::SquareXf { cx, cy, half, rotation, argb } => UiDraw::SquareXf { cx: cx + fx, cy: cy + fy, half, rotation, argb },
        UiDraw::NineSlice { name, x, y, w, h, slice, tex } => {
            UiDraw::NineSlice { name, x: x + dx, y: y + dy, w, h, slice, tex }
        }
        UiDraw::Tooltip { x, y, lines } => UiDraw::Tooltip { x: x + dx, y: y + dy, lines },
    }
}

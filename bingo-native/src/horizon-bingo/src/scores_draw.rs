//! The SCORES tab of the end screen and the game info panel, drawn: `hud/screen/BingoEndScoresWidget.kt` and
//! `hud/screen/GameInfoWidget.kt`. `screen_end` lays the list out and says what each piece is; this module puts the
//! list in the slot `screen_main::layout().scores` gives, and turns it into `UiDraw`s.
//!
//! The Kotlin draws the rows at fixed positions: `ScrollableWidget.extractContents` never applies the scroll offset,
//! so `scores_frame` does not either. The scroll offset moves only the scroll bar's thumb. `clamp_scroll` and `wheel`
//! are the scroll area's own rules.
//!
//! The scroll bar is `AbstractScrollArea`'s, drawn last, as `history_ctl` draws it for the same base class: a 6 px
//! column at the box's right edge, the background over the box's height and the thumb on top of it.
//!
//! Not here: the winner and summary lines (they are not in these two Kotlin files).

use crate::history_ctl::SCROLLBAR_WIDTH;
use crate::screen_end::{self, GameInfo, Hit, ScoresInput, ScreenDraw as EndDraw, DEFAULT_PADDING, ENTRY_HEIGHT};
use crate::ui_draw::UiDraw;

/// `IScrollableWidget.background` (`0x40_000000`): the box drawn behind the contents.
const BACKGROUND: u32 = 0x40_000000;
/// `Font.lineHeight` of the 26.3 font (`screen_end`'s `TEXT_HEIGHT`, private there).
const TEXT_HEIGHT: i32 = 9;
/// `(entryHeight/2) - (textHeight/2)`: where the text sits inside a row (`screen_end`'s `Y_OFFSET`, private there).
const Y_OFFSET: i32 = ENTRY_HEIGHT / 2 - TEXT_HEIGHT / 2;
/// `ScrollableWidget.scrollRate()`: one wheel notch moves the list this many pixels.
const SCROLL_RATE: f64 = 9.0;
/// `totalInnerPadding()` above and below the content: `AbstractScrollArea` measures the content with it, so the scroll
/// range and the thumb use the content plus this. The padding is `screen_end::DEFAULT_PADDING` (the callers' 4).
const TOTAL_PADDING: i32 = 2 * DEFAULT_PADDING;
/// `AbstractScrollArea.scrollbarHeight`: the thumb is at least 32 px and at most the box less 8 (`history_ctl`'s
/// `THUMB_MIN` and `THUMB_INSET`, private there).
const THUMB_MIN: i32 = 32;
const THUMB_INSET: i32 = 8;
// TODO(integration): vanilla's scroll bar sprites: the mod's atlas does not hold them, so `ui_draw::run` skips them
// until the host looks them up in the vanilla atlas (as `history_ctl` notes).
const SCROLLER_BACKGROUND: &str = "gui/sprites/widget/scroller_background";
const SCROLLER: &str = "gui/sprites/widget/scroller";

/// What the SCORES tab draws this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoresFrame {
    /// The box, then the title row and each visible row, then a tooltip when a team cell is hovered, then the scroll
    /// bar when the list is taller than the box.
    pub draws: Vec<UiDraw>,
    /// The team cells that have a tooltip, in GUI pixels and clipped to the box.
    // TODO(integration): these are GUI pixels, not the content pixels `screen_end::Hit` documents.
    pub hits: Vec<Hit>,
    /// `measureContentHeight`: the title row and the rows, without the padding. `clamp_scroll` adds the padding.
    pub content_height: i32,
}

/// `BingoEndScoresWidget` in the box `area` (`x, y, w, h` in GUI pixels), with `input.width` equal to `w`.
///
/// Content sits `input.padding` inside the box (`ScrollableWidget.extractWidgetRenderState`). A row whose band lies
/// fully outside the box is left out; a row that is partly outside is drawn, the host's scissor cuts it. `mouse` is in
/// GUI pixels; `now_epoch_ms` is the wall clock.
// TODO(integration): the host scissors the text to the box and draws `obfuscated` text as random glyphs.
pub fn scores_frame(
    input: &ScoresInput<'_>,
    area: (i32, i32, i32, i32),
    scroll: f32,
    mouse: (i32, i32),
    now_epoch_ms: i64,
) -> ScoresFrame {
    let (ax, ay, aw, ah) = area;
    let pad = input.padding;
    let (ox, oy) = (ax + pad, ay + pad);
    let lay = screen_end::layout(input);
    let content_height = lay.content_height();
    // The rows ignore the offset (see the module doc); the thumb shows it.
    let scroll = clamp_scroll(scroll, content_height, ah);
    let inside = |x: i32, y: i32| x >= ax && x < ax + aw && y >= ay && y < ay + ah;
    // A row band `[row_top, row_top + ENTRY_HEIGHT)` in content pixels is shown when it meets the box's rows.
    let row_shown = |row_top: i32| {
        let top = oy + row_top;
        top + ENTRY_HEIGHT > ay && top < ay + ah
    };

    let mut draws = Vec::new();
    // `ScrollableWidget`: the background covers the content plus the padding on both sides, inside the box.
    let box_h = (content_height + 2 * pad).min(ah);
    if aw > 0 && box_h > 0 {
        draws.push(UiDraw::Fill { x: ax as f32, y: ay as f32, w: aw as f32, h: box_h as f32, argb: BACKGROUND });
    }

    for d in lay.draw(now_epoch_ms, mouse.0 - ox, mouse.1 - oy) {
        match &d {
            // A head is drawn with its row's text, so it is left out with the row.
            EndDraw::Text { y, .. } | EndDraw::Sprite { y, .. } => {
                if row_shown(y - Y_OFFSET) {
                    draws.push(shift(UiDraw::from(&d), ox, oy));
                }
            }
            EndDraw::Tooltip { .. } => {
                // `drawTooltip` at the mouse, which the Kotlin draws inside the content's translation: so the tooltip
                // is at the mouse itself. It shows only while the mouse is over the box.
                if inside(mouse.0, mouse.1) {
                    let mut tip = UiDraw::from(&d);
                    if let UiDraw::Tooltip { x, y, .. } = &mut tip {
                        *x = mouse.0;
                        *y = mouse.1;
                    }
                    draws.push(tip);
                }
            }
            other => draws.push(shift(UiDraw::from(other), ox, oy)),
        }
    }

    // `AbstractScrollArea.extractScrollbar`, after the contents: shown while the scroll range is positive.
    let range = scroll_range(content_height, ah);
    if range > 0 {
        let bar_x = ax + aw - SCROLLBAR_WIDTH;
        let bar = thumb_height(ah, content_height + TOTAL_PADDING);
        let travel = (ah - bar) as f32;
        let offset = (scroll / range as f32 * travel).floor() as i32;
        draws.push(UiDraw::Sprite {
            name: SCROLLER_BACKGROUND.into(),
            x: bar_x as f32,
            y: ay as f32,
            w: SCROLLBAR_WIDTH as f32,
            h: ah as f32,
        });
        draws.push(UiDraw::Sprite {
            name: SCROLLER.into(),
            x: bar_x as f32,
            y: (ay + offset) as f32,
            w: SCROLLBAR_WIDTH as f32,
            h: bar as f32,
        });
    }

    let hits = lay
        .hits()
        .iter()
        .filter_map(|h| {
            let x0 = (h.x + ox).max(ax);
            let y0 = (h.y + oy).max(ay);
            let x1 = (h.x + ox + h.w).min(ax + aw);
            let y1 = (h.y + oy + h.h).min(ay + ah);
            (x1 > x0 && y1 > y0).then(|| Hit { x: x0, y: y0, w: x1 - x0, h: y1 - y0, action: h.action })
        })
        .collect();

    ScoresFrame { draws, hits, content_height }
}

/// `AbstractScrollArea.setScrollAmount`: the offset kept between 0 and `scroll_range`. `content_height` is
/// `measureContentHeight` (the padding is added here).
pub fn clamp_scroll(scroll: f32, content_height: i32, area_h: i32) -> f32 {
    scroll.clamp(0.0, scroll_range(content_height, area_h) as f32)
}

/// `getMaxScrollAmount`: the content with its padding above and below, less the box's height, never negative.
fn scroll_range(content_height: i32, area_h: i32) -> i32 {
    (content_height + TOTAL_PADDING - area_h).max(0)
}

/// `scrollBarHeight`: the box squared over the content, held between `THUMB_MIN` and the box less `THUMB_INSET`.
fn thumb_height(box_h: i32, content: i32) -> i32 {
    let raw = box_h * box_h / content.max(1);
    if raw < THUMB_MIN {
        THUMB_MIN
    } else {
        raw.min(box_h - THUMB_INSET)
    }
}

/// `mouseScrolled`: `scroll - scrollY * scrollRate()`, then clamped. A positive `amount` is the wheel up.
pub fn wheel(scroll: f32, amount: f64, content_height: i32, area_h: i32) -> f32 {
    clamp_scroll((f64::from(scroll) - amount * SCROLL_RATE) as f32, content_height, area_h)
}

/// `GameInfoWidget.render`: the frame and the centred lines, with the widget's origin at `(x, y)`.
pub fn game_info_draws(info: &GameInfo, x: i32, y: i32) -> Vec<UiDraw> {
    info.draw().iter().map(|d| shift(UiDraw::from(d), x, y)).collect()
}

/// Moves a text, a sprite or a nine-slice by a whole offset; the other commands are not placed by a corner.
fn shift(d: UiDraw, dx: i32, dy: i32) -> UiDraw {
    match d {
        UiDraw::Text { x, y, text, argb, shadow, obfuscated } => UiDraw::Text {
            x: x + (dx as f32),
            y: y + (dy as f32),
            text,
            argb,
            shadow,
            obfuscated,
        },
        UiDraw::Sprite { name, x, y, w, h } => {
            UiDraw::Sprite { name, x: x + (dx as f32), y: y + (dy as f32), w, h }
        }
        UiDraw::NineSlice { name, x, y, w, h, slice, tex } => {
            UiDraw::NineSlice { name, x: x + dx, y: y + dy, w, h, slice, tex }
        }
        other => other,
    }
}

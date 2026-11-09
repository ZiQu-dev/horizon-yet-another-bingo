//! The ready/countdown line above the hotbar, as draw commands. Ported from the original client's
//! `client/common/hud/ReadyHudRenderer.kt`, placed by `BingoHudController.drawHudInfo` (centred on the screen, 40 px
//! from the top, 100 px wide).
//!
//! Lines, top to bottom: the title (pulsing white while not ready, solid white once ready), the subtitle (mint),
//! a progress bar showing the time used, and one mark per player (up to 10): a tick for those ready, a dot for the
//! rest. Nothing is drawn unless the server says the ready timer is running.
//!
//! The progress bar is smoothed from frame to frame, so the caller keeps `progress_interpolated` between frames
//! (the Kotlin field of the same name) and it only moves on frames where the line is drawn.
//!
//! Not drawn yet, on purpose: `canSendReady` is not read here (the original only uses it for the key that readies up).

use std::f64::consts::PI;

use crate::card_draw::Draw;
use crate::ready_update::ReadyUpdatePacket;
use crate::text_plain::{flatten, Lang};

/// `ReadyHudRenderer.TEXT_COLOR` (mint).
pub const TEXT_COLOR: u32 = 0xffa3f5a3;
/// `ReadyHudRenderer.LOADING_COLOR` (white): the ready title, the bar, the dots of players not ready.
pub const LOADING_COLOR: u32 = 0xffffffff;
/// `font.getTextHeight()`: the vanilla font's line height.
// TODO(H3): the host's font height should come in beside `width_of`, as the width does; 9 is vanilla's value.
pub const FONT_HEIGHT: i32 = 9;

/// `drawHudInfo`: the line's top is 40 GUI pixels down, its width is 100, centred on the screen.
const TOP: i32 = 40;
const WIDTH: i32 = 100;
/// At most this many player marks are drawn.
const MAX_MARKS: i32 = 10;
/// The bar's height, drawn as `height` pixels plus a one-pixel bottom edge.
const BAR_HEIGHT: i32 = 5;
/// `coerceAtLeast(1.seconds)`: a ready timer shorter than a second counts as a second.
const MIN_TOTAL_MS: i64 = 1_000;

/// The ready line for the given packet. `now_ms` is the host's clock in milliseconds, `screen_width` the GUI width,
/// `lang` looks up translation keys, `width_of` measures a string in pixels. `progress_interpolated` is the bar's
/// smoothed progress, updated when this returns draws.
pub fn draw_ready(
    ready: Option<&ReadyUpdatePacket>,
    now_ms: u64,
    screen_width: i32,
    lang: Lang<'_>,
    width_of: &dyn Fn(&str) -> i32,
    progress_interpolated: &mut f32,
) -> Vec<Draw> {
    let mut out = Vec::new();
    let Some(ready) = ready else { return out };
    if !ready.is_running {
        return out;
    }
    let x = screen_width / 2 - 50;
    let y = TOP;
    let width = WIDTH;
    let font_h = FONT_HEIGHT;

    let title_argb = if ready.is_ready { LOADING_COLOR } else { pulse_argb(now_ms) };
    if let Some(title) = &ready.title {
        let text = flatten(title, lang);
        let tx = x + width / 2 - width_of(&text) / 2;
        out.push(Draw::Text { x: tx, y, text, argb: title_argb });
    }
    if let Some(subtitle) = &ready.subtitle {
        let text = flatten(subtitle, lang);
        let tx = x + width / 2 - width_of(&text) / 2;
        out.push(Draw::Text { x: tx, y: y + font_h + 2, text, argb: TEXT_COLOR });
    }

    // remainingDuration / totalDuration.coerceAtLeast(1 s), then the part still to come as the bar's target.
    let total = ready.total_millis.max(MIN_TOTAL_MS);
    let remaining_progress = (ready.remaining_millis as f64 / total as f64) as f32;
    let target = 1.0 - remaining_progress.clamp(0.0, 1.0);
    progress_bar(&mut out, x, y + font_h * 2 + 5, width, BAR_HEIGHT, target, progress_interpolated);

    // One mark per player, up to 10. Slot i is ready when i * total < count * ready (integer arithmetic).
    let count = ready.total_players.min(MAX_MARKS);
    let marks: Vec<(&str, u32, i32)> = (0..count)
        .map(|i| {
            let is_ready = i * ready.total_players < count * ready.ready_players;
            let ch = if is_ready { "\u{2714}" } else { "\u{2022}" };
            let color = if is_ready { TEXT_COLOR } else { LOADING_COLOR };
            (ch, color, width_of(ch) + 2)
        })
        .collect();
    let char_width = width_of("\u{2714}");
    let total_width = char_width * marks.len() as i32;
    let mark_y = y + font_h * 2 + 12;
    for (i, (ch, color, mark_width)) in marks.iter().enumerate() {
        let mx = x + width / 2 - total_width / 2 + (i as i32) * char_width + (char_width - mark_width) / 2;
        out.push(Draw::Text { x: mx, y: mark_y, text: ch.to_string(), argb: *color });
    }
    out
}

/// The title's colour while not ready: white whose alpha pulses once a second. `sin(phase * 2 PI) * 0.4 + 0.6`, where
/// the phase is the sub-second part of the clock (`Instant.nano`), so the pulse repeats every 1000 ms.
fn pulse_argb(now_ms: u64) -> u32 {
    let nano_of_second = (now_ms % 1000) * 1_000_000;
    let phase = nano_of_second as f64 / 1_000_000_000.0;
    let alpha = (phase * 2.0 * PI).sin() * 0.4 + 0.6;
    // `(alpha * 255).toInt()`: truncated, not rounded.
    let a = (alpha * 255.0) as i32;
    ((a as u32) << 24) | 0x00ff_ffff
}

/// `drawProgressBar`: a white outline of `width` x `height` whose left part is filled up to the smoothed progress.
fn progress_bar(out: &mut Vec<Draw>, x: i32, y: i32, width: i32, height: i32, progress: f32, smoothed: &mut f32) {
    // `(previous.takeIf { isFinite } ?: 0) * 0.8 + progress * 0.2`
    let previous = if smoothed.is_finite() { *smoothed } else { 0.0 };
    *smoothed = previous * 0.8 + progress * 0.2;

    let x_progress = x + ((width - 1) as f32 * *smoothed).round() as i32 + 1;
    fill(out, x, y, x_progress, y + height + 1, LOADING_COLOR);
    hline(out, x_progress, x + width, y, LOADING_COLOR);
    hline(out, x_progress, x + width, y + height, LOADING_COLOR);
    vline(out, x + width, y, y + height, LOADING_COLOR);
}

/// `DrawContext.fill(x1, y1, x2, y2)`: the far corner is exclusive.
fn fill(out: &mut Vec<Draw>, x1: i32, y1: i32, x2: i32, y2: i32, argb: u32) {
    out.push(Draw::Fill { x: x1, y: y1, w: x2 - x1, h: y2 - y1, argb });
}

/// `drawHorizontalLine(x1, x2, y)`: both ends inclusive, one pixel thick.
fn hline(out: &mut Vec<Draw>, x1: i32, x2: i32, y: i32, argb: u32) {
    let (a, b) = (x1.min(x2), x1.max(x2));
    out.push(Draw::Fill { x: a, y, w: b - a + 1, h: 1, argb });
}

/// `drawVerticalLine(x, y1, y2)`: both ends inclusive, one pixel wide.
fn vline(out: &mut Vec<Draw>, x: i32, y1: i32, y2: i32, argb: u32) {
    let (a, b) = (y1.min(y2), y1.max(y2));
    out.push(Draw::Fill { x, y: a, w: 1, h: b - a + 1, argb });
}

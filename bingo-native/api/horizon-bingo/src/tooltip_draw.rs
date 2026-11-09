//! The tooltip shown while the player looks at an entity, ported from `hud/TooltipHudRenderer.kt`: which lines show for
//! an entity id from `HudState.tooltips_by_entity` (after 20 ticks on it), the text wrapped at 200 px, the box's place
//! and the fade in and out. Pure: the host gives the entity under the crosshair, the time, the window size, the
//! language and the font's width function; nothing is drawn here.
//!
//! The original places the tooltip at the centre of the screen, not beside the entity, so the entity's position on
//! screen is not needed and not taken.

use std::collections::HashMap;

use crate::text::Text;
use crate::text_plain::{flatten, Lang};
use crate::tooltip_v2::TooltipPacket;

/// `lookingAtTicks >= 20`: the entity must be looked at for 20 ticks before its tooltip shows (the counter is capped at 20).
pub const LOOK_TICKS: i32 = 20;
/// `TooltipPacket.text` is wrapped with `font.wrapLines(line, 200)`.
pub const WRAP_WIDTH: i32 = 200;
/// `fadeIn` and `fadeOut` both take 250 ms.
pub const FADE_MS: u64 = 250;
/// The fade-out starts 100 ms after the last update (`tooltipUpdatedAt.plus(100.milliseconds)`).
pub const FADE_OUT_DELAY_MS: u64 = 100;
/// `translate(0, 0, 2000)`: the tooltip is drawn above the rest of the HUD.
pub const TOOLTIP_Z: f32 = 2000.0;
/// `scaledWindowWidth / 2 - tooltipSize / 2 - 12`.
pub const HORIZONTAL_SHIFT: i32 = 12;
/// `(scaledWindowHeight * 0.5).toInt() + 18 + 12`.
pub const VERTICAL_OFFSET: i32 = 18 + 12;

/// What the host must draw for a tooltip. The box and its lines are vanilla's tooltip (the mod calls
/// `drawService.drawTooltip`, which hands the lines to the game's own tooltip renderer), so this is a request, not
/// pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum TooltipDraw {
    /// The tooltip lines at (x, y), drawn at depth `z` with the shader colour's alpha set to `alpha`
    /// (`fadeIn * fadeOut`). `alpha` is 0 at the start of a fade and when the tooltip is leaving.
    // TODO(integration): alpha has no place in `card_draw::Draw`, and vanilla's tooltip takes no alpha. The host
    // must either apply `alpha` to the tooltip it opens, or accept a tooltip that fades in only through its own
    // box. Depth `z` is likewise the host's to honour. Nothing else in the original is lost.
    Tooltip { lines: Vec<String>, x: i32, y: i32, z: f32, alpha: f32 },
}

/// The state `TooltipHudRenderer` keeps between ticks and frames.
#[derive(Debug, Default, Clone)]
pub struct TooltipState {
    /// `lookingAtEntity`: the entity under the crosshair, if the host reports one.
    looking_at: Option<u128>,
    /// `lookingAtTicks`: how long it has been under the crosshair, capped at `LOOK_TICKS`.
    looking_ticks: i32,
    /// `tooltipPacket`: the tooltip being shown or faded out. `None` when there is none.
    shown: Option<TooltipPacket>,
    /// `tooltipStartedAt` (`None` for `Instant.MIN`): when this tooltip started to show.
    started_at: Option<u64>,
    /// `tooltipUpdatedAt`: the last tick at which the tooltip was looked at for long enough.
    updated_at: Option<u64>,
}

/// `tick()`: `looking_at` is the entity id of the entity hit by the crosshair (`EntityHitResult.entityId`), `None`
/// when the crosshair is on nothing or on a block. `tooltips` is `HudState.tooltips_by_entity`.
impl TooltipState {
    pub fn tick(&mut self, looking_at: Option<u128>, tooltips: &HashMap<u128, TooltipPacket>, now_ms: u64) {
        if self.looking_at == looking_at {
            self.looking_ticks = (self.looking_ticks + 1).min(LOOK_TICKS);
        } else {
            self.looking_at = looking_at;
            self.looking_ticks = 0;
        }

        // `tooltipsByEntity[lookingAtEntity ?: return] ?: return`: nothing to show for this entity.
        let Some(entity) = self.looking_at else { return };
        let Some(tooltip) = tooltips.get(&entity) else { return };
        if self.looking_ticks >= LOOK_TICKS {
            if self.shown.is_none() {
                self.started_at = Some(now_ms);
            }
            // A tooltip for another entity replaces the one showing, without restarting the fade-in.
            self.shown = Some(tooltip.clone());
            self.updated_at = Some(now_ms);
        }
    }

    /// `drawTooltip(drawService)`: the commands for this frame, empty when no tooltip is showing. When the fade-out has
    /// finished the tooltip is forgotten, but its last command (with alpha 0) is still returned, as in the original.
    /// `screen` is the scaled window size (width, height).
    pub fn draw(
        &mut self,
        now_ms: u64,
        screen: (i32, i32),
        lang: Lang<'_>,
        width_of: &dyn Fn(&str) -> i32,
    ) -> Vec<TooltipDraw> {
        let Some(packet) = self.shown.as_ref() else { return Vec::new() };

        let lines = tooltip_lines(packet, lang, width_of);
        let size = lines.iter().map(|l| width_of(l)).max().unwrap_or(0);
        let x = screen.0 / 2 - size / 2 - HORIZONTAL_SHIFT;
        let y = (screen.1 as f64 * 0.5) as i32 + VERTICAL_OFFSET;

        // `tooltipStartedAt` at `Instant.MIN` is far in the past, so the fade-in is complete.
        let fade_in = match self.started_at {
            Some(started) => ratio(now_ms as i64 - started as i64, FADE_MS),
            None => 1.0,
        };
        let updated = self.updated_at.unwrap_or(0);
        let fade_out = 1.0 - ratio(now_ms as i64 - (updated + FADE_OUT_DELAY_MS) as i64, FADE_MS);
        let alpha = (fade_in * fade_out) as f32;

        let out = vec![TooltipDraw::Tooltip { lines, x, y, z: TOOLTIP_Z, alpha }];
        if fade_out <= 0.0 {
            self.shown = None;
        }
        out
    }
}

/// `(elapsed / span).coerceIn(0, 1)`.
fn ratio(elapsed_ms: i64, span_ms: u64) -> f64 {
    (elapsed_ms as f64 / span_ms as f64).clamp(0.0, 1.0)
}

/// The lines of a tooltip: each text entry flattened, then wrapped at 200 px on its own (`flatMap` in the original).
pub fn tooltip_lines(packet: &TooltipPacket, lang: Lang<'_>, width_of: &dyn Fn(&str) -> i32) -> Vec<String> {
    packet
        .text
        .iter()
        .flat_map(|t: &Text| wrap_lines(&flatten(t, lang), WRAP_WIDTH, width_of))
        .collect()
}

/// `Font.wrapLines` for plain text: a newline always breaks; a line that is too wide breaks at its last space, the
/// space itself dropped; a word wider than the line is cut where the line is full. A single character is never cut.
// TODO(integration): the vanilla splitter works on styled text and on each glyph's own width, so a wrap can fall
// one glyph off from it. The mod's tooltips are drawn in one colour and their widths come from the same font, so
// the difference is only in the rare line that is cut in the middle of a word.
pub fn wrap_lines(text: &str, max_width: i32, width_of: &dyn Fn(&str) -> i32) -> Vec<String> {
    text.split('\n').flat_map(|paragraph| wrap_paragraph(paragraph, max_width, width_of)).collect()
}

fn wrap_paragraph(text: &str, max_width: i32, width_of: &dyn Fn(&str) -> i32) -> Vec<String> {
    if width_of(text) <= max_width {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut line = String::new();
    // Byte index in `line` of its last space: the place a break may go.
    let mut space_at: Option<usize> = None;
    for c in text.chars() {
        line.push(c);
        if c == ' ' {
            space_at = Some(line.len() - 1);
        }
        if line.chars().count() > 1 && width_of(&line) > max_width {
            match space_at {
                Some(i) if i > 0 => {
                    // the words after the space start the next line; the space is dropped
                    let rest = line.split_off(i + 1);
                    line.pop();
                    out.push(std::mem::replace(&mut line, rest));
                }
                _ => {
                    // no space to break at: the character that overflowed starts the next line
                    let last = line.pop().expect("a character was just pushed");
                    out.push(std::mem::take(&mut line));
                    line.push(last);
                }
            }
            space_at = None;
        }
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

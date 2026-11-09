//! The in-world player tooltip, from `hud/TooltipHudRenderer.kt`: the local player looks at another player for 20 ticks
//! and that player's tooltip shows near the middle of the screen, fading in and out.
//!
//! Two pure parts:
//! - [`LookTooltip`]: the state machine. `update` runs once per game tick with the entity under the crosshair (the
//!   host's `looked_at_player` below, or a block-free test of its own); `draw` gives the tooltip for a frame.
//! - [`looked_at_player`]: which player the view ray from the eyes hits first, with vanilla's view vector and a plain
//!   ray against each player's box. No host calls.
//!
//! Where the original's timing and placement are kept: the 20-tick look (`tick`: the first tick only records the
//! entity), the box at `width / 2 - maxLine / 2 - 12` and `height * 0.5 + 30`, lines wrapped at 200 px, a 250 ms
//! fade-in from when the tooltip first showed, and a fade-out that starts 100 ms after the last update and takes 250 ms.
//!
//! Differences from the original, each where it is handled:
//! - `draw` is `&self`, so the tooltip is forgotten in `update` once its fade-out has finished (the original clears it
//!   in `drawTooltip`). Its last frame, the one with `fadeOut <= 0`, is not drawn; the frame before it is the last.
//! - The fade is `alpha`, not part of the draw: `UiDraw::Tooltip` has no alpha. The host applies it through
//!   `Host::begin_alpha` and `Host::end_alpha` around the tooltip (`TooltipHudRenderer` fades the whole tooltip, since
//!   `setShaderColor` tints the box's sprites and its text).
//! - The text is read from `HudState::tooltips_by_entity` at draw time. The original latches the packet on each tick
//!   while the entity is looked at, so the text can differ by one tick from the packet the server sent last.

use crate::state::HudState;
use crate::text_plain::Lang;
use crate::tooltip_draw::{tooltip_lines, FADE_MS, FADE_OUT_DELAY_MS, HORIZONTAL_SHIFT, LOOK_TICKS, VERTICAL_OFFSET};
use crate::ui_draw::UiDraw;

/// `TooltipHudRenderer`'s state between ticks and frames.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LookTooltip {
    /// `lookingAtEntity`: the entity under the crosshair, by UUID (`TooltipPacket.entityId`).
    looked_at: Option<u128>,
    /// The game tick at which `looked_at` was last set; `ticks - looking_since` is `lookingAtTicks` (capped at
    /// `LOOK_TICKS` in the original, compared against it here).
    looking_since: u64,
    /// `tooltipPacket`: the entity whose tooltip is showing or fading out. `None` when there is none.
    shown: Option<u128>,
    /// `tooltipStartedAt` (ms): when the tooltip started to show. `None` while nothing shows.
    started_at: Option<u64>,
    /// `tooltipUpdatedAt` (ms): the last tick at which the entity was looked at for long enough.
    updated_at: Option<u64>,
}

impl LookTooltip {
    /// `TooltipHudRenderer.tick()`. `looked_at` is the entity under the crosshair (`None` for nothing or a block),
    /// `ticks` the game tick counter, which must rise by one per call, and `now_ms` the time of the tick.
    pub fn update(&mut self, looked_at: Option<u128>, state: &HudState, ticks: u64, now_ms: u64) {
        // `drawTooltip`'s `tooltipPacket = null` once the fade-out is over, done here because `draw` takes `&self`.
        if self.shown.is_some() && self.fade_out_done(now_ms) {
            self.shown = None;
        }

        if self.looked_at != looked_at {
            self.looked_at = looked_at;
            self.looking_since = ticks;
        }

        // `tooltipsByEntity[lookingAtEntity ?: return] ?: return`: nothing to show for this entity.
        let Some(entity) = self.looked_at else { return };
        if !state.tooltips_by_entity.contains_key(&entity) {
            return;
        }
        if ticks.saturating_sub(self.looking_since) >= LOOK_TICKS as u64 {
            if self.shown.is_none() {
                self.started_at = Some(now_ms);
            }
            self.shown = Some(entity);
            self.updated_at = Some(now_ms);
        }
    }

    /// `drawTooltip(drawService)`: the tooltip box for this frame, empty when none shows. `gui` is the scaled window
    /// size; `text_width` is the font's width function, the one `wrapLines` and `getTextWidth` use.
    ///
    /// `UiDraw::Tooltip`'s `(x, y)` is the anchor the original passes to `drawService.drawTooltip` (the cursor-like
    /// point vanilla's `DefaultTooltipPositioner` moves by +12, -12 and keeps on the screen), not the box's corner.
    pub fn draw(
        &self,
        state: &HudState,
        now_ms: u64,
        gui: (i32, i32),
        lang: Lang<'_>,
        text_width: &dyn Fn(&str) -> i32,
    ) -> Vec<UiDraw> {
        let Some(packet) = self.shown.and_then(|e| state.tooltips_by_entity.get(&e)) else { return Vec::new() };
        if self.fade_out_done(now_ms) {
            return Vec::new();
        }
        let lines = tooltip_lines(packet, lang, text_width);
        let widest = lines.iter().map(|l| text_width(l)).max().unwrap_or(0);
        let x = gui.0 / 2 - widest / 2 - HORIZONTAL_SHIFT;
        let y = (gui.1 as f64 * 0.5) as i32 + VERTICAL_OFFSET;
        vec![UiDraw::Tooltip { x, y, lines }]
    }

    /// The alpha the tooltip is drawn with at `now_ms`: `fadeIn * fadeOut`, 0 to 1. Zero while nothing shows.
    pub fn alpha(&self, now_ms: u64) -> f32 {
        let (Some(started), Some(updated)) = (self.started_at, self.updated_at) else { return 0.0 };
        if self.shown.is_none() {
            return 0.0;
        }
        let fade_in = ratio(now_ms as i64 - started as i64);
        let fade_out = 1.0 - ratio(now_ms as i64 - (updated + FADE_OUT_DELAY_MS) as i64);
        (fade_in * fade_out) as f32
    }

    /// `fadeOut <= 0`: the fade-out has finished, 350 ms after the last update.
    fn fade_out_done(&self, now_ms: u64) -> bool {
        match self.updated_at {
            Some(updated) => now_ms >= updated + FADE_OUT_DELAY_MS + FADE_MS,
            None => true,
        }
    }
}

/// `(elapsed / span).coerceIn(0, 1)` for a span of `FADE_MS`.
fn ratio(elapsed_ms: i64) -> f64 {
    (elapsed_ms as f64 / FADE_MS as f64).clamp(0.0, 1.0)
}

/// The player the view ray from `eye` hits first, within `max_dist` blocks, or `None`.
///
/// `yaw_deg` and `pitch_deg` are the player's rotation in degrees (`Entity.getYRot` and `getXRot`: yaw 0 faces south,
/// +Z, and 90 faces west, -X; pitch is positive looking down). Each player is `(uuid, feet, width, height)`: the
/// bottom centre of its box and its size, as `Entity.getBoundingBox` gives them. The ray is vanilla's view vector
/// (`Entity.calculateViewVector`) and it hits a box when it passes through it, touching included. A player whose box
/// the eye is inside is hit at distance 0. Blocks are not tested: the caller passes a `max_dist` no longer than the
/// block the ray hits first.
pub fn looked_at_player(
    eye: [f64; 3],
    yaw_deg: f32,
    pitch_deg: f32,
    players: &[(u128, [f64; 3], f32, f32)],
    max_dist: f64,
) -> Option<u128> {
    let dir = view_vector(yaw_deg, pitch_deg);
    let mut best: Option<(f64, u128)> = None;
    for &(uuid, feet, width, height) in players {
        let half = f64::from(width) / 2.0;
        let min = [feet[0] - half, feet[1], feet[2] - half];
        let max = [feet[0] + half, feet[1] + f64::from(height), feet[2] + half];
        if let Some(t) = ray_box_entry(eye, dir, min, max, max_dist) {
            if best.map_or(true, |(bt, _)| t < bt) {
                best = Some((t, uuid));
            }
        }
    }
    best.map(|(_, uuid)| uuid)
}

/// `Entity.calculateViewVector(xRot, yRot)`: `(sin(-yaw) * cos(pitch), -sin(pitch), cos(-yaw) * cos(pitch))`. Its
/// length is 1, so a distance along it is the distance along the ray.
fn view_vector(yaw_deg: f32, pitch_deg: f32) -> [f64; 3] {
    let yaw = f64::from(-yaw_deg).to_radians();
    let pitch = f64::from(pitch_deg).to_radians();
    [yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}

/// The distance along `origin + t * dir` (for `t` in `0..=max_dist`) at which the ray enters the box, by the slab
/// method: each axis gives an interval of `t` in which the ray is between its two planes, and the ray is inside the
/// box where all three intervals overlap.
fn ray_box_entry(origin: [f64; 3], dir: [f64; 3], min: [f64; 3], max: [f64; 3], max_dist: f64) -> Option<f64> {
    let mut enter = 0.0_f64;
    let mut exit = max_dist;
    for axis in 0..3 {
        if dir[axis] == 0.0 {
            // parallel to these two planes: the ray is either between them for every `t` or for none
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir[axis];
        let (mut a, mut b) = ((min[axis] - origin[axis]) * inv, (max[axis] - origin[axis]) * inv);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        enter = enter.max(a);
        exit = exit.min(b);
        if enter > exit {
            return None;
        }
    }
    Some(enter)
}

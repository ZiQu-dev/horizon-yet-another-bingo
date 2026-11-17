//! The stars of the end screen's firework, port of `BingoEndFireworkRenderer.kt`.
//!
//! Fifty stars leave the centre of the screen on straight lines, each turning as it goes and fading as it moves away.
//! A star is dropped for good once its alpha reaches the fade-out threshold, so a later draw never brings it back.
//! Time is in milliseconds since the firework started; the Kotlin counts 50 ms ticks, so positions are `vel * ms / 50`.

use std::f64::consts::PI;

use crate::ui_draw::UiDraw;

/// Number of stars a firework starts with.
const STAR_COUNT: usize = 50;
/// Distance at which a star's alpha reaches zero, in GUI pixels, for the end screen (`range = 500`).
pub const RANGE: f64 = 500.0;
/// A star whose alpha is at or below this is removed.
const FADE_OUT: f64 = 0.025;
/// Milliseconds per tick, as the Kotlin counts them.
const MS_PER_TICK: f64 = 50.0;
/// Scale on the initial speed of every star.
const VELOCITY_SCALE: f64 = 1.5;
/// Upward drift added to every star's y velocity.
const Y_DRIFT: f64 = -0.2;
/// The star glyph, drawn with `TextXf`.
const STAR_TEXT: &str = "★";
/// Offset of the glyph from the star's position, so the glyph is centred on it.
const GLYPH_OFFSET: f32 = -4.0;

/// The six colours a star picks from, as 0xRRGGBB (the Kotlin's ARGB with the alpha byte removed).
const COLOURS: [u32; 6] = [0xF5C0A3, 0xA3F5A3, 0xA3F5C7, 0xA3ABF5, 0xCCA3F5, 0xF5A3D6];

/// One star. Its position at tick `t` is `(x_vel * t, y_vel * t)` from the centre; it turns from `rot` by `rot_vel * t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    pub x_vel: f64,
    pub y_vel: f64,
    pub rot: f64,
    pub rot_vel: f64,
    /// Colour as 0xRRGGBB.
    pub rgb: u32,
}

/// A running firework: its stars and the time it started.
#[derive(Debug, Clone)]
pub struct Fireworks {
    started_ms: u64,
    range: f64,
    stars: Vec<Star>,
}

impl Fireworks {
    /// Starts a firework at `started_ms`, with `STAR_COUNT` stars whose random values come from `seed`, over the end
    /// screen's [`RANGE`].
    pub fn new(started_ms: u64, seed: u64) -> Self {
        Self::with_range(started_ms, seed, RANGE)
    }

    /// As [`Fireworks::new`], with stars fading out at `range` pixels (the HUD's is `100 * cardScale`).
    pub fn with_range(started_ms: u64, seed: u64, range: f64) -> Self {
        let mut rng = XorShift64Star::new(seed);
        // Draw order per star matches the Kotlin: theta, vel, rot, rotVel, colour.
        let stars = (0..STAR_COUNT)
            .map(|_| {
                let theta = rng.next_f64() * PI * 2.0;
                let vel = 1.0 + rng.next_f64();
                let rot = rng.next_f64() * PI * 2.0;
                let rot_vel = (rng.next_f64() - 0.5) * 0.5 * VELOCITY_SCALE;
                let colour_index = (rng.next_f64() * COLOURS.len() as f64) as usize;
                Star {
                    x_vel: theta.cos() * vel * VELOCITY_SCALE,
                    y_vel: theta.sin() * vel * VELOCITY_SCALE + Y_DRIFT,
                    rot,
                    rot_vel,
                    rgb: COLOURS[colour_index],
                }
            })
            .collect();
        Fireworks { started_ms, range, stars }
    }

    /// The stars still alive.
    pub fn stars(&self) -> &[Star] {
        &self.stars
    }

    /// How many stars are still alive.
    pub fn star_count(&self) -> usize {
        self.stars.len()
    }

    /// True once every star has faded out.
    pub fn is_done(&self) -> bool {
        self.stars.is_empty()
    }

    /// Advances the firework to `now_ms` and returns the glyphs of the stars still visible, around the centre
    /// (`center_x`, `center_y`). Stars that fade out at this time are removed and never returned again.
    pub fn draw(&mut self, now_ms: u64, center_x: i32, center_y: i32) -> Vec<UiDraw> {
        let ticks = (now_ms as f64 - self.started_ms as f64) / MS_PER_TICK;
        let mut out = Vec::new();
        self.stars.retain(|star| {
            let x = star.x_vel * ticks;
            let y = star.y_vel * ticks;
            let rot = star.rot + star.rot_vel * ticks;
            let alpha = 1.0 - (x.hypot(y) / self.range).powi(3);
            if alpha <= FADE_OUT {
                return false;
            }
            let alpha_byte = ((alpha * 255.0) as i32).clamp(1, 255) as u32;
            out.push(UiDraw::TextXf {
                text: STAR_TEXT.into(),
                anchor: (center_x as f32 + x as f32, center_y as f32 + y as f32),
                offset: (GLYPH_OFFSET, GLYPH_OFFSET),
                scale: 1.0,
                rotation: rot as f32,
                argb: (alpha_byte << 24) | star.rgb,
                shadow: false,
            });
            true
        });
        out
    }
}

/// xorshift64* generator: small, deterministic, enough for scattering stars.
struct XorShift64Star {
    state: u64,
}

impl XorShift64Star {
    fn new(seed: u64) -> Self {
        // The state must not be zero, or the generator stays at zero forever.
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        if state == 0 {
            state = 0x9E37_79B9_7F4A_7C15;
        }
        XorShift64Star { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in [0, 1), from the top 53 bits.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

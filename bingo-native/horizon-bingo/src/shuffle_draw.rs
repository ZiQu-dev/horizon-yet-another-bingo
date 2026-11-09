//! The card's shuffle animation, ported from `utils/Interpolate.kt` (the easings and the interpolation) and from
//! `hud/card/ClientCardBufferRenderer.kt` (`createInterpolations`, the `shufflePositions` lookup and `SHUFFLE_DURATION`
//! in `drawCard`). Pure: the caller gives the time in milliseconds and the permutation, and gets back the top left
//! corner of each tile, relative to the card's top left corner, the same origin `card_draw` uses.
//!
//! Randomness stays with the caller. The original calls `shufflePositions.shuffled()` on each `card_shuffled` packet;
//! here the caller draws the order with its own RNG and applies it with [`shuffle_positions_after`].

/// `ClientCardBufferRenderer.SHUFFLE_DURATION` (280 ms).
pub const SHUFFLE_DURATION_MS: u64 = 280;
/// `ClientCardBufferRenderer.SHUFFLE_DURATION_PADDING` (50 ms). The animation stops being computed after the sum of both;
/// the value is never visible in the output, because at the end the interpolation returns `to` exactly.
pub const SHUFFLE_DURATION_PADDING_MS: u64 = 50;
/// The tile grid: `12 + 20 x`, `22 + 20 y`, five tiles a row (`drawCard`).
pub const TILE_ORIGIN_X: i32 = 12;
pub const TILE_ORIGIN_Y: i32 = 22;
pub const TILE_PITCH: i32 = 20;

/// `Interpolate.Easing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    InOut,
    InOutReverse,
    ExpOut,
    ExpOutReverse,
}

/// `utils/Interpolate`: a value that moves from `from` to `to` over `duration_ms`, starting at `started_at_ms`.
/// Times are the host's milliseconds, as everywhere in this crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interpolate {
    pub from: f32,
    pub to: f32,
    pub duration_ms: u64,
    pub started_at_ms: u64,
    pub easing: Easing,
}

/// `easeInOut`: `-(cos(PI * p) - 1) / 2`, computed in double and cut to float as the Kotlin does.
fn ease_in_out(p: f32) -> f32 {
    let c = (std::f64::consts::PI * p as f64).cos();
    (-((c - 1.0) as f32)) / 2.0
}

/// `expOut`: `1 - 2^(-10 p)`, the power in double, then cut to float.
fn exp_out(p: f32) -> f32 {
    1.0 - (2.0f64.powf((-10.0 * p) as f64) as f32)
}

impl Interpolate {
    pub fn new(from: f32, to: f32, duration_ms: u64, started_at_ms: u64, easing: Easing) -> Self {
        Interpolate { from, to, duration_ms, started_at_ms, easing }
    }

    /// `progress(easing, now)`: the elapsed fraction clamped to 0..=1, then shaped by the easing. Before the start it
    /// is 0 and after the end it is 1. A zero duration is not used by the card; it finishes at once.
    pub fn progress(&self, easing: Easing, now_ms: u64) -> f32 {
        let elapsed = now_ms as i64 - self.started_at_ms as i64;
        let p = if self.duration_ms == 0 {
            if elapsed >= 0 { 1.0 } else { 0.0 }
        } else {
            (elapsed as f64 / self.duration_ms as f64).clamp(0.0, 1.0) as f32
        };
        match easing {
            Easing::Linear => p,
            Easing::InOut => ease_in_out(p),
            Easing::InOutReverse => 1.0 - ease_in_out(p),
            Easing::ExpOut => exp_out(p),
            Easing::ExpOutReverse => 1.0 - exp_out(p),
        }
    }

    /// `get(easing, now)`: `from * (1 - progress) + to * progress`, in float as in the original.
    pub fn get(&self, easing: Easing, now_ms: u64) -> f32 {
        let progress = self.progress(easing, now_ms);
        (self.from * (1.0 - progress)) + (self.to * progress)
    }

    /// `isDone()`: strictly after the end. At the end instant itself it is not done yet.
    pub fn is_done(&self, now_ms: u64) -> bool {
        now_ms > self.started_at_ms.saturating_add(self.duration_ms)
    }
}

/// The home corner of a grid slot (0..25): where the tile sits when the card is not animating.
pub fn slot_origin(slot: usize) -> (f32, f32) {
    let (x, y) = ((slot % 5) as i32, (slot / 5) as i32);
    ((TILE_ORIGIN_X + x * TILE_PITCH) as f32, (TILE_ORIGIN_Y + y * TILE_PITCH) as f32)
}

/// `createInterpolations`: the animation runs while `now < shuffledAt + 280 + 50`. A card never shuffled
/// (`shuffledAt` is `Instant.MIN` in the original) is not animated.
pub fn is_animating(shuffled_at_ms: Option<u64>, now_ms: u64) -> bool {
    match shuffled_at_ms {
        None => false,
        Some(at) => now_ms < at.saturating_add(SHUFFLE_DURATION_MS + SHUFFLE_DURATION_PADDING_MS),
    }
}

// `ClientCard.shuffle_positions` (the Kotlin `shufflePositions`, starting at 0..25) is replaced by `HudState` on each
// `card_shuffled` that passes the debounce, with [`shuffled`] and the state's [`ShuffleRng`]. Not ported:
// `Interpolate.update()`, a per-frame flag with no drawing effect.

/// The top left corner of tile `tile_index` (0..25) at `now_ms`, relative to the card.
///
/// While the card is animating, the tile starts at the home corner of the slot `shuffle_positions[tile_index % len]`
/// and eases (InOut, 280 ms from the shuffle) to its own home corner. An empty `shuffle_positions` (the original
/// would divide by zero) falls back to the tile's own slot, so nothing moves.
pub fn tile_origin(
    tile_index: usize,
    shuffled_at_ms: Option<u64>,
    now_ms: u64,
    shuffle_positions: &[usize],
) -> (f32, f32) {
    let home = slot_origin(tile_index);
    if !is_animating(shuffled_at_ms, now_ms) {
        return home;
    }
    let started_at = shuffled_at_ms.unwrap_or(0);
    let from_slot = if shuffle_positions.is_empty() {
        tile_index
    } else {
        shuffle_positions[tile_index % shuffle_positions.len()]
    };
    let from = slot_origin(from_slot);
    let x = Interpolate::new(from.0, home.0, SHUFFLE_DURATION_MS, started_at, Easing::InOut).get(Easing::InOut, now_ms);
    let y = Interpolate::new(from.1, home.1, SHUFFLE_DURATION_MS, started_at, Easing::InOut).get(Easing::InOut, now_ms);
    (x, y)
}

/// `shufflePositions.shuffled()` with the randomness supplied: `order` is a permutation of `0..current.len()`, and the
/// new positions are `current` read in that order. `None` when `order` is not such a permutation.
pub fn shuffle_positions_after(current: &[usize], order: &[usize]) -> Option<Vec<usize>> {
    if order.len() != current.len() {
        return None;
    }
    let mut seen = vec![false; current.len()];
    for &i in order {
        if i >= current.len() || std::mem::replace(&mut seen[i], true) {
            return None;
        }
    }
    Some(order.iter().map(|&i| current[i]).collect())
}

/// The seed a fresh [`ShuffleRng`] starts from. Fixed, so the crate reads no clock: the host calls
/// `HudState::set_shuffle_seed` when it wants another order.
pub const DEFAULT_SHUFFLE_SEED: u64 = 0;

/// SplitMix64, the order source for `shufflePositions.shuffled()`. Only the shuffles' order depends on it: the Kotlin
/// draws from `kotlin.random.Random`, which is not reproducible either, so the same seed here gives the same sequence
/// of shuffles on every run, which is what the tests need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShuffleRng {
    state: u64,
}

impl Default for ShuffleRng {
    fn default() -> Self {
        ShuffleRng::new(DEFAULT_SHUFFLE_SEED)
    }
}

impl ShuffleRng {
    pub fn new(seed: u64) -> Self {
        ShuffleRng { state: seed }
    }

    /// The next 64 bits of the SplitMix64 sequence.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n` with no modulo bias: values past the last multiple of `n` are drawn again.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has no answer");
        let n = n as u64;
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let v = self.next_u64();
            if v < zone {
                return (v % n) as usize;
            }
        }
    }

    /// A random permutation of `0..n`: Fisher-Yates from the last index down, as `java.util.Collections.shuffle` does.
    pub fn permutation(&mut self, n: usize) -> Vec<usize> {
        let mut p: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = self.below(i + 1);
            p.swap(i, j);
        }
        p
    }
}

/// `shufflePositions.shuffled()`: the list `current` read in a random order drawn from `rng`. The length is kept, so
/// the result is a permutation of `current`'s elements.
pub fn shuffled(current: &[usize], rng: &mut ShuffleRng) -> Vec<usize> {
    let order = rng.permutation(current.len());
    order.iter().map(|&i| current[i]).collect()
}

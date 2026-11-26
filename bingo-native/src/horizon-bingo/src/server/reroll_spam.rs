//! The reroll spam of the lobby panel (the original's `CardReroll.kt`). A reroll within a second of the last one plays a
//! primed TNT, louder and higher with each repeat; the repeat after the hundredth spams 20 TNT around the player.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::{Duration, Instant};

/// The primed TNT's sound, played on a repeated reroll.
pub const SOUND_TNT_PRIMED: &str = "minecraft:entity.tnt.primed";
/// `REROLL_SPAM_AMOUNT`: the repeats before the spam.
pub const SPAM_AMOUNT: u32 = 100;
/// The window in which a reroll repeats the last one (`1.seconds`).
const WINDOW: Duration = Duration::from_secs(1);
/// The TNT one spam summons (`repeat(20)`).
pub const SPAM_TNT: usize = 20;

/// The spam counter. Global to the lobby in the original; here it lives in the session.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct RerollSpam {
    last: Option<Instant>,
    rerolls: u32,
}

/// What one reroll does besides shuffling the card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Nothing: the first reroll, or one after a pause.
    Quiet,
    /// A primed TNT at this volume and pitch.
    Sound { volume: f32, pitch: f32 },
    /// The spam: `SPAM_TNT` TNT around the player.
    Spam,
}

impl RerollSpam {
    /// A reroll at `now`. `prevent_chaos` is the config's `preventLobbyChaos`: it turns the spam off.
    pub fn reroll(&mut self, now: Instant, prevent_chaos: bool) -> Effect {
        let repeat = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < WINDOW);
        self.last = Some(now);
        if !repeat || prevent_chaos {
            self.rerolls = 0;
            return Effect::Quiet;
        }
        let before = self.rerolls;
        self.rerolls += 1;
        if before > SPAM_AMOUNT {
            self.rerolls = 0;
            return Effect::Spam;
        }
        let fraction = self.rerolls as f32 / SPAM_AMOUNT as f32;
        Effect::Sound {
            volume: fraction,
            pitch: 0.5 + 0.5 * fraction,
        }
    }
}

/// The console command that plays the primed TNT to the player called `name`, at its volume and pitch.
pub fn sound_command(name: &str, volume: f32, pitch: f32) -> String {
    format!("playsound {SOUND_TNT_PRIMED} blocks {name} {volume:.3} {pitch:.3}")
}

/// The commands that summon one spam's TNT around the player called `name`: each `SPAM_TNT` one at a random place (x and z
/// in -1..1, y in 0..2 from the player) with a fuse of 10..=50 ticks. `random` gives numbers in 0..1.
pub fn spam_commands(name: &str, mut random: impl FnMut() -> f64) -> Vec<String> {
    (0..SPAM_TNT)
        .map(|_| {
            let x = -1.0 + 2.0 * random();
            let y = 2.0 * random();
            let z = -1.0 + 2.0 * random();
            let fuse = 10 + (random() * 41.0).min(40.0) as u32;
            format!("execute as {name} at @s run summon minecraft:tnt ~{x:.3} ~{y:.3} ~{z:.3} {{fuse:{fuse}}}")
        })
        .collect()
}

/// A random number in 0..1 from the standard library's randomly keyed hasher (no seeded game state is used).
pub fn random_unit() -> f64 {
    let bits = RandomState::new().build_hasher().finish();
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

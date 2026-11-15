//! Card generation: draws a card from the tier pool with a seeded SplitMix64 generator.
//! The same seed, pool and distribution always give the same card.
use super::types::{Card, Objective};
use crate::card_tile::TierLabel;
use serde_json::Value;

/// Squares per tier, in the order S, A, B, C, D. Sums to 25.
pub const DEFAULT_DISTRIBUTION: [usize; 5] = [0, 0, 0, 9, 16];

const TIER_NAMES: [&str; 5] = ["S", "A", "B", "C", "D"];
const TIER_LABELS: [TierLabel; 5] = [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D];

const POOL_JSON: &str = include_str!("../../data/items.tierlist.json");

/// Items per tier, in the order S, A, B, C, D. Each tier lists an item at most once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pool {
    pub tiers: [Vec<String>; 5],
}

/// The pool built into the mod (`data/items.tierlist.json`).
pub fn load_pool() -> Pool {
    pool_from_json(POOL_JSON).expect("built-in items.tierlist.json is malformed")
}

/// Reads a tier list: the keys S, A, B, C, D each hold an array of item ids. Entries starting with `#` are
/// tags and are skipped; other keys are ignored. Duplicates inside a tier are dropped, keeping the first.
pub fn pool_from_json(json: &str) -> Result<Pool, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let object = root.as_object().ok_or("tier list is not a JSON object")?;
    let mut tiers: [Vec<String>; 5] = Default::default();
    for (index, name) in TIER_NAMES.iter().enumerate() {
        let entries = object
            .get(*name)
            .ok_or_else(|| format!("tier {name} is missing"))?
            .as_array()
            .ok_or_else(|| format!("tier {name} is not an array"))?;
        for entry in entries {
            let id = entry.as_str().ok_or_else(|| format!("tier {name} has a non-string entry"))?;
            if id.starts_with('#') || tiers[index].iter().any(|seen| seen == id) {
                continue;
            }
            tiers[index].push(id.to_string());
        }
    }
    Ok(Pool { tiers })
}

/// SplitMix64, the generator of Java's `SplittableRandom`.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform value in `0..n`, `n > 0`. Values that would favour small results are rejected, so there is no bias.
    pub fn next_below(&mut self, n: usize) -> usize {
        assert!(n > 0, "next_below needs n > 0");
        let n = n as u64;
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < limit {
                return (x % n) as usize;
            }
        }
    }
}

/// Draws new items for the squares `indices` of `card`, each square keeping its tier. No item is on the card twice, and
/// a drawn item is not drawn again. A tier that runs out takes from the next easier tier, as [`generate_card`] does. A
/// square that no tier can fill keeps its item.
pub fn replace_squares(seed: u64, pool: &Pool, card: &mut Card, indices: &[usize]) {
    let mut rng = Rng::new(seed);
    let mut used: Vec<String> = card.squares.iter().map(|square| square.item.clone()).collect();
    for &index in indices {
        let Some(tier) = card.squares.get(index).map(|square| square.tier) else { continue };
        let first = TIER_LABELS.iter().position(|label| *label == tier).unwrap_or(TIER_LABELS.len() - 1);
        for source in first..TIER_LABELS.len() {
            let candidates: Vec<&String> = pool.tiers[source].iter().filter(|item| !used.contains(item)).collect();
            if candidates.is_empty() {
                continue;
            }
            let item = candidates[rng.next_below(candidates.len())].clone();
            used.push(item.clone());
            card.squares[index] = Objective { item, tier };
            break;
        }
    }
}

/// Builds a card: `distribution[i]` squares of tier `i`, no item twice. A tier that runs out of items takes
/// them from the next easier tier (the square keeps its own tier). Panics if the pool cannot fill the card.
pub fn generate_card(seed: u64, pool: &Pool, distribution: [usize; 5]) -> Card {
    let mut rng = Rng::new(seed);
    let mut used: Vec<String> = Vec::new();
    let mut squares: Vec<Objective> = Vec::with_capacity(distribution.iter().sum());

    for (tier, &wanted) in distribution.iter().enumerate() {
        let mut remaining = wanted;
        for source in tier..5 {
            if remaining == 0 {
                break;
            }
            let mut candidates: Vec<&String> =
                pool.tiers[source].iter().filter(|item| !used.contains(item)).collect();
            let take = remaining.min(candidates.len());
            // Partial Fisher-Yates: the first `take` entries are a uniform sample without replacement.
            for j in 0..take {
                let r = j + rng.next_below(candidates.len() - j);
                candidates.swap(j, r);
            }
            for item in &candidates[..take] {
                used.push((*item).clone());
                squares.push(Objective { item: (*item).clone(), tier: TIER_LABELS[tier] });
            }
            remaining -= take;
        }
        if remaining > 0 {
            panic!(
                "card generation: the pool has too few items for tier {}: {} square(s) cannot be filled",
                TIER_NAMES[tier], remaining
            );
        }
    }

    // Final Fisher-Yates shuffle over the whole card.
    for i in (1..squares.len()).rev() {
        let j = rng.next_below(i + 1);
        squares.swap(i, j);
    }
    Card { squares }
}

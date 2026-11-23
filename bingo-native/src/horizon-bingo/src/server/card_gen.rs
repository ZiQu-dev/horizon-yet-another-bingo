//! Card generation: draws a card from the card pool with a seeded SplitMix64 generator. The same seed, pool and
//! distribution always give the same card. The pool itself (tier lists, tags, groups) is built in [`super::pool`].
//! The draw is not the original's `kotlin.random.Random`, so the same seed gives other cards than the original does.
use super::types::{Card, Objective, FREE_SPACE};
use crate::card_tile::TierLabel;
use serde_json::Value;
use std::collections::HashSet;

/// Squares per tier, in the order S, A, B, C, D. Sums to 25.
pub const DEFAULT_DISTRIBUTION: [usize; 5] = [0, 0, 0, 9, 16];

const TIER_NAMES: [&str; 5] = ["S", "A", "B", "C", "D"];
const TIER_LABELS: [TierLabel; 5] = [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D];

/// The items per tier, in the order S, A, B, C, D. An item is listed at most once per tier. `weights[t][i]` is the
/// draw weight of `tiers[t][i]`; `groups` are sets of items that a card holds at most one of.
#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    pub tiers: [Vec<String>; 5],
    pub weights: [Vec<f64>; 5],
    pub groups: Vec<Vec<String>>,
    /// Tags the pool could not expand (for example vanilla item tags missing from the assets). Logged, not fatal.
    pub skipped_tags: Vec<String>,
    /// How many vanilla item tag files were read from the assets.
    pub tag_files: usize,
}

/// The pool built into the mod (see [`super::pool::builtin_pool`]).
pub fn load_pool() -> Pool {
    super::pool::builtin_pool()
}

/// Reads a plain tier list: the keys S, A, B, C, D each hold an array of item ids. Entries starting with `#` are
/// tags and are skipped here (no tag table); other keys are ignored. Duplicates inside a tier are dropped, keeping the
/// first. All weights are 1 and there are no groups.
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
    let weights = tiers.clone().map(|tier| vec![1.0; tier.len()]);
    Ok(Pool { tiers, weights, groups: Vec::new(), skipped_tags: Vec::new(), tag_files: 0 })
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

    /// A uniform value in `[0, 1)`, with 53 random bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
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

/// The tiers a draw for tier `tier` takes from, nearest first. Ties go to the lower index (the harder tier first), as
/// the original's stable sort by distance does.
fn walk_order(tier: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..5).collect();
    order.sort_by_key(|&index| (index.abs_diff(tier), index));
    order
}

/// Marks `item` and every member of the groups it is in as taken.
fn exclude_with_groups(pool: &Pool, excluded: &mut HashSet<String>, item: &str) {
    excluded.insert(item.to_string());
    for group in pool.groups.iter().filter(|group| group.iter().any(|member| member == item)) {
        excluded.extend(group.iter().cloned());
    }
}

/// Draws up to `count` items from tier `source` by weight, without replacement. Items already excluded are skipped; a
/// drawn item excludes its groups for the rest of the card.
fn draw(rng: &mut Rng, pool: &Pool, source: usize, excluded: &mut HashSet<String>, count: usize) -> Vec<String> {
    let items = &pool.tiers[source];
    let weights = &pool.weights[source];
    let mut open: Vec<usize> = (0..items.len()).filter(|&i| !excluded.contains(&items[i])).collect();
    let mut drawn = Vec::new();
    while drawn.len() < count && !open.is_empty() {
        let total: f64 = open.iter().map(|&i| weights[i]).sum();
        let mut point = rng.next_f64() * total;
        let mut chosen = open.len() - 1;
        for (position, &i) in open.iter().enumerate() {
            if point < weights[i] {
                chosen = position;
                break;
            }
            point -= weights[i];
        }
        let item = items[open.remove(chosen)].clone();
        exclude_with_groups(pool, excluded, &item);
        open.retain(|&i| !excluded.contains(&items[i]));
        drawn.push(item);
    }
    drawn
}

/// Draws new items for the squares `indices` of `card`, each square keeping its tier. No item is on the card twice, and
/// a drawn item is not drawn again. A tier that runs out takes from the nearest tiers, as [`generate_card`] does. A
/// square that no tier can fill keeps its item.
pub fn replace_squares(seed: u64, pool: &Pool, card: &mut Card, indices: &[usize]) {
    let mut rng = Rng::new(seed);
    let mut excluded = HashSet::new();
    for square in &card.squares {
        exclude_with_groups(pool, &mut excluded, &square.item);
    }
    for &index in indices {
        let Some(tier) = card.squares.get(index).map(|square| square.tier) else { continue };
        let tier_index = TIER_LABELS.iter().position(|label| *label == tier).unwrap_or(TIER_LABELS.len() - 1);
        for source in walk_order(tier_index) {
            if let Some(item) = draw(&mut rng, pool, source, &mut excluded, 1).pop() {
                card.squares[index] = Objective { item, tier };
                break;
            }
        }
    }
}

/// Builds a card: `distribution[i]` squares of tier `i`, no item twice, no two items of one group. A tier that runs out
/// of items takes them from the nearest tiers, nearest first (the square keeps its own tier). A pool that cannot fill
/// the card is topped up with free spaces ([`FREE_SPACE`]), as the original does.
pub fn generate_card(seed: u64, pool: &Pool, distribution: [usize; 5]) -> Card {
    let mut rng = Rng::new(seed);
    let mut excluded = HashSet::new();
    let mut squares: Vec<Objective> = Vec::with_capacity(distribution.iter().sum());

    for (tier, &wanted) in distribution.iter().enumerate() {
        let mut remaining = wanted;
        for source in walk_order(tier) {
            if remaining == 0 {
                break;
            }
            for item in draw(&mut rng, pool, source, &mut excluded, remaining) {
                squares.push(Objective { item, tier: TIER_LABELS[tier] });
                remaining -= 1;
            }
        }
        if remaining > 0 {
            // The original logs an error and pads the card with free spaces (CardService.kt:565-576).
            squares.extend((0..remaining).map(|_| Objective { item: FREE_SPACE.to_string(), tier: TIER_LABELS[tier] }));
        }
    }

    // Final Fisher-Yates shuffle over the whole card.
    for i in (1..squares.len()).rev() {
        let j = rng.next_below(i + 1);
        squares.swap(i, j);
    }
    Card { squares }
}

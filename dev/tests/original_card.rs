//! Card generation against the original's CardService.generateObjectives, replaceEntries and the tier walk in
//! tierlist/TierListConfig.kt. The original draws with kotlin.random.Random and this port with SplitMix64, so no test
//! compares exact cards. Each test checks a rule the original enforces, with expected values hand-traced from:
//!   - CardService.kt:437-576 (generateObjectives: addToExcluded, the tier loop, the free-space padding)
//!   - CardService.kt:259-316 (replaceEntries: tier kept, card items and their groups excluded)
//!   - tierlist/TierListConfig.kt:106-147 (pickFromTier, pick: nearest tier first, ties to the lower tier)
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::card_gen::{generate_card, pool_from_json, replace_squares, Pool, DEFAULT_DISTRIBUTION};
use horizon_bingo::server::types::Card;
use std::collections::HashSet;

fn items(prefix: &str, n: usize) -> String {
    (1..=n).map(|i| format!("\"{prefix}{i}\"")).collect::<Vec<_>>().join(",")
}

/// A pool with the given item counts per tier (S, A, B, C, D), ids such as "c3".
fn pool_with(counts: [usize; 5]) -> Pool {
    let names = ["s", "a", "b", "c", "d"];
    let body: Vec<String> = names
        .iter()
        .zip(counts)
        .map(|(name, n)| format!("\"{}\": [{}]", name.to_uppercase(), items(name, n)))
        .collect();
    pool_from_json(&format!("{{\"replace\": false, {}}}", body.join(","))).unwrap()
}

fn count_tier(card: &Card, tier: TierLabel) -> usize {
    card.squares.iter().filter(|square| square.tier == tier).count()
}

fn count_item(card: &Card, prefix: &str) -> usize {
    card.squares.iter().filter(|square| square.item.starts_with(prefix)).count()
}

fn assert_no_repeats(card: &Card) {
    let set: HashSet<&str> = card.squares.iter().map(|s| s.item.as_str()).collect();
    assert_eq!(set.len(), card.squares.len(), "an item appears twice on the card");
}

#[test]
fn card_has_the_default_distribution_of_tiers() {
    // Default distribution is easy-free: 9 C and 16 D squares (CardService.kt:537-550 fills each tier to its count).
    let pool = pool_with([12, 12, 12, 12, 20]);
    let card = generate_card(2024, &pool, DEFAULT_DISTRIBUTION);
    assert_eq!(card.squares.len(), 25);
    assert_eq!(count_tier(&card, TierLabel::C), 9);
    assert_eq!(count_tier(&card, TierLabel::D), 16);
    assert_eq!(count_tier(&card, TierLabel::S), 0);
}

#[test]
fn no_item_appears_twice_on_a_card() {
    // addToExcluded (CardService.kt:517-520) puts each chosen item in the excluded set before the next pick.
    let pool = pool_with([0, 0, 0, 12, 20]);
    for seed in 0..20u64 {
        let card = generate_card(seed, &pool, [0, 0, 0, 9, 16]);
        assert_no_repeats(&card);
    }
}

#[test]
fn two_members_of_one_item_group_never_share_a_card() {
    // addToExcluded adds every group containing a chosen item to the excluded set (CardService.kt:521-527).
    // C holds c1..c12 with the group [c1, c2]; 11 usable items remain, so 9 squares can always be filled.
    let mut pool = pool_with([0, 0, 0, 12, 0]);
    pool.groups = vec![vec!["c1".to_string(), "c2".to_string()]];
    for seed in 0..50u64 {
        let card = generate_card(seed, &pool, [0, 0, 0, 9, 0]);
        let has = |id: &str| card.squares.iter().any(|s| s.item == id);
        assert!(!(has("c1") && has("c2")), "seed {seed}: both members of the group are on the card");
    }
}

#[test]
fn a_tier_that_runs_short_takes_the_adjacent_tiers_nearest_first() {
    // C has 2 items, B has 3, D has 10; 9 squares of tier C are wanted.
    // pick(C) walks C (distance 0), then B and D (distance 1; the tie goes to the lower tier, B before D), then A, S
    // (CardService/TierListConfig.kt:140-147). So all 2 C items, all 3 B items, then 4 D items: 2 + 3 + 4 = 9.
    let pool = pool_with([0, 0, 3, 2, 10]);
    let card = generate_card(99, &pool, [0, 0, 0, 9, 0]);
    assert_eq!(card.squares.len(), 9);
    assert_eq!(count_tier(&card, TierLabel::C), 9, "the square keeps its own tier");
    assert_eq!(count_item(&card, "c"), 2);
    assert_eq!(count_item(&card, "b"), 3, "every B item is used before any D item");
    assert_eq!(count_item(&card, "d"), 4);
    assert_no_repeats(&card);
}

#[test]
fn a_pool_too_small_for_the_card_is_padded_not_a_panic() {
    // Only 2 C items for 9 squares: the original logs an error and pads to 9 with free spaces.
    let pool = pool_with([0, 0, 0, 2, 0]);
    let result = std::panic::catch_unwind(|| generate_card(5, &pool, [0, 0, 0, 9, 0]));
    let card = result.expect("generate_card should not panic when the pool is short");
    assert_eq!(card.squares.len(), 9);
}

#[test]
fn a_reroll_keeps_the_square_tier_and_avoids_every_item_on_the_card() {
    // replaceEntries keeps an entry's tier (CardService.kt:264-275 uses entry.tier) and excludes card.objectives.keys
    // (CardService.kt:284-287). The rerolled square must differ from all other squares.
    let pool = pool_with([0, 0, 0, 12, 20]);
    let card = generate_card(7, &pool, [0, 0, 0, 9, 16]);
    let before: Vec<String> = card.squares.iter().map(|s| s.item.clone()).collect();
    let mut rerolled = card.clone();
    replace_squares(8, &pool, &mut rerolled, &[0]);

    assert_eq!(rerolled.squares[0].tier, card.squares[0].tier);
    for other in before.iter().skip(1) {
        assert_ne!(&rerolled.squares[0].item, other, "the rerolled item is already on the card");
    }
    for (i, square) in rerolled.squares.iter().enumerate().skip(1) {
        assert_eq!(square.item, before[i], "untouched square {i} changed");
    }
    assert_no_repeats(&rerolled);
}

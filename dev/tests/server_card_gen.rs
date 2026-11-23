//! Card generation: the SplitMix64 generator against its reference outputs, the tier list reader, and the card
//! builder's rules (distribution, no repeats, fallback to easier tiers, determinism). Expected values are worked
//! out by hand or from the reference generator, not taken from the code under test.
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::card_gen::{generate_card, load_pool, pool_from_json, Pool, Rng, DEFAULT_DISTRIBUTION};
use horizon_bingo::server::types::{Card, CARD_SQUARES};

const TIERS: [TierLabel; 5] = [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D];

/// A tiny pool: three items per tier, named after their tier.
fn small_pool() -> Pool {
    pool_from_json(
        r#"{
            "replace": false,
            "values": [],
            "S": ["s1", "s2", "s3"],
            "A": ["a1", "a2", "a3"],
            "B": ["b1", "b2", "b3"],
            "C": ["c1", "c2", "c3"],
            "D": ["d1", "d2", "d3"]
        }"#,
    )
    .unwrap()
}

fn tier_of(pool: &Pool, item: &str) -> TierLabel {
    let index = pool.tiers.iter().position(|tier| tier.iter().any(|x| x == item)).expect("item is in the pool");
    TIERS[index]
}

fn count_tier(card: &Card, tier: TierLabel) -> usize {
    card.squares.iter().filter(|square| square.tier == tier).count()
}

fn assert_no_repeats(card: &Card) {
    let mut items: Vec<&str> = card.squares.iter().map(|s| s.item.as_str()).collect();
    items.sort_unstable();
    items.dedup();
    assert_eq!(items.len(), card.squares.len(), "an item appears twice on the card");
}

#[test]
fn splitmix64_matches_reference_outputs_for_seed_zero() {
    let mut rng = Rng::new(0);
    assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
}

#[test]
fn next_below_seed_zero_first_draw() {
    // 0xE220A8397B1DCDAF = 16294208416658607535, which ends in 5.
    assert_eq!(Rng::new(0).next_below(10), 5);
}

#[test]
fn next_below_stays_in_range_and_covers_every_value() {
    let mut rng = Rng::new(7);
    let mut seen = [false; 3];
    for _ in 0..3000 {
        let v = rng.next_below(3);
        assert!(v < 3);
        seen[v] = true;
    }
    assert_eq!(seen, [true, true, true]);
    for _ in 0..100 {
        assert_eq!(rng.next_below(1), 0);
    }
    for _ in 0..1000 {
        assert!(rng.next_below(25) < 25);
    }
}

#[test]
fn pool_skips_tags_and_ignores_other_keys() {
    let pool = pool_from_json(
        r##"{"replace": false, "values": ["x"], "groups": {},
            "S": ["#c:music_discs", "minecraft:a"], "A": ["minecraft:b"], "B": [],
            "C": ["#minecraft:tag_only"], "D": ["minecraft:c"]}"##,
    )
    .unwrap();
    assert_eq!(pool.tiers[0], vec!["minecraft:a"]);
    assert_eq!(pool.tiers[1], vec!["minecraft:b"]);
    assert!(pool.tiers[2].is_empty());
    assert!(pool.tiers[3].is_empty());
    assert_eq!(pool.tiers[4], vec!["minecraft:c"]);
}

#[test]
fn pool_drops_duplicates_within_a_tier_but_not_across_tiers() {
    let pool = pool_from_json(r#"{"S": ["x", "y", "x"], "A": ["x"], "B": [], "C": [], "D": []}"#).unwrap();
    assert_eq!(pool.tiers[0], vec!["x", "y"]);
    assert_eq!(pool.tiers[1], vec!["x"]);
}

#[test]
fn pool_rejects_malformed_json() {
    assert!(pool_from_json("{").is_err());
    assert!(pool_from_json("[]").is_err());
    assert!(pool_from_json(r#"{"S": [], "A": [], "B": [], "C": []}"#).is_err(), "tier D missing");
    assert!(pool_from_json(r#"{"S": "x", "A": [], "B": [], "C": [], "D": []}"#).is_err(), "tier is not an array");
    assert!(pool_from_json(r#"{"S": [1], "A": [], "B": [], "C": [], "D": []}"#).is_err(), "non-string entry");
}

#[test]
fn built_in_pool_has_five_tiers_of_items_without_tags() {
    let pool = load_pool();
    // Independent count: the original's four tier lists combined, tags tedious and unobtainable removed, plain item
    // ids only, and no vanilla tags (the music disc tags are missing without the assets): 47, 366, 200, 355, 302.
    let sizes: Vec<usize> = pool.tiers.iter().map(Vec::len).collect();
    assert_eq!(sizes, vec![47, 366, 200, 355, 302]);
    assert_eq!(pool.skipped_tags, vec!["#c:music_discs".to_string(), "#minecraft:music_discs".to_string()]);
    for tier in &pool.tiers {
        assert!(tier.iter().all(|item| !item.starts_with('#')));
    }
}

#[test]
fn card_has_distribution_sizes_and_labels_matching_the_pool() {
    let pool = small_pool();
    for seed in 0..50 {
        let card = generate_card(seed, &pool, [1, 1, 1, 1, 1]);
        assert_eq!(card.squares.len(), 5);
        assert_no_repeats(&card);
        for square in &card.squares {
            assert_eq!(tier_of(&pool, &square.item), square.tier, "seed {seed}");
        }
        for tier in TIERS {
            assert_eq!(count_tier(&card, tier), 1, "seed {seed}");
        }
    }
}

#[test]
fn card_counts_per_tier_follow_the_distribution() {
    let pool = small_pool();
    let card = generate_card(9, &pool, [2, 0, 1, 0, 2]);
    assert_eq!(card.squares.len(), 5);
    assert_eq!(count_tier(&card, TierLabel::S), 2);
    assert_eq!(count_tier(&card, TierLabel::A), 0);
    assert_eq!(count_tier(&card, TierLabel::B), 1);
    assert_eq!(count_tier(&card, TierLabel::C), 0);
    assert_eq!(count_tier(&card, TierLabel::D), 2);
}

#[test]
fn default_distribution_gives_a_full_card() {
    assert_eq!(DEFAULT_DISTRIBUTION.iter().sum::<usize>(), CARD_SQUARES);
    let pool = load_pool();
    let card = generate_card(1, &pool, DEFAULT_DISTRIBUTION);
    assert_eq!(card.squares.len(), CARD_SQUARES);
    assert_eq!(count_tier(&card, TierLabel::C), 9);
    assert_eq!(count_tier(&card, TierLabel::D), 16);
    assert_no_repeats(&card);
    for square in &card.squares {
        assert_eq!(tier_of(&pool, &square.item), square.tier);
    }
}

#[test]
fn same_seed_gives_the_same_card() {
    let pool = load_pool();
    assert_eq!(generate_card(42, &pool, DEFAULT_DISTRIBUTION), generate_card(42, &pool, DEFAULT_DISTRIBUTION));
}

#[test]
fn different_seeds_give_different_cards() {
    let pool = load_pool();
    let a = generate_card(1, &pool, DEFAULT_DISTRIBUTION);
    let b = generate_card(2, &pool, DEFAULT_DISTRIBUTION);
    assert_ne!(a, b);
}

#[test]
fn no_item_repeats_on_a_card_over_200_seeds() {
    let pool = load_pool();
    for seed in 0..200 {
        let card = generate_card(seed, &pool, DEFAULT_DISTRIBUTION);
        assert_eq!(card.squares.len(), CARD_SQUARES, "seed {seed}");
        assert_no_repeats(&card);
    }
}

#[test]
fn squares_are_shuffled_not_grouped_by_tier() {
    let pool = load_pool();
    // Tier D has 16 of 25 squares, so across 20 seeds some card must start with a C square.
    let starts_with_c = (0..20).any(|seed| {
        generate_card(seed, &pool, DEFAULT_DISTRIBUTION).squares[0].tier == TierLabel::C
    });
    assert!(starts_with_c);
}

#[test]
fn a_tier_that_runs_out_takes_items_from_the_next_easier_tier() {
    // S has one item, so the second S square comes from A. Both keep the S label.
    let pool = pool_from_json(r#"{"S": ["s1"], "A": ["a1", "a2"], "B": [], "C": [], "D": []}"#).unwrap();
    let card = generate_card(3, &pool, [2, 0, 0, 0, 0]);
    assert_eq!(card.squares.len(), 2);
    assert!(card.squares.iter().all(|s| s.tier == TierLabel::S));
    assert!(card.squares.iter().any(|s| s.item == "s1"));
    assert!(card.squares.iter().any(|s| s.item == "a1" || s.item == "a2"));
    assert_no_repeats(&card);
}

#[test]
fn pads_with_a_free_space_when_no_tier_can_fill_the_card() {
    let pool = pool_from_json(r#"{"S": ["s1"], "A": [], "B": [], "C": [], "D": []}"#).unwrap();
    let card = generate_card(0, &pool, [2, 0, 0, 0, 0]);
    assert_eq!(card.squares.len(), 2);
    assert_eq!(card.squares.iter().filter(|s| s.item == "bingo:free_space").count(), 1);
}

#[test]
fn a_tier_that_runs_out_takes_from_the_nearest_tier_even_a_harder_one() {
    // D is empty: its square takes from C, B, A, then S (distance order), so it keeps the D label with an S item.
    let pool = pool_from_json(r#"{"S": ["s1", "s2", "s3"], "A": [], "B": [], "C": [], "D": []}"#).unwrap();
    let card = generate_card(0, &pool, [0, 0, 0, 0, 1]);
    assert_eq!(card.squares.len(), 1);
    assert_eq!(card.squares[0].tier, TierLabel::D);
    assert!(["s1", "s2", "s3"].contains(&card.squares[0].item.as_str()));
}

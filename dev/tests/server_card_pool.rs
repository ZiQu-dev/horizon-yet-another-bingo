//! The card pool as the original builds it: tier lists combined, tags expanded (vanilla from the assets, the mod's own),
//! item groups as weights and exclusions, the default filter (tedious and unobtainable out), plain item ids only, and
//! the walk to the nearest tier. Expected values are hand-computed or come from an independent reading of the
//! original's JSON (see NOTICE.md), not from the code under test.
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::card_gen::{generate_card, load_pool, pool_from_json, replace_squares, DEFAULT_DISTRIBUTION};
use horizon_bingo::server::pool::{build_pool, parse_tag, parse_tier_list, pool_from_assets, TagTable, TierList, DEFAULT_EXCLUDED_TAGS};
use horizon_bingo::server::types::{Card, Objective};
use std::fs;
use std::path::PathBuf;

fn list(json: &str) -> TierList {
    parse_tier_list(json).unwrap()
}

fn items(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bingo-pool-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn names(card: &Card) -> Vec<&str> {
    card.squares.iter().map(|s| s.item.as_str()).collect()
}

// ---- parsing ----

#[test]
fn tier_list_reads_optional_keys_and_ignores_a_groups_value_that_is_not_an_array() {
    let l = list(r#"{"S": ["minecraft:a"], "A": [], "B": [], "C": [], "D": [], "groups": {}}"#);
    assert!(!l.replace);
    assert!(l.values.is_empty());
    assert!(l.groups.is_empty());
    assert_eq!(l.tiers[0], items(&["minecraft:a"]));

    let l = list(r#"{"replace": true, "values": ["minecraft:v"], "S": [], "A": [], "B": [], "C": [], "D": [],
        "groups": [["minecraft:x", "minecraft:y"]]}"#);
    assert!(l.replace);
    assert_eq!(l.values, items(&["minecraft:v"]));
    assert_eq!(l.groups, vec![items(&["minecraft:x", "minecraft:y"])]);
}

#[test]
fn tag_values_accept_plain_ids_and_required_objects() {
    let values = parse_tag(r##"{"values": ["minecraft:a", {"id": "minecraft:b", "required": false}, "#minecraft:c"]}"##).unwrap();
    assert_eq!(values, items(&["minecraft:a", "minecraft:b", "#minecraft:c"]));
    assert!(parse_tag("[]").is_err());
}

// ---- combining, tags, filter ----

#[test]
fn lists_combine_by_union_per_tier_and_an_item_may_sit_in_two_tiers() {
    let a = list(r#"{"S": ["minecraft:a"], "A": [], "B": [], "C": [], "D": []}"#);
    let b = list(r#"{"S": ["minecraft:a", "minecraft:b"], "A": [], "B": [], "C": [], "D": ["minecraft:a"]}"#);
    let pool = build_pool(&[a, b], &TagTable::default(), &[]);
    assert_eq!(pool.tiers[0], items(&["minecraft:a", "minecraft:b"]));
    assert_eq!(pool.tiers[4], items(&["minecraft:a"]));
}

#[test]
fn a_replacing_list_takes_the_place_of_the_lists_before_it() {
    let a = list(r#"{"S": ["minecraft:a"], "A": [], "B": [], "C": [], "D": []}"#);
    let b = list(r#"{"replace": true, "S": ["minecraft:b"], "A": [], "B": [], "C": [], "D": []}"#);
    let pool = build_pool(&[a, b], &TagTable::default(), &[]);
    assert_eq!(pool.tiers[0], items(&["minecraft:b"]));
}

#[test]
fn vanilla_tags_expand_recursively_and_unknown_tags_are_reported() {
    let mut tags = TagTable::default();
    tags.add_vanilla("minecraft:discs", items(&["minecraft:d1", "#minecraft:nested"]));
    tags.add_vanilla("minecraft:nested", items(&["minecraft:d2"]));
    let l = list(r##"{"S": ["#minecraft:discs", "#minecraft:nope", "minecraft:a"], "A": [], "B": [], "C": [], "D": []}"##);
    let pool = build_pool(&[l], &tags, &[]);
    assert_eq!(pool.tiers[0], items(&["minecraft:d1", "minecraft:d2", "minecraft:a"]));
    assert_eq!(pool.skipped_tags, items(&["#minecraft:nope"]));
}

#[test]
fn excluded_tags_expand_through_the_mods_own_tags_and_leave_their_items_out() {
    let mut tags = TagTable::default();
    tags.add_own("tedious", items(&["#yet-another-minecraft-bingo:needs_silk_touch", "minecraft:beacon"]));
    tags.add_own("needs_silk_touch", items(&["minecraft:ice"]));
    tags.add_own("unobtainable", items(&["minecraft:air"]));
    let l = list(r#"{"S": ["minecraft:beacon", "minecraft:ice", "minecraft:air", "minecraft:apple", "minecraft:stone"],
        "A": [], "B": [], "C": [], "D": []}"#);
    let pool = build_pool(&[l], &tags, &DEFAULT_EXCLUDED_TAGS);
    assert_eq!(pool.tiers[0], items(&["minecraft:apple", "minecraft:stone"]));
}

#[test]
fn only_plain_item_ids_stay_in_the_pool() {
    let l = list(r#"{"S": ["minecraft:apple", "minecraft:adventure/kill_a_mob", "yet-another-minecraft-bingo:challenge/all_dyes"],
        "A": [], "B": [], "C": [], "D": ["minecraft:adventure/sleep_in_bed"], "values": ["minecraft:x/y"]}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    assert_eq!(pool.tiers[0], items(&["minecraft:apple"]));
    assert!(pool.tiers[4].is_empty());
}

#[test]
fn uncategorized_values_join_every_tier_at_a_fifth_of_their_weight() {
    let l = list(r#"{"values": ["minecraft:v", "minecraft:a"], "S": ["minecraft:a"], "A": [], "B": [], "C": [], "D": []}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    // minecraft:a is categorized (in S), so it is not an uncategorized value; minecraft:v is in every tier.
    assert_eq!(pool.tiers[0], items(&["minecraft:a", "minecraft:v"]));
    assert_eq!(pool.weights[0][0], 1.0);
    assert!((pool.weights[0][1] - 0.2).abs() < 1e-12);
    for tier in 1..5 {
        assert_eq!(pool.tiers[tier], items(&["minecraft:v"]));
        assert!((pool.weights[tier][0] - 0.2).abs() < 1e-12);
    }
}

// ---- groups as weights ----

#[test]
fn an_item_weighs_one_over_the_sizes_of_its_groups() {
    // x is in a group of 2 and a group of 3: 1 / (2 + 3). y is in 2 (1 / 2). z and w are in 3 (1 / 3). u is in none.
    let l = list(r#"{"S": ["minecraft:x", "minecraft:y", "minecraft:z", "minecraft:u"], "A": [], "B": [], "C": [], "D": [],
        "groups": [["minecraft:x", "minecraft:y"], ["minecraft:x", "minecraft:z", "minecraft:w"]]}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    let weight = |id: &str| pool.weights[0][pool.tiers[0].iter().position(|x| x == id).unwrap()];
    assert!((weight("minecraft:x") - 0.2).abs() < 1e-12);
    assert!((weight("minecraft:y") - 0.5).abs() < 1e-12);
    assert!((weight("minecraft:z") - 1.0 / 3.0).abs() < 1e-12);
    assert_eq!(weight("minecraft:u"), 1.0);
    assert_eq!(pool.groups.len(), 2);
}

#[test]
fn a_group_member_outside_the_tier_still_counts_in_the_size() {
    // p's group is {p, q} and q is not in S: p still weighs 1/2, against 1 for r.
    let l = list(r#"{"S": ["minecraft:p", "minecraft:r"], "A": [], "B": [], "C": [], "D": [],
        "groups": [["minecraft:p", "minecraft:q"]]}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    assert!((pool.weights[0][0] - 0.5).abs() < 1e-12);
    assert_eq!(pool.weights[0][1], 1.0);
}

#[test]
fn a_drawn_item_takes_its_whole_group_out_of_the_card() {
    let l = list(r#"{"S": ["minecraft:a", "minecraft:b", "minecraft:c"], "A": [], "B": [], "C": [], "D": [],
        "groups": [["minecraft:a", "minecraft:b"]]}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    for seed in 0..50 {
        let card = generate_card(seed, &pool, [2, 0, 0, 0, 0]);
        let found = names(&card);
        assert!(found.contains(&"minecraft:c"), "seed {seed}");
        let pair = found.iter().filter(|x| **x == "minecraft:a" || **x == "minecraft:b").count();
        assert_eq!(pair, 1, "seed {seed}: a group holds one item on a card");
    }
}

#[test]
fn draw_frequency_follows_the_weights() {
    // S: p (weight 1/2, its group is {p, q}, q not in the tier) and r (weight 1). One square: P(p) = 0.5 / 1.5 = 1/3.
    let l = list(r#"{"S": ["minecraft:p", "minecraft:r"], "A": [], "B": [], "C": [], "D": [],
        "groups": [["minecraft:p", "minecraft:q"]]}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    let trials = 3000;
    let p_count = (0..trials).filter(|&seed| names(&generate_card(seed, &pool, [1, 0, 0, 0, 0]))[0] == "minecraft:p").count();
    // Expected 1000, standard deviation about 26: the window is about 3 sigma wide.
    assert!((920..=1080).contains(&p_count), "p drawn {p_count} of {trials}");
}

#[test]
fn replacing_a_square_respects_the_groups_of_the_items_on_the_card() {
    let l = list(r#"{"S": ["minecraft:a", "minecraft:b", "minecraft:c", "minecraft:d"], "A": [], "B": [], "C": [], "D": [],
        "groups": [["minecraft:a", "minecraft:b"]]}"#);
    let pool = build_pool(&[l], &TagTable::default(), &[]);
    let mut card = Card {
        squares: vec![
            Objective { item: "minecraft:a".into(), tier: TierLabel::S },
            Objective { item: "minecraft:c".into(), tier: TierLabel::S },
        ],
    };
    // a is on the card, so b (its group) is out, c is on the card: only d is left.
    replace_squares(5, &pool, &mut card, &[1]);
    assert_eq!(names(&card), vec!["minecraft:a", "minecraft:d"]);
}

// ---- generation rules ----

#[test]
fn a_tier_that_runs_out_takes_from_the_nearest_tier_and_ties_go_to_the_harder_one() {
    // C is empty. B and D are both one step away: B (the harder) wins the tie, so the square is b1.
    let pool = pool_from_json(r#"{"S": ["s1"], "A": ["a1"], "B": ["b1"], "C": [], "D": ["d1"]}"#).unwrap();
    for seed in 0..20 {
        let card = generate_card(seed, &pool, [0, 0, 0, 1, 0]);
        assert_eq!(names(&card), vec!["b1"], "seed {seed}");
        assert_eq!(card.squares[0].tier, TierLabel::C);
    }
}

#[test]
fn the_nearer_tier_wins_over_a_farther_one() {
    // C is empty. D is one step away, A is two: the square is d1.
    let pool = pool_from_json(r#"{"S": ["s1"], "A": ["a1"], "B": [], "C": [], "D": ["d1"]}"#).unwrap();
    let card = generate_card(3, &pool, [0, 0, 0, 1, 0]);
    assert_eq!(names(&card), vec!["d1"]);
}

#[test]
fn an_item_is_never_on_the_card_twice_even_across_tiers() {
    let pool = pool_from_json(r#"{"S": ["x"], "A": [], "B": [], "C": [], "D": ["x", "y"]}"#).unwrap();
    for seed in 0..20 {
        let card = generate_card(seed, &pool, [1, 0, 0, 0, 1]);
        let mut found = names(&card);
        found.sort_unstable();
        assert_eq!(found, vec!["x", "y"], "seed {seed}");
    }
}

// ---- files and the built-in pool ----

#[test]
fn the_built_in_pool_leaves_out_tedious_and_unobtainable_items() {
    let pool = load_pool();
    for tier in &pool.tiers {
        for banned in ["minecraft:beacon", "minecraft:creeper_head", "minecraft:air", "minecraft:black_shulker_box"] {
            assert!(!tier.iter().any(|x| x == banned), "{banned} is tedious or unobtainable");
        }
    }
    assert!(pool.tiers[1].iter().any(|x| x == "minecraft:diamond"), "diamond is in tier A of the original");
    assert!(pool.tiers[4].iter().any(|x| x == "minecraft:apple"), "apple is in tier D of the original");
}

#[test]
fn the_built_in_pool_never_puts_a_banned_item_on_a_card() {
    let pool = load_pool();
    for seed in 0..200 {
        let card = generate_card(seed, &pool, DEFAULT_DISTRIBUTION);
        assert!(!names(&card).contains(&"minecraft:beacon"), "seed {seed}");
    }
}

#[test]
fn the_built_in_pool_lists_the_missing_vanilla_tags_it_could_not_expand() {
    let pool = load_pool();
    assert_eq!(pool.tag_files, 0);
    assert_eq!(pool.skipped_tags, vec!["#c:music_discs".to_string(), "#minecraft:music_discs".to_string()]);
}

#[test]
fn vanilla_item_tags_from_the_assets_folder_fill_the_tags() {
    let dir = scratch("assets-tags");
    fs::create_dir_all(dir.join("tags/item/minecraft")).unwrap();
    fs::create_dir_all(dir.join("tags/item/c")).unwrap();
    fs::write(dir.join("tags/item/minecraft/music_discs.json"), r#"{"values": ["minecraft:music_disc_13"]}"#).unwrap();
    fs::write(dir.join("tags/item/c/music_discs.json"), r#"{"values": ["minecraft:music_disc_cat"]}"#).unwrap();
    fs::write(dir.join("tags/item/c/notes.txt"), "not a tag").unwrap();

    let pool = pool_from_assets(&dir);
    assert_eq!(pool.tag_files, 2);
    assert!(pool.skipped_tags.is_empty());
    assert!(pool.tiers[0].iter().any(|x| x == "minecraft:music_disc_13"));
    assert!(pool.tiers[0].iter().any(|x| x == "minecraft:music_disc_cat"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_assets_folder_gives_the_built_in_pool_with_the_tags_left_out() {
    let dir = scratch("no-tags");
    let pool = pool_from_assets(&dir);
    assert_eq!(pool.tag_files, 0);
    assert_eq!(pool.tiers, load_pool().tiers);
    assert_eq!(pool.skipped_tags.len(), 2);
    let _ = fs::remove_dir_all(&dir);
}

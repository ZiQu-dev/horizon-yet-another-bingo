//! The shuffle animation against the original (utils/Interpolate.kt and ClientCardBufferRenderer.kt). Expected values are
//! worked out by hand from the Kotlin: SHUFFLE_DURATION 280 ms, the tile grid 12 + 20x by 22 + 20y, InOut easing
//! -(cos(PI p) - 1) / 2, ExpOut 1 - 2^(-10 p), and the window now < shuffledAt + 280 + 50.
use horizon_bingo::shuffle_draw::*;

/// Float comparison for the eased values, which the Kotlin also computes in floating point.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn assert_near(actual: (f32, f32), expected: (f32, f32)) {
    assert!(
        near(actual.0, expected.0) && near(actual.1, expected.1),
        "got {actual:?}, expected {expected:?}"
    );
}

/// Slot 0 takes the place of slot 24 and slot 6 that of slot 0; the rest stay where they are.
fn swap_corners() -> Vec<usize> {
    let mut p: Vec<usize> = (0..25).collect();
    p[0] = 24;
    p[6] = 0;
    p
}

#[test]
fn the_easings_match_the_kotlin_formulas_at_known_points() {
    // InOut at 0.25: (1 - cos(PI/4)) / 2 = 0.1464466
    let i = Interpolate::new(0.0, 1.0, 1000, 0, Easing::InOut);
    assert!(near(i.progress(Easing::InOut, 250), 0.146_446_6));
    assert!(near(i.progress(Easing::InOut, 500), 0.5), "InOut is symmetric about the middle");
    assert!(near(i.progress(Easing::InOutReverse, 250), 0.853_553_4), "the reverse is 1 - InOut");
    // ExpOut at 0.5: 1 - 2^-5 = 0.96875; at 1: 1 - 2^-10 = 0.9990234375
    let e = Interpolate::new(0.0, 1.0, 1000, 0, Easing::ExpOut);
    assert!(near(e.progress(Easing::ExpOut, 500), 0.968_75));
    assert!(near(e.progress(Easing::ExpOut, 1000), 0.999_023_4));
    assert!(near(e.progress(Easing::ExpOutReverse, 500), 0.031_25));
    assert_eq!(e.progress(Easing::Linear, 250), 0.25);
}

#[test]
fn the_progress_is_clamped_before_the_start_and_after_the_end() {
    let i = Interpolate::new(10.0, 30.0, 280, 1000, Easing::Linear);
    assert_eq!(i.get(Easing::Linear, 900), 10.0, "before the start it is still at `from`");
    assert_eq!(i.get(Easing::Linear, 1070), 15.0, "a quarter of the way: 10 + 20 x 0.25");
    assert_eq!(i.get(Easing::Linear, 5000), 30.0, "after the end it is at `to`");
}

#[test]
fn an_interpolation_is_done_only_strictly_after_its_end() {
    let i = Interpolate::new(0.0, 1.0, 280, 1000, Easing::Linear);
    assert!(!i.is_done(1280), "isDone is `now > startedAt + duration`");
    assert!(i.is_done(1281));
}

#[test]
fn a_slot_is_a_tile_on_the_grid_at_12_plus_20x_and_22_plus_20y() {
    assert_eq!(slot_origin(0), (12.0, 22.0));
    assert_eq!(slot_origin(6), (32.0, 42.0), "column 1, row 1");
    assert_eq!(slot_origin(4), (92.0, 22.0), "last column, first row");
    assert_eq!(slot_origin(24), (92.0, 102.0), "last column, last row: 22 + 4 x 20");
}

#[test]
fn a_card_never_shuffled_is_not_animated() {
    // shuffledAt is Instant.MIN in the original: no animation, the tile is at home whatever the positions say
    assert!(!is_animating(None, 0));
    assert_eq!(tile_origin(6, None, 0, &swap_corners()), (32.0, 42.0));
}

#[test]
fn the_animation_starts_where_the_tile_was_and_ends_at_home() {
    let p = swap_corners();
    // at the shuffle instant the tile is at its old slot: slot 0 (12, 22) for tile 6
    assert_eq!(tile_origin(6, Some(1000), 1000, &p), (12.0, 22.0));
    // 280 ms later the progress is 1 and the tile is home; from then on the window still holds until 1330
    assert_eq!(tile_origin(6, Some(1000), 1280, &p), (32.0, 42.0));
    assert!(is_animating(Some(1000), 1329));
    assert!(!is_animating(Some(1000), 1330), "280 + 50 ms after the shuffle the animation is over");
}

#[test]
fn halfway_through_the_tile_is_halfway_between_its_old_and_new_corner() {
    let p = swap_corners();
    // tile 6 (home 32, 42) from slot 0 (12, 22): midway at 1140 is (22, 32)
    assert_near(tile_origin(6, Some(1000), 1140, &p), (22.0, 32.0));
    // tile 0 (home 12, 22) from slot 24 (92, 102): midway is (52, 62)
    assert_near(tile_origin(0, Some(1000), 1140, &p), (52.0, 62.0));
}

#[test]
fn the_movement_eases_in_and_out_not_linearly() {
    let p = swap_corners();
    // tile 0 at a quarter of the time: InOut(0.25) = 0.1464466, so x = 92 - 80 x 0.1464466 = 80.284 (linear would be 72),
    // y = 102 - 80 x 0.1464466 = 90.284
    assert_near(tile_origin(0, Some(1000), 1070, &p), (80.284, 90.284));
}

#[test]
fn a_tile_index_past_the_permutation_wraps_with_the_modulo() {
    // positions of length 5: tile 7 uses positions[7 % 5] = positions[2] = slot 2 at (52, 22); home is (52, 42)
    let five = [4usize, 3, 2, 1, 0];
    assert_near(tile_origin(7, Some(1000), 1140, &five), (52.0, 32.0));
}

#[test]
fn with_no_permutation_nothing_moves() {
    // the original would divide by zero; here the tile starts at its own slot, so it is at home at every time
    assert_eq!(tile_origin(6, Some(1000), 1140, &[]), (32.0, 42.0));
}

#[test]
fn a_new_order_is_the_current_list_read_in_the_order_given() {
    // shufflePositions.shuffled(): with the order [2, 0, 3, 1], the new list is current[2], current[0], ...
    assert_eq!(shuffle_positions_after(&[10, 11, 12, 13], &[2, 0, 3, 1]), Some(vec![12, 10, 13, 11]));
    // applied twice the orders compose: [1, 0, 2, 3] over [12, 10, 13, 11] gives [10, 12, 13, 11]
    let once = shuffle_positions_after(&[10, 11, 12, 13], &[2, 0, 3, 1]).unwrap();
    assert_eq!(shuffle_positions_after(&once, &[1, 0, 2, 3]), Some(vec![10, 12, 13, 11]));
}

#[test]
fn an_order_that_is_not_a_permutation_is_refused() {
    assert_eq!(shuffle_positions_after(&[0, 1, 2, 3], &[0, 0, 1, 2]), None, "a repeated index");
    assert_eq!(shuffle_positions_after(&[0, 1, 2, 3], &[0, 1, 2]), None, "too short");
    assert_eq!(shuffle_positions_after(&[0, 1, 2, 3], &[0, 1, 2, 4]), None, "out of range");
}

#[test]
fn a_tile_moves_from_its_old_corner_to_its_home_over_the_280_ms_of_the_shuffle() {
    // Tile 0 (home (12, 22)) starts in slot 24 (92, 102). With p = (now - 1000) / 280 and the InOut value e(p) at
    // p = 0, 0.25, 0.5, 1: 0 -> (92, 102); e = 0.1464466 -> x = 92 - 80 e, y = 102 - 80 e; 0.5 -> (52, 62); 1 -> home.
    let p = swap_corners();
    assert_near(tile_origin(0, Some(1000), 1000, &p), (92.0, 102.0));
    assert_near(tile_origin(0, Some(1000), 1070, &p), (80.284_27, 90.284_27));
    assert_near(tile_origin(0, Some(1000), 1140, &p), (52.0, 62.0));
    assert_near(tile_origin(0, Some(1000), 1280, &p), (12.0, 22.0));
    // Tile 6 (home (32, 42)) starts in slot 0 (12, 22): once the window (280 + 50 ms) is over it is at home.
    assert_eq!(tile_origin(6, Some(1000), 1330, &p), (32.0, 42.0));
    assert_eq!(tile_origin(6, Some(1000), 9_000, &p), (32.0, 42.0));
}

#[test]
fn the_shuffle_rng_gives_the_splitmix64_values_of_seed_zero() {
    // SplitMix64 from seed 0: the reference sequence (Vigna's C code), computed outside this crate.
    let mut rng = ShuffleRng::new(0);
    assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
    assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
}

#[test]
fn a_seed_gives_a_known_fisher_yates_order() {
    // permutation(25) from seed 42, Fisher-Yates from index 24 down with rejection sampling, computed outside this crate.
    let expected = [20, 3, 21, 1, 15, 18, 4, 8, 5, 0, 10, 22, 11, 16, 23, 17, 6, 14, 9, 2, 24, 12, 7, 19, 13];
    assert_eq!(ShuffleRng::new(42).permutation(25), expected);
}

#[test]
fn every_permutation_holds_each_index_once() {
    for seed in 0..200u64 {
        for n in [0usize, 1, 2, 5, 25] {
            let mut sorted = ShuffleRng::new(seed).permutation(n);
            sorted.sort_unstable();
            assert_eq!(sorted, (0..n).collect::<Vec<_>>(), "seed {seed}, n {n}");
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_orders_and_another_seed_another_order() {
    let mut a = ShuffleRng::new(7);
    let mut b = ShuffleRng::new(7);
    let first_a = a.permutation(25);
    assert_eq!(first_a, b.permutation(25));
    assert_eq!(a.permutation(25), b.permutation(25), "the second order too");
    assert_ne!(ShuffleRng::new(43).permutation(25), ShuffleRng::new(42).permutation(25));
}

#[test]
fn shuffled_reorders_the_current_positions_by_a_permutation_drawn_from_the_rng() {
    let current: Vec<usize> = (0..25).map(|i| (i * 7) % 25).collect();
    let mut rng = ShuffleRng::new(42);
    let got = shuffled(&current, &mut rng);
    // the same as applying the order the rng gives to the current list
    let order = ShuffleRng::new(42).permutation(25);
    assert_eq!(Some(got.clone()), shuffle_positions_after(&current, &order));
    let mut sorted = got;
    sorted.sort_unstable();
    assert_eq!(sorted, (0..25).collect::<Vec<_>>(), "the elements are the same, only their order changed");
}

//! The end-screen firework. Every expected number is worked out by hand in the comment beside it, from the Kotlin
//! `BingoEndFireworkRenderer`, not read back from the code under test.
use horizon_bingo::fireworks::{Fireworks, Star};
use horizon_bingo::ui_draw::UiDraw;
use std::f64::consts::PI;

/// The six colours from the Kotlin, as 0xRRGGBB (its 0xFF_xxxxxx with the alpha byte dropped).
const KOTLIN_COLOURS: [u32; 6] = [0xF5C0A3, 0xA3F5A3, 0xA3F5C7, 0xA3ABF5, 0xCCA3F5, 0xF5A3D6];
const CX: i32 = 320;
const CY: i32 = 180;
const STARTED: u64 = 10_000;

#[derive(Debug)]
struct Glyph {
    text: String,
    anchor: (f32, f32),
    offset: (f32, f32),
    scale: f32,
    rotation: f32,
    argb: u32,
    shadow: bool,
}

fn draw(fw: &mut Fireworks, now: u64) -> Vec<Glyph> {
    fw.draw(now, CX, CY)
        .into_iter()
        .map(|d| match d {
            UiDraw::TextXf { text, anchor, offset, scale, rotation, argb, shadow } => {
                Glyph { text, anchor, offset, scale, rotation, argb, shadow }
            }
            other => panic!("expected a TextXf, got {other:?}"),
        })
        .collect()
}

fn alpha_byte(g: &Glyph) -> u32 {
    g.argb >> 24
}

fn colour(g: &Glyph) -> u32 {
    g.argb & 0x00FF_FFFF
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!((actual - expected).abs() < 1e-3, "{what}: expected {expected}, got {actual}");
}

#[test]
fn starts_with_fifty_stars_still_alive() {
    let fw = Fireworks::new(STARTED, 1);
    assert_eq!(fw.star_count(), 50);
    assert_eq!(fw.stars().len(), 50);
    assert!(!fw.is_done());
}

#[test]
fn at_start_every_star_is_drawn_centred_and_opaque() {
    let mut fw = Fireworks::new(STARTED, 1);
    let stars: Vec<Star> = fw.stars().to_vec();
    let glyphs = draw(&mut fw, STARTED);
    assert_eq!(glyphs.len(), 50);
    for (g, star) in glyphs.iter().zip(&stars) {
        // Zero ticks: no travel, so the glyph sits on the centre with alpha 1 - 0 = 1, which is byte 255.
        assert_eq!(g.anchor, (320.0, 180.0));
        assert_eq!(g.offset, (-4.0, -4.0));
        assert_eq!(g.scale, 1.0);
        assert_eq!(g.text, "★");
        assert!(!g.shadow);
        assert_eq!(alpha_byte(g), 255);
        assert_eq!(colour(g), star.rgb);
        assert!((g.rotation as f64 - star.rot).abs() < 1e-5);
    }
}

#[test]
fn stars_have_the_kotlin_speeds_spins_and_colours() {
    // Hand bounds: vel = 1 + random is in [1, 2), so the speed |(x_vel, y_vel + 0.2)| = 1.5 * vel is in [1.5, 3.0).
    // rot is random * 2PI in [0, 2PI). rot_vel = (random - 0.5) * 0.5 * 1.5 is in [-0.375, 0.375).
    for seed in [0u64, 1, 42, u64::MAX] {
        let fw = Fireworks::new(STARTED, seed);
        for s in fw.stars() {
            let speed = s.x_vel.hypot(s.y_vel + 0.2);
            assert!((1.5 - 1e-9..3.0 + 1e-9).contains(&speed), "seed {seed}: speed {speed}");
            assert!((0.0..2.0 * PI).contains(&s.rot), "seed {seed}: rot {}", s.rot);
            assert!((-0.375..0.375).contains(&s.rot_vel), "seed {seed}: rot_vel {}", s.rot_vel);
            assert!(KOTLIN_COLOURS.contains(&s.rgb), "seed {seed}: colour {:06X}", s.rgb);
        }
        // A zero seed must not freeze the generator: the stars still differ.
        let distinct: std::collections::BTreeSet<u64> = fw.stars().iter().map(|s| s.x_vel.to_bits()).collect();
        assert!(distinct.len() > 1, "seed {seed}: all stars identical");
    }
}

#[test]
fn later_positions_follow_velocity_and_spin() {
    let mut fw = Fireworks::new(STARTED, 5);
    let stars: Vec<Star> = fw.stars().to_vec();
    // 1000 ms is 20 ticks of 50 ms.
    let glyphs = draw(&mut fw, STARTED + 1000);
    assert_eq!(glyphs.len(), 50, "no star has left the range after 20 ticks");
    for (g, s) in glyphs.iter().zip(&stars) {
        assert_close(g.anchor.0 as f64, 320.0 + 20.0 * s.x_vel, "anchor x");
        assert_close(g.anchor.1 as f64, 180.0 + 20.0 * s.y_vel, "anchor y");
        assert_close(g.rotation as f64, s.rot + 20.0 * s.rot_vel, "rotation");
        // Speed at most 3.0 per tick, so 20 ticks is at most 60 px from the centre:
        // (60 / 500)^3 = 0.001728, alpha >= 0.998272, times 255 = 254.56, so byte 254 or 255.
        assert!(matches!(alpha_byte(g), 254 | 255), "alpha byte {}", alpha_byte(g));
    }
}

#[test]
fn star_fades_out_at_threshold_and_never_comes_back() {
    let mut fw = Fireworks::new(STARTED, 7);
    let target = fw.stars()[0];
    let speed = target.x_vel.hypot(target.y_vel);

    // The star is at distance speed * ticks. Hand values: alpha = 1 - (d / 500)^3 and removal at alpha <= 0.025,
    // so the cut-off distance is 500 * 0.975^(1/3), about 495.8 px.
    //   d = 495.0: (0.99)^3 = 0.970299, alpha = 0.029701 > 0.025, still drawn, byte trunc(7.57) = 7.
    //   d = 496.5: (0.993)^3 = 0.979147, alpha = 0.020853 <= 0.025, removed.
    let ms_in = (495.0 * 50.0 / speed).floor() as u64;
    let ms_out = (496.5 * 50.0 / speed).ceil() as u64;

    let inside = draw(&mut fw, STARTED + ms_in);
    assert!(fw.stars().contains(&target), "still alive at 495 px");
    assert_eq!(inside.len(), fw.star_count());
    // The target is the first star, so it is the first glyph; at ms_in its alpha byte is 7 (see above).
    assert_eq!(alpha_byte(&inside[0]), 7);
    assert_eq!(colour(&inside[0]), target.rgb);

    let before = fw.star_count();
    draw(&mut fw, STARTED + ms_out);
    assert!(!fw.stars().contains(&target), "removed past 496.5 px");
    assert!(fw.star_count() < before);

    // Time goes back to just before the fade: the removed star is not drawn again.
    let rewound = draw(&mut fw, STARTED + ms_in);
    assert_eq!(rewound.len(), fw.star_count());
    assert!(!fw.stars().contains(&target));
    let again = draw(&mut fw, STARTED);
    assert_eq!(again.len(), fw.star_count());
}

#[test]
fn alpha_byte_stays_in_visible_range() {
    // Only stars with alpha > 0.025 are drawn, so byte = trunc(255 * alpha) is at least trunc(6.375) = 6. Clamping
    // to 1..=255 keeps every byte valid; at zero ticks it is exactly 255.
    let mut fw = Fireworks::new(STARTED, 11);
    let mut now = STARTED;
    while !fw.is_done() {
        for g in draw(&mut fw, now) {
            let a = alpha_byte(&g);
            assert!((6..=255).contains(&a), "alpha byte {a} at {now}");
        }
        now += 997;
        assert!(now < STARTED + 600_000, "firework never finished");
    }
}

#[test]
fn same_seed_gives_same_stars_and_other_seeds_differ() {
    let a = Fireworks::new(STARTED, 99);
    let b = Fireworks::new(STARTED, 99);
    let c = Fireworks::new(STARTED, 100);
    assert_eq!(a.stars(), b.stars());
    assert_ne!(a.stars(), c.stars());
}

#[test]
fn after_a_long_time_nothing_is_left_to_draw() {
    let mut fw = Fireworks::new(STARTED, 3);
    // 10^9 ms is 2 * 10^7 ticks: every star is thousands of pixels away.
    assert!(draw(&mut fw, STARTED + 1_000_000_000).is_empty());
    assert!(fw.is_done());
    assert_eq!(fw.star_count(), 0);
    assert!(draw(&mut fw, STARTED + 1_000_000_001).is_empty());
}

#[test]
fn a_firework_over_a_smaller_range_fades_its_stars_sooner() {
    // The HUD's firework fades out at 100 * cardScale = 100 px. Its cut-off is 100 * 0.975^(1/3), about 99.17 px.
    //   d = 98.0: alpha = 1 - 0.98^3 = 0.058808 > 0.025, still drawn.
    //   d = 99.5: alpha = 1 - 0.995^3 = 0.014925 <= 0.025, removed.
    // The end screen's 500 px firework still has that same star at 99.5 px: alpha = 1 - 0.199^3 = 0.992 is drawn.
    let mut hud = Fireworks::with_range(STARTED, 7, 100.0);
    let mut end = Fireworks::new(STARTED, 7);
    let target = hud.stars()[0];
    let speed = target.x_vel.hypot(target.y_vel);
    let ms_in = (98.0 * 50.0 / speed).round() as u64;
    let ms_out = (99.5 * 50.0 / speed).round() as u64;

    draw(&mut hud, STARTED + ms_in);
    assert!(hud.stars().contains(&target), "still drawn at 98 px");

    draw(&mut hud, STARTED + ms_out);
    assert!(!hud.stars().contains(&target), "removed past the 100 px range");

    draw(&mut end, STARTED + ms_out);
    assert!(end.stars().contains(&target), "the end screen's range is 500 px");
}

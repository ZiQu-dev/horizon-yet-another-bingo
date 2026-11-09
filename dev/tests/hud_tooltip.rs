//! The in-world player tooltip (hud/TooltipHudRenderer.kt) and the look test. Expected values are worked out by hand from
//! the Kotlin and from the geometry, never taken from the code under test.
//!
//! Time in these tests: game tick `t` is at `1000 + 50 * (t - 100)` ms (50 ms a tick, from tick 100 at 1000 ms).
//! The font is 6 px a character. The window is 853 x 481 scaled pixels.
use horizon_bingo::hud_tooltip::{looked_at_player, LookTooltip};
use horizon_bingo::state::HudState;
use horizon_bingo::text::Text;
use horizon_bingo::text_plain::Lang;
use horizon_bingo::tooltip_v2::TooltipPacket;
use horizon_bingo::ui_draw::UiDraw;

const GUI: (i32, i32) = (853, 481);

fn width6(s: &str) -> i32 {
    6 * s.chars().count() as i32
}

fn no_lang(_: &str) -> Option<String> {
    None
}

fn lang() -> Lang<'static> {
    &no_lang
}

/// A host state whose `tooltips_by_entity` holds one tooltip per `(entity, lines)`.
fn hud_with(tooltips: &[(u128, &[&str])]) -> HudState {
    let mut hud = HudState::new();
    for (entity, lines) in tooltips {
        let text = lines.iter().map(|t| Text::plain(t)).collect();
        hud.tooltips_by_entity.insert(*entity, TooltipPacket { text, entity_id: *entity });
    }
    hud
}

/// The time of game tick `tick` in these tests.
fn at(tick: u64) -> u64 {
    1000 + 50 * (tick - 100)
}

/// One game tick of `update`, at tick `tick`'s time.
fn step(tip: &mut LookTooltip, hud: &HudState, looked: Option<u128>, tick: u64) {
    tip.update(looked, hud, tick, at(tick));
}

fn draw_at(tip: &LookTooltip, hud: &HudState, now: u64) -> Vec<UiDraw> {
    tip.draw(hud, now, GUI, lang(), &width6)
}

/// The one tooltip a frame draws: its lines and anchor.
fn tooltip(draws: &[UiDraw]) -> (Vec<String>, i32, i32) {
    match draws {
        [UiDraw::Tooltip { x, y, lines }] => (lines.clone(), *x, *y),
        other => panic!("expected one tooltip, got {other:?}"),
    }
}

/// Looks at `entity` for ticks `from..=to`.
fn look(tip: &mut LookTooltip, hud: &HudState, entity: Option<u128>, from: u64, to: u64) {
    for tick in from..=to {
        step(tip, hud, entity, tick);
    }
}

// --- timing -----------------------------------------------------------------------------------------------------

#[test]
fn nothing_shows_for_19_ticks_and_the_tooltip_shows_on_the_20th() {
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    // The first tick under the crosshair (tick 100) only records the entity: 19 more ticks make 19, not 20.
    for tick in 100..=119 {
        step(&mut tip, &hud, Some(7), tick);
        assert!(draw_at(&tip, &hud, at(tick)).is_empty(), "tick {tick} is {} ticks in", tick - 100);
    }
    step(&mut tip, &hud, Some(7), 120);
    assert_eq!(tooltip(&draw_at(&tip, &hud, at(120))).0, ["hello"]);
}

#[test]
fn switching_to_another_entity_starts_the_count_again_from_zero() {
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 115); // 15 ticks in
    step(&mut tip, &hud, None, 116); // looking at nothing resets the count
    step(&mut tip, &hud, Some(7), 117); // back on 7: 0 ticks in
    for tick in 118..=136 {
        step(&mut tip, &hud, Some(7), tick);
        assert!(draw_at(&tip, &hud, at(tick)).is_empty(), "tick {tick}: only {} ticks in", tick - 117);
    }
    step(&mut tip, &hud, Some(7), 137);
    assert_eq!(tooltip(&draw_at(&tip, &hud, at(137))).0, ["hello"]);
}

#[test]
fn the_old_tooltip_keeps_fading_while_the_new_entity_counts_up() {
    let hud = hud_with(&[(7, &["hello"]), (8, &["bye"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 125); // shown from tick 120, last updated at tick 125 (2250 ms)
    step(&mut tip, &hud, Some(8), 126); // the crosshair moves to 8
    look(&mut tip, &hud, Some(8), 127, 130);
    // 2500 ms: 7's fade-out runs until 2250 + 350 = 2600, so "hello" is still there while 8 has 4 ticks
    assert_eq!(tooltip(&draw_at(&tip, &hud, at(130))).0, ["hello"]);
    look(&mut tip, &hud, Some(8), 131, 145);
    // 8 has 19 ticks at 145 (3250 ms): 7 has faded out, and 8 is not yet shown
    assert!(draw_at(&tip, &hud, at(145)).is_empty());
    step(&mut tip, &hud, Some(8), 146);
    assert_eq!(tooltip(&draw_at(&tip, &hud, at(146))).0, ["bye"]);
}

#[test]
fn an_entity_without_a_tooltip_never_shows_and_looking_at_nothing_shows_nothing() {
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(9), 100, 140);
    assert!(draw_at(&tip, &hud, at(140)).is_empty(), "entity 9 has no tooltip");
    look(&mut tip, &hud, None, 141, 180);
    assert!(draw_at(&tip, &hud, at(180)).is_empty(), "nothing under the crosshair");
    assert_eq!(tip.alpha(at(180)), 0.0);
}

// --- fades ------------------------------------------------------------------------------------------------------

#[test]
fn the_fade_in_takes_250_ms_from_when_the_tooltip_first_showed() {
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 120); // shown at 2000 ms
    assert_eq!(tip.alpha(at(120)), 0.0, "the fade-in starts at 0");
    step(&mut tip, &hud, Some(7), 121); // 2050 ms: 50 of 250
    assert!((tip.alpha(at(121)) - 0.2).abs() < 1e-6);
    step(&mut tip, &hud, Some(7), 125); // 2250 ms: 250 of 250, and it stays at 1 while looked at
    assert_eq!(tip.alpha(at(125)), 1.0);
}

#[test]
fn the_fade_out_starts_100_ms_after_the_last_update_and_ends_350_ms_after_it() {
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 125); // last updated at 2250 ms
    step(&mut tip, &hud, None, 126); // looks away at 2300 ms
    assert_eq!(tip.alpha(2300), 1.0, "100 ms after the last update, the fade-out has not begun");
    assert!((tip.alpha(2350) - 1.0).abs() < 1e-6, "the fade-out begins at 2250 + 100");
    assert!((tip.alpha(2475) - 0.5).abs() < 1e-6, "125 of 250 ms into the fade-out");
    assert!(!draw_at(&tip, &hud, 2599).is_empty());
    assert!((tip.alpha(2599) - 0.004).abs() < 1e-6, "249 of 250 ms into the fade-out");
    assert!(draw_at(&tip, &hud, 2600).is_empty(), "the fade-out ends at 2250 + 350");
    assert_eq!(tip.alpha(2600), 0.0);
}

#[test]
fn after_the_fade_out_a_new_look_fades_in_again_from_zero() {
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 125);
    look(&mut tip, &hud, None, 126, 199); // the tooltip is long gone by 2600 ms
    look(&mut tip, &hud, Some(7), 200, 220); // shown again at tick 220 (7000 ms)
    assert_eq!(tip.alpha(at(220)), 0.0, "the fade-in restarts, it does not carry on from the last fade");
    assert_eq!(tooltip(&draw_at(&tip, &hud, at(220))).0, ["hello"]);
}

// --- placement and text -----------------------------------------------------------------------------------------

#[test]
fn the_anchor_is_centred_on_the_widest_line_12_px_left_and_30_px_below_the_middle() {
    // "hello" is 30 px and "hi there" 48 px: x = 853 / 2 - 48 / 2 - 12 = 426 - 24 - 12 = 390; y = 481 * 0.5 -> 240, + 30.
    let hud = hud_with(&[(7, &["hello", "hi there"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 120);
    assert_eq!(tooltip(&draw_at(&tip, &hud, at(120))), (vec!["hello".into(), "hi there".into()], 390, 270));
}

#[test]
fn an_odd_width_and_height_round_down_as_the_original_does() {
    // One line of 30 px: x = 426 - 15 - 12 = 399. Height 483: 483 * 0.5 = 241.5 -> 241, + 30 = 271.
    let hud = hud_with(&[(7, &["hello"])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 120);
    let odd = tip.draw(&hud, at(120), (853, 483), lang(), &width6);
    assert_eq!(tooltip(&odd), (vec!["hello".into()], 399, 271));
}

#[test]
fn a_long_line_wraps_at_200_px_and_the_box_is_as_wide_as_the_wrapped_line() {
    // 40 characters are 240 px: the first line takes 33 of them (198 px), the rest is 7 (42 px).
    // The box is 198 px wide: x = 426 - 99 - 12 = 315.
    let long = "c".repeat(40);
    let hud = hud_with(&[(7, &[long.as_str()])]);
    let mut tip = LookTooltip::default();
    look(&mut tip, &hud, Some(7), 100, 120);
    assert_eq!(
        tooltip(&draw_at(&tip, &hud, at(120))),
        (vec!["c".repeat(33), "c".repeat(7)], 315, 270)
    );
}

// --- looked_at_player -------------------------------------------------------------------------------------------

/// A player box: its uuid, feet, width and height.
fn player(uuid: u128, feet: [f64; 3], width: f32, height: f32) -> (u128, [f64; 3], f32, f32) {
    (uuid, feet, width, height)
}

const EYE: [f64; 3] = [0.0, 1.0, 0.0];

#[test]
fn a_player_straight_ahead_is_hit_at_its_near_face() {
    // Box x -0.5..0.5, y 0..2, z 4.5..5.5. Facing south (yaw 0), the ray meets z = 4.5 at 4.5 blocks.
    let a = [player(1, [0.0, 0.0, 5.0], 1.0, 2.0)];
    assert_eq!(looked_at_player(EYE, 0.0, 0.0, &a, 5.0), Some(1));
}

#[test]
fn yaw_90_faces_west_and_the_player_to_the_west_is_hit() {
    // yaw 90 is -X: the box x -5.5..-4.5 is at 4.5 blocks; the one to the east is behind.
    let west = [player(2, [-5.0, 0.0, 0.0], 1.0, 2.0)];
    assert_eq!(looked_at_player(EYE, 90.0, 0.0, &west, 5.0), Some(2));
    assert_eq!(looked_at_player(EYE, -90.0, 0.0, &west, 5.0), None, "yaw -90 faces east");
}

#[test]
fn a_player_to_the_side_is_missed_and_one_behind_is_missed() {
    let a = [player(1, [0.0, 0.0, 5.0], 1.0, 2.0)];
    assert_eq!(looked_at_player(EYE, 90.0, 0.0, &a, 5.0), None, "to the side, facing west");
    assert_eq!(looked_at_player(EYE, 180.0, 0.0, &a, 5.0), None, "behind, facing north");
}

#[test]
fn a_player_beyond_the_max_distance_is_missed_and_one_inside_it_is_hit() {
    let a = [player(1, [0.0, 0.0, 5.0], 1.0, 2.0)];
    assert_eq!(looked_at_player(EYE, 0.0, 0.0, &a, 4.4), None, "the near face is at 4.5");
    assert_eq!(looked_at_player(EYE, 0.0, 0.0, &a, 4.6), Some(1));
}

#[test]
fn pitch_looks_down_and_up() {
    // Eye at y 3. Pitch 30 is down: the ray (0, -0.5, 0.866) enters z = 4.5 at 5.196 blocks, at y 0.402 (inside 0..2).
    let a = [player(3, [0.0, 0.0, 5.0], 1.0, 2.0)];
    let eye = [0.0, 3.0, 0.0];
    assert_eq!(looked_at_player(eye, 0.0, 30.0, &a, 5.0), None, "5.196 is past 5");
    assert_eq!(looked_at_player(eye, 0.0, 30.0, &a, 5.5), Some(3));
    assert_eq!(looked_at_player(eye, 0.0, -30.0, &a, 10.0), None, "looking up, above the box");
}

#[test]
fn the_nearest_player_on_the_ray_is_the_one_hit_whatever_the_order() {
    let near = player(4, [0.0, 0.0, 3.0], 1.0, 2.0); // near face at 2.5
    let far = player(5, [0.0, 0.0, 6.0], 1.0, 2.0); // near face at 5.5
    assert_eq!(looked_at_player(EYE, 0.0, 0.0, &[far, near], 10.0), Some(4));
    assert_eq!(looked_at_player(EYE, 0.0, 0.0, &[], 10.0), None, "no players");
}

#[test]
fn a_diagonal_ray_hits_the_player_on_its_side_of_the_view() {
    // yaw 45 faces south-west: the view vector is (-0.7071, 0, 0.7071). The box at (-2, 2) with width 2 is x -3..-1,
    // z 1..3: the ray enters it at 1.414 blocks. The box at (2, 2) is to the east and never met.
    let south_west = [player(6, [-2.0, 0.0, 2.0], 2.0, 2.0)];
    assert_eq!(looked_at_player(EYE, 45.0, 0.0, &south_west, 5.0), Some(6));
    let south_east = [player(7, [2.0, 0.0, 2.0], 2.0, 2.0)];
    assert_eq!(looked_at_player(EYE, 45.0, 0.0, &south_east, 5.0), None);
}

//! The entity tooltip against the original (hud/TooltipHudRenderer.kt). Expected values are worked out by hand from the
//! Kotlin: 20 looking ticks (the first tick only records the entity), wrapping at 200 px, the box at
//! (width / 2 - maxLine / 2 - 12, height / 2 + 30), and fades of 250 ms in and out, the fade-out starting 100 ms after
//! the last update. The font is 6 px a character.
use std::collections::HashMap;

use horizon_bingo::text::Text;
use horizon_bingo::text_plain::Lang;
use horizon_bingo::tooltip_draw::*;
use horizon_bingo::tooltip_v2::TooltipPacket;

const SCREEN: (i32, i32) = (853, 481);

fn width6(s: &str) -> i32 {
    6 * s.chars().count() as i32
}

fn no_lang(_: &str) -> Option<String> {
    None
}

fn lang() -> Lang<'static> {
    &no_lang
}

fn packet(texts: &[&str], entity: u128) -> TooltipPacket {
    TooltipPacket { text: texts.iter().map(|t| Text::plain(t)).collect(), entity_id: entity }
}

fn tips(p: TooltipPacket) -> HashMap<u128, TooltipPacket> {
    HashMap::from([(p.entity_id, p)])
}

/// Looks at `entity` for `ticks` ticks, all at `now`.
fn look(state: &mut TooltipState, tooltips: &HashMap<u128, TooltipPacket>, entity: Option<u128>, ticks: usize, now: u64) {
    for _ in 0..ticks {
        state.tick(entity, tooltips, now);
    }
}

fn draw(state: &mut TooltipState, now: u64) -> Vec<TooltipDraw> {
    state.draw(now, SCREEN, lang(), &width6)
}

fn alpha_of(d: &[TooltipDraw]) -> f32 {
    match d {
        [TooltipDraw::Tooltip { alpha, .. }] => *alpha,
        other => panic!("expected one tooltip, got {other:?}"),
    }
}

#[test]
fn the_tooltip_shows_on_the_21st_tick_of_looking_because_the_first_tick_only_records_the_entity() {
    let t = tips(packet(&["hello"], 7));
    let mut s = TooltipState::default();
    look(&mut s, &t, Some(7), 20, 1000);
    assert!(draw(&mut s, 1000).is_empty(), "20 ticks: the first set the entity to 0, so the counter is 19");
    look(&mut s, &t, Some(7), 1, 1000);
    assert_eq!(draw(&mut s, 1000).len(), 1, "the 21st tick brings the counter to 20");
}

#[test]
fn nothing_shows_for_no_entity_or_for_an_entity_without_a_tooltip() {
    let t = tips(packet(&["hello"], 7));
    let mut s = TooltipState::default();
    look(&mut s, &t, Some(8), 40, 0);
    assert!(draw(&mut s, 0).is_empty(), "entity 8 has no tooltip");
    look(&mut s, &t, None, 40, 0);
    assert!(draw(&mut s, 0).is_empty(), "looking at nothing");
}

#[test]
fn looking_away_starts_the_count_again() {
    let t = tips(packet(&["hello"], 7));
    let mut s = TooltipState::default();
    look(&mut s, &t, Some(7), 20, 1000);
    look(&mut s, &t, None, 1, 1000);
    look(&mut s, &t, Some(7), 20, 1000);
    assert!(draw(&mut s, 1000).is_empty(), "back on entity 7 the count is 19 again, not 39");
    look(&mut s, &t, Some(7), 1, 1000);
    assert_eq!(draw(&mut s, 1000).len(), 1);
}

#[test]
fn text_wraps_at_the_last_space_before_200_px_and_the_space_is_dropped() {
    // 30 a, a space, 10 b: 41 characters, 246 px. The 34th character is the fourth b, 204 px: break at the space.
    let text = format!("{} {}", "a".repeat(30), "b".repeat(10));
    assert_eq!(wrap_lines(&text, WRAP_WIDTH, &width6), ["a".repeat(30), "b".repeat(10)]);
}

#[test]
fn a_word_longer_than_the_line_is_cut_where_the_line_is_full() {
    // 33 characters are 198 px, the 34th would be 204 px: [33 c, 7 c]
    let lines = wrap_lines(&"c".repeat(40), WRAP_WIDTH, &width6);
    assert_eq!(lines, ["c".repeat(33), "c".repeat(7)]);
}

#[test]
fn a_line_of_exactly_200_px_stays_whole_and_one_more_character_breaks_it() {
    assert_eq!(wrap_lines(&"e".repeat(33), WRAP_WIDTH, &width6), ["e".repeat(33)]);
    assert_eq!(wrap_lines(&"e".repeat(34), WRAP_WIDTH, &width6), ["e".repeat(33), "e".to_string()]);
}

#[test]
fn each_text_entry_is_wrapped_on_its_own_and_a_newline_always_breaks() {
    let p = packet(&["hello", "a\nb"], 7);
    assert_eq!(tooltip_lines(&p, lang(), &width6), ["hello", "a", "b"]);
}

#[test]
fn the_box_is_centred_on_the_widest_line_and_sits_30_px_below_the_middle() {
    // widest line "hi there" = 8 characters = 48 px: x = 853 / 2 - 48 / 2 - 12 = 390; y = 481 x 0.5 -> 240, + 30 = 270
    let t = tips(packet(&["hello", "hi there"], 7));
    let mut s = TooltipState::default();
    look(&mut s, &t, Some(7), 21, 1000);
    assert_eq!(
        draw(&mut s, 1000),
        [TooltipDraw::Tooltip {
            lines: vec!["hello".into(), "hi there".into()],
            x: 390,
            y: 270,
            z: 2000.0,
            alpha: 0.0,
        }]
    );
}

#[test]
fn the_tooltip_fades_in_over_250_ms_from_when_it_first_showed() {
    let t = tips(packet(&["hello"], 7));
    let mut s = TooltipState::default();
    look(&mut s, &t, Some(7), 21, 1000);
    assert_eq!(alpha_of(&draw(&mut s, 1000)), 0.0, "fade-in starts at 0");
    // 100 ms of 250 is 0.4; the fade-out has not started (it begins at 1000 + 100)
    assert!((alpha_of(&draw(&mut s, 1100)) - 0.4).abs() < 1e-6);
}

#[test]
fn the_fade_out_starts_100_ms_after_the_last_update_and_ends_at_250_ms_more() {
    let t = tips(packet(&["hello"], 7));
    let mut s = TooltipState::default();
    look(&mut s, &t, Some(7), 21, 1000);
    // fade-in 1 (225 of 250 is 0.9, not 1: so 0.9); fade-out 1 - (1225 - 1100) / 250 = 0.5; alpha 0.45
    assert!((alpha_of(&draw(&mut s, 1225)) - 0.45).abs() < 1e-6);
    // at 1350 the fade-out is over: alpha 0, and the tooltip is forgotten after this frame
    assert_eq!(alpha_of(&draw(&mut s, 1350)), 0.0);
    assert!(draw(&mut s, 1400).is_empty());
}

#[test]
fn another_entity_replaces_the_tooltip_but_keeps_the_fade_in_that_is_done() {
    let mut both = tips(packet(&["hello"], 7));
    both.insert(8, packet(&["bye"], 8));
    let mut s = TooltipState::default();
    look(&mut s, &both, Some(7), 21, 1000);
    // looking at entity 8 from 1500: its text takes the place of the first, and the fade-in (started at 1000) is done
    look(&mut s, &both, Some(8), 21, 1500);
    match draw(&mut s, 1500).as_slice() {
        [TooltipDraw::Tooltip { lines, alpha, .. }] => {
            assert_eq!(lines, &["bye".to_string()]);
            assert_eq!(*alpha, 1.0, "the fade-in is not started again");
        }
        other => panic!("expected one tooltip, got {other:?}"),
    }
}

//! The message history's rows, scroll and scroll bar against the original's numbers (MessageHistoryWidget.kt,
//! BingoMessageRenderer.kt, GameInfoWidget.kt, and the vanilla AbstractScrollArea rules).
//!
//! The messages and status come from the real packet rows of tests/oracle/game_message.tsv and game_status.tsv, fed
//! through `HudState::on_packet`. The expected numbers are written by hand from the Kotlin. Text is measured at 6 pixels
//! a character, so the wrapping and the centring below are worked out from that.

use std::cell::Cell;

use horizon_bingo::card_draw::ItemNames;
use horizon_bingo::game_message::MessageType;
use horizon_bingo::history_ctl::{Area, HistoryCtl, HistoryFrame};
use horizon_bingo::state::HudState;
use horizon_bingo::text_plain::Lang;
use horizon_bingo::ui_draw::UiDraw;

const MESSAGES: &str = include_str!("oracle/game_message.tsv");
const STATUS: &str = include_str!("oracle/game_status.tsv");

/// The widget's area in the sidebar: `x` 300, `y` 40, 150 wide, 100 high. Its messages wrap at 118 pixels.
const AREA: (i32, i32, i32, i32) = (300, 40, 150, 100);
const BAR_X: i32 = 444;

struct Names;
impl ItemNames for Names {
    fn item_name(&self, id: i32) -> Option<String> {
        Some(format!("item_{id}"))
    }
}

/// Six pixels a character, counted in characters (the test's font has no spaces of its own).
fn width(s: &str) -> i32 {
    s.chars().count() as i32 * 6
}

/// One row of an oracle table: its channel and its bytes.
fn packet_row(tsv: &str, name: &str) -> (String, Vec<u8>) {
    let line = tsv.lines().find(|l| l.split('\t').next() == Some(name)).expect("the row exists");
    let cols: Vec<&str> = line.split('\t').collect();
    let hex = cols[2];
    let bytes = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("hex")).collect();
    (cols[1].to_string(), bytes)
}

/// Feeds the named rows, in order, through the state's packet handler.
fn feed(hud: &mut HudState, tsv: &str, names: &[&str]) {
    for name in names {
        let (channel, bytes) = packet_row(tsv, name);
        hud.on_packet(&channel, &bytes).expect("the row decodes");
    }
}

fn frame(ctl: &mut HistoryCtl, hud: &HudState, area: (i32, i32, i32, i32)) -> HistoryFrame {
    let no_text: &dyn Fn(&str) -> Option<String> = &|_| None;
    let width_of: &dyn Fn(&str) -> i32 = &width;
    let lang: Lang<'_> = no_text;
    ctl.frame(hud, area, (0, 0), &Names, lang, width_of)
}

/// White text, as the history draws its status and its messages.
fn txt(x: f32, y: f32, text: &str) -> UiDraw {
    UiDraw::Text { x, y, text: text.into(), argb: 0xffff_ffff, shadow: false, obfuscated: false }
}

fn sprite(name: &str, x: f32, y: f32, w: f32, h: f32) -> UiDraw {
    UiDraw::Sprite { name: name.into(), x, y, w, h }
}

fn texts(draws: &[UiDraw]) -> Vec<String> {
    draws.iter().filter_map(|d| if let UiDraw::Text { text, .. } = d { Some(text.clone()) } else { None }).collect()
}

fn text_at(draws: &[UiDraw], wanted: &str) -> (f32, f32) {
    draws
        .iter()
        .find_map(|d| match d {
            UiDraw::Text { x, y, text, .. } if text == wanted => Some((*x, *y)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {wanted:?} in {draws:?}"))
}

/// The y of the last text drawn, the bottom-most row of a list drawn top to bottom.
fn last_text_y(draws: &[UiDraw]) -> f32 {
    draws
        .iter()
        .rev()
        .find_map(|d| if let UiDraw::Text { y, .. } = d { Some(*y) } else { None })
        .expect("a text")
}

fn scroller_y(draws: &[UiDraw]) -> f32 {
    draws
        .iter()
        .find_map(|d| match d {
            UiDraw::Sprite { name, y, .. } if name == "gui/sprites/widget/scroller" => Some(*y),
            _ => None,
        })
        .expect("a scroll bar")
}

fn game_container_y(draws: &[UiDraw]) -> i32 {
    draws
        .iter()
        .find_map(|d| match d {
            UiDraw::NineSlice { name, y, .. } if name == "bingo/game_container" => Some(*y),
            _ => None,
        })
        .expect("a status box")
}

#[test]
fn an_empty_history_is_only_its_margins() {
    // No status, no game over, no messages: 12 + (16 + 8) + 0 + 12 = 48 pixels, nothing to draw, nothing to scroll.
    let mut ctl = HistoryCtl::new();
    let f = frame(&mut ctl, &HudState::new(), AREA);
    assert_eq!(f.content_height, 48);
    assert!(f.draws.is_empty());
}

#[test]
fn the_time_line_is_centred_in_a_status_box_one_line_high() {
    // ingame 0 ms, no remaining time: "Time: 00m 00s" is 13 characters, 78 pixels. The box is 1 * 11 + 16 = 27 high,
    // so the content is 12 + (27 + 8) + 12 = 59. The line is at (150 - 78) / 2 = 36 across and 9 below the box's top.
    let mut hud = HudState::new();
    feed(&mut hud, STATUS, &["ingame_zero_remaining_null"]);
    let f = frame(&mut HistoryCtl::new(), &hud, AREA);
    assert_eq!(f.content_height, 59);
    assert_eq!(
        f.draws,
        vec![
            UiDraw::NineSlice { name: "bingo/game_container".into(), x: 300, y: 52, w: 150, h: 27, slice: 8, tex: None },
            txt(336.0, 61.0, "Time: "),
            UiDraw::Text { x: 372.0, y: 61.0, text: "00m 00s".into(), argb: 0xffff_ff55, shadow: false, obfuscated: false },
        ]
    );
}

#[test]
fn waiting_to_start_shows_while_no_ingame_time_has_arrived() {
    // The default status packet is received (not the Kotlin's DEFAULT instance) with both durations empty: its one
    // line is "Waiting to start..." (19 characters, 114 pixels), centred at (150 - 114) / 2 = 18.
    let mut hud = HudState::new();
    feed(&mut hud, STATUS, &["default_both_null"]);
    let f = frame(&mut HistoryCtl::new(), &hud, AREA);
    assert_eq!(f.content_height, 59);
    assert_eq!(text_at(&f.draws, "Waiting to start..."), (318.0, 61.0));
}

/// The five distinct messages of the oracle rows, fed in the order a game would send them, with the time line above.
/// Sorted by time they are minimal (0 s), uuid_high_bit (0 s, after it: a stable sort), non_ascii (5 ms), styled_message
/// (41.893 s) and plain_message (90 s). Each is one line of 18 pixels: 5 * 18 = 90, so the content is
/// 12 + 35 + 90 + 12 = 149, 49 pixels taller than the area.
fn five_messages_and_a_status() -> HudState {
    let mut hud = HudState::new();
    feed(&mut hud, STATUS, &["ingame_zero_remaining_null"]);
    feed(&mut hud, MESSAGES, &["minimal", "styled_message", "plain_message", "non_ascii", "uuid_high_bit"]);
    hud
}

#[test]
fn messages_run_newest_first_below_the_status_box_with_their_icons() {
    // The status box takes 12..47 (27 + 8 below it); then the rows from 47 newest first, 18 pixels each.
    // Each row's icon is 2 pixels from the left and (height/2 - 8) = 1 below the row's top; its text is 22 pixels
    // across and (height/2 - 4) = 5 below the top. The area is at (300, 40). The non-ASCII row has a texture icon,
    // which screen_cards does not draw yet, so that row has no icon here.
    let hud = five_messages_and_a_status();
    let f = frame(&mut HistoryCtl::new(), &hud, AREA);
    assert_eq!(f.content_height, 149);
    assert_eq!(
        f.draws,
        vec![
            UiDraw::NineSlice { name: "bingo/game_container".into(), x: 300, y: 52, w: 150, h: 27, slice: 8, tex: None },
            txt(336.0, 61.0, "Time: "),
            UiDraw::Text { x: 372.0, y: 61.0, text: "00m 00s".into(), argb: 0xffff_ff55, shadow: false, obfuscated: false },
            sprite("bingo/message_item_hidden", 302.0, 88.0, 16.0, 16.0),
            txt(322.0, 92.0, "Line scored"),
            sprite("bingo/message_check", 302.0, 106.0, 16.0, 16.0),
            // the message's own colour (`red`) now reaches the history
            UiDraw::Text { x: 322.0, y: 110.0, text: "Team Red".into(), argb: 0xffff_5555, shadow: false, obfuscated: false },
            txt(322.0, 128.0, "é 茶 🎉"),
            sprite("bingo/message_leading", 302.0, 142.0, 16.0, 16.0),
            txt(322.0, 146.0, ""),
            sprite("bingo/message_leading", 302.0, 160.0, 16.0, 16.0),
            txt(322.0, 164.0, ""),
            // The bar: the column at 444 (300 + 150 - 6), the background the full height, the thumb
            // (100 * 100 / 149 = 67, the travel 100 - 67 = 33) at the top.
            sprite("gui/sprites/widget/scroller_background", 444.0, 40.0, 6.0, 100.0),
            sprite("gui/sprites/widget/scroller", 444.0, 40.0, 6.0, 67.0),
        ]
    );
}

#[test]
fn the_wheel_moves_nine_pixels_a_step_and_stops_at_both_ends() {
    // The content scrolls 149 - 100 = 49 pixels at most. One step is 9 pixels: the status box moves from 52 to 43, and
    // the thumb to 40 + floor(9 / 49 * 33) = 46. Ten steps reach the end at 49 (the thumb at 40 + 33 = 73, its foot on
    // the area's foot at 140). The last message's bottom then sits 12 pixels above the foot: the bottom margin.
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame(&mut ctl, &hud, AREA);

    assert!(!ctl.wheel((450, 60), -1.0, AREA), "the right edge is outside");
    assert!(!ctl.wheel((350, 140), -1.0, AREA), "the bottom edge is outside");
    assert_eq!(ctl.scroll(), 0.0);

    assert!(ctl.wheel((350, 60), -1.0, AREA));
    let f = frame(&mut ctl, &hud, AREA);
    assert_eq!(game_container_y(&f.draws), 43);
    assert_eq!(scroller_y(&f.draws), 46.0);

    assert!(ctl.wheel((350, 60), -10.0, AREA));
    let f = frame(&mut ctl, &hud, AREA);
    assert_eq!(ctl.scroll(), 49.0);
    assert_eq!(game_container_y(&f.draws), 3);
    assert_eq!(scroller_y(&f.draws), 73.0);
    assert_eq!(last_text_y(&f.draws), 115.0, "the bottom row: 164 - 49");
    assert_eq!(AREA.1 + f.content_height - 49, 140, "the bottom margin ends at the area's foot");

    assert!(ctl.wheel((350, 60), 100.0, AREA));
    let f = frame(&mut ctl, &hud, AREA);
    assert_eq!(game_container_y(&f.draws), 52, "back at the top");
    assert_eq!(scroller_y(&f.draws), 40.0);
}

#[test]
fn dragging_the_thumb_moves_the_content_by_max_over_travel_per_pixel() {
    // Travel is 33 and the content scrolls 49, so a pixel of the pointer scrolls 49 / 33 pixels. Three pixels move the
    // content 4.4545 pixels, drawn 4 whole pixels down. Below the area the content goes to its end and above it to its
    // top, and a release ends the drag.
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame(&mut ctl, &hud, AREA);

    assert!(ctl.drag_start((446, 50), AREA));
    assert!(ctl.drag((446, 53), AREA));
    let f = frame(&mut ctl, &hud, AREA);
    assert!((ctl.scroll() - 3.0 * 49.0 / 33.0).abs() < 1e-4);
    assert_eq!(game_container_y(&f.draws), 48);

    assert!(ctl.drag((446, 200), AREA));
    assert_eq!(ctl.scroll(), 49.0);
    assert!(ctl.drag((446, 20), AREA));
    assert_eq!(ctl.scroll(), 0.0);

    assert!(ctl.release());
    assert!(!ctl.drag((446, 100), AREA), "no thumb is held after the release");
    assert_eq!(ctl.scroll(), 0.0);
}

#[test]
fn the_scroll_bar_is_the_right_six_pixels_of_the_area() {
    // Columns 444 to 449 are the bar; 443 and 450 are beside it. A press elsewhere in the area holds nothing.
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame(&mut ctl, &hud, AREA);
    assert!(!ctl.drag_start((BAR_X - 1, 50), AREA));
    assert!(ctl.drag_start((BAR_X, 50), AREA));
    assert!(ctl.release());
    assert!(ctl.drag_start((BAR_X + 5, 50), AREA));
    assert!(ctl.release());
    assert!(!ctl.drag_start((BAR_X + 6, 50), AREA));
    assert!(!ctl.drag_start((350, 50), AREA), "the text column does not hold the bar");
}

#[test]
fn a_shrinking_scroll_is_clamped_when_the_area_grows_and_the_content_fits() {
    // Scrolled to the end of a 100-pixel area, then the area grows to 200: the content (149) fits, so the scroll is
    // 0 and the bar is gone.
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame(&mut ctl, &hud, AREA);
    assert!(ctl.wheel((350, 60), -10.0, AREA));
    let tall = (300, 40, 150, 200);
    let f = frame(&mut ctl, &hud, tall);
    assert_eq!(ctl.scroll(), 0.0);
    assert_eq!(f.content_height, 149);
    assert_eq!(game_container_y(&f.draws), 52);
    assert!(f.draws.iter().all(|d| !matches!(d, UiDraw::Sprite { name, .. } if name.starts_with("gui/sprites/widget/"))));
}

#[test]
fn a_wrapped_message_takes_two_lines_and_twenty_six_pixels() {
    // The area is 120 wide: its messages wrap at 120 - 4 - 28 = 88 pixels. "type LEADING_TEAM" is 102 pixels, so it
    // breaks into "type" and "LEADING_TEAM" (72 pixels). Two lines are 2 * 9 + 2 = 20, plus 6: 26 pixels, above
    // the 18 an item takes. "Line scored" (66 pixels) stays on one line.
    // Its lines are centred on the row: 26 / 2 - 10 = 3 below the row's top, then 11 pixels apart.
    // Content: 12 + 24 + (18 + 26) + 12 = 92. The newest row (plain_message, 90 s) is at the top.
    let mut hud = HudState::new();
    feed(&mut hud, MESSAGES, &["type_leading_team", "plain_message"]);
    let f = frame(&mut HistoryCtl::new(), &hud, (0, 0, 120, 200));
    assert_eq!(f.content_height, 92);
    assert_eq!(
        f.draws,
        vec![
            sprite("bingo/message_item_hidden", 2.0, 13.0, 16.0, 16.0),
            txt(22.0, 17.0, "Line scored"),
            sprite("bingo/message_leading", 2.0, 35.0, 16.0, 16.0),
            txt(22.0, 33.0, "type"),
            txt(22.0, 44.0, "LEADING_TEAM"),
        ]
    );
}

#[test]
fn an_update_replaces_the_older_message_with_its_id() {
    // styled_message (id ..0a0f10, 41.893 s) is replaced by list_three, the same id at 2 ms with the update flag
    // set. Only "three" is left, one row of 18 pixels: content 12 + 24 + 18 + 12 = 66, with no status box drawn.
    // The text is 22 across, 5 below the row's top at 12, so (322, 57) inside the area at (300, 40).
    let mut hud = HudState::new();
    feed(&mut hud, MESSAGES, &["styled_message"]);
    feed(&mut hud, MESSAGES, &["list_three"]);
    let f = frame(&mut HistoryCtl::new(), &hud, AREA);
    assert_eq!(f.content_height, 66);
    assert_eq!(texts(&f.draws), vec!["three".to_string()]);
    assert_eq!(text_at(&f.draws, "three"), (322.0, 57.0));
}

#[test]
fn contains_follows_the_area_as_it_was_last_drawn() {
    // The right and bottom edges are outside the area: x 300..450 and y 40..140.
    let mut ctl = HistoryCtl::new();
    assert!(!ctl.contains((300, 40)), "nothing is drawn yet");
    frame(&mut ctl, &HudState::new(), AREA);
    assert!(ctl.contains((300, 40)));
    assert!(ctl.contains((449, 139)));
    assert!(!ctl.contains((450, 40)));
    assert!(!ctl.contains((300, 140)));
    assert!(!ctl.contains((299, 40)));
}

/// The game-over rows of tests/oracle/game_over.tsv.
const GAME_OVER: &str = include_str!("oracle/game_over.tsv");

/// One frame, with the width function counted: `calls` is the number of text measurements it made.
fn frame_counted(ctl: &mut HistoryCtl, hud: &HudState, area: Area, lang: Lang<'_>, calls: &Cell<usize>) -> HistoryFrame {
    let counting = |s: &str| {
        calls.set(calls.get() + 1);
        width(s)
    };
    let width_of: &dyn Fn(&str) -> i32 = &counting;
    ctl.frame(hud, area, (0, 0), &Names, lang, width_of)
}

fn no_text_lang(_: &str) -> Option<String> {
    None
}

/// The text draws as (x, y, text, colour), in paint order.
fn texts_at(draws: &[UiDraw]) -> Vec<(f32, f32, String, u32)> {
    draws
        .iter()
        .filter_map(|d| match d {
            UiDraw::Text { x, y, text, argb, .. } => Some((*x, *y, text.clone(), *argb)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_second_identical_frame_measures_no_text() {
    // The first frame builds the rows, measuring every message; the same frame again is served from the cache.
    let calls = Cell::new(0);
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    let first = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert!(calls.get() > 0, "the first frame builds the rows");
    calls.set(0);
    let second = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert_eq!(calls.get(), 0, "an identical frame makes no text_width call");
    assert_eq!(second, first);
}

#[test]
fn a_cached_frame_is_the_same_as_a_fresh_build() {
    let hud = five_messages_and_a_status();
    let mut cached = HistoryCtl::new();
    frame(&mut cached, &hud, AREA);
    let hit = frame(&mut cached, &hud, AREA);
    assert_eq!(hit, frame(&mut HistoryCtl::new(), &hud, AREA));
}

#[test]
fn a_cached_frame_scrolled_is_the_fresh_build_moved_up_by_the_scroll() {
    // Two wheel steps down: 18 px. The text moves up by 18, and the scroll bar is drawn from the scroll as before.
    let hud = five_messages_and_a_status();
    let mut cached = HistoryCtl::new();
    frame(&mut cached, &hud, AREA);
    assert!(cached.wheel((320, 60), -2.0, AREA));
    let scrolled = frame(&mut cached, &hud, AREA);
    let fresh = frame(&mut HistoryCtl::new(), &hud, AREA);
    let moved: Vec<(f32, f32, String, u32)> =
        texts_at(&fresh.draws).into_iter().map(|(x, y, t, argb)| (x, y - 18.0, t, argb)).collect();
    assert_eq!(texts_at(&scrolled.draws), moved);
    assert_eq!(scrolled.content_height, fresh.content_height);
}

#[test]
fn a_new_message_rebuilds_the_rows() {
    let calls = Cell::new(0);
    let mut hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    feed(&mut hud, MESSAGES, &["type_leading_team"]);
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert!(calls.get() > 0, "a message that is not yet in the rows rebuilds them");
    assert_eq!(after, frame(&mut HistoryCtl::new(), &hud, AREA));
}

#[test]
fn an_update_that_keeps_the_count_and_the_last_message_rebuilds_the_rows() {
    // `list_three` replaces styled_message (the same id, the update flag) at 2 ms: the count stays five and the last
    // message is still plain_message, so only the message itself tells the rows apart.
    let calls = Cell::new(0);
    let mut hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    let before = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    feed(&mut hud, MESSAGES, &["list_three"]);
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert!(calls.get() > 0, "the replaced message rebuilds the rows");
    assert_ne!(after, before);
    assert!(texts(&after.draws).contains(&"three".to_string()));
    assert_eq!(after, frame(&mut HistoryCtl::new(), &hud, AREA));
}

#[test]
fn a_status_time_change_rebuilds_the_rows() {
    // The status line carries the running time, so the rows change each second.
    let calls = Cell::new(0);
    let mut hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    feed(&mut hud, STATUS, &["ingame_41893ms_remaining_null"]);
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert!(calls.get() > 0, "a new time rebuilds the rows");
    assert_eq!(after, frame(&mut HistoryCtl::new(), &hud, AREA));
}

#[test]
fn a_new_width_rebuilds_the_rows() {
    let calls = Cell::new(0);
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    let narrow = (300, 40, 120, 100);
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, narrow, &no_text_lang, &calls);
    assert!(calls.get() > 0, "a new width rebuilds the rows, which wrap at it");
    assert_eq!(after, frame(&mut HistoryCtl::new(), &hud, narrow));
}

#[test]
fn a_game_over_rebuilds_the_rows() {
    let calls = Cell::new(0);
    let mut hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    feed(&mut hud, GAME_OVER, &["v1_tab_scores"]);
    assert!(hud.game_over.is_some(), "the row is a game over");
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert!(calls.get() > 0, "a game over rebuilds the rows");
    assert_eq!(after, frame(&mut HistoryCtl::new(), &hud, AREA));
}

#[test]
fn a_new_language_rebuilds_the_rows() {
    // The status label comes from the language: "Time: " becomes "Zeit: ", and the rows follow the language.
    let calls = Cell::new(0);
    let hud = five_messages_and_a_status();
    let mut ctl = HistoryCtl::new();
    let english = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    let german = |key: &str| if key == "bingo.scoreboard.time" { Some("Zeit: %s".to_string()) } else { None };
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, AREA, &german, &calls);
    assert!(calls.get() > 0, "a new language rebuilds the rows");
    assert_ne!(after, english);
    assert_eq!(after, build_in(&hud, &german));
}

#[test]
fn a_message_whose_icon_changes_rebuilds_the_rows_though_its_text_does_not() {
    // `type_leading_team` draws its icon, chosen by its kind; the text is the same whatever the kind is.
    let calls = Cell::new(0);
    let mut hud = HudState::new();
    feed(&mut hud, MESSAGES, &["type_leading_team"]);
    let mut ctl = HistoryCtl::new();
    let before = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    hud.past_messages[0].packet.message_type = MessageType::CardCompleted;
    calls.set(0);
    let after = frame_counted(&mut ctl, &hud, AREA, &no_text_lang, &calls);
    assert!(calls.get() > 0, "a new kind rebuilds the rows");
    assert_ne!(after, before, "the icon is another one");
    assert_eq!(after, frame(&mut HistoryCtl::new(), &hud, AREA));
}

fn build_in(hud: &HudState, lang: Lang<'_>) -> HistoryFrame {
    let width_of: &dyn Fn(&str) -> i32 = &width;
    HistoryCtl::new().frame(hud, AREA, (0, 0), &Names, lang, width_of)
}

//! The card placement screen against the numbers in BingoCardPlacementScreen.kt. Every expected value below is worked
//! out by hand from the Kotlin (window 400 x 300 unless a test says otherwise; the card is 122 x 132 at scale 1).
use horizon_bingo::card_draw::{Draw, ItemNames, TeamColors};
use horizon_bingo::screen_placement::*;

struct Names;
impl ItemNames for Names {
    fn item_name(&self, _id: i32) -> Option<String> {
        None
    }
}

/// The en_us strings the screen reads. Keys not listed fall back to the Kotlin's own defaults ("Cancel", "Ok").
fn lang(key: &str) -> Option<String> {
    match key {
        "bingo.config.card_position.help_move" => {
            Some("Left click and drag the center of the card to move it around the screen.".into())
        }
        "bingo.config.card_position.help_resize" => {
            Some("Click and drag near the bottom right of the card to resize it.".into())
        }
        "bingo.game.message.captured_item" => Some("%s got %s".into()),
        "bingo.objective.example.your_nose" => Some("Your Nose".into()),
        _ => None,
    }
}

/// Every string is 100 px wide, so centring is easy to check by hand.
fn width(_s: &str) -> i32 {
    100
}

fn view<'a>(colors: &'a TeamColors, now_ms: u64, width_fn: &'a dyn Fn(&str) -> i32) -> PlacementView<'a> {
    PlacementView { now_ms, chat_scale: 1.0, colors, items: &Names, lang: &lang, text_width: width_fn }
}

fn config(alignment: CardAlignment, x: i32, y: i32) -> ClientConfig {
    ClientConfig { card_alignment: alignment, card_offset_x: x, card_offset_y: y, ..ClientConfig::default() }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn texts(frame: &PlacementFrame) -> Vec<(i32, i32, String, u32)> {
    frame
        .draws
        .iter()
        .filter_map(|d| match d {
            PlacementDraw::Draw(Draw::Text { x, y, text, argb }) => Some((*x, *y, text.clone(), *argb)),
            _ => None,
        })
        .collect()
}

fn sprites(frame: &PlacementFrame) -> Vec<(String, f32, f32, f32, f32)> {
    frame
        .draws
        .iter()
        .filter_map(|d| match d {
            PlacementDraw::Draw(Draw::Sprite { name, x, y, w, h }) => Some((name.clone(), *x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_top_left_card_sits_at_its_offsets() {
    // TOP_LEFT: x = 0 * (400 - 122) + 10 = 10; y = 0 * (300 - 132) + 20 = 20.
    assert_eq!(init_position(CardAlignment::TopLeft, 10, 20, 1.0, (400, 300)), (10.0, 20.0));
}

#[test]
fn a_bottom_right_card_counts_its_offsets_inwards() {
    // BOTTOM_RIGHT: x = 1 * (400 - 122) - 10 = 268; y = 1 * (300 - 132) - 20 = 148.
    assert_eq!(init_position(CardAlignment::BottomRight, 10, 20, 1.0, (400, 300)), (268.0, 148.0));
}

#[test]
fn a_card_placed_off_the_window_is_pulled_back_inside() {
    // TOP_LEFT (-50, 500): left = clamp(-50, 0, 278) = 0; top = clamp(500, 0, 168) = 168.
    assert_eq!(init_position(CardAlignment::TopLeft, -50, 500, 1.0, (400, 300)), (0.0, 168.0));
    // TOP_LEFT (500, 0): left = clamp(500, 0, 400 - 122 = 278) = 278.
    assert_eq!(init_position(CardAlignment::TopLeft, 500, 0, 1.0, (400, 300)), (278.0, 0.0));
}

#[test]
fn a_scaled_card_is_anchored_by_its_scaled_size() {
    // BOTTOM_RIGHT at scale 2: the card is 244 x 264, so left = 400 - 244 = 156 and top = 300 - 264 = 36.
    assert_eq!(init_position(CardAlignment::BottomRight, 0, 0, 2.0, (400, 300)), (156.0, 36.0));
}

#[test]
fn a_card_near_the_right_edge_is_saved_as_top_right_with_the_gap_to_that_edge() {
    // right = 200 + 122 = 322; 400 - 322 = 78 < 200 (right edge); 300 - 142 = 158 is not < 10 (top edge).
    assert_eq!(apply_position(200.0, 10.0, 1.0, (400, 300)), (CardAlignment::TopRight, 78, 10));
}

#[test]
fn offsets_round_half_up_as_kotlin_rounds() {
    // Neither edge is near: TOP_LEFT with offsets round(100.5) = 101 and round(50.4) = 50.
    assert_eq!(apply_position(100.5, 50.4, 1.0, (400, 300)), (CardAlignment::TopLeft, 101, 50));
}

#[test]
fn a_card_in_the_bottom_right_corner_is_saved_from_both_edges() {
    // right = 372, 400 - 372 = 28 < 250; bottom = 282, 300 - 282 = 18 < 150: BOTTOM_RIGHT, offsets 28 and 18.
    assert_eq!(apply_position(250.0, 150.0, 1.0, (400, 300)), (CardAlignment::BottomRight, 28, 18));
}

#[test]
fn snapping_pulls_within_0_05_of_a_value_and_no_further() {
    assert!(approx(snap_scale(0.96), 1.0)); // 0.04 below 1
    assert!(approx(snap_scale(0.94), 0.94)); // 0.06 below 1: stays
    assert!(approx(snap_scale(1.46), 1.5)); // 0.04 below 1.5
    assert!(approx(snap_scale(0.51), 0.5));
    assert!(approx(snap_scale(1.2), 1.2)); // between 1 and 1.5, far from both
}

#[test]
fn resizing_is_the_distance_ratio_snapped_and_clamped() {
    // Anchor (0, 0), drag started 100 px out at scale 1. Mouse 120 px out: 1 / 100 * 120 = 1.2, no snap.
    assert!(approx(resize_scale(1.0, 0.0, 0.0, (100.0, 0.0), (120.0, 0.0), 300), 1.2));
    // Mouse 1000 px out: 10 is clamped to the window height 300 / 132, and no snap value is within 0.05 of it.
    assert!(approx(resize_scale(1.0, 0.0, 0.0, (100.0, 0.0), (1000.0, 0.0), 300), 300.0 / 132.0));
    // Mouse 5 px out: 0.05 is clamped up to 0.25.
    assert!(approx(resize_scale(1.0, 0.0, 0.0, (100.0, 0.0), (5.0, 0.0), 300), 0.25));
}

#[test]
fn dragging_moves_the_card_by_the_mouse_travel_without_clamping() {
    // From (10, 20), mouse went (100, 100) -> (130, 60): left 10 + 30 = 40, top 20 - 40 = -20.
    assert_eq!(drag_position((10.0, 20.0), (100.0, 100.0), (130.0, 60.0)), (40.0, -20.0));
}

#[test]
fn a_move_from_the_body_saves_the_new_top_left_offsets() {
    let mut p = Placement::open(&config(CardAlignment::TopLeft, 10, 20), (400, 300));
    assert!(p.mouse_dragged((50.0, 50.0))); // inside the body, away from the handles: a move starts
    assert!(p.mouse_dragged((80.0, 90.0)));
    assert_eq!((p.card().left, p.card().top), (40.0, 60.0));
    assert!(p.mouse_released());
    // Settled: right = 162 and 400 - 162 = 238 is not < 40; bottom = 192 and 300 - 192 = 108 is not < 60: TOP_LEFT.
    assert_eq!(p.press(Action::Save), Some(Outcome::Ok(config(CardAlignment::TopLeft, 40, 60))));
}

#[test]
fn a_drag_from_the_right_edge_resizes_to_the_distance_ratio() {
    let mut p = Placement::open(&ClientConfig::default(), (400, 300));
    // (122, 0) is on the right handle (x in 112..=132, y in 0..=132): a resize starts there, from scale 1.
    assert!(p.mouse_dragged((122.0, 0.0)));
    // Distance 183 from the anchor (0, 0) against 122 at the start: 1 / 122 * 183 = 1.5, snapped to 1.5.
    assert!(p.mouse_dragged((183.0, 0.0)));
    assert!(p.mouse_released());
    assert!(approx(p.card().right(), 183.0));
    assert_eq!(p.config().card_scale, 1.5);
    assert_eq!(p.press(Action::Save), Some(Outcome::Ok(ClientConfig { card_scale: 1.5, ..ClientConfig::default() })));
}

#[test]
fn a_bottom_right_card_saved_unchanged_keeps_its_offsets() {
    // Open at (268, 148); settle: right = 390, 400 - 390 = 10 < 268; bottom = 280, 300 - 280 = 20 < 148: BOTTOM_RIGHT.
    let mut p = Placement::open(&config(CardAlignment::BottomRight, 10, 20), (400, 300));
    assert_eq!(p.press(Action::Save), Some(Outcome::Ok(config(CardAlignment::BottomRight, 10, 20))));
}

#[test]
fn a_resized_window_keeps_the_edited_alignment() {
    // BOTTOM_RIGHT (10, 20) in a 500 x 400 window: left = 500 - 122 - 10 = 368; top = 400 - 132 - 20 = 248.
    let mut p = Placement::open(&config(CardAlignment::BottomRight, 10, 20), (400, 300));
    p.resize((500, 400));
    assert_eq!((p.card().left, p.card().top), (368.0, 248.0));
}

#[test]
fn the_cursor_shows_which_edge_or_the_body_is_under_the_mouse() {
    let p = Placement::open(&config(CardAlignment::TopLeft, 10, 10), (400, 300));
    // Card 10..=132 by 10..=142. The right handle is x in 122..=142.
    assert_eq!(cursor_at(&p, (132.0, 50.0)), Some(Cursor::ResizeHorizontal));
    // The bottom handle is y in 132..=152, and x = 60 is not on the right handle.
    assert_eq!(cursor_at(&p, (60.0, 150.0)), Some(Cursor::ResizeVertical));
    // Where both handles meet, the right one wins, as in the Kotlin.
    assert_eq!(cursor_at(&p, (125.0, 141.0)), Some(Cursor::ResizeHorizontal));
    assert_eq!(cursor_at(&p, (60.0, 60.0)), Some(Cursor::ResizeAll));
    assert_eq!(cursor_at(&p, (300.0, 250.0)), None);
}

#[test]
fn a_drag_in_progress_shows_the_move_cursor_and_the_overlay_over_the_scaled_card() {
    let colors = TeamColors::default();
    let w = width;
    let mut p = Placement::open(&config(CardAlignment::TopLeft, 10, 10), (400, 300));
    assert!(p.mouse_dragged((60.0, 60.0)));
    assert!(p.mouse_dragged((70.0, 60.0)));
    let frame = draw_placement(&p, &view(&colors, 0, &w), (70.0, 60.0));
    assert_eq!(frame.cursor, Some(Cursor::ResizeAll));
    assert!(frame.draws.contains(&PlacementDraw::ScaledFill { x: 20.0, y: 10.0, w: 122.0, h: 132.0, argb: 0x80ff_ffff }));
    // The moved card: left 10 + 10 = 20.
    assert_eq!(p.card().left, 20.0);
}

#[test]
fn the_hit_regions_are_the_handles_and_the_body_of_the_card() {
    let colors = TeamColors::default();
    let w = width;
    let p = Placement::open(&config(CardAlignment::TopLeft, 10, 10), (400, 300));
    let frame = draw_placement(&p, &view(&colors, 0, &w), (0.0, 0.0));
    // Buttons first (Cancel, Save), then the right handle, the bottom handle, and the body.
    assert_eq!(frame.hits[2], Hit { x: 122.0, y: 10.0, w: 20.0, h: 132.0, action: Action::Resize });
    assert_eq!(frame.hits[3], Hit { x: 10.0, y: 132.0, w: 122.0, h: 20.0, action: Action::Resize });
    assert_eq!(frame.hits[4], Hit { x: 10.0, y: 10.0, w: 122.0, h: 132.0, action: Action::Move });
}

#[test]
fn the_card_and_its_preview_message_are_drawn_at_the_card_corner() {
    let colors = TeamColors::default();
    let w = width;
    let p = Placement::open(&config(CardAlignment::TopLeft, 10, 20), (400, 300));
    let frame = draw_placement(&p, &view(&colors, 1000, &w), (0.0, 0.0));
    assert_eq!(sprites(&frame)[0], ("bingo/card_preview".to_string(), 10.0, 20.0, 122.0, 132.0));
    // The message anchor is (10, 20 + 132 * 1 * (1 - 0)) = (10, 152). Its text sits at x 28 and y 5 inside it:
    // the text is 500 ms old, fully faded in, so it is opaque white.
    let message = "\u{1F33A} fennifith got [Your Nose]";
    assert!(texts(&frame).contains(&(38, 157, message.to_string(), 0xffff_ffff)));
}

#[test]
fn the_help_lines_are_centred_at_the_top_in_green() {
    let colors = TeamColors::default();
    let w = width;
    let p = Placement::open(&config(CardAlignment::TopLeft, 0, 0), (400, 300));
    let frame = draw_placement(&p, &view(&colors, 0, &w), (0.0, 0.0));
    // Width 100 each: x = 200 - 50 = 150. Lines at y 8, and 16 + 9 = 25 for the second.
    let t = texts(&frame);
    assert!(t.contains(&(150, 8, "Left click and drag the center of the card to move it around the screen.".into(), 0xffa3_f5a3)));
    assert!(t.contains(&(150, 25, "Click and drag near the bottom right of the card to resize it.".into(), 0xffa3_f5a3)));
}

#[test]
fn the_buttons_sit_either_side_of_the_centre_and_highlight_under_the_mouse() {
    let colors = TeamColors::default();
    let w = width;
    let p = Placement::open(&config(CardAlignment::TopLeft, 0, 0), (400, 300));
    // Cancel: 200 - 150 - 4 = 46. Save: 200 + 4 = 204. Both at y = 300 - 20 - 8 = 272.
    let frame = draw_placement(&p, &view(&colors, 0, &w), (50.0, 280.0));
    assert_eq!(frame.hits[0], Hit { x: 46.0, y: 272.0, w: 150.0, h: 20.0, action: Action::Cancel });
    assert_eq!(frame.hits[1], Hit { x: 204.0, y: 272.0, w: 150.0, h: 20.0, action: Action::Save });
    // Labels centred: 46 + 75 - 50 = 71 and 204 + 75 - 50 = 229, at y 272 + 6 = 278.
    let t = texts(&frame);
    assert!(t.contains(&(71, 278, "Cancel".into(), 0xffff_ffff)));
    assert!(t.contains(&(229, 278, "Ok".into(), 0xffff_ffff)));
    // The mouse is over Cancel, so its sprite is the highlighted one.
    let s = sprites(&frame);
    assert!(s.contains(&("widget/button_highlighted".into(), 46.0, 272.0, 150.0, 20.0)));
    assert!(s.contains(&("widget/button".into(), 204.0, 272.0, 150.0, 20.0)));
}

#[test]
fn cancel_gives_no_config_and_escape_is_cancel() {
    let mut p = Placement::open(&config(CardAlignment::TopLeft, 10, 20), (400, 300));
    assert!(p.mouse_dragged((50.0, 50.0)));
    assert!(p.mouse_dragged((90.0, 90.0)));
    assert!(p.mouse_released());
    assert_eq!(p.press(Action::Cancel), Some(Outcome::Cancel));
}

// ---- the config the screen edits ----

/// A config file's client config with values that are not the defaults, so a copy that drops one shows.
fn saved_config() -> horizon_bingo::config::ClientConfig {
    use horizon_bingo::config::ClientConfig as FileConfig;
    FileConfig {
        enable_hud: false,
        card_scale: 0.75,
        card_alignment: CardAlignment::TopRight,
        card_offset_x: 12,
        card_offset_y: 34,
        card_overlap: CardOverlap::Underneath,
        message_duration_seconds: 9,
        message_scale: 2.0,
        hide_on_f3: false,
        card_team_outlines: false,
        sound_volumes: [("item_scored".to_string(), 0.25)].into_iter().collect(),
        ..FileConfig::default()
    }
}

#[test]
fn the_screen_reads_the_placement_fields_of_the_config() {
    // Every field the screen edits or previews, copied by name from the file's values above.
    assert_eq!(
        ClientConfig::from_config(&saved_config()),
        ClientConfig {
            enable_hud: false,
            card_scale: 0.75,
            card_alignment: CardAlignment::TopRight,
            card_offset_x: 12,
            card_offset_y: 34,
            card_overlap: CardOverlap::Underneath,
            message_duration_seconds: 9,
            message_scale: 2.0,
        }
    );
}

#[test]
fn writing_the_placement_changes_only_the_four_placement_fields() {
    use horizon_bingo::config::ClientConfig as FileConfig;
    let mut file = saved_config();
    let edit = ClientConfig {
        card_scale: 1.5,
        card_alignment: CardAlignment::BottomLeft,
        card_offset_x: 40,
        card_offset_y: 7,
        ..ClientConfig::from_config(&file)
    };
    edit.write_placement(&mut file);
    // The four fields are the edit's; the rest of the file is the saved one, the overlap and messages included.
    assert_eq!(
        file,
        FileConfig {
            card_scale: 1.5,
            card_alignment: CardAlignment::BottomLeft,
            card_offset_x: 40,
            card_offset_y: 7,
            ..saved_config()
        }
    );
}

#[test]
fn a_placement_copy_written_back_unchanged_leaves_the_file_as_it_was() {
    let mut file = saved_config();
    ClientConfig::from_config(&file).write_placement(&mut file);
    assert_eq!(file, saved_config());
}

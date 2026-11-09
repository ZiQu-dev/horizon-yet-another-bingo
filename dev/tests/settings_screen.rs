//! The settings screen's layout and clicks, worked out by hand. Rows are 24 px, headers 16 px; the list starts at y 28 and
//! ends 32 px above the window's bottom; a widget is 150 x 20, 4 px in from the list's right edge, and the list is 308
//! wide (46 px in from the left of a 400 px window). Text is 6 px a character. The row tops below are for a 400 x 600
//! window, scroll 0, unless a test says otherwise.
use horizon_bingo::config::{CardAlignment, CardOverlap, ClientConfig};
use horizon_bingo::settings_screen::{SettingsScreen, SettingsView};
use horizon_bingo::ui_draw::UiDraw;

fn lang(key: &str) -> Option<String> {
    match key {
        "options.on" => Some("ON".into()),
        "options.off" => Some("OFF".into()),
        _ => Some(format!("L:{key}")),
    }
}

fn width(s: &str) -> i32 {
    6 * s.chars().count() as i32
}

fn view<'a>(lang_fn: &'a dyn Fn(&str) -> Option<String>, width_fn: &'a dyn Fn(&str) -> i32) -> SettingsView<'a> {
    SettingsView { lang: lang_fn, text_width: width_fn }
}

/// One frame, then the screen is ready for input: the hit regions are those of this frame.
fn frame(screen: &mut SettingsScreen, config: &ClientConfig, gui: (i32, i32)) -> Vec<UiDraw> {
    frame_at(screen, config, (-1.0, -1.0), gui)
}

fn frame_at(screen: &mut SettingsScreen, config: &ClientConfig, mouse: (f64, f64), gui: (i32, i32)) -> Vec<UiDraw> {
    let v = view(&lang, &width);
    screen.frame(config, mouse, gui, &v)
}

fn texts(draws: &[UiDraw]) -> Vec<(f32, f32, String)> {
    draws
        .iter()
        .filter_map(|d| match d {
            UiDraw::Text { x, y, text, .. } => Some((*x, *y, text.clone())),
            _ => None,
        })
        .collect()
}

fn has_text(draws: &[UiDraw], x: f32, y: f32, text: &str) -> bool {
    texts(draws).iter().any(|(tx, ty, t)| *tx == x && *ty == y && t == text)
}

#[test]
fn a_toggle_shows_on_or_off_centred_on_its_widget() {
    // Enable HUD is the second row of the HUD group: header 28..44, quick start 44..68, header 68..84, this row 84..108.
    // Its widget is at (200, 86, 150, 20); "ON" is 12 px wide, so it starts at 200 + 75 - 6 = 269, and its text sits 6
    // px below the widget's top: 92.
    let mut s = SettingsScreen::new();
    let draws = frame(&mut s, &ClientConfig::default(), (400, 600));
    assert!(has_text(&draws, 269.0, 92.0, "ON"));
    let off = ClientConfig { enable_hud: false, ..ClientConfig::default() };
    let draws = frame(&mut s, &off, (400, 600));
    assert!(has_text(&draws, 266.0, 92.0, "OFF")); // "OFF" is 18 px: 200 + 75 - 9
}

#[test]
fn labels_are_the_kotlin_language_keys_at_the_list_edge() {
    // The label of a row sits 4 px in from the list's left edge: 46 + 4 = 50, and centred vertically in its row.
    let mut s = SettingsScreen::new();
    let draws = frame(&mut s, &ClientConfig::default(), (400, 600));
    assert!(has_text(&draws, 50.0, 51.0, "L:bingo.config.quick_start_button")); // row 44, label 44 + 7
    assert!(has_text(&draws, 50.0, 91.0, "L:bingo.config.enable_client_hud")); // row 84, label 84 + 7
    // The title is centred on the window's middle, at y 8.
    assert!(texts(&draws).iter().any(|(_, y, t)| *y == 8.0 && t == "L:bingo.full_name"));
}

#[test]
fn a_click_flips_a_toggle_and_says_the_config_changed() {
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(s.click(&mut config, (275.0, 96.0))); // the enable HUD widget, 200..350 x 86..106
    assert!(!config.enable_hud);
    frame(&mut s, &config, (400, 600));
    assert!(s.click(&mut config, (275.0, 96.0)));
    assert!(config.enable_hud);
}

#[test]
fn a_click_outside_every_widget_changes_nothing() {
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    // On a label, to the left of the widgets, and on a header.
    assert!(!s.click(&mut config, (100.0, 96.0)));
    assert!(!s.click(&mut config, (10.0, 10.0)));
    assert!(!s.click(&mut config, (275.0, 70.0))); // the HUD header's row
    assert_eq!(config, ClientConfig::default());
}

#[test]
fn an_enum_cycles_in_its_declaration_order() {
    // Card alignment is row 404..428, its widget at (200, 406). Top left, top right, bottom left, bottom right, top left.
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    let at = (275.0, 416.0);
    assert!(s.click(&mut config, at));
    assert_eq!(config.card_alignment, CardAlignment::TopRight);
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, at);
    assert_eq!(config.card_alignment, CardAlignment::BottomLeft);
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, at);
    assert_eq!(config.card_alignment, CardAlignment::BottomRight);
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, at);
    assert_eq!(config.card_alignment, CardAlignment::TopLeft);
}

#[test]
fn card_overlap_toggles_between_its_two_values() {
    // Card overlap is row 132..156, widget at (200, 134).
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(s.click(&mut config, (275.0, 144.0)));
    assert_eq!(config.card_overlap, CardOverlap::Underneath);
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (275.0, 144.0));
    assert_eq!(config.card_overlap, CardOverlap::Above);
}

#[test]
fn an_enum_shows_its_current_value() {
    let mut s = SettingsScreen::new();
    let config = ClientConfig { card_alignment: CardAlignment::BottomRight, ..ClientConfig::default() };
    let draws = frame(&mut s, &config, (400, 600));
    // "L:bingo.config.card_alignment.bottom_right" is 42 chars, 252 px: x = 275 - 126 = 149, text at 406 + 6.
    let key = "L:bingo.config.card_alignment.bottom_right";
    assert!(has_text(&draws, 275.0 - (width(key) / 2) as f32, 412.0, key));
}

#[test]
fn a_scale_slider_maps_the_track_to_its_range_in_steps() {
    // Card scale is row 380..404, widget (200, 382, 150, 20). The handle travels 142 px (150 less its 8). The mouse at
    // 204 + 142 * 0.4 = 260.8 is 40% along: 0.25 + 0.4 * 3.75 = 1.75, which is on a 0.05 step (30 steps).
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(s.click(&mut config, (260.8, 392.0)));
    assert!((config.card_scale - 1.75).abs() < 1e-4, "{}", config.card_scale);
}

#[test]
fn a_slider_ends_are_its_range_ends() {
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (204.0, 392.0));
    assert!((config.card_scale - 0.25).abs() < 1e-4);
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (346.0, 392.0));
    assert!((config.card_scale - 4.0).abs() < 1e-4);
}

#[test]
fn an_integer_slider_steps_by_one() {
    // Message duration is row 292..316, widget (200, 294). Its range is 1 to 30. At the far ends: 1 and 30.
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (204.0, 306.0));
    assert_eq!(config.message_duration_seconds, 1);
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (346.0, 306.0));
    assert_eq!(config.message_duration_seconds, 30);
    // 10 of 29 steps along (1 + 10 = 11): 204 + 142 * 10 / 29 = 252.9655 is at 11.
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (204.0 + 142.0 * 10.0 / 29.0, 306.0));
    assert_eq!(config.message_duration_seconds, 11);
}

#[test]
fn a_slider_shows_its_value_as_a_percentage() {
    // Card scale 1.0 reads "100%", 24 px wide, centred: 200 + 75 - 12 = 263, with its text at 382 + 6.
    let mut s = SettingsScreen::new();
    let draws = frame(&mut s, &ClientConfig::default(), (400, 600));
    assert!(has_text(&draws, 263.0, 388.0, "100%"));
}

#[test]
fn a_sound_volume_is_cut_to_a_percentage_as_kotlin_does() {
    // The item scored volume is row 508..532, widget (200, 510). At the middle of its track: 0.75, shown "75%".
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(s.click(&mut config, (275.0, 520.0)));
    assert!((config.sound_volume("item_scored") - 0.75).abs() < 1e-4);
    assert_eq!(config.sound_volumes.len(), 1);
    let draws = frame(&mut s, &config, (400, 600));
    assert!(has_text(&draws, 266.0, 516.0, "75%")); // 18 px: 200 + 75 - 9
}

#[test]
fn a_drag_moves_a_slider_until_the_button_comes_up() {
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (260.8, 392.0)); // to 1.75
    assert!(s.drag(&mut config, (346.0, 392.0)));
    assert_eq!(config.card_scale, 4.0);
    s.release();
    assert!(!s.drag(&mut config, (204.0, 392.0)));
    assert_eq!(config.card_scale, 4.0);
}

#[test]
fn a_drag_past_the_track_stops_at_its_end() {
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    s.click(&mut config, (260.8, 392.0));
    s.drag(&mut config, (900.0, 392.0));
    assert_eq!(config.card_scale, 4.0);
    s.drag(&mut config, (-900.0, 392.0));
    assert_eq!(config.card_scale, 0.25);
}

#[test]
fn reset_restores_the_defaults_and_says_whether_anything_changed() {
    // Reset is the left footer button, at (46, 572, 150, 20).
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig { enable_hud: false, card_scale: 2.0, ..ClientConfig::default() };
    frame(&mut s, &config, (400, 600));
    assert!(s.click(&mut config, (120.0, 582.0)));
    assert_eq!(config, ClientConfig::default());
    frame(&mut s, &config, (400, 600));
    assert!(!s.click(&mut config, (120.0, 582.0)));
}

#[test]
fn done_ends_the_screen_and_changes_nothing() {
    // Done is the right footer button, at (204, 572, 150, 20).
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(!s.click(&mut config, (280.0, 582.0)));
    assert!(s.is_done());
}

#[test]
fn escape_ends_the_screen() {
    let mut s = SettingsScreen::new();
    assert!(!s.is_done());
    s.escape();
    assert!(s.is_done());
}

#[test]
fn the_card_position_row_asks_for_the_placement_screen_once() {
    // Card position is row 108..132, widget (200, 110).
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(!s.click(&mut config, (275.0, 120.0)));
    assert!(s.take_placement_request());
    assert!(!s.take_placement_request());
    assert_eq!(config, ClientConfig::default());
}

#[test]
fn a_test_button_asks_for_its_sound_once() {
    // The item scored test button is row 532..556, widget (200, 534).
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    frame(&mut s, &config, (400, 600));
    assert!(!s.click(&mut config, (275.0, 544.0)));
    assert_eq!(s.take_test_sound(), Some("item_scored"));
    assert_eq!(s.take_test_sound(), None);
}

#[test]
fn the_list_scrolls_by_rows_and_stops_at_both_ends() {
    // At 400 x 300 the list shows 240 px (28 to 268). Its content is 976 px, so it scrolls at most 736.
    let mut s = SettingsScreen::new();
    let config = ClientConfig::default();
    frame(&mut s, &config, (400, 300));

    // Three rows down: 72 px. Card overlap (content 104) is then at 28 + 104 - 72 = 60, its label at 67.
    s.scroll(-3.0);
    let draws = frame(&mut s, &config, (400, 300));
    assert!(has_text(&draws, 50.0, 67.0, "L:bingo.config.card_overlap"));
    assert!(!texts(&draws).iter().any(|(_, _, t)| t == "L:bingo.config.quick_start_button"));

    // Far past the end: the last sound's test button (content 952) sits at 28 + 952 - 736 = 244, and clicking it asks
    // for the last sound.
    s.scroll(-1000.0);
    let mut config = config;
    frame(&mut s, &config, (400, 300));
    assert!(!s.click(&mut config, (275.0, 256.0)));
    assert_eq!(s.take_test_sound(), Some("start_release"));

    // Back up past the top: the first row is where it was.
    s.scroll(1000.0);
    let draws = frame(&mut s, &config, (400, 300));
    assert!(has_text(&draws, 50.0, 51.0, "L:bingo.config.quick_start_button"));
}

#[test]
fn a_row_only_draws_and_takes_clicks_when_it_fits_the_list() {
    // At 400 x 600 the list ends at 568. The item lost header (556..572) is the first row that does not fit.
    let mut s = SettingsScreen::new();
    let mut config = ClientConfig::default();
    let draws = frame(&mut s, &config, (400, 600));
    assert!(!texts(&draws).iter().any(|(_, _, t)| t == "L:bingo.sound.item_lost"));
    // The item scored header (492..508) fits.
    assert!(texts(&draws).iter().any(|(_, _, t)| t == "L:bingo.sound.item_scored"));
    // The quick start row is at 44: a click on its widget at (275, 56) flips it.
    assert!(s.click(&mut config, (275.0, 56.0)));
    assert!(!config.show_quick_start_button);
}

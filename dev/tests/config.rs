//! The client config against the Kotlin: `ClientConfig.kt` (defaults, the `json` file format), the YACL ranges of
//! `YetAnotherConfigLibIntegration.kt`, and `BingoHudController.getCardPos`. Expected values are worked out by hand from
//! those files, not from the code under test. The window is 400 x 300 GUI pixels unless a test says otherwise; the card
//! is 122 x 132 at scale 1.
use horizon_bingo::config::{
    alignment_xy, card_pos, card_scale, message_scale, overlap_z, CardAlignment, CardOverlap, ClientConfig,
};
use horizon_bingo::screen_placement::init_position;

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn with(alignment: CardAlignment, x: i32, y: i32, scale: f32) -> ClientConfig {
    ClientConfig {
        card_alignment: alignment,
        card_offset_x: x,
        card_offset_y: y,
        card_scale: scale,
        ..ClientConfig::default()
    }
}

#[test]
fn defaults_are_the_kotlin_constructor_defaults() {
    let c = ClientConfig::default();
    assert!(c.enable_hud);
    assert!(c.show_quick_start_button);
    assert!(c.card_pauses_game);
    assert_eq!(c.card_scale, 1.0);
    assert_eq!(c.card_alignment, CardAlignment::TopLeft);
    assert_eq!((c.card_offset_x, c.card_offset_y), (0, 0));
    assert_eq!(c.card_overlap, CardOverlap::Above);
    assert!(c.card_team_outlines);
    assert!(!c.show_multiple_cards);
    assert!(c.hide_on_f3);
    assert!(!c.hide_on_chat);
    assert!(c.message_from_other_teams);
    assert_eq!(c.message_duration_seconds, 5);
    assert_eq!(c.message_scale, 1.0);
    assert!(c.sound_volumes.is_empty());
    // `getSoundVolume`: a sound with no entry plays at 0.8.
    assert!(approx(c.sound_volume("item_scored"), 0.8));
}

#[test]
fn card_pos_top_left_is_the_offset_from_the_corner() {
    // Scale 2 makes the card 244 x 264. A top left anchor adds its offsets as they are.
    let c = with(CardAlignment::TopLeft, 10, 5, 2.0);
    assert_eq!(card_pos(&c, 400, 300), (10.0, 5.0));
}

#[test]
fn card_pos_counts_a_right_or_bottom_offset_inwards() {
    // x = ax * (400 - 122 * 2) + offset, with the offset negated on a right anchor; y likewise with 300 - 132 * 2.
    let top_right = with(CardAlignment::TopRight, 10, 5, 2.0);
    assert_eq!(card_pos(&top_right, 400, 300), (146.0, 5.0)); // 156 - 10, 0 + 5
    let bottom_left = with(CardAlignment::BottomLeft, 10, 5, 2.0);
    assert_eq!(card_pos(&bottom_left, 400, 300), (10.0, 31.0)); // 0 + 10, 36 - 5
    let bottom_right = with(CardAlignment::BottomRight, 10, 5, 2.0);
    assert_eq!(card_pos(&bottom_right, 400, 300), (146.0, 31.0));
}

#[test]
fn card_pos_at_scale_one_hugs_the_far_corner() {
    let bottom_right = with(CardAlignment::BottomRight, 0, 0, 1.0);
    assert_eq!(card_pos(&bottom_right, 400, 300), (278.0, 168.0)); // 400 - 122, 300 - 132
}

#[test]
fn card_pos_is_not_clamped_into_the_window() {
    // getCardPos does not clamp: at scale 4 the card is 488 x 528, so in a 300 x 200 window a right anchor hangs off
    // the left edge and a bottom anchor off the top. The placement screen's init_position clamps; see the next test.
    assert_eq!(card_pos(&with(CardAlignment::TopRight, 0, 0, 4.0), 300, 200), (-188.0, 0.0));
    assert_eq!(card_pos(&with(CardAlignment::BottomLeft, 0, 0, 4.0), 300, 200), (0.0, -328.0));
}

#[test]
fn card_pos_agrees_with_the_placement_screen_when_the_card_fits() {
    // screen_placement::init_position is the placement screen's initCardPosition. The two formulas are the same, so
    // they agree wherever the card is inside the window. Scale 1.5 makes the card 183 x 198, which fits.
    for alignment in [
        CardAlignment::TopLeft,
        CardAlignment::TopRight,
        CardAlignment::BottomLeft,
        CardAlignment::BottomRight,
    ] {
        let c = with(alignment, 7, 9, 1.5);
        assert_eq!(card_pos(&c, 400, 300), init_position(alignment, 7, 9, 1.5, (400, 300)), "{alignment:?}");
    }
}

#[test]
fn card_pos_differs_from_the_placement_clamp_when_the_card_does_not_fit() {
    // The same 300 x 200 window and scale 4 as above: the placement screen pulls the card back to x = 0.
    let c = with(CardAlignment::TopRight, 0, 0, 4.0);
    assert_eq!(card_pos(&c, 300, 200).0, -188.0);
    assert_eq!(init_position(CardAlignment::TopRight, 0, 0, 4.0, (300, 200)).0, 0.0);
}

#[test]
fn alignment_xy_is_one_on_the_right_and_the_bottom() {
    assert_eq!(alignment_xy(CardAlignment::TopLeft), (0, 0));
    assert_eq!(alignment_xy(CardAlignment::TopRight), (1, 0));
    assert_eq!(alignment_xy(CardAlignment::BottomLeft), (0, 1));
    assert_eq!(alignment_xy(CardAlignment::BottomRight), (1, 1));
}

#[test]
fn card_scale_is_kept_in_the_slider_range() {
    let scaled = |s: f32| ClientConfig { card_scale: s, ..ClientConfig::default() };
    assert_eq!(card_scale(&scaled(10.0)), 4.0);
    assert_eq!(card_scale(&scaled(0.01)), 0.25);
    assert_eq!(card_scale(&scaled(2.5)), 2.5);
    assert_eq!(card_scale(&scaled(f32::NAN)), 1.0);
}

#[test]
fn message_scale_is_the_message_scale_times_the_chat_text_size() {
    let c = ClientConfig { message_scale: 0.75, ..ClientConfig::default() };
    assert!(approx(message_scale(&c, 1.5), 1.125));
}

#[test]
fn overlap_z_is_kotlins_z_of_each_overlap() {
    assert_eq!(overlap_z(CardOverlap::Above), 800);
    assert_eq!(overlap_z(CardOverlap::Underneath), -800);
}

#[test]
fn to_json_writes_the_kotlin_field_names_in_declaration_order() {
    // The text Kotlin's `json` (prettyPrint, 4 spaces, one field per line) gives for the defaults, minus the comments.
    let expected = concat!(
        "{\n",
        "    \"enableHud\": true,\n",
        "    \"showQuickStartButton\": true,\n",
        "    \"cardPausesGame\": true,\n",
        "    \"cardScale\": 1.0,\n",
        "    \"cardAlignment\": \"TOP_LEFT\",\n",
        "    \"cardOffsetX\": 0,\n",
        "    \"cardOffsetY\": 0,\n",
        "    \"cardOverlap\": \"ABOVE\",\n",
        "    \"cardTeamOutlines\": true,\n",
        "    \"showMultipleCards\": false,\n",
        "    \"hideOnF3\": true,\n",
        "    \"hideOnChat\": false,\n",
        "    \"messageFromOtherTeams\": true,\n",
        "    \"messageDurationSeconds\": 5,\n",
        "    \"messageScale\": 1.0,\n",
        "    \"soundVolumes\": {}\n",
        "}",
    );
    assert_eq!(ClientConfig::default().to_json(), expected);
}

#[test]
fn a_customised_config_survives_a_json_round_trip() {
    let mut c = ClientConfig {
        enable_hud: false,
        show_quick_start_button: false,
        card_pauses_game: false,
        card_scale: 2.15,
        card_alignment: CardAlignment::BottomRight,
        card_offset_x: 12,
        card_offset_y: 34,
        card_overlap: CardOverlap::Underneath,
        card_team_outlines: false,
        show_multiple_cards: true,
        hide_on_f3: false,
        hide_on_chat: true,
        message_from_other_teams: false,
        message_duration_seconds: 17,
        message_scale: 0.75,
        ..ClientConfig::default()
    };
    c.sound_volumes.insert("item_scored".into(), 1.25);
    c.sound_volumes.insert("game_won".into(), 0.0);
    assert_eq!(ClientConfig::from_json(&c.to_json()), c);
}

#[test]
fn a_file_with_kotlins_comments_is_read() {
    // Kotlin's file: a `//` line above each field, which `allowComments` reads.
    let text = r#"{
    // Whether the Bingo Card HUD is rendered.
    "enableHud": false,
    // How big the card is.
    "cardScale": 2.5,
    "cardAlignment": "BOTTOM_RIGHT",
    "cardOffsetX": 12,
    "soundVolumes": {
        "item_scored": 1.25,
        "game_won": 0.5
    }
}"#;
    let c = ClientConfig::from_json(text);
    assert!(!c.enable_hud);
    assert_eq!(c.card_scale, 2.5);
    assert_eq!(c.card_alignment, CardAlignment::BottomRight);
    assert_eq!(c.card_offset_x, 12);
    assert_eq!(c.sound_volume("item_scored"), 1.25);
    assert_eq!(c.sound_volume("game_won"), 0.5);
    // Fields the file does not name keep their defaults.
    assert_eq!(c.card_offset_y, 0);
    assert!(c.hide_on_f3);
}

#[test]
fn a_missing_field_takes_its_default() {
    let c = ClientConfig::from_json(r#"{"cardScale": 3.0}"#);
    assert_eq!(c, ClientConfig { card_scale: 3.0, ..ClientConfig::default() });
}

#[test]
fn an_unknown_or_wrongly_typed_field_takes_its_default() {
    let text = r#"{
        "enableHud": "yes",
        "futureField": [1, 2],
        "hideOnF3": 0,
        "cardAlignment": "MIDDLE",
        "cardOverlap": "top_left",
        "messageDurationSeconds": "ten",
        "soundVolumes": ["item_scored"]
    }"#;
    assert_eq!(ClientConfig::from_json(text), ClientConfig::default());
}

#[test]
fn numbers_outside_their_range_are_clamped_into_it() {
    let text = r#"{
        "cardScale": 9,
        "messageScale": 0.01,
        "messageDurationSeconds": 99,
        "cardOffsetX": -5,
        "cardOffsetY": 70000000000,
        "soundVolumes": {"item_scored": 3.0, "item_lost": -1, "game_won": "loud", "timer_tick": 0.5}
    }"#;
    let c = ClientConfig::from_json(text);
    assert_eq!(c.card_scale, 4.0);
    assert_eq!(c.message_scale, 0.25);
    assert_eq!(c.message_duration_seconds, 30);
    assert_eq!(c.card_offset_x, 0);
    assert_eq!(c.card_offset_y, i32::MAX);
    assert_eq!(c.sound_volume("item_scored"), 1.5);
    assert_eq!(c.sound_volume("item_lost"), 0.0);
    // A volume that is not a number is no entry at all: the sound plays at the default.
    assert!(!c.sound_volumes.contains_key("game_won"));
    assert_eq!(c.sound_volume("timer_tick"), 0.5);
}

#[test]
fn a_duration_below_one_second_is_raised_to_one() {
    assert_eq!(ClientConfig::from_json(r#"{"messageDurationSeconds": 0}"#).message_duration_seconds, 1);
}

#[test]
fn text_that_is_not_a_config_object_gives_the_defaults_and_never_panics() {
    let deep = "[".repeat(100_000);
    let inputs = [
        "",
        "{",
        "[1, 2]",
        "null",
        "\"cardScale\"",
        r#"{"cardScale": 1e400}"#,
        "\u{0}\u{1}",
        deep.as_str(),
    ];
    for text in inputs {
        assert_eq!(ClientConfig::from_json(text), ClientConfig::default(), "input {:?}", &text[..text.len().min(20)]);
    }
}

#[test]
fn an_alignment_name_is_matched_exactly() {
    assert_eq!(ClientConfig::from_json(r#"{"cardAlignment": "BOTTOM_RIGHT"}"#).card_alignment, CardAlignment::BottomRight);
    assert_eq!(ClientConfig::from_json(r#"{"cardAlignment": "top_left"}"#).card_alignment, CardAlignment::TopLeft);
}

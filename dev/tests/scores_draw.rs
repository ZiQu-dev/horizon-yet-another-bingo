//! The SCORES tab and the game info panel as draw commands, against numbers worked out by hand from
//! BingoEndScoresWidget.kt and GameInfoWidget.kt. The input is the `v2_winner_team` row of the oracle
//! (tests/oracle/game_over.tsv), decoded by `HudState::on_packet`; its three teams were read off the bytes by hand:
//! Red (index 0): 10 items, 2 lines, 1 card, 1:30. Blue (index 1): all zero, no duration. Green (index 2): 3 items,
//! duration PT0S. The text width is 6 px a character, as in tests/screen_end.rs.
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::display_cards::PlayerProfile;
use horizon_bingo::screen_end::{Action, GameInfo, Hit, Run, ScoresInput, StatusLine};
use horizon_bingo::scores_draw::*;
use horizon_bingo::state::{Cards, ClientCard, HudState};
use horizon_bingo::ui_draw::UiDraw;

const GRAY: u32 = 0xFFAAAAAA;
const YELLOW: u32 = 0xFFFFFF55;
const WHITE: u32 = 0xFFFFFFFF;
const BLUE: u32 = 0xFF5555FF;
const BOX_BACKGROUND: u32 = 0x40000000;
const TSV: &str = include_str!("oracle/game_over.tsv");

/// en_us.json, for the keys the scores read (a missing key is `None`, as the translation lookup is).
fn lang(key: &str) -> Option<String> {
    let v = match key {
        "bingo.game.end.scores.team" => "Team",
        "bingo.game.end.scores.score" => "Score",
        "bingo.game.end.scores.time" => "Time",
        "bingo.game.end.scores.dnf" => "DNF",
        "bingo.game.card_count" => "%s cards",
        "bingo.game.card_count_1" => "1 card",
        "bingo.game.line_count" => "%s lines",
        "bingo.game.item_count" => "%s items",
        "bingo.scoreboard.time" => "Time: %s",
        _ => return None,
    };
    Some(v.to_string())
}

/// Six pixels a character.
fn width(s: &str) -> i32 {
    s.chars().count() as i32 * 6
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex")).collect()
}

/// The hex of one oracle row (`name`, channel, hex, expected).
fn oracle_bytes(name: &str) -> Vec<u8> {
    let row = TSV.lines().map(|l| l.split('\t').collect::<Vec<_>>()).find(|c| c[0] == name).expect("row");
    hex(row[2])
}

/// A team's card with two players, Alice and Bob.
fn red_card() -> ClientCard {
    let mut card = ClientCard::new_for_tests();
    card.display.players = vec![
        PlayerProfile { uuid: 1, name: "Alice".to_string() },
        PlayerProfile { uuid: 2, name: "Bob".to_string() },
    ];
    card
}

/// The end screen's state after the `v2_winner_team` packet, with a card for Red only.
fn end_state() -> HudState {
    let mut state = HudState::new();
    state.on_packet("yet-another-minecraft-bingo:game_over_v2", &oracle_bytes("v2_winner_team")).expect("decodes");
    state.cards.insert(Some("bingo_red".to_string()), red_card());
    state
}

fn ended_at(state: &HudState) -> i64 {
    state.game_over.as_ref().expect("game over").ended_at_millis
}

/// The frame for the state's packet, in the box `area`.
fn frame(state: &HudState, area: (i32, i32, i32, i32), mouse: (i32, i32), now: i64) -> ScoresFrame {
    let packet: &GameOverPacket = state.game_over.as_ref().expect("game over");
    let cards: &Cards = &state.cards;
    let text_width: &dyn Fn(&str) -> i32 = &width;
    let input = ScoresInput { game_over: packet, cards, lang: &lang, text_width, width: area.2, padding: 4 };
    scores_frame(&input, area, 0.0, mouse, now)
}

/// A text as the list draws it: shadowed, its top left at (x, y) in GUI pixels.
fn t(x: i32, y: i32, text: &str, argb: u32, obfuscated: bool) -> UiDraw {
    UiDraw::Text { x: x as f32, y: y as f32, text: text.to_string(), argb, shadow: true, obfuscated }
}

/// The box at (10, 20), 400 wide: the contents start at (14, 24) after the 4 px padding. Columns: team 18, score
/// 145, duration 378 (see `full_list_columns`); rows at y 45, 62, 79; the title at y 28.
const AREA: (i32, i32, i32, i32) = (10, 20, 400, 120);

#[test]
fn the_decoded_packet_has_the_three_teams_the_oracle_row_holds() {
    let state = end_state();
    let p = state.game_over.as_ref().expect("game over");
    let got: Vec<_> = p
        .scores
        .iter()
        .map(|s| (s.index, s.key.as_str(), s.score.items, s.score.lines, s.score.cards, s.duration.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, "bingo_red", 10, 2, 1, Some("PT1M30S")),
            (1, "bingo_blue", 0, 0, 0, None),
            (2, "bingo_green", 3, 0, 0, Some("PT0S")),
        ]
    );
    assert_eq!(p.winner.as_deref(), Some("bingo_red"));
}

#[test]
fn the_full_list_is_the_box_then_titles_then_each_team_row() {
    // Content width 400 - 8 = 392. Team texts "#1 Red" (36), "#2 Blue" (42), "#3 Green" (48), title "Team" (24): 48.
    // Score texts "1 card, 2 lines, 10 items" (25 chars, 150), "0" (6), "3 items" (42), title "Score" (30): 150.
    // Durations "1:30" (24), "DNF" (18), "0:00" (24), title "Time" (24): 24.
    // duration_col = 392 - 24 - 4 = 364. score_max_x = 364 - 150 - 4 = 210. score_min_x = 4 + 48 = 52.
    // score_col = (52 + 210) / 2 = 131, below 210. Content origin (14, 24): x 18, 145, 378; rows y 45, 62, 79.
    let state = end_state();
    let now = ended_at(&state) + 10_000;
    let f = frame(&state, AREA, (0, 0), now);
    assert_eq!(f.content_height, 17 * 4, "a title row and three team rows, 17 px each");
    assert_eq!(
        f.draws,
        vec![
            UiDraw::Fill { x: 10.0, y: 20.0, w: 400.0, h: 76.0, argb: BOX_BACKGROUND },
            t(18, 28, "Team", GRAY, false),
            t(145, 28, "Score", GRAY, false),
            t(378, 28, "Time", GRAY, false),
            t(18, 45, "#1 Red", WHITE, false),
            t(145, 45, "1 card, 2 lines, 10 items", WHITE, false),
            t(378, 45, "1:30", WHITE, false),
            // Blue's name is a component with `"color":"blue"` in the packet bytes: its own run, at "#2 "'s width (18 px).
            t(18, 62, "#2 ", WHITE, false),
            t(36, 62, "Blue", BLUE, false),
            t(145, 62, "0", WHITE, false),
            t(378, 62, "DNF", WHITE, false),
            t(18, 79, "#3 Green", WHITE, false),
            t(145, 79, "3 items", WHITE, false),
            t(378, 79, "0:00", WHITE, false),
        ]
    );
}

#[test]
fn each_row_is_revealed_once_its_number_of_whole_seconds_has_passed() {
    let state = end_state();
    let end = ended_at(&state);
    let obfuscated_at = |now: i64| -> (bool, bool, bool) {
        let f = frame(&state, AREA, (0, 0), now);
        let flag = |y: i32| {
            f.draws.iter().any(|d| matches!(d, UiDraw::Text { y: yy, obfuscated: true, .. } if *yy == y as f32))
        };
        (flag(45), flag(62), flag(79))
    };
    assert_eq!(obfuscated_at(end + 999), (true, true, true), "0.999 s: every row is still scrambled");
    assert_eq!(obfuscated_at(end + 1000), (false, true, true), "1 s: row 1 shows");
    assert_eq!(obfuscated_at(end + 2000), (false, false, true), "2 s: row 2 shows");
    let title_scrambled = frame(&state, AREA, (0, 0), end).draws.iter().any(|d| {
        matches!(d, UiDraw::Text { y, obfuscated: true, .. } if *y == 28.0)
    });
    assert!(!title_scrambled, "the title row is never scrambled");
}

#[test]
fn a_rows_band_fully_below_the_box_is_left_out_and_the_hit_is_cut_to_the_box() {
    // Box 38 px high: the title band (content 0..17) and team 1's band (17..34) are in. Team 2's band starts at
    // content 34, which is box y 58 (20 + 4 + 34), the box's bottom edge: fully outside. Team 1's hit is content
    // 17..35, which the box cuts at y 58 (17 px tall, not 18).
    let state = end_state();
    let f = frame(&state, (10, 20, 400, 38), (0, 0), ended_at(&state) + 10_000);
    // The list is 76 px with the padding, taller than the 38 px box: the scroll bar's two sprites come last.
    assert_eq!(f.draws.len(), 1 + 3 + 3 + 2, "the box, the title row, team 1's row, the bar; team 2 and 3 are left out");
    assert_eq!(f.draws[0], UiDraw::Fill { x: 10.0, y: 20.0, w: 400.0, h: 38.0, argb: BOX_BACKGROUND });
    assert!(f.draws.iter().all(|d| !matches!(d, UiDraw::Text { y, .. } if *y >= 62.0)));
    assert_eq!(
        f.hits,
        vec![Hit { x: 18, y: 41, w: 33, h: 17, action: Action::TeamPlayers { row: 0 } }],
        "Red's cell: x 18 for 33 px (team width 36 less the 4 px team column, plus one), y 41 for 17 px"
    );
}

#[test]
fn hovering_a_team_cell_shows_its_players_at_the_mouse() {
    // Mouse (30, 50): content (16, 26). Red's team width is 36; the row's band is content 17..=34, so the cell
    // holds x 4..=36 and y 17..=34: hovered. Blue and Green have no card, so no tooltip and no hit.
    let state = end_state();
    let f = frame(&state, AREA, (30, 50), ended_at(&state) + 10_000);
    let tips: Vec<_> = f.draws.iter().filter(|d| matches!(d, UiDraw::Tooltip { .. })).collect();
    assert_eq!(
        tips,
        vec![&UiDraw::Tooltip { x: 30, y: 50, lines: vec!["Red".into(), "Alice".into(), "Bob".into()] }],
        "the team name first (more than one player), then the players, at the mouse"
    );
    assert_eq!(f.hits, vec![Hit { x: 18, y: 41, w: 33, h: 18, action: Action::TeamPlayers { row: 0 } }]);
}

#[test]
fn the_tooltip_is_only_shown_inside_the_box_and_only_over_the_team_text() {
    let state = end_state();
    let now = ended_at(&state) + 10_000;
    // Box 30 px high (y 20..50): the mouse at y 52 is over row 0's band in content but below the box.
    let below = frame(&state, (10, 20, 400, 30), (30, 52), now);
    assert!(!below.draws.iter().any(|d| matches!(d, UiDraw::Tooltip { .. })), "outside the box: no tooltip");
    let inside = frame(&state, (10, 20, 400, 30), (30, 45), now);
    assert!(inside.draws.iter().any(|d| matches!(d, UiDraw::Tooltip { .. })), "inside the box: tooltip");
    // x 60 is past the team text (content x 46 > 36): not hovered.
    let beside = frame(&state, AREA, (60, 50), now);
    assert!(!beside.draws.iter().any(|d| matches!(d, UiDraw::Tooltip { .. })), "past the team text: no tooltip");
}

#[test]
fn the_scroll_offset_does_not_move_the_rows_as_in_the_original() {
    let state = end_state();
    let now = ended_at(&state) + 10_000;
    let packet = state.game_over.as_ref().expect("game over");
    let text_width: &dyn Fn(&str) -> i32 = &width;
    let input = ScoresInput { game_over: packet, cards: &state.cards, lang: &lang, text_width, width: 400, padding: 4 };
    assert_eq!(
        scores_frame(&input, AREA, 0.0, (0, 0), now).draws,
        scores_frame(&input, AREA, 37.0, (0, 0), now).draws
    );
}

#[test]
fn the_scroll_offset_is_kept_between_zero_and_the_padded_content_less_the_box() {
    // The range is content + 2 * 4 (the padding above and below) - box. Content 100, box 40: 100 + 8 - 40 = 68.
    assert_eq!(clamp_scroll(30.0, 100, 40), 30.0);
    assert_eq!(clamp_scroll(90.0, 100, 40), 68.0, "the padding adds 8 to the range");
    assert_eq!(clamp_scroll(-5.0, 100, 40), 0.0);
    assert_eq!(clamp_scroll(50.0, 68, 120), 0.0);
    assert_eq!(clamp_scroll(10.0, 30, 40), 0.0, "content shorter than the box");
    // The three teams (content 68) in a 40 px box: 68 + 8 - 40 = 36. In a 76 px box the padded content just fits.
    assert_eq!(clamp_scroll(100.0, 68, 40), 36.0);
    assert_eq!(clamp_scroll(10.0, 68, 76), 0.0, "content plus padding equal to the box: no room");
}

#[test]
fn a_wheel_notch_moves_nine_pixels_up_for_a_positive_amount_and_stays_in_range() {
    // scrollRate 9: scroll - amount * 9, then the clamp of the test above (content 100, box 40: 0..=68).
    assert_eq!(wheel(0.0, -1.0, 100, 40), 9.0, "wheel down moves the list down");
    assert_eq!(wheel(20.0, -2.0, 100, 40), 38.0);
    assert_eq!(wheel(20.0, 1.0, 100, 40), 11.0, "wheel up moves it back");
    assert_eq!(wheel(0.0, 1.0, 100, 40), 0.0, "wheel up at the top stays at the top");
    assert_eq!(wheel(50.0, -2.0, 100, 40), 68.0, "past the bottom (100 + 8 - 40) stops at the bottom");
}

/// The frame at scroll `scroll` for the state's packet, in the box `area`.
fn frame_at(state: &HudState, area: (i32, i32, i32, i32), scroll: f32, now: i64) -> ScoresFrame {
    let packet: &GameOverPacket = state.game_over.as_ref().expect("game over");
    let text_width: &dyn Fn(&str) -> i32 = &width;
    let input = ScoresInput { game_over: packet, cards: &state.cards, lang: &lang, text_width, width: area.2, padding: 4 };
    scores_frame(&input, area, scroll, (0, 0), now)
}

const SCROLLER_BACKGROUND: &str = "gui/sprites/widget/scroller_background";
const SCROLLER: &str = "gui/sprites/widget/scroller";

fn sprite(name: &str, x: f32, y: f32, w: f32, h: f32) -> UiDraw {
    UiDraw::Sprite { name: name.to_string(), x, y, w, h }
}

/// The scroll bar's draws: the background, then the thumb.
fn bar_sprites(f: &ScoresFrame) -> Vec<UiDraw> {
    f.draws.iter().filter(|d| matches!(d, UiDraw::Sprite { .. })).cloned().collect()
}

#[test]
fn the_scroll_bar_shows_when_the_padded_list_is_taller_than_the_box() {
    // The list is 68 px of title and rows, 76 with the padding. In a 76 px box it fits (76 is not more than 76): no
    // bar. In a 75 px box it is 1 px too tall: a bar. The 120 px box holds it.
    let state = end_state();
    let now = ended_at(&state) + 10_000;
    assert!(bar_sprites(&frame_at(&state, (10, 20, 400, 76), 0.0, now)).is_empty(), "76 px in a 76 px box");
    assert_eq!(bar_sprites(&frame_at(&state, (10, 20, 400, 75), 0.0, now)).len(), 2, "1 px too tall: the bar");
    assert!(bar_sprites(&frame_at(&state, AREA, 0.0, now)).is_empty(), "the 120 px box holds it");
}

#[test]
fn the_thumb_is_the_box_squared_over_the_padded_list_held_between_32_px_and_the_box_less_8() {
    // Box 40 px, list 76 px: 40 * 40 / 76 = 21, raised to the minimum 32. Box 70 px: 70 * 70 / 76 = 64, lowered to
    // 62 (70 - 8). Both at the top, the thumb is 6 px wide at the box's right edge: 10 + 400 - 6 = 404.
    let state = end_state();
    let now = ended_at(&state) + 10_000;
    let small = bar_sprites(&frame_at(&state, (10, 20, 400, 40), 0.0, now));
    assert_eq!(small[1], sprite(SCROLLER, 404.0, 20.0, 6.0, 32.0));
    let big = bar_sprites(&frame_at(&state, (10, 20, 400, 70), 0.0, now));
    assert_eq!(big[1], sprite(SCROLLER, 404.0, 20.0, 6.0, 62.0));
}

#[test]
fn the_thumb_moves_with_the_scroll_and_the_rows_do_not() {
    // Box 60 px at y 20, list 76 px: the range is 76 - 60 = 16. The thumb is 60 * 60 / 76 = 47 px and travels
    // 60 - 47 = 13 px. Scroll 0: the top, y 20. Scroll 8: 8 / 16 * 13 = 6.5, floored to 6: y 26. Scroll 16, the
    // bottom: 13, y 33. Scroll 100 is clamped to 16.
    let state = end_state();
    let now = ended_at(&state) + 10_000;
    let at = |scroll: f32| frame_at(&state, (10, 20, 400, 60), scroll, now);
    let thumb_at = |scroll: f32| bar_sprites(&at(scroll))[1].clone();
    assert_eq!(bar_sprites(&at(0.0))[0], sprite(SCROLLER_BACKGROUND, 404.0, 20.0, 6.0, 60.0), "the box's height");
    assert_eq!(thumb_at(0.0), sprite(SCROLLER, 404.0, 20.0, 6.0, 47.0));
    assert_eq!(thumb_at(8.0), sprite(SCROLLER, 404.0, 26.0, 6.0, 47.0));
    assert_eq!(thumb_at(16.0), sprite(SCROLLER, 404.0, 33.0, 6.0, 47.0));
    assert_eq!(thumb_at(100.0), sprite(SCROLLER, 404.0, 33.0, 6.0, 47.0), "past the end: the bottom");
    let rows = |scroll: f32| -> Vec<UiDraw> {
        at(scroll).draws.into_iter().filter(|d| matches!(d, UiDraw::Text { .. })).collect()
    };
    assert_eq!(rows(0.0), rows(16.0), "the scroll moves the thumb only");
}

#[test]
fn the_game_info_panel_is_a_frame_with_its_lines_centred_under_the_top_edge() {
    // One line "Time left: 1:02:03": "Time left: " (11 chars, 66 px) in white, "1:02:03" (7 chars, 42 px) in yellow.
    // Line width 108; box 200 wide: x = (200 - 108) / 2 = 46. Frame height: 1 line * (9 + 2) + 16 = 27. The origin is
    // (300, 40), so the line is at (346, 49) and its second run at 346 + 66 = 412.
    let line = StatusLine {
        runs: vec![
            Run { text: "Time left: ".to_string(), argb: WHITE },
            Run { text: "1:02:03".to_string(), argb: YELLOW },
        ],
    };
    let text_width: &dyn Fn(&str) -> i32 = &width;
    let info = GameInfo::new(vec![line], 200, text_width);
    assert_eq!(
        game_info_draws(&info, 300, 40),
        vec![
            UiDraw::NineSlice {
                name: "bingo/game_container".to_string(),
                x: 300,
                y: 40,
                w: 200,
                h: 27,
                slice: 8,
                tex: Some((200, 40)),
            },
            UiDraw::Text { x: 346.0, y: 49.0, text: "Time left: ".to_string(), argb: WHITE, shadow: false, obfuscated: false },
            UiDraw::Text { x: 412.0, y: 49.0, text: "1:02:03".to_string(), argb: YELLOW, shadow: false, obfuscated: false },
        ]
    );
}

const RED: u32 = 0xFFFF5555;

/// A team name of these component parts, in order (`{player: {id: [I; 4]}}` for a head, `{text}` for text).
fn name_parts(parts: Vec<horizon_bingo::nbt::Nbt>) -> horizon_bingo::text::Text {
    use horizon_bingo::nbt::Nbt;
    horizon_bingo::text::Text(Nbt::List { elem: 10, items: parts })
}

fn head_part(uuid: i32) -> horizon_bingo::nbt::Nbt {
    use horizon_bingo::nbt::Nbt;
    Nbt::Compound(vec![(
        "player".to_string(),
        Nbt::Compound(vec![("id".to_string(), Nbt::IntArray(vec![0, 0, 0, uuid]))]),
    )])
}

fn text_part(text: &str, color: Option<&str>) -> horizon_bingo::nbt::Nbt {
    use horizon_bingo::nbt::Nbt;
    let mut entries = vec![("text".to_string(), Nbt::String(text.to_string()))];
    if let Some(c) = color {
        entries.push(("color".to_string(), Nbt::String(c.to_string())));
    }
    Nbt::Compound(entries)
}

#[test]
fn a_head_in_a_team_name_is_a_face_sprite_in_the_content_origin_after_the_runs_before_it() {
    // Red's name: a head (player 7), then "Red" in red. The row's "#1 " is 18 px and the head's two spaces 12 px, so the
    // face is at content x 4 + 18 = 22, GUI 14 + 22 = 36, and "Red" at content 4 + 18 + 12 = 34, GUI 48. The row is at
    // GUI y 45. The team text "#1   Red" is 48 px, as wide as "#3 Green", so the columns stay where they were.
    let mut state = end_state();
    state.game_over.as_mut().expect("game over").scores[0].name =
        name_parts(vec![head_part(7), text_part("Red", Some("red"))]);
    let f = frame(&state, AREA, (0, 0), ended_at(&state) + 10_000);
    let on_row_45: Vec<UiDraw> = f
        .draws
        .iter()
        .filter(|d| match d {
            UiDraw::Text { y, .. } | UiDraw::Sprite { y, .. } => *y == 45.0,
            _ => false,
        })
        .cloned()
        .collect();
    assert_eq!(
        on_row_45,
        vec![
            t(18, 45, "#1 ", WHITE, false),
            sprite(&horizon_bingo::card_images::face_sprite(7), 36.0, 45.0, 8.0, 8.0),
            t(48, 45, "Red", RED, false),
            t(145, 45, "1 card, 2 lines, 10 items", WHITE, false),
            t(378, 45, "1:30", WHITE, false),
        ]
    );
}

#[test]
fn a_face_in_a_row_the_box_leaves_out_is_left_out_with_its_row() {
    // Green's name: a head (player 7), then "Grn": "#3 " 18 px, the head 12 px, "Grn" 18 px = 48 px, as before, so the
    // columns do not move. Green is row 2, at content y 34..51. In the 38 px box its band starts at GUI y 58, the box's
    // bottom edge: no face. In the 120 px box the face is at GUI (36, 79).
    let mut state = end_state();
    state.game_over.as_mut().expect("game over").scores[2].name = name_parts(vec![head_part(7), text_part("Grn", None)]);
    let face = horizon_bingo::card_images::face_sprite(7);
    let is_face = |d: &UiDraw| matches!(d, UiDraw::Sprite { name, .. } if *name == face);
    let now = ended_at(&state) + 10_000;
    let short = frame(&state, (10, 20, 400, 38), (0, 0), now);
    assert!(!short.draws.iter().any(is_face), "the row is outside the box: no face");
    let full = frame(&state, AREA, (0, 0), now);
    assert!(full.draws.contains(&sprite(&face, 36.0, 79.0, 8.0, 8.0)), "{:?}", full.draws);
}

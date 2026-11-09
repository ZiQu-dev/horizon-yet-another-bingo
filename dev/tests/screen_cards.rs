//! The card screen's row and the message history against the numbers in the original widgets
//! (BingoCardsWidget.kt, MessageHistoryWidget.kt, ClientCardRenderer.kt). Every expected value is worked out by hand
//! from the Kotlin. The font is 6 px per character; the widget is 400 x 200 at (200, 100), so its cards area is 240 px.
use horizon_bingo::card_draw::{Draw, ItemNames, TeamColors};
use horizon_bingo::card_tile::{CardTile, TierLabel};
use horizon_bingo::card_tile_image::CardTileImage;
use horizon_bingo::game_message::{GameMessagePacket, MessageType};
use horizon_bingo::game_over::{EndScreenTab, GameOverPacket};
use horizon_bingo::game_status::JavaDuration;
use horizon_bingo::screen_cards::*;
use horizon_bingo::screen_controller::card_commands;
use horizon_bingo::state::{empty_tile, ClientCard, ScoreMessage};
use horizon_bingo::text::Text;
use horizon_bingo::ui_draw::UiDraw;

struct Names;
impl ItemNames for Names {
    fn item_name(&self, id: i32) -> Option<String> {
        Some(format!("item_{id}"))
    }
}

fn width6(s: &str) -> i32 {
    6 * s.chars().count() as i32
}

fn no_lang(_: &str) -> Option<String> {
    None
}

fn no_outline(_: &str) -> u32 {
    0
}

const CARD_BG: &str = "bingo/card_preview";

fn area() -> Area {
    Area { x: 200.0, y: 100.0, width: 400, height: 200 }
}

fn colors() -> TeamColors {
    TeamColors::default()
}

fn card() -> ClientCard {
    let mut c = ClientCard::new_for_tests();
    c.tiles = vec![empty_tile(); 25];
    c.tile_updated_at = vec![None; 25];
    c
}

/// A card whose tile 0 is named `name` with `lore`.
fn card_with_tile0(name: &str, lore: &[&str]) -> ClientCard {
    let mut c = card();
    c.tiles[0].name = Some(Text::plain(name));
    c.tiles[0].lore = lore.iter().map(|l| Text::plain(l)).collect();
    c
}

fn sprites_named(d: &[Draw], name: &str) -> Vec<(f32, f32)> {
    d.iter()
        .filter_map(|c| match c {
            Draw::Sprite { name: n, x, y, .. } if n == name => Some((*x, *y)),
            _ => None,
        })
        .collect()
}

fn fills(d: &[Draw]) -> Vec<(i32, i32, i32, i32, u32)> {
    d.iter()
        .filter_map(|c| match c {
            Draw::Fill { x, y, w, h, argb } => Some((*x, *y, *w, *h, *argb)),
            _ => None,
        })
        .collect()
}

/// Runs one frame with the defaults of the card screen: no entrance, selectable, no opacity change.
fn run<'a>(
    slots: &'a [CardSlot<'a>],
    mouse: (i32, i32),
    scroll: &CardsScroll,
    outline: &'a dyn Fn(&str) -> u32,
    names: &'a dyn ItemNames,
    lang: &'a dyn Fn(&str) -> Option<String>,
    width: &'a dyn Fn(&str) -> i32,
) -> CardsFrame {
    let input = CardsInput {
        area: area(),
        slots,
        winner: None,
        entrance: None,
        elapsed_ms: 0,
        now_ms: 0,
        opacity: 1.0,
        selectable: true,
        mouse,
        show_outlines: false,
        team_outline: outline,
        items: names,
        images: None,
        lang,
        text_width: width,
    };
    frame(&input, scroll)
}

#[test]
fn the_row_starts_at_the_winners_place_and_at_the_start_without_one() {
    let c = card();
    let col = colors();
    let slots: Vec<CardSlot> = ["red", "blue", "green", "yellow", "pink"]
        .iter()
        .map(|t| CardSlot { team: Some(t.to_string()), card: &c, colors: &col, team_name: None })
        .collect();
    // "green" is the 3rd of 5 cards: 2 / 4
    assert_eq!(initial_scroll(&slots, Some("green")), 0.5);
    assert_eq!(initial_scroll(&slots, None), 0.0);
    assert_eq!(initial_scroll(&slots, Some("purple")), 0.0);
    // one card: the original divides by zero and resets to 0
    assert_eq!(initial_scroll(&slots[..1], Some("red")), 0.0);
}

#[test]
fn the_wheel_moves_the_target_by_half_a_card_per_turn_inside_the_widget_only() {
    let mut s = CardsScroll::starting_at(0.0);
    // amount -1 over 5 cards: 0 + (-1 * -0.5 / 5) = 0.1
    assert!(wheel(&mut s, &area(), 5, (200.0, 150.0), -1.0));
    assert!((s.target_x - 0.1).abs() < 1e-6);
    // the widget spans x 0..=400 and y 0..=200: a turn outside is not taken
    let mut t = CardsScroll::starting_at(0.0);
    assert!(!wheel(&mut t, &area(), 5, (500.0, 150.0), -1.0));
    assert_eq!(t.target_x, 0.0);
    // the target stays within 0 and 1
    assert!(wheel(&mut t, &area(), 5, (200.0, 150.0), 1.0));
    assert_eq!(t.target_x, 0.0);
}

#[test]
fn an_arrow_key_moves_one_card_from_the_rounded_index() {
    let mut s = CardsScroll::starting_at(0.5);
    // index round(0.5 * 4) = 2 of 5: right is 3 -> 0.75, left is 1 -> 0.25
    assert!(key(&mut s, 5, Key::Right));
    assert_eq!(s.target_x, 0.75);
    let mut s = CardsScroll::starting_at(0.5);
    assert!(key(&mut s, 5, Key::Left));
    assert_eq!(s.target_x, 0.25);
    // past the first card the target is held at 0, and past the last at 1
    let mut first = CardsScroll::starting_at(0.0);
    key(&mut first, 5, Key::Left);
    assert_eq!(first.target_x, 0.0);
    let mut last = CardsScroll::starting_at(1.0);
    key(&mut last, 5, Key::Right);
    assert_eq!(last.target_x, 1.0);
}

#[test]
fn the_drawn_scroll_eases_a_quarter_of_the_way_to_the_target_each_frame() {
    let mut s = CardsScroll::starting_at(0.0);
    s.target_x = 1.0;
    // (0 * 3 + 1) / 4 = 0.25, then (0.25 * 3 + 1) / 4 = 0.4375
    step(&mut s);
    assert_eq!(s.scroll_x, 0.25);
    step(&mut s);
    assert_eq!(s.scroll_x, 0.4375);
}

#[test]
fn cards_are_held_inside_the_area_and_the_nearest_to_the_middle_is_painted_last() {
    let c = card();
    let col = colors();
    let slots: Vec<CardSlot> = ["red", "blue", "green"]
        .iter()
        .map(|t| CardSlot { team: Some(t.to_string()), card: &c, colors: &col, team_name: None })
        .collect();
    let (o, n, w) = (no_outline, Names, width6);
    let (l, nm) = (&no_lang, &n);
    let f = run(&slots, (-1000, -1000), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    // the row is 366 px, the area 240 px: at scroll 0 the cards start at 63 px right of the left edge (x 17 + 63 = 80)
    // and the second and third are held 4 px apart from the right edge (198): x 202 and 206
    // (the area is 80..=198 for a card's left edge, minus its 122 px width); painted: card 2, card 1, card 0.
    assert_eq!(sprites_named(&f.draws, CARD_BG), vec![(206.0, 34.0), (202.0, 34.0), (80.0, 34.0)]);
}

#[test]
fn a_click_on_a_tile_hits_that_tile_and_a_click_on_the_title_selects_the_card() {
    let c = card_with_tile0("Diamond", &[]);
    let col = colors();
    let red = CardSlot { team: Some("red".into()), card: &c, colors: &col, team_name: None };
    let blue_card = card();
    let blue = CardSlot { team: Some("blue".into()), card: &blue_card, colors: &col, team_name: None };
    let slots = [red, blue];
    let (o, n, w) = (no_outline, Names, width6);
    let (l, nm) = (&no_lang, &n);
    let f = run(&slots, (100, 60), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    // the red card is at x 80 (screen_x 200 - 183 + 0 offset) with its top at 34: tile 0 is at (91, 55) to (111, 75)
    // (the card's own grid is 11 px in and 21 px down, 20 px a tile)
    let hit = f.hits.iter().find(|h| h.contains((100, 60))).expect("a hit");
    assert_eq!(hit.action, Action::Tile { team: Some("red".into()), index: 0 });
    // the title strip above the tiles belongs to the card
    let title = f.hits.iter().find(|h| h.contains((100, 45))).expect("a hit");
    assert_eq!(title.action, Action::SelectCard(Some("red".into())));
    // the right edge pixel of a card (x 80 + 122 = 202) is inside it: the card is clickable there
    let edge = f.hits.iter().find(|h| h.contains((202, 45))).expect("a hit");
    assert_eq!(edge.action, Action::SelectCard(Some("red".into())));
}

#[test]
fn hovering_a_named_tile_gives_its_name_as_the_tooltip() {
    let c = card_with_tile0("Diamond", &["Found in caves"]);
    let col = colors();
    let red = CardSlot { team: Some("red".into()), card: &c, colors: &col, team_name: None };
    let slots = [red];
    let (o, n, w) = (no_outline, Names, width6);
    let (l, nm) = (&no_lang, &n);
    // one card: its box is x 139..=261, tile 0 is at x 150..170 and y 55..75
    let f = run(&slots, (160, 60), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    let tip = f.tooltip.expect("a tooltip");
    assert_eq!(tip.index, 0);
    assert_eq!(tip.lines, vec!["Diamond".to_string(), "Found in caves".to_string()]);
    // the empty grid cell 1 (x 170..190) has no name, so no tooltip
    let f = run(&slots, (180, 60), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    assert!(f.tooltip.is_none());
}

#[test]
fn a_tooltip_wraps_every_lore_line_but_the_last_at_200_px() {
    let mut tile: CardTile = empty_tile();
    tile.name = Some(Text::plain("Diamond"));
    // 43 characters (258 px) do not fit in 200 px; the first 32 characters (192 px) do, the next word does not.
    tile.lore = vec![
        Text::plain("aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd"),
        Text::plain(&"L".repeat(50)),
    ];
    let w = width6;
    let lines = tile_tooltip_lines(&tile, &no_lang, &w);
    assert_eq!(
        lines,
        vec![
            "Diamond".to_string(),
            "aaaaaaaaaa bbbbbbbbbb cccccccccc".to_string(),
            "dddddddddd".to_string(),
            "L".repeat(50),
        ]
    );
}

#[test]
fn a_wrapped_line_breaks_at_the_last_word_that_fits() {
    let w = width6;
    // at 168 px (28 characters): the first line takes 21 characters, the next word would make 32 (192 px)
    assert_eq!(
        wrap_lines("aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd", 168, &w),
        vec!["aaaaaaaaaa bbbbbbbbbb".to_string(), "cccccccccc dddddddddd".to_string()]
    );
}

#[test]
fn the_scroll_bar_thumb_is_the_visible_share_of_the_track_and_sits_at_the_scroll() {
    let c = card();
    let col = colors();
    let slots: Vec<CardSlot> = ["red", "blue", "green"]
        .iter()
        .map(|t| CardSlot { team: Some(t.to_string()), card: &c, colors: &col, team_name: None })
        .collect();
    let (o, n, w) = (no_outline, Names, width6);
    let (l, nm) = (&no_lang, &n);
    // 3 cards: 366 px; area 240 px; the thumb 240 * 240 / 366 = 157 px; the track is 83 px more than the thumb
    // at scroll 0, the bar sits at x 200 - 120 = 80 and y 100 + 100 + 3 = 203
    let f = run(&slots, (-1000, -1000), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    assert!(fills(&f.draws).contains(&(80, 203, 157, 2, 0xff6b_6b85)));
    // the track is black at 80 of 255 alpha: 0x50000000, and it fills the rest of the 240 px
    assert!(fills(&f.draws).contains(&(237, 203, 83, 2, 0x5000_0000)));
    // at scroll 1 the thumb is at the far end: 80 + 83 = 163
    let end = run(&slots, (-1000, -1000), &CardsScroll::starting_at(1.0), &o, nm, l, &w);
    assert!(fills(&end.draws).contains(&(163, 203, 157, 2, 0xff6b_6b85)));
    // one card fits the area: no bar is drawn
    let one = run(&slots[..1], (-1000, -1000), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    assert!(fills(&one.draws).iter().all(|f| f.3 != 2));
}

#[test]
fn a_drag_on_the_bar_moves_the_thumb_and_a_drag_in_the_widget_pans_the_row() {
    let mut s = CardsScroll::starting_at(0.0);
    // the bar's grab zone: y 201..=207 at x 0..=400; the thumb can move 240 - 157 = 83 px
    assert!(drag(&mut s, &area(), 3, (150.0, 205.0)));
    assert!(drag(&mut s, &area(), 3, (191.5, 205.0)));
    assert_eq!(s.scroll_x, 0.5);
    assert_eq!(s.target_x, 0.5);
    assert!(release(&mut s));

    // a drag that starts in the widget, not on the bar, pans: it moves the target by the distance over 366 - 240 = 126
    let mut p = CardsScroll::starting_at(0.0);
    assert!(drag(&mut p, &area(), 3, (100.0, 100.0)));
    assert!(drag(&mut p, &area(), 3, (37.0, 100.0)));
    assert_eq!(p.target_x, 0.5);
    assert_eq!(p.scroll_x, 0.0);

    // a press outside the widget starts nothing
    let mut none = CardsScroll::starting_at(0.0);
    assert!(!drag(&mut none, &area(), 3, (1000.0, 100.0)));
    assert!(none.drag.is_none());
}

#[test]
fn the_history_lists_the_newest_message_first_with_its_text_28_px_from_the_icon() {
    let older = ScoreMessage { created_at: 0, packet: packet("Hello") };
    let newer = ScoreMessage { created_at: 1, packet: packet("Second") };
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let h = history(&HistoryView {
        width: 200,
        game_over: None,
        state_has_game_over: false,
        status_lines: vec![],
        messages: &[older, newer],
        items: &n,
        lang: l,
        text_width: w,
    });
    // each message is one line: 9 + 6 = 15, at least 18, so 18. The top is 12. 0 status lines: a status box of 16 px
    // plus 8 is still measured, so 12 + 24 + 18 + 18 + 12 = 84.
    assert_eq!(h.content_height, 84);
    let texts: Vec<(i32, i32, String)> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { x, y, text, .. }) => Some((*x, *y, text.clone())),
            _ => None,
        })
        .collect();
    // newest at the top: its text is at y 12 + (9 - 4) = 17; the older one is 18 lower, at 35; both at x 22 (28 - 6)
    assert_eq!(texts, vec![(22, 17, "Second".to_string()), (22, 35, "Hello".to_string())]);
}

#[test]
fn a_long_message_wraps_to_the_history_width_and_is_as_tall_as_its_lines() {
    let long = ScoreMessage {
        created_at: 0,
        packet: packet("aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd"),
    };
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let h = history(&HistoryView {
        width: 200,
        game_over: None,
        state_has_game_over: false,
        status_lines: vec![],
        messages: std::slice::from_ref(&long),
        items: &n,
        lang: l,
        text_width: w,
    });
    // 258 px is above 196 - 28 = 168: two lines, 2 * 9 + 2 = 20, plus 6 = 26
    assert_eq!(h.content_height, 12 + 24 + 26 + 12);
    let lines: Vec<String> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { text, .. }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec!["aaaaaaaaaa bbbbbbbbbb".to_string(), "cccccccccc dddddddddd".to_string()]);
}

#[test]
fn the_game_over_title_is_drawn_at_twice_the_size_in_a_box_above_the_messages() {
    let game_over = GameOverPacket {
        title: Text::plain("Winner"),
        subtitle: Text::plain("Good game"),
        winner: Some("bingo_red".into()),
        duration: JavaDuration::of_millis(0),
        is_ready_vote_available: false,
        is_resume_available: false,
        resume_description: None,
        is_winner: true,
        is_update: false,
        win_streak: None,
        best_win_streak: None,
        is_best_win_streak: false,
        captured_items: None,
        best_captured_items: None,
        is_best_captured_items: false,
        ended_at_millis: 0,
        is_best_time: false,
        prev_best_time: None,
        seed: None,
        scores: vec![],
        default_tab: EndScreenTab::Cards,
    };
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let h = history(&HistoryView {
        width: 200,
        game_over: Some(&game_over),
        state_has_game_over: true,
        status_lines: vec![],
        messages: &[],
        items: &n,
        lang: l,
        text_width: w,
    });
    // title: 1 line of 11 * 2 = 22; subtitle: 1 line of 11; then 4 + 12 + 8: a 57 px box, measured as 57 + 8 = 65
    assert_eq!(h.content_height, 12 + 65 + 12);
    assert_eq!(
        h.draws[0],
        ScreenDraw::NinePatch { name: "bingo/game_container".into(), x: 0, y: 12, w: 200, h: 57, slice: 8 }
    );
    // the glyphs start at (14 / 2, 12) before the 2x scale: 12 px down from the box's top and 14 px in
    assert_eq!(
        h.draws[1],
        ScreenDraw::ScaledText { x: 7.0, y: 12.0, scale: 2.0, text: "Winner".into(), argb: 0xffff_ffff }
    );
    // the subtitle follows the title block: 12 + 22 + 4 = 38 below the box's top, at x 14
    assert_eq!(
        h.draws[2],
        ScreenDraw::Plain(Draw::Text { x: 14, y: 12 + 38, text: "Good game".into(), argb: 0xffff_ffff })
    );
}

#[test]
fn the_tile_tier_is_not_in_the_tooltip_text_only_the_name_and_lore() {
    // The original draws the tier as the tile's hidden-tile sprite, not in the tooltip: a tier does not add a line.
    let mut c = card_with_tile0("Diamond", &[]);
    c.tiles[0].item_tier = Some(TierLabel::S);
    let col = colors();
    let red = CardSlot { team: Some("red".into()), card: &c, colors: &col, team_name: None };
    let slots = [red];
    let (o, n, w) = (no_outline, Names, width6);
    let (l, nm) = (&no_lang, &n);
    let f = run(&slots, (160, 60), &CardsScroll::starting_at(0.0), &o, nm, l, &w);
    assert_eq!(f.tooltip.expect("a tooltip").lines, vec!["Diamond".to_string()]);
}

fn packet(text: &str) -> GameMessagePacket {
    GameMessagePacket {
        id: 1,
        time_elapsed: JavaDuration::of_millis(0),
        team: "bingo_red".into(),
        image: CardTileImage::default(),
        image_list: Vec::new(),
        message_type: MessageType::LineScored,
        message: Text::plain(text),
        is_update: false,
    }
}

#[test]
fn a_card_pushed_past_the_left_edge_is_held_4_px_further_out_per_card() {
    let c = card();
    let col = colors();
    let slots: Vec<CardSlot> = ["red", "blue", "green"]
        .iter()
        .map(|t| CardSlot { team: Some(t.to_string()), card: &c, colors: &col, team_name: None })
        .collect();
    let (o, n, w) = (no_outline, Names, width6);
    let (l, nm) = (&no_lang, &n);
    // scroll 1: the row is shifted 63 px left. Card 0 would start at 17 - 63 = -46, card 1 at 76, card 2 at 198.
    // The left edge is 80, so card 0 is held at 80 - 4 * 2 = 72 and card 1 at 80 - 4 * 1 = 76.
    // Painted order: the farthest from the middle first, so card 0, card 1, card 2.
    let f = run(&slots, (-1000, -1000), &CardsScroll::starting_at(1.0), &o, nm, l, &w);
    assert_eq!(sprites_named(&f.draws, CARD_BG), vec![(72.0, 34.0), (76.0, 34.0), (198.0, 34.0)]);
}

#[test]
fn a_message_of_29_characters_is_just_too_wide_for_the_history_line_and_wraps() {
    // 29 characters = 174 px: above 196 - 28 = 168 px, so it wraps after 28 characters, the last one alone.
    let text = "a".repeat(29);
    let m = ScoreMessage { created_at: 0, packet: packet(&text) };
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let h = history(&HistoryView {
        width: 200,
        game_over: None,
        state_has_game_over: false,
        status_lines: vec![],
        messages: std::slice::from_ref(&m),
        items: &n,
        lang: l,
        text_width: w,
    });
    // two lines: 2 * 9 + 2 + 6 = 26 px high
    assert_eq!(h.content_height, 12 + 24 + 26 + 12);
    let lines: Vec<String> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { text, .. }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec!["a".repeat(28), "a".to_string()]);
}

#[test]
fn the_history_wraps_at_the_widget_width_less_4_and_less_28_for_the_icon() {
    // a font of 6 px a character plus 1: 28 characters are 169 px, above the 168 px line (200 - 4 - 28)
    let wide = |s: &str| 6 * s.chars().count() as i32 + 1;
    let m = ScoreMessage { created_at: 0, packet: packet(&"a".repeat(28)) };
    let n = Names;
    let (l, w) = (&no_lang, &wide);
    let h = history(&HistoryView {
        width: 200,
        game_over: None,
        state_has_game_over: false,
        status_lines: vec![],
        messages: std::slice::from_ref(&m),
        items: &n,
        lang: l,
        text_width: w,
    });
    assert_eq!(h.content_height, 12 + 24 + 26 + 12);
    let lines: Vec<String> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { text, .. }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec!["a".repeat(27), "a".to_string()]);
}

#[test]
fn a_status_line_draws_its_runs_one_after_another_in_their_own_colours_centred_as_a_whole() {
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let h = history(&HistoryView {
        width: 200,
        game_over: None,
        state_has_game_over: false,
        // "Time: " (36 wide at 6 per glyph) in white then "08m" (18) in yellow: 54 wide, centred in 200 at x = 73
        status_lines: vec![vec![("Time: ".to_string(), 0xffff_ffff), ("08m".to_string(), 0xffff_ff55)]],
        messages: &[],
        items: &n,
        lang: l,
        text_width: w,
    });
    let runs: Vec<(i32, String, u32)> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { x, text, argb, .. }) => Some((*x, text.clone(), *argb)),
            _ => None,
        })
        .collect();
    assert_eq!(runs, vec![(73, "Time: ".to_string(), 0xffff_ffff), (109, "08m".to_string(), 0xffff_ff55)]);
}

#[test]
fn history_lines_keep_each_runs_colour_and_a_head_takes_its_two_spaces() {
    use horizon_bingo::nbt::Nbt;
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let head = comp(vec![("player", comp(vec![("id", Nbt::IntArray(vec![0, 0, 0, 7]))]))]);
    let name = comp(vec![("text", s("Bob")), ("color", s("blue"))]);
    let mut m = ScoreMessage { created_at: 0, packet: packet("") };
    m.packet.message = Text(Nbt::List { elem: 10, items: vec![comp(vec![("text", s("[1:12]")), ("color", s("gray"))]), head, name] });
    let h = history(&HistoryView {
        width: 200,
        game_over: None,
        state_has_game_over: false,
        status_lines: vec![],
        messages: &[m],
        items: &n,
        lang: l,
        text_width: w,
    });
    let drawn: Vec<(i32, String, u32)> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { x, text, argb, .. }) => Some((*x, text.clone(), *argb)),
            _ => None,
        })
        .collect();
    // "[1:12]" gray (6 glyphs, 36 wide) from x 22, the head two spaces (12 at 6 per glyph) after it, then the name in blue
    assert_eq!(drawn, vec![(22, "[1:12]".to_string(), 0xffaaaaaa), (70, "Bob".to_string(), 0xff5555ff)]);
    let face = h.draws.iter().find_map(|d| match d {
        ScreenDraw::Plain(Draw::Sprite { name, x, w, h, .. }) if name.starts_with("face/") => Some((name.clone(), *x, *w, *h)),
        _ => None,
    });
    assert_eq!(face, Some(("face/00000000000000000000000000000007".to_string(), 58.0, 8.0, 8.0)));
}

#[test]
fn the_game_over_subtitle_keeps_the_colours_and_the_head_of_the_winning_player() {
    use horizon_bingo::nbt::Nbt;
    let s = |v: &str| Nbt::String(v.into());
    let comp = |entries: Vec<(&str, Nbt)>| Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let head = comp(vec![("player", comp(vec![("id", Nbt::IntArray(vec![0, 0, 0, 5]))]))]);
    let subtitle = Text(Nbt::List {
        elem: 10,
        items: vec![head, comp(vec![("text", s("Bob")), ("color", s("yellow"))]), comp(vec![("text", s(" won"))])],
    });
    let game_over = GameOverPacket {
        title: Text::plain("Winner"),
        subtitle,
        winner: None,
        duration: JavaDuration::of_millis(0),
        is_ready_vote_available: false,
        is_resume_available: false,
        resume_description: None,
        is_winner: true,
        is_update: false,
        win_streak: None,
        best_win_streak: None,
        is_best_win_streak: false,
        captured_items: None,
        best_captured_items: None,
        is_best_captured_items: false,
        ended_at_millis: 0,
        is_best_time: false,
        prev_best_time: None,
        seed: None,
        scores: vec![],
        default_tab: EndScreenTab::Cards,
    };
    let n = Names;
    let (l, w) = (&no_lang, &width6);
    let h = history(&HistoryView {
        width: 200,
        game_over: Some(&game_over),
        state_has_game_over: true,
        status_lines: vec![],
        messages: &[],
        items: &n,
        lang: l,
        text_width: w,
    });
    // the head takes two spaces (12 px) from x 14, the name follows in yellow, then the rest in white
    let sprite = h.draws.iter().find_map(|d| match d {
        ScreenDraw::Plain(Draw::Sprite { name, x, y, .. }) if name.starts_with("face/") => Some((name.clone(), *x, *y)),
        _ => None,
    });
    assert_eq!(sprite, Some(("face/00000000000000000000000000000005".to_string(), 14.0, 50.0)));
    let texts: Vec<(i32, String, u32)> = h
        .draws
        .iter()
        .filter_map(|d| match d {
            ScreenDraw::Plain(Draw::Text { x, text, argb, .. }) => Some((*x, text.clone(), *argb)),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec![(26, "Bob".to_string(), 0xffff_ff55), (44, " won".to_string(), 0xffff_ffff)]);
}

#[test]
fn the_winners_card_carries_a_turned_label_in_the_cycling_colour_and_the_others_do_not() {
    let c = card();
    let col = colors();
    let slots = [
        CardSlot { team: Some("red".into()), card: &c, colors: &col, team_name: None },
        CardSlot { team: Some("blue".into()), card: &c, colors: &col, team_name: None },
    ];
    let (o, n, w) = (no_outline, Names, width6);
    let lang = |k: &str| (k == "bingo.card.winner").then(|| "WINNER!".to_string());
    let input = |now_ms: u64, winner: Option<&'static str>| CardsInput {
        area: area(),
        slots: &slots,
        winner,
        entrance: None,
        elapsed_ms: 0,
        now_ms,
        opacity: 1.0,
        selectable: true,
        mouse: (-1, -1),
        show_outlines: false,
        team_outline: &o,
        items: &n,
        images: None,
        lang: &lang,
        text_width: &w,
    };
    let scroll = CardsScroll::starting_at(0.0);
    let none = frame(&input(0, None), &scroll);
    assert!(none.labels.is_empty());
    // at 2.5 s the third colour (`0xffa3f5c7`) shows; the label sits at (90, -4) of the winning card, turned by 0.25
    let won = frame(&input(2_500, Some("blue")), &scroll);
    assert_eq!(won.labels.len(), 1);
    match &won.labels[0] {
        ScreenDraw::Label { text, scale, rotation, argb, shadow, .. } => {
            assert_eq!((text.as_str(), *rotation, *argb, *shadow), ("WINNER!", 0.25, 0xffa3_f5c7, true));
            assert!(*scale > 0.0);
        }
        other => panic!("{other:?}"),
    }
    // the label is 90 px right of the winning (second) card's left edge and 4 above its top, in that card's scale
    let ScreenDraw::Label { anchor, scale, .. } = &won.labels[0] else { unreachable!() };
    let cards = sprites_named(&won.draws, &col.card_texture);
    assert_eq!(cards.len(), 2, "{:?}", won.draws);
    // exactly one of the two cards is the one the label belongs to (90 right of its edge, 4 above its top)
    let hits = cards
        .iter()
        .filter(|(x, y)| (anchor.0 - (x + 90.0 * scale)).abs() < 0.01 && (anchor.1 - (y - 4.0 * scale)).abs() < 0.01)
        .count();
    assert_eq!(hits, 1, "{anchor:?} {cards:?}");
}

// ---- the card spans, and the card sliding in from the HUD ------------------------------------------------------------

/// Two cards, A then B (the same empty card twice), in the 400 x 200 widget: the row is 244 px wide, so there is a
/// scroll bar after the cards. `entrance` is the team sliding in (150 ms), `elapsed_ms` the time into it.
fn ab_frame(entrance: Option<&str>, elapsed_ms: u64, winner: Option<&str>) -> CardsFrame {
    let c = card();
    let col = colors();
    let slots = [
        CardSlot { team: Some("A".into()), card: &c, colors: &col, team_name: None },
        CardSlot { team: Some("B".into()), card: &c, colors: &col, team_name: None },
    ];
    let (o, n, w) = (no_outline, Names, width6);
    let lang = |k: &str| (k == "bingo.card.winner").then(|| "WINNER!".to_string());
    let input = CardsInput {
        area: area(),
        slots: &slots,
        winner,
        entrance: entrance.map(|team| Entrance {
            team: Some(team.into()),
            from_x: 0.0,
            from_y: 0.0,
            from_scale: 1.0,
            duration_ms: 150,
        }),
        elapsed_ms,
        now_ms: 0,
        opacity: 1.0,
        selectable: true,
        mouse: (-1, -1),
        show_outlines: false,
        team_outline: &o,
        items: &n,
        images: None,
        lang: &lang,
        text_width: &w,
    };
    frame(&input, &CardsScroll::starting_at(0.0))
}

fn span_of<'a>(f: &'a CardsFrame, team: &str) -> &'a CardSpan {
    f.card_spans.iter().find(|s| s.team.as_deref() == Some(team)).expect("a span for the team")
}

#[test]
fn the_card_that_slid_in_stays_the_entrance_card_for_the_screens_whole_life() {
    // A slides in: 50 ms in, and at exactly 150 ms (the slide's last instant, `elapsed > duration` is done), it is A's
    // span that is marked; B is an ordinary card, so it never is.
    let f = ab_frame(Some("A"), 50, None);
    assert_eq!(f.card_spans.len(), 2);
    let i = f.entrance_card.expect("A is still sliding");
    assert_eq!(f.card_spans[i].team.as_deref(), Some("A"));
    assert_eq!(ab_frame(Some("A"), 150, None).entrance_card, f.entrance_card);
    // 151 ms and later: still marked, `interpolateKey` is never cleared (`BingoHudScreen.kt:123`).
    assert_eq!(ab_frame(Some("A"), 151, None).entrance_card, f.entrance_card);
    assert_eq!(ab_frame(Some("A"), 60_000, None).entrance_card, f.entrance_card);
    // Without an entrance (the end of a game), or for a team with no card, none is marked either.
    assert_eq!(ab_frame(None, 50, None).entrance_card, None);
    assert_eq!(ab_frame(Some("C"), 50, None).entrance_card, None);
}

#[test]
fn each_card_owns_its_draws_and_its_label_and_the_scroll_bar_is_in_neither() {
    let f = ab_frame(Some("A"), 50, Some("B"));
    let a = span_of(&f, "A").clone();
    let b = span_of(&f, "B").clone();
    // The two cards are painted one after another from the start; the scroll bar comes after them.
    let (first, second) = if a.draws.0 == 0 { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
    assert_eq!(first.draws.0, 0);
    assert_eq!(first.draws.1, second.draws.0, "no gap between the two cards");
    assert!(second.draws.1 < f.draws.len(), "the scroll bar is drawn after the cards: {:?}", f.card_spans);
    // The same card twice draws the same number of commands.
    assert_eq!(a.draws.1 - a.draws.0, b.draws.1 - b.draws.0);
    // The winner's label is B's alone, and it sits in B's label range.
    assert_eq!((a.labels.1 - a.labels.0, b.labels.1 - b.labels.0), (0, 1));
    assert_eq!(b.labels, (0, 1));
    // The label of the entrance card is in the entrance card's own span when that card wins.
    let g = ab_frame(Some("A"), 50, Some("A"));
    let (ga, gb) = (span_of(&g, "A").labels, span_of(&g, "B").labels);
    assert_eq!(ga.1 - ga.0, 1, "A's label is in A's span");
    assert_eq!(gb.0, gb.1, "B has no label: {gb:?}");
}

// ---- the winner's label is painted with its card, so a nearer card covers it ----------------------------------------

/// Three cards A, B, C in the 400 x 200 widget, scrolled to the start. The area is 240 px wide and the row 366 px, so
/// the row is 63 px past its centre and the right-hand cards are held at the edge (x 198): B is at x 202 and C at 206,
/// so B covers C from x 206 on. The distances from the middle (x 200) are A 59, B 63 and C 185: C is painted first and
/// A, the nearest, last. The cards are at y 34 (the area's top 100 less half the card's height 132).
fn row3(winner: Option<&str>, entrance: Option<&str>) -> CardsFrame {
    let c = card();
    let col = colors();
    let slots = [
        CardSlot { team: Some("A".into()), card: &c, colors: &col, team_name: None },
        CardSlot { team: Some("B".into()), card: &c, colors: &col, team_name: None },
        CardSlot { team: Some("C".into()), card: &c, colors: &col, team_name: None },
    ];
    let (o, n, w) = (no_outline, Names, width6);
    let lang = |k: &str| (k == "bingo.card.winner").then(|| "WINNER!".to_string());
    let input = CardsInput {
        area: area(),
        slots: &slots,
        winner,
        entrance: entrance.map(|team| Entrance {
            team: Some(team.into()),
            from_x: 0.0,
            from_y: 0.0,
            from_scale: 1.0,
            duration_ms: 150,
        }),
        elapsed_ms: 50,
        now_ms: 0,
        opacity: 1.0,
        selectable: true,
        mouse: (-1, -1),
        show_outlines: false,
        team_outline: &o,
        items: &n,
        images: None,
        lang: &lang,
        text_width: &w,
    };
    frame(&input, &CardsScroll::starting_at(0.0))
}

/// The card pictures among `cmds`, by their top left corner.
fn pictures(cmds: &[UiDraw]) -> Vec<(f32, f32)> {
    cmds.iter()
        .filter_map(|c| match c {
            UiDraw::Sprite { name, x, y, .. } if name == "bingo/card_preview" => Some((*x, *y)),
            _ => None,
        })
        .collect()
}

fn label_at(cmds: &[UiDraw]) -> Option<usize> {
    cmds.iter().position(|c| matches!(c, UiDraw::TextXf { text, .. } if text == "WINNER!"))
}

#[test]
fn the_winners_label_is_painted_right_after_its_card_so_a_nearer_card_covers_it() {
    let f = row3(Some("C"), None);
    let teams: Vec<&str> = f.card_spans.iter().map(|s| s.team.as_deref().unwrap()).collect();
    assert_eq!(teams, ["C", "B", "A"], "farthest from the middle first");
    assert_eq!(sprites_named(&f.draws, "bingo/card_preview"), vec![(206.0, 34.0), (202.0, 34.0), (80.0, 34.0)]);

    let (cmds, _) = card_commands(&f);
    // C's draws, then C's label, then B's and A's draws: the label is at the index C's draws end.
    let at = label_at(&cmds).expect("the label is drawn");
    assert_eq!(at, f.card_spans[0].draws.1 - f.card_spans[0].draws.0, "the label follows C's draws directly");
    assert!(matches!(&cmds[at], UiDraw::TextXf { anchor, scale, rotation, .. } if *anchor == (296.0, 30.0) && *scale == 1.0 && *rotation == 0.25));
    // Before the label only C's picture; after it B's and then A's, so B (nearer) is painted over C's label.
    assert_eq!(pictures(&cmds[..at]), vec![(206.0, 34.0)]);
    assert_eq!(pictures(&cmds[at + 1..]), vec![(202.0, 34.0), (80.0, 34.0)]);

    // The whole order: each card's draws in paint order, the label within C's, the scroll bar last.
    let s = &f.card_spans;
    let mut expected: Vec<UiDraw> = Vec::new();
    expected.extend(f.draws[s[0].draws.0..s[0].draws.1].iter().map(UiDraw::from));
    expected.extend(f.labels.iter().map(UiDraw::from));
    expected.extend(f.draws[s[1].draws.0..s[1].draws.1].iter().map(UiDraw::from));
    expected.extend(f.draws[s[2].draws.0..s[2].draws.1].iter().map(UiDraw::from));
    expected.extend(f.draws[s[2].draws.1..].iter().map(UiDraw::from));
    assert_eq!(cmds, expected);
    assert!(f.draws.len() > s[2].draws.1, "the scroll bar is drawn after the cards");
}

#[test]
fn with_no_winner_the_commands_are_the_draws_in_the_frames_own_order() {
    let f = row3(None, None);
    assert!(f.labels.is_empty());
    let (cmds, faded) = card_commands(&f);
    assert_eq!(cmds, f.draws.iter().map(UiDraw::from).collect::<Vec<_>>());
    assert_eq!(faded, vec![(0, cmds.len())], "without an entrance the whole list fades");
    // With a winner, the other commands are the same ones in the same order: only the label is added.
    let won = row3(Some("C"), None);
    let (won_cmds, _) = card_commands(&won);
    let without_label: Vec<UiDraw> = won_cmds.into_iter().filter(|c| !matches!(c, UiDraw::TextXf { .. })).collect();
    assert_eq!(without_label, cmds);
}

#[test]
fn the_fade_takes_each_card_with_its_label_and_the_entrance_card_keeps_both() {
    // C wins and slides in: C's draws and its label are the entrance hole, and every other command fades.
    let f = row3(Some("C"), Some("C"));
    assert_eq!(f.entrance_card, Some(0));
    let (cmds, faded) = card_commands(&f);
    let c_draws = f.card_spans[0].draws.1;
    assert_eq!(label_at(&cmds), Some(c_draws));
    assert_eq!(faded, vec![(c_draws + 1, cmds.len())]);

    // C wins but A slides in: A (painted last) is the hole; C's label is faded with C.
    let g = row3(Some("C"), Some("A"));
    assert_eq!(g.entrance_card, Some(2));
    let (cmds, faded) = card_commands(&g);
    // In the command list C's label comes before A, so A's draws are one place later than in the frame.
    let a = g.card_spans[2].draws;
    assert_eq!(faded, vec![(0, a.0 + 1), (a.1 + 1, cmds.len())]);
    let c_label = label_at(&cmds).unwrap();
    assert!(faded.iter().any(|&(s, e)| s <= c_label && c_label < e), "C's label fades with C");

    // A wins and slides in: A's label is in A's own hole, not faded.
    let h = row3(Some("A"), Some("A"));
    let (cmds, faded) = card_commands(&h);
    let a_label = label_at(&cmds).unwrap();
    // A's draws are the commands before its label, and its hole is those plus the label.
    let a_draws = h.card_spans[2].draws.1 - h.card_spans[2].draws.0;
    assert_eq!(faded, vec![(0, a_label - a_draws), (a_label + 1, cmds.len())]);
}

#[test]
fn a_200000_character_word_wraps_in_under_a_second_into_the_lines_its_widths_give() {
    // 6 px a character and 168 px a line: 28 characters a line. 200000 = 28 * 7142 + 24, so 7142 full lines of 168 px
    // and a last one of 24 characters, 144 px.
    let word = "x".repeat(200_000);
    let started = std::time::Instant::now();
    let lines = wrap_lines(&word, 168, &width6);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(1), "wrapping took {took:?}");
    assert_eq!(lines.len(), 7143);
    assert!(lines[..7142].iter().all(|l| width6(l) == 168));
    assert_eq!(width6(&lines[7142]), 144);
    assert_eq!(lines.concat(), word, "nothing is lost or repeated");
}

#[test]
fn an_8000_character_message_wraps_into_286_lines_within_a_second() {
    // The history wraps at 196 - 28 = 168 px: 28 characters a line, so 8000 = 28 * 285 + 20 gives 286 lines, the last
    // 20 characters (120 px). 8000 is under every limit `runs` may set on plain text, so the count does not depend on it.
    let message = ScoreMessage { created_at: 0, packet: packet(&"a".repeat(8_000)) };
    let started = std::time::Instant::now();
    let wrapped = wrap_message(&message, 196, &no_lang, &width6);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(1), "wrapping took {took:?}");
    assert_eq!(wrapped.lines.len(), 286);
    assert!(wrapped.lines[..285].iter().all(|l| width6(l) == 168));
    assert_eq!(width6(&wrapped.lines[285]), 120);
    let run_text: Vec<String> =
        wrapped.runs.iter().map(|line| line.iter().map(|r| r.text.as_str()).collect()).collect();
    assert_eq!(run_text, wrapped.lines, "each line's runs are the line");
}

#[test]
fn a_word_cut_by_characters_takes_whole_characters_however_many_bytes_they_take() {
    // "é" is two bytes and 6 px: 28 a line at 168 px, so 50 of them are 28 and then 22.
    let word = "é".repeat(50);
    assert_eq!(wrap_lines(&word, 168, &width6), vec!["é".repeat(28), "é".repeat(22)]);
}

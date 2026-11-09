//! The ready line against the numbers worked out by hand from `ReadyHudRenderer.kt` and `BingoHudController.drawHudInfo`.
//!
//! Setup used throughout: a 200 px wide GUI, so the line's left edge is 200 / 2 - 50 = 50 and its centre is x = 100;
//! top at y = 40; font height 9. The fake font is 6 px per ASCII character, the tick (U+2714) is 7 px, the dot
//! (U+2022) is 3 px. A player mark is its glyph plus 2 px, so a tick has a mark width of 9 and a dot of 5.
use horizon_bingo::card_draw::Draw;
use horizon_bingo::game::{GameState, GameStatus};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::ready_draw::draw_ready;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::text::Text;

const TICK: char = '\u{2714}';
const DOT: char = '\u{2022}';

fn width(s: &str) -> i32 {
    s.chars()
        .map(|c| match c {
            TICK => 7,
            DOT => 3,
            _ => 6,
        })
        .sum()
}

fn no_lang(_: &str) -> Option<String> {
    None
}

#[allow(clippy::too_many_arguments)]
fn packet(
    running: bool,
    ready: bool,
    title: Option<&str>,
    subtitle: Option<&str>,
    remaining: i64,
    total: i64,
    ready_players: i32,
    total_players: i32,
) -> ReadyUpdatePacket {
    ReadyUpdatePacket {
        is_running: running,
        is_ready: ready,
        state: GameState { identifier: "pregame".into(), status: GameStatus::Pregame },
        remaining_millis: remaining,
        total_millis: total,
        ready_players,
        total_players,
        title: title.map(Text::plain),
        subtitle: subtitle.map(Text::plain),
        can_send_ready: true,
        vote_options: Vec::new(),
        vote_pending: None,
    }
}

fn texts(d: &[Draw]) -> Vec<(i32, i32, String, u32)> {
    d.iter()
        .filter_map(|c| match c {
            Draw::Text { x, y, text, argb } => Some((*x, *y, text.clone(), *argb)),
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

/// The bar's outline for a fill that ends at `x_progress` (the test layout: bar at y 63, 5 px high, x 50 to 150).
fn bar_at(x_progress: i32) -> Vec<(i32, i32, i32, i32, u32)> {
    vec![
        (50, 63, x_progress - 50, 6, 0xffffffff),
        (x_progress, 63, 150 - x_progress + 1, 1, 0xffffffff),
        (x_progress, 68, 150 - x_progress + 1, 1, 0xffffffff),
        (150, 63, 1, 6, 0xffffffff),
    ]
}

#[test]
fn nothing_is_drawn_without_a_packet_or_while_the_timer_is_not_running() {
    let mut smoothed = 0.0;
    assert!(draw_ready(None, 0, 200, &no_lang, &width, &mut smoothed).is_empty());
    let idle = packet(false, false, Some("Ready"), Some("Waiting"), 60_000, 60_000, 0, 4);
    assert!(draw_ready(Some(&idle), 0, 200, &no_lang, &width, &mut smoothed).is_empty());
    assert_eq!(smoothed, 0.0, "a line that is not drawn does not move the bar");
}

#[test]
fn a_full_timer_draws_the_title_centred_at_the_top_then_the_empty_bar() {
    // Title "Ready" is 30 px: x = 50 + 50 - 15 = 85. Remaining equals total, so nothing has elapsed: the bar is
    // filled to 0.2 * 0 = 0, so only its one-pixel leading edge reaches x = 51.
    let p = packet(true, false, Some("Ready"), None, 60_000, 60_000, 0, 0);
    let mut smoothed = 0.0;
    let d = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(texts(&d), vec![(85, 40, "Ready".to_string(), 0x99ffffff)]);
    assert_eq!(fills(&d), bar_at(51));
}

#[test]
fn a_ready_player_sees_a_solid_white_title_at_any_time() {
    let p = packet(true, true, Some("Ready"), None, 60_000, 60_000, 0, 0);
    for now in [0, 250, 750] {
        let mut smoothed = 0.0;
        let d = draw_ready(Some(&p), now, 200, &no_lang, &width, &mut smoothed);
        assert_eq!(texts(&d)[0].3, 0xffffffff, "now {now}");
    }
}

#[test]
fn the_unready_title_pulses_once_a_second_from_0x99_to_0x32_alpha() {
    // alpha = sin(phase * 2 PI) * 0.4 + 0.6 with phase = the sub-second part of the clock; alpha * 255 is truncated.
    // phase 0: 0.6 -> 153 (0x99). phase 0.25: sin = 1 -> 1.0 -> 255 (0xff). phase 0.75: sin = -1 -> 0.2 (0.19999...
    // in doubles), 255 * that is 50.999... and truncates to 50 (0x32).
    let p = packet(true, false, Some("Ready"), None, 60_000, 60_000, 0, 0);
    let title_at = |now: u64| {
        let mut smoothed = 0.0;
        texts(&draw_ready(Some(&p), now, 200, &no_lang, &width, &mut smoothed))[0].3
    };
    assert_eq!(title_at(0), 0x99ffffff);
    assert_eq!(title_at(250), 0xffffffff);
    assert_eq!(title_at(750), 0x32ffffff);
    // Only the sub-second part matters: the pulse repeats every 1000 ms.
    assert_eq!(title_at(1250), 0xffffffff);
    assert_eq!(title_at(1750), 0x32ffffff);
}

#[test]
fn the_subtitle_is_mint_and_centred_nine_plus_two_pixels_below_the_title() {
    // "Waiting" is 42 px: x = 100 - 21 = 79; y = 40 + 9 + 2 = 51.
    let p = packet(true, false, None, Some("Waiting"), 60_000, 60_000, 0, 0);
    let mut smoothed = 0.0;
    let d = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(texts(&d), vec![(79, 51, "Waiting".to_string(), 0xffa3f5a3)]);
}

#[test]
fn the_bar_eases_towards_the_elapsed_share_at_one_fifth_per_frame() {
    // Half the time gone: target 0.5. Frame 1: 0.8 * 0 + 0.2 * 0.5 = 0.1, the edge at 50 + round(99 * 0.1) + 1 = 61.
    // Frame 2: 0.8 * 0.1 + 0.2 * 0.5 = 0.18, the edge at 50 + round(99 * 0.18) + 1 = 50 + round(17.82) + 1 = 69.
    let p = packet(true, false, None, None, 30_000, 60_000, 0, 0);
    let mut smoothed = 0.0;
    let first = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(fills(&first), bar_at(61));
    assert!((smoothed - 0.1).abs() < 1e-6);
    let second = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(fills(&second), bar_at(69));
    assert!((smoothed - 0.18).abs() < 1e-6);
}

#[test]
fn a_timer_shorter_than_a_second_counts_as_one_second() {
    // Total 0 is coerced to 1000 ms; 500 ms remaining is then half gone, the same first frame as a 60 s timer above.
    let p = packet(true, false, None, None, 500, 0, 0, 0);
    let mut smoothed = 0.0;
    let d = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(fills(&d), bar_at(61));
}

#[test]
fn the_elapsed_share_is_clamped_to_the_bar() {
    // Remaining -5000 of 60000 is 1.0 elapsed after clamping: target 1, first frame 0.2, edge 50 + round(19.8) + 1 = 71.
    // A NaN left over from before is read as 0 (`takeIf { isFinite } ?: 0`), so the result is the same.
    let p = packet(true, false, None, None, -5_000, 60_000, 0, 0);
    let mut smoothed = f32::NAN;
    let d = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(fills(&d), bar_at(71));
    assert!((smoothed - 0.2).abs() < 1e-6);
}

#[test]
fn players_are_marked_as_ticks_when_ready_and_dots_otherwise() {
    // 4 players, 2 ready. Slot i is ready when i * 4 < 4 * 2, so i = 0 and 1. The row is 4 marks of 7 px:
    // left edge 100 - 28 / 2 = 86. A tick (mark width 9) sits (7 - 9) / 2 = -1 px in, a dot (mark width 5) +1 px in.
    // Marks at y = 40 + 18 + 12 = 70: tick 85, tick 92, dot 101, dot 108.
    let p = packet(true, false, None, None, 60_000, 60_000, 2, 4);
    let mut smoothed = 0.0;
    let d = draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed);
    assert_eq!(
        texts(&d),
        vec![
            (85, 70, TICK.to_string(), 0xffa3f5a3),
            (92, 70, TICK.to_string(), 0xffa3f5a3),
            (101, 70, DOT.to_string(), 0xffffffff),
            (108, 70, DOT.to_string(), 0xffffffff),
        ]
    );
}

#[test]
fn at_most_ten_marks_are_drawn() {
    // 25 players: 10 marks, 70 px wide, left edge 100 - 35 = 65, and all dots (0 ready). Dot i sits at 65 + 7 i + 1.
    let p = packet(true, false, None, None, 60_000, 60_000, 0, 25);
    let mut smoothed = 0.0;
    let marks: Vec<(i32, i32, String, u32)> = texts(&draw_ready(Some(&p), 0, 200, &no_lang, &width, &mut smoothed))
        .into_iter()
        .filter(|t| t.2 == DOT.to_string())
        .collect();
    assert_eq!(marks.len(), 10);
    assert_eq!(marks.first().map(|t| t.0), Some(66));
    assert_eq!(marks.last().map(|t| t.0), Some(66 + 9 * 7));
}

#[test]
fn a_translated_title_is_flattened_with_its_arguments() {
    // The key "bingo.ready.count" is "Ready %s/%s" with arguments 2 and 4: "Ready 2/4" is 54 px, x = 100 - 27 = 73.
    let lang = |key: &str| (key == "bingo.ready.count").then(|| "Ready %s/%s".to_string());
    let title = Text(Nbt::compound(vec![
        ("translate", Nbt::string("bingo.ready.count")),
        ("with", Nbt::list(vec![Nbt::string("2"), Nbt::string("4")])),
    ]));
    let mut p = packet(true, true, None, None, 60_000, 60_000, 0, 0);
    p.title = Some(title);
    let mut smoothed = 0.0;
    let d = draw_ready(Some(&p), 0, 200, &lang, &width, &mut smoothed);
    assert_eq!(texts(&d), vec![(73, 40, "Ready 2/4".to_string(), 0xffffffff)]);
}

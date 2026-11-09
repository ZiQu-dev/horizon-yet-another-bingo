//! The end-of-game scores list and the game info panel, against numbers worked out by hand from
//! BingoEndScoresWidget.kt and GameInfoWidget.kt. The text width is 6 px a character, so every width below is the
//! character count times 6, and the expected columns come from the Kotlin arithmetic written out in each test.
use horizon_bingo::display_cards::PlayerProfile;
use horizon_bingo::game_over::{EndScreenTab, GameOverPacket, ScoreRanking};
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};
use horizon_bingo::score_update::TeamScore;
use horizon_bingo::screen_end::*;
use horizon_bingo::state::{Cards, ClientCard};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;

const GRAY: u32 = 0xFFAAAAAA;
const YELLOW: u32 = 0xFFFFFF55;
const WHITE: u32 = 0xFFFFFFFF;
const RED: u32 = 0xFFFF5555;
const BLUE: u32 = 0xFF5555FF;
const GREEN: u32 = 0xFF00FF00;

/// en_us.json, for the keys these widgets read (a missing key is `None`, as the translation lookup is).
fn lang(key: &str) -> Option<String> {
    let v = match key {
        "bingo.game.card_count" => "%s cards",
        "bingo.game.card_count_1" => "1 card",
        "bingo.game.line_count" => "%s lines",
        "bingo.game.item_count" => "%s items",
        "bingo.game.item_count_1" => "1 item",
        "bingo.game.item_count_5" => "5 items",
        "bingo.game.end.scores.team" => "Team",
        "bingo.game.end.scores.score" => "Score",
        "bingo.game.end.scores.time" => "Time",
        "bingo.game.end.scores.dnf" => "DNF",
        "bingo.game.end.tab.cards" => "Cards",
        "bingo.game.end.tab.scores" => "Scores",
        "bingo.scoreboard.time_left" => "Time left: %s",
        "bingo.scoreboard.time" => "Time: %s",
        "bingo.game.waiting_to_start" => "Waiting to start...",
        _ => return None,
    };
    Some(v.to_string())
}

/// Six pixels a character.
fn width(s: &str) -> i32 {
    s.chars().count() as i32 * 6
}

fn ranking(index: i32, key: &str, name: &str, cards: i32, lines: i32, items: i32, duration: Option<&str>) -> ScoreRanking {
    ScoreRanking {
        index,
        key: key.to_string(),
        name: Text::plain(name),
        score: TeamScore { items, lines, cards },
        duration: duration.map(str::to_string),
    }
}

fn packet(scores: Vec<ScoreRanking>) -> GameOverPacket {
    GameOverPacket { ended_at_millis: 1_000_000, scores, ..GameOverPacket::default() }
}

/// A team's card with these players (uuid, name).
fn team_card(players: &[(u128, &str)]) -> ClientCard {
    let mut card = ClientCard::new_for_tests();
    card.display.players = players.iter().map(|(uuid, name)| PlayerProfile { uuid: *uuid, name: name.to_string() }).collect();
    card
}

fn layout_of(p: &GameOverPacket, cards: &Cards, width_px: i32) -> ScoresLayout {
    let width_of: &dyn Fn(&str) -> i32 = &width;
    layout(&ScoresInput { game_over: p, cards, lang: &lang, text_width: width_of, width: width_px, padding: 4 })
}

fn t(x: i32, y: i32, text: &str, argb: u32, obfuscated: bool) -> ScreenDraw {
    ScreenDraw::Text { x, y, text: text.to_string(), argb, shadow: true, obfuscated }
}

/// Two teams: Red with 1 card and 5 items, 1:30; Blue with 3 items and no time (DNF).
fn two_teams() -> GameOverPacket {
    packet(vec![
        ranking(0, "bingo_red", "Red", 1, 0, 5, Some("PT1M30S")),
        ranking(1, "bingo_blue", "Blue", 0, 0, 3, None),
    ])
}

#[test]
fn columns_come_from_the_widths_of_the_longest_texts() {
    // Content width 200 - 2*4 = 192. Team texts "#1 Red" (36) and "#2 Blue" (42), title "Team" (24): team max 42.
    // Score texts "1 card, 5 items" (90) and "3 items" (42), title "Score" (30): score max 90.
    // Durations "1:30" (24), "DNF" (18), title "Time" (24): duration max 24.
    // duration_col = 192 - 24 - 4 = 164. score_max_x = 164 - 90 - 4 = 70. score_min_x = 4 + 42 = 46.
    // score_col = (46 + 70) / 2 = 58, capped at 70: 58.
    let l = layout_of(&two_teams(), &Cards::default(), 200);
    assert_eq!(l.score_col, 58);
    assert_eq!(l.duration_col, 164);
    assert_eq!(l.content_height(), 17 * 3);

    // 1.5 s after the end: row 0 is revealed (1 s passed), row 1 is not (2 s needed).
    let d = l.draw(1_000_000 + 1500, 0, 0);
    assert_eq!(
        d,
        vec![
            t(4, 4, "Team", GRAY, false),
            t(58, 4, "Score", GRAY, false),
            t(164, 4, "Time", GRAY, false),
            t(4, 21, "#1 Red", WHITE, false),
            t(58, 21, "1 card, 5 items", WHITE, false),
            t(164, 21, "1:30", WHITE, false),
            t(4, 38, "#2 Blue", WHITE, true),
            t(58, 38, "3 items", WHITE, true),
            t(164, 38, "DNF", WHITE, true),
        ]
    );
}

#[test]
fn row_n_is_revealed_once_n_plus_one_whole_seconds_have_passed() {
    let l = layout_of(&two_teams(), &Cards::default(), 200);
    let obfuscated_rows = |now_offset_ms: i64| -> (bool, bool) {
        let d = l.draw(1_000_000 + now_offset_ms, 0, 0);
        let flag = |y: i32| d.iter().any(|c| matches!(c, ScreenDraw::Text { y: yy, obfuscated: true, .. } if *yy == y));
        (flag(21), flag(38))
    };
    assert_eq!(obfuscated_rows(999), (true, true), "0.999 s: both rows still scrambled");
    assert_eq!(obfuscated_rows(1000), (false, true), "exactly 1 s: row 0 shows");
    assert_eq!(obfuscated_rows(1999), (false, true));
    assert_eq!(obfuscated_rows(2000), (false, false), "2 s: row 1 shows too");
    assert_eq!(obfuscated_rows(-1), (true, true), "before the end (floor of -0.001 s is -1 s): all scrambled");
}

#[test]
fn score_text_lists_cards_lines_and_items_and_says_zero_for_nothing() {
    assert_eq!(score_text(&lang, 0, 0, 0), "0");
    assert_eq!(score_text(&lang, 1, 0, 5), "1 card, 5 items");
    // "2 cards" and "2 lines" have no _2 key here, so the plural template is filled with the number.
    assert_eq!(score_text(&lang, 2, 2, 0), "2 cards, 2 lines");
    // Ten or more never use a numbered key: "10 items".
    assert_eq!(score_text(&lang, 0, 0, 10), "10 items");
}

#[test]
fn hhmmss_drops_the_hours_when_there_are_none_and_keeps_the_sign() {
    let fmt = |s: &str| format_hhmmss(parse_java_duration(s).expect("parses"));
    assert_eq!(fmt("PT1H2M3S"), "1:02:03");
    assert_eq!(fmt("PT59M59S"), "59:59");
    assert_eq!(fmt("PT1H"), "1:00:00");
    assert_eq!(fmt("PT90M"), "1:30:00", "minutes past an hour roll into hours");
    assert_eq!(fmt("PT0S"), "0:00");
    assert_eq!(fmt("PT0.999S"), "0:00", "a fraction below a second is dropped");
    assert_eq!(fmt("PT-0.5S"), "-0:00", "a negative duration keeps its sign");
    assert_eq!(fmt("PT-1M-30S"), "-1:30");
}

#[test]
fn parse_accepts_only_the_duration_to_string_form() {
    assert_eq!(parse_java_duration("PT1M30S"), Some(90_000_000_000));
    assert_eq!(parse_java_duration("PT-1.5S"), Some(-1_500_000_000));
    assert_eq!(parse_java_duration("1M30S"), None, "no P");
    assert_eq!(parse_java_duration("PT5X"), None, "unknown unit");
}

#[test]
fn format_string_is_the_long_form_with_days_and_hours_only_when_present() {
    // 41.893 s: no days, no hours, the minutes and seconds padded.
    assert_eq!(format_string(nanos_of(JavaDuration { seconds: 41, nanos: 893_000_000 })), "00m 41s");
    // 74 h 1 m 32 s = 3 d 2 h 1 m 32 s.
    assert_eq!(format_string(parse_java_duration("PT74H1M32S").unwrap()), "3d 2h 01m 32s");
    // 90 minutes: hours 1, minutes 30, no days.
    assert_eq!(format_string(parse_java_duration("PT1H30M").unwrap()), "1h 30m 00s");
    // Negative: the sign, then the rest of the form (-1.5 s is 1 whole second).
    assert_eq!(format_string(-1_500_000_000), "-00m 01s");
}

#[test]
fn a_team_with_a_card_shows_its_players_and_more_than_one_player_adds_the_team_name() {
    let mut cards = Cards::default();
    cards.insert(Some("bingo_red".into()), team_card(&[(1, "Ann"), (2, "Bob")]));
    cards.insert(Some("bingo_blue".into()), team_card(&[(3, "Cy")]));
    let l = layout_of(&two_teams(), &cards, 200);

    // Red has two players: the team name comes first, then each player with its head.
    assert_eq!(
        l.rows[0].tooltip,
        Some(vec![
            TooltipLine { head: None, text: "Red".into() },
            TooltipLine { head: Some(1), text: "Ann".into() },
            TooltipLine { head: Some(2), text: "Bob".into() },
        ])
    );
    // Blue has one: no team-name line.
    assert_eq!(l.rows[1].tooltip, Some(vec![TooltipLine { head: Some(3), text: "Cy".into() }]));

    // Hovering the Red team cell (x 4..=36, y 17..=34) shows Red's list at the mouse.
    let d = l.draw(0, 10, 20);
    assert!(d.contains(&ScreenDraw::Tooltip { x: 10, y: 20, lines: l.rows[0].tooltip.clone().unwrap() }));
    // Below Red's row band (y 35 is past 34) and inside Blue's (34..=51): Blue's list.
    let d = l.draw(0, 10, 35);
    assert!(d.contains(&ScreenDraw::Tooltip { x: 10, y: 35, lines: l.rows[1].tooltip.clone().unwrap() }));
    assert!(!d.iter().any(|c| matches!(c, ScreenDraw::Tooltip { lines, .. } if lines.len() == 3)));
}

#[test]
fn no_card_means_no_tooltip_and_no_hit_region() {
    let l = layout_of(&two_teams(), &Cards::default(), 200);
    assert!(l.rows.iter().all(|r| r.tooltip.is_none()));
    assert!(l.hits().is_empty());
    let d = l.draw(0, 10, 20);
    assert!(!d.iter().any(|c| matches!(c, ScreenDraw::Tooltip { .. })));
}

#[test]
fn the_hover_range_includes_both_ends_and_the_hit_region_matches_it() {
    let mut cards = Cards::default();
    cards.insert(Some("bingo_red".into()), team_card(&[(1, "Ann")]));
    let l = layout_of(&two_teams(), &cards, 200);
    // Red's text is 36 px wide, so the range is x 4..=36 (the original compares with the width itself), and the
    // row is y 17..=34 (its top to top + 17, both inclusive).
    let hit = |x: i32, y: i32| l.draw(0, x, y).iter().any(|c| matches!(c, ScreenDraw::Tooltip { .. }));
    assert!(hit(36, 17));
    assert!(hit(4, 34));
    assert!(!hit(37, 20), "one px past the text's width");
    assert!(!hit(10, 16), "one px above the row");
    assert_eq!(
        l.hits(),
        vec![Hit { x: 4, y: 17, w: 33, h: 18, action: Action::TeamPlayers { row: 0 } }],
        "x 4..=36 is 33 px wide, y 17..=34 is 18 px tall"
    );
}

#[test]
fn a_long_team_name_is_cut_to_the_space_before_the_score_column() {
    // "#1 Crimson Dynamo" is 17 characters: 102 px. "1 card, 5 items" is 90 px, so score_max_x = 164 - 90 - 4 = 70,
    // score_min_x = 4 + 102 = 106, score_col = min((106 + 70) / 2, 70) = 70. The team gets 70 - 4 = 66 px: 11 characters.
    let p = packet(vec![ranking(0, "bingo_red", "Crimson Dynamo", 1, 0, 5, Some("PT5S"))]);
    let l = layout_of(&p, &Cards::default(), 200);
    assert_eq!(l.score_col, 70);
    assert_eq!(l.rows[0].team, "#1 Crimson ");
    assert_eq!(l.rows[0].team_width, 66);
    assert_eq!(l.rows[0].duration, "0:05");
}

#[test]
fn the_game_info_shows_the_remaining_time_over_the_elapsed_one_and_no_waiting_line() {
    let status = GameStatusPacket {
        ingame_duration: Some(JavaDuration { seconds: 41, nanos: 0 }),
        remaining_duration: Some(JavaDuration { seconds: 90, nanos: 0 }),
    };
    let lines = status_lines(Some(&status), &lang);
    let info = GameInfo::new(lines, 122, &width);
    // "Time left: 01m 30s" is 18 characters, 108 px: centred in 122 px, x = 7. "Time left: " is 66 px.
    assert_eq!(info.height, 11 + 16);
    assert_eq!(
        info.draw(),
        vec![
            ScreenDraw::NineSlice { name: "bingo/game_container".into(), x: 0, y: 0, w: 122, h: 27, slice: 8, tex_w: 200, tex_h: 40 },
            ScreenDraw::Text { x: 7, y: 9, text: "Time left: ".into(), argb: WHITE, shadow: false, obfuscated: false },
            ScreenDraw::Text { x: 73, y: 9, text: "01m 30s".into(), argb: YELLOW, shadow: false, obfuscated: false },
        ]
    );
}

#[test]
fn the_game_info_waits_to_start_when_a_status_came_without_an_elapsed_time() {
    // A status that arrived with both durations empty is not the default: the waiting line shows.
    let status = GameStatusPacket::default();
    let lines = status_lines(Some(&status), &lang);
    assert_eq!(lines.len(), 1);
    let info = GameInfo::new(lines, 122, &width);
    // "Waiting to start..." is 19 characters, 114 px: x = (122 - 114) / 2 = 4, in grey.
    assert_eq!(info.draw()[1], ScreenDraw::Text { x: 4, y: 9, text: "Waiting to start...".into(), argb: GRAY, shadow: false, obfuscated: false });
    // No status at all (the default instance): no lines, only the frame of 16 px.
    let none = GameInfo::new(status_lines(None, &lang), 122, &width);
    assert_eq!(none.height, 16);
    assert_eq!(none.draw().len(), 1);
}

#[test]
fn the_elapsed_time_line_is_centred_with_its_value_in_yellow_and_two_lines_add_one_step() {
    let status = GameStatusPacket {
        ingame_duration: Some(JavaDuration { seconds: 41, nanos: 893_000_000 }),
        remaining_duration: None,
    };
    let info = GameInfo::new(status_lines(Some(&status), &lang), 122, &width);
    // "Time: 00m 41s": 13 characters, 78 px: x = (122 - 78) / 2 = 22. "Time: " is 36 px, so the value starts at 58.
    assert_eq!(info.draw()[1], ScreenDraw::Text { x: 22, y: 9, text: "Time: ".into(), argb: WHITE, shadow: false, obfuscated: false });
    assert_eq!(info.draw()[2], ScreenDraw::Text { x: 58, y: 9, text: "00m 41s".into(), argb: YELLOW, shadow: false, obfuscated: false });

    // A remaining time and no elapsed time: the time line and the waiting line, 2 * 11 + 16 = 38 px tall.
    let both = GameStatusPacket {
        ingame_duration: None,
        remaining_duration: Some(JavaDuration { seconds: 90, nanos: 0 }),
    };
    let two = GameInfo::new(status_lines(Some(&both), &lang), 122, &width);
    assert_eq!(two.height, 38);
    let second_line = two.draw().into_iter().find(|c| matches!(c, ScreenDraw::Text { y: 20, .. }));
    assert!(matches!(second_line, Some(ScreenDraw::Text { ref text, .. }) if text == "Waiting to start..."));
}

#[test]
fn the_tabs_show_when_several_teams_have_a_time_or_the_scores_tab_is_open_or_the_default() {
    let one = packet(vec![ranking(0, "bingo_red", "Red", 1, 0, 0, Some("PT1M"))]);
    let t1 = tabs(&one, EndScreenTab::Cards, &lang);
    assert!(!t1.shown, "one team, Cards open, Cards by default: no tab row");
    assert_eq!(t1.labels, ["Cards".to_string(), "Scores".to_string()]);
    assert_eq!(t1.current, 0);

    assert!(tabs(&two_teams(), EndScreenTab::Cards, &lang).shown, "two scored teams always show the row");
    let scores_default = tabs(&one, EndScreenTab::Scores, &lang);
    assert!(scores_default.shown);
    assert_eq!(scores_default.current, 1);
}

/// A text component `{text, color}`.
fn coloured(text: &str, color: &str) -> Nbt {
    Nbt::Compound(vec![
        ("text".to_string(), Nbt::String(text.to_string())),
        ("color".to_string(), Nbt::String(color.to_string())),
    ])
}

/// A team name made of these component parts, in order.
fn name_of(parts: Vec<Nbt>) -> Text {
    Text(Nbt::List { elem: 10, items: parts })
}

/// One scored team with a name, one card and a 5 s time: "1 card" (36 px), "0:05" (24 px).
fn named_team(name: Text, items: i32) -> GameOverPacket {
    packet(vec![ScoreRanking {
        index: 0,
        key: "bingo_red".to_string(),
        name,
        score: TeamScore { items, lines: 0, cards: 1 },
        duration: Some("PT5S".to_string()),
    }])
}

#[test]
fn a_team_name_is_drawn_run_by_run_in_the_colours_of_its_parts() {
    // Name: "Red" in red, then " Team" in #00FF00. The "#1 " is unstyled, so the row's white: runs "#1 " (18 px),
    // "Red" (18 px), " Team" (30 px). "#1 Red Team" is 66 px. Width 200, content 192: duration_max 24 (also "Time"),
    // duration_col 192 - 24 - 4 = 164. Score "1 card" 36 px: score_max_x 164 - 36 - 4 = 124, score_min_x 4 + 66 = 70,
    // score_col (70 + 124) / 2 = 97. The team has 97 - 4 = 93 px, so it is not cut.
    let name = name_of(vec![coloured("Red", "red"), coloured(" Team", "#00FF00")]);
    let p = named_team(name, 0);
    let l = layout_of(&p, &Cards::default(), 200);
    assert_eq!(l.score_col, 97);
    assert_eq!(
        l.rows[0].team_runs,
        vec![
            TeamRun { text: "#1 ".into(), argb: WHITE, head: None, width: 18 },
            TeamRun { text: "Red".into(), argb: RED, head: None, width: 18 },
            TeamRun { text: " Team".into(), argb: GREEN, head: None, width: 30 },
        ]
    );
    // 1.5 s after the end the row is shown: each run at the x its width puts it, the row's y 21.
    assert_eq!(
        l.draw(1_000_000 + 1500, 0, 0),
        vec![
            t(4, 4, "Team", GRAY, false),
            t(97, 4, "Score", GRAY, false),
            t(164, 4, "Time", GRAY, false),
            t(4, 21, "#1 ", WHITE, false),
            t(22, 21, "Red", RED, false),
            t(40, 21, " Team", GREEN, false),
            t(97, 21, "1 card", WHITE, false),
            t(164, 21, "0:05", WHITE, false),
        ]
    );
}

/// A player's head component (`{player: {id: [I; 4]}}`) for player `uuid`, as a team name holds it.
fn head_of(uuid: i32) -> Nbt {
    Nbt::Compound(vec![(
        "player".to_string(),
        Nbt::Compound(vec![("id".to_string(), Nbt::IntArray(vec![0, 0, 0, uuid]))]),
    )])
}

/// The 8 x 8 face of player `uuid` at (x, y), as the row draws it.
fn face(uuid: u128, x: i32, y: i32) -> ScreenDraw {
    ScreenDraw::Sprite { name: horizon_bingo::card_images::face_sprite(uuid), x, y, w: 8, h: 8 }
}

#[test]
fn a_player_head_in_a_team_name_is_an_8_by_8_face_where_its_two_spaces_are() {
    // Name: "Red" in red, a head (player 7), " X" in blue. Runs: "#1 " 18 px, "Red" 18 px, the head's "  " 12 px,
    // " X" 12 px: the name is 60 px. Score column: score_min_x 4 + 60 = 64, score_col (64 + 124) / 2 = 94.
    // "Red" starts at 22, the face at 22 + 18 = 40 (its top at the text's top, 21), " X" at 40 + 12 = 52.
    let name = name_of(vec![coloured("Red", "red"), head_of(7), coloured(" X", "blue")]);
    let p = named_team(name, 0);
    let l = layout_of(&p, &Cards::default(), 200);
    assert_eq!(l.score_col, 94);
    assert_eq!(l.rows[0].team, "#1 Red   X", "the head's two spaces are part of the text");
    assert_eq!(
        l.rows[0].team_runs,
        vec![
            TeamRun { text: "#1 ".into(), argb: WHITE, head: None, width: 18 },
            TeamRun { text: "Red".into(), argb: RED, head: None, width: 18 },
            TeamRun { text: "  ".into(), argb: WHITE, head: Some(7), width: 12 },
            TeamRun { text: " X".into(), argb: BLUE, head: None, width: 12 },
        ]
    );
    let row: Vec<ScreenDraw> = l
        .draw(1_000_000 + 1500, 0, 0)
        .into_iter()
        .filter(|c| matches!(c, ScreenDraw::Text { y: 21, .. } | ScreenDraw::Sprite { y: 21, .. }))
        .collect();
    assert_eq!(
        row,
        vec![
            t(4, 21, "#1 ", WHITE, false),
            t(22, 21, "Red", RED, false),
            face(7, 40, 21),
            t(52, 21, " X", BLUE, false),
            t(94, 21, "1 card", WHITE, false),
            t(164, 21, "0:05", WHITE, false),
        ]
    );
}

#[test]
fn a_head_split_by_the_cut_of_a_long_team_name_is_not_drawn_and_nothing_after_it_is() {
    // "#1 Crimson" (red), a head (player 7), " Dynamo" (blue): 3 + 7 + 2 + 7 = 19 characters. Score column 70 (as the
    // cut test below), so the name keeps 66 px: 11 characters, "#1 Crimson " by the cut. The head's two spaces are the
    // 11th and 12th: only one fits, so the head is not drawn and nothing after it is either.
    let name = name_of(vec![coloured("Crimson", "red"), head_of(7), coloured(" Dynamo", "blue")]);
    let p = named_team(name, 5);
    let l = layout_of(&p, &Cards::default(), 200);
    assert_eq!(l.score_col, 70);
    assert_eq!(l.rows[0].team, "#1 Crimson ");
    assert_eq!(l.rows[0].team_width, 66);
    assert_eq!(
        l.rows[0].team_runs,
        vec![
            TeamRun { text: "#1 ".into(), argb: WHITE, head: None, width: 18 },
            TeamRun { text: "Crimson".into(), argb: RED, head: None, width: 42 },
        ]
    );
    let row: Vec<ScreenDraw> = l
        .draw(1_000_000 + 1500, 0, 0)
        .into_iter()
        .filter(|c| matches!(c, ScreenDraw::Text { y: 21, .. } | ScreenDraw::Sprite { y: 21, .. }))
        .collect();
    assert_eq!(
        row,
        vec![t(4, 21, "#1 ", WHITE, false), t(22, 21, "Crimson", RED, false), t(70, 21, "1 card, 5 items", WHITE, false), t(164, 21, "0:05", WHITE, false)]
    );
}

#[test]
fn a_cut_team_name_keeps_the_colour_of_each_character_it_keeps() {
    // "#1 Crimson Dynamo" cut to 66 px (11 characters, as the test above): "#1 " white, "Crimson" red, " " blue.
    let name = name_of(vec![coloured("Crimson", "red"), coloured(" Dynamo", "blue")]);
    let p = named_team(name, 5);
    let l = layout_of(&p, &Cards::default(), 200);
    assert_eq!(l.score_col, 70);
    assert_eq!(l.rows[0].team, "#1 Crimson ");
    assert_eq!(
        l.rows[0].team_runs,
        vec![
            TeamRun { text: "#1 ".into(), argb: WHITE, head: None, width: 18 },
            TeamRun { text: "Crimson".into(), argb: RED, head: None, width: 42 },
            TeamRun { text: " ".into(), argb: BLUE, head: None, width: 6 },
        ]
    );
}

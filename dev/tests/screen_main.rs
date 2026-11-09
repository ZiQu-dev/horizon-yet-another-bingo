//! The card screen against the numbers in BingoHudScreen.kt (updatePositions, the title functions, the vote marks,
//! the tab rules and the closing animation). Expected values are worked out by hand from the Kotlin, GUI 854 x 480,
//! and text is 5 px a character.
use horizon_bingo::card_draw::Draw;
use horizon_bingo::game::{GameState, GameStatus};
use horizon_bingo::game_over::{EndScreenTab, GameOverPacket, ScoreRanking};
use horizon_bingo::game_status::JavaDuration;
use horizon_bingo::ready_update::{ReadyUpdatePacket, VoteOption};
use horizon_bingo::score_update::TeamScore;
use horizon_bingo::screen_main::*;
use horizon_bingo::text::Text;

fn lang(key: &str) -> Option<String> {
    let v = match key {
        "bingo.card.close" => "Close",
        "bingo.card.settings_button" => "Settings",
        "bingo.options.more" => "More...",
        "bingo.world.return_to_lobby" => "Return to Lobby",
        "bingo.game.end.tab.cards" => "Cards",
        "bingo.game.end.tab.scores" => "Scores",
        "bingo.stats.game_duration" => "Game Duration",
        "bingo.stats.best_time" => "Best time: %s",
        "bingo.stats.best_time.new" => "New best time! (%s)",
        "bingo.stats.win_streak" => "Win Streak",
        "bingo.stats.win_streak.max" => "Max streak: %s",
        "bingo.stats.win_streak.new_max" => "New max streak!",
        _ => return None,
    };
    Some(v.to_string())
}

fn width(s: &str) -> i32 {
    5 * s.chars().count() as i32
}

fn view<'a>(now_ms: u64, game_over: Option<&'a GameOverPacket>, ready: Option<&'a ReadyUpdatePacket>) -> MainView<'a> {
    MainView {
        width: 854,
        height: 480,
        now_ms,
        mouse: (-1, -1),
        game_over,
        state_has_game_over: game_over.is_some(),
        ready,
        settings_installed: false,
        ready_vote_supported: true,
        lang: &lang,
        text_width: &width,
    }
}

fn game_over(scores: usize, default_tab: EndScreenTab) -> GameOverPacket {
    let mut g = GameOverPacket::default();
    g.scores = (0..scores)
        .map(|i| ScoreRanking {
            index: i as i32,
            key: format!("bingo_{i}"),
            name: Text::plain("Team"),
            score: TeamScore::default(),
            duration: None,
        })
        .collect();
    g.default_tab = default_tab;
    g
}

fn vote(id: u128, title: &str, votes: i32) -> VoteOption {
    VoteOption { id, title: Text::plain(title), votes, is_selected: false }
}

fn ready(options: Vec<VoteOption>, pending: Option<u128>, is_ready: bool, can_send: bool) -> ReadyUpdatePacket {
    ReadyUpdatePacket {
        is_running: true,
        is_ready,
        state: GameState { identifier: "lobby".into(), status: GameStatus::Pregame },
        remaining_millis: 5_000,
        total_millis: 10_000,
        ready_players: 1,
        total_players: 4,
        title: Some(Text::plain("Ready?")),
        subtitle: None,
        can_send_ready: can_send,
        vote_options: options,
        vote_pending: pending,
    }
}

/// A screen opened at 0 and settled (the sidebar fully in) by a frame at 1000 ms.
fn settled(tab: EndScreenTab, go: Option<&GameOverPacket>, ready: Option<&ReadyUpdatePacket>) -> ScreenState {
    let mut s = ScreenState::open(0, tab, true);
    s.frame(&view(1000, go, ready));
    s
}

fn texts(d: &[ScreenDraw]) -> Vec<String> {
    d.iter()
        .filter_map(|x| match x {
            ScreenDraw::Base(Draw::Text { text, .. }) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn base_text_at(d: &[ScreenDraw], text: &str) -> Option<(i32, i32)> {
    d.iter().find_map(|x| match x {
        ScreenDraw::Base(Draw::Text { x, y, text: t, .. }) if t == text => Some((*x, *y)),
        _ => None,
    })
}

fn label_of<'a>(d: &'a [ScreenDraw], text: &str) -> &'a ScreenDraw {
    d.iter()
        .find(|x| matches!(x, ScreenDraw::Label { text: t, .. } if t == text))
        .unwrap_or_else(|| panic!("no label {text:?} in {d:?}"))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn nine_patch_names(d: &[ScreenDraw]) -> Vec<String> {
    d.iter()
        .filter_map(|x| match x {
            ScreenDraw::NinePatch { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_fresh_card_screen_lays_out_the_sidebar_close_button_and_card() {
    // 854 * 0.2 = 170.8 -> 170, sidebar 270 wide at x 584. Card at (584/2, 480/2 + 12). Close: 270 - 24 = 246 wide,
    // y = 480 - 20 - 12 = 448. Messages end 10 above the close button: 448 - 10 = 438.
    let s = settled(EndScreenTab::Cards, None, None);
    let l = layout(&s, &view(1000, None, None));
    assert_eq!((l.sidebar_width, l.sidebar_x), (270, 584));
    assert_eq!(l.card, Some(CardSlot { x: 292.0, y: 252.0, w: 584, h: 132 }));
    assert_eq!(l.close, Rect { x: 596, y: 448, w: 246, h: 20 });
    assert_eq!(l.messages, Rect { x: 596, y: 0, w: 246, h: 438 });
    assert!(l.tabs.is_empty() && l.scores.is_none() && l.settings.is_none() && l.more.is_none() && l.primary.is_none());
}

#[test]
fn an_installed_settings_button_halves_the_close_button_with_a_gap_of_two() {
    // (246 / 2) - 1 = 122 for each; settings starts at 596 + 122 + 2 = 720.
    let s = settled(EndScreenTab::Cards, None, None);
    let v = MainView { settings_installed: true, ..view(1000, None, None) };
    let l = layout(&s, &v);
    assert_eq!(l.close, Rect { x: 596, y: 448, w: 122, h: 20 });
    assert_eq!(l.settings, Some(Rect { x: 720, y: 448, w: 122, h: 20 }));
}

#[test]
fn the_sidebar_slides_in_on_the_exp_out_curve_over_150_ms() {
    // At 0 ms the sidebar is off screen (x = 854). At 75 ms, progress 0.5: 1 - 2^-5 = 0.96875, times 270 = 261.5625,
    // rounded half up to 262, so x = 854 - 262 = 592.
    let mut s = ScreenState::open(0, EndScreenTab::Cards, true);
    assert!(s.frame(&view(0, None, None)).relayout);
    assert_eq!(layout(&s, &view(0, None, None)).sidebar_x, 854);
    s.frame(&view(75, None, None));
    assert_eq!(layout(&s, &view(75, None, None)).sidebar_x, 592);
}

#[test]
fn the_layout_is_redone_once_more_after_the_animation_ends_and_then_not_again() {
    // Kotlin's `update()` reports a change while the previous frame had not seen the end.
    let mut s = ScreenState::open(0, EndScreenTab::Cards, true);
    assert!(s.frame(&view(0, None, None)).relayout);
    assert!(s.frame(&view(200, None, None)).relayout);
    assert!(!s.frame(&view(300, None, None)).relayout);
}

#[test]
fn the_scores_tab_takes_the_card_slot_and_two_tab_rects_split_the_strip() {
    // Strip: 584 wide. MenuTabBar: inner = 400 is not reached, so inner = 584 would be capped to 400 - 28 = 372, two
    // tabs of 186 (372 / 2) from x = round_up((584 - 372) / 2 = 106) = 106. Scores: x = (584 * 0.1) = 58,
    // y = 240 - 66 + 12 = 186, w = 584 * 0.8 = 467, h = 132.
    let go = game_over(2, EndScreenTab::Scores);
    let s = settled(EndScreenTab::Scores, Some(&go), None);
    let l = layout(&s, &view(1000, Some(&go), None));
    assert_eq!(l.card, None);
    assert_eq!(l.scores, Some(Rect { x: 58, y: 186, w: 467, h: 132 }));
    assert_eq!(
        l.tabs,
        vec![Rect { x: CARDS_X, y: 0, w: TAB_W, h: 24 }, Rect { x: SCORES_X, y: 0, w: TAB_W, h: 24 }]
    );
}

#[test]
fn the_tab_strip_shows_only_when_the_end_screen_has_a_reason() {
    let one_team_cards = game_over(1, EndScreenTab::Cards);
    let two_teams = game_over(2, EndScreenTab::Cards);
    let scores_default = game_over(1, EndScreenTab::Scores);
    let cards = ScreenState::open(0, EndScreenTab::Cards, true);
    let scores = ScreenState::open(0, EndScreenTab::Scores, true);
    let mut closing = ScreenState::open(0, EndScreenTab::Cards, true);
    closing.request_close(0);

    assert!(!tabs_shown(&cards, &view(0, None, None)), "no game over");
    assert!(!tabs_shown(&cards, &view(0, Some(&one_team_cards), None)), "one team, cards by default");
    assert!(tabs_shown(&cards, &view(0, Some(&two_teams), None)), "more than one team scored");
    assert!(tabs_shown(&cards, &view(0, Some(&scores_default), None)), "scores is the default tab");
    assert!(tabs_shown(&scores, &view(0, Some(&one_team_cards), None)), "the scores tab is selected");
    assert!(!tabs_shown(&closing, &view(0, Some(&two_teams), None)), "closing hides the strip");
}

/// The two tabs of a 584-wide strip (GUI 854): `MenuTabBar` puts them at x 106 and 292, 186 wide each.
const CARDS_X: i32 = 106;
const SCORES_X: i32 = 292;
const TAB_W: i32 = 186;

/// The name of the tab frame drawn at x, on the strip's row (y 0, 186 x 24).
fn tab_frame(d: &[ScreenDraw], x: i32) -> String {
    d.iter()
        .find_map(|s| match s {
            ScreenDraw::NinePatch { name, x: fx, y: 0, w, h: 24, .. } if *fx == x && *w == TAB_W => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no tab frame at x {x} in {d:?}"))
}

/// The position in the draw list of the first draw that matches `pred`.
fn position_of(d: &[ScreenDraw], pred: impl Fn(&ScreenDraw) -> bool, what: &str) -> usize {
    d.iter().position(pred).unwrap_or_else(|| panic!("no {what} in {d:?}"))
}

#[test]
fn each_tab_frame_is_drawn_before_its_label() {
    // Two tabs: Cards at x 106 and Scores at x 292, both 186 x 24 at y 0.
    let go = game_over(2, EndScreenTab::Cards);
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let d = draw(&s, &v, &layout(&s, &v));

    let frame_at = |x: i32| {
        position_of(&d, |s| matches!(s, ScreenDraw::NinePatch { x: fx, y: 0, w, h: 24, .. } if *fx == x && *w == TAB_W), "frame")
    };
    let label_of_text = |t: &str| {
        position_of(&d, |s| matches!(s, ScreenDraw::Label { text, .. } if text == t), "label")
    };
    let (cards_frame, cards_label) = (frame_at(CARDS_X), label_of_text("Cards"));
    let (scores_frame, scores_label) = (frame_at(SCORES_X), label_of_text("Scores"));
    assert!(cards_frame < cards_label, "the cards frame is under its label");
    assert!(scores_frame < scores_label, "the scores frame is under its label");
    assert!(cards_label < scores_frame, "the tabs are drawn left to right, each frame then its label");
}

#[test]
fn the_open_tab_has_the_lighter_frame_and_the_other_the_plain_one() {
    let go = game_over(2, EndScreenTab::Cards);
    let cards = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let d = draw(&cards, &v, &layout(&cards, &v));
    assert_eq!(tab_frame(&d, CARDS_X), "widget/tab_selected");
    assert_eq!(tab_frame(&d, SCORES_X), "widget/tab");

    let go = game_over(2, EndScreenTab::Scores);
    let scores = settled(EndScreenTab::Scores, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let d = draw(&scores, &v, &layout(&scores, &v));
    assert_eq!(tab_frame(&d, CARDS_X), "widget/tab");
    assert_eq!(tab_frame(&d, SCORES_X), "widget/tab_selected");
}

#[test]
fn hovering_a_tab_lightens_its_frame_and_the_mouse_edges_are_outside() {
    // The strip is y 0..24 and Cards covers x 106..292. (291, 23) is in Cards, (292, 0) in Scores; x 105 is left of the
    // strip and y 24 below it.
    let go = game_over(2, EndScreenTab::Cards);
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let frames_at = |mouse: (i32, i32)| {
        let v = MainView { mouse, ..view(1000, Some(&go), None) };
        let d = draw(&s, &v, &layout(&s, &v));
        (tab_frame(&d, CARDS_X), tab_frame(&d, SCORES_X))
    };

    assert_eq!(frames_at((110, 10)), ("widget/tab_selected_highlighted".into(), "widget/tab".into()));
    assert_eq!(frames_at((291, 23)), ("widget/tab_selected_highlighted".into(), "widget/tab".into()));
    assert_eq!(frames_at((300, 10)), ("widget/tab_selected".into(), "widget/tab_highlighted".into()));
    assert_eq!(frames_at((105, 10)), ("widget/tab_selected".into(), "widget/tab".into()), "x 105 is left of the strip");
    assert_eq!(frames_at((300, 24)), ("widget/tab_selected".into(), "widget/tab".into()), "y 24 is below the strip");
}

#[test]
fn choosing_a_tab_changes_it_and_the_strip_is_clickable_only_while_shown() {
    let go = game_over(2, EndScreenTab::Cards);
    let mut s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    assert!(s.click(Action::Tab(1), &v).is_empty());
    assert_eq!(s.tab, EndScreenTab::Scores);
    s.click(Action::Tab(0), &v);
    assert_eq!(s.tab, EndScreenTab::Cards);

    let l = layout(&s, &v);
    let tab_hits: Vec<Action> = hits(&s, &v, &l)
        .into_iter()
        .filter(|h| matches!(h.action, Action::Tab(_)))
        .map(|h| h.action)
        .collect();
    assert_eq!(tab_hits, vec![Action::Tab(0), Action::Tab(1)]);

    s.request_close(1000);
    let l = layout(&s, &v);
    assert!(hits(&s, &v, &l).iter().all(|h| !matches!(h.action, Action::Tab(_))));
}

#[test]
fn close_folds_the_more_options_away_before_it_closes_the_screen() {
    let go = game_over(1, EndScreenTab::Cards);
    let options = vec![vote(1, "A", 0), vote(2, "B", 0)];
    let r = ready(options, None, false, false);
    let mut s = settled(EndScreenTab::Cards, Some(&go), Some(&r));
    let v = view(1000, Some(&go), Some(&r));
    s.click(Action::MoreOptions, &v);
    assert!(s.more_options_open);

    s.click(Action::Close, &v);
    assert!(!s.more_options_open && !s.closing);
    s.click(Action::Close, &v);
    assert!(s.closing);
}

#[test]
fn escape_closes_only_a_screen_that_can_be_escaped() {
    let mut yes = ScreenState::open(0, EndScreenTab::Cards, true);
    assert!(yes.escape(500));
    assert!(yes.closing);
    let mut no = ScreenState::open(0, EndScreenTab::Cards, false);
    assert!(!no.escape(500));
    assert!(!no.closing);
}

#[test]
fn the_closing_animation_ends_strictly_after_150_ms_and_a_second_close_does_not_restart_it() {
    let mut s = settled(EndScreenTab::Cards, None, None);
    s.click(Action::Close, &view(1000, None, None));
    assert_eq!(s.interp.started_at_ms, 1000);
    s.click(Action::Close, &view(1100, None, None));
    assert_eq!(s.interp.started_at_ms, 1000, "a second request keeps the first start");

    assert!(!s.frame(&view(1150, None, None)).close, "exactly 150 ms is not yet done");
    assert!(s.frame(&view(1151, None, None)).close);
}

#[test]
fn a_game_over_cleared_by_the_server_closes_the_screen() {
    let go = game_over(1, EndScreenTab::Cards);
    let mut s = settled(EndScreenTab::Cards, Some(&go), None);
    let gone = MainView { state_has_game_over: false, ..view(1000, Some(&go), None) };
    assert!(!s.frame(&gone).close);
    assert!(s.closing);
    assert!(s.frame(&MainView { now_ms: 1200, ..gone }).close);
}

#[test]
fn the_primary_button_is_active_only_for_a_ready_player_who_is_not_ready() {
    // The primary button exists when the packet offers a ready vote. Its `active` is set on frames where the primary
    // branch runs: `ready?.isReady == false`, so no ready timer means inactive.
    let go = {
        let mut g = game_over(1, EndScreenTab::Cards);
        g.is_ready_vote_available = true;
        g
    };
    let unready = ready(vec![], None, false, true);
    let is_ready = ready(vec![], None, true, true);

    let mut s = ScreenState::open(0, EndScreenTab::Cards, true);
    s.frame(&view(1000, Some(&go), Some(&unready)));
    assert!(s.primary_active);
    s.frame(&view(1000, Some(&go), Some(&is_ready)));
    assert!(!s.primary_active);

    let mut none = ScreenState::open(0, EndScreenTab::Cards, true);
    none.frame(&view(1000, Some(&go), None));
    assert!(!none.primary_active);
    assert!(hits(&none, &view(1000, Some(&go), None), &layout(&none, &view(1000, Some(&go), None)))
        .iter()
        .all(|h| h.action != Action::Ready(None)));
}

#[test]
fn the_primary_label_keeps_the_last_vote_title_after_the_vote_clears() {
    // `primaryButton.message = it.title` runs only on frames with a pending vote, and nothing resets it.
    let go = {
        let mut g = game_over(1, EndScreenTab::Cards);
        g.is_ready_vote_available = true;
        g
    };
    let options = vec![vote(7, "Map A", 0)];
    let pending = ready(options.clone(), Some(7), false, true);
    let cleared = ready(options, None, false, true);

    let mut s = ScreenState::open(0, EndScreenTab::Cards, true);
    s.frame(&view(1000, Some(&go), None));
    let before = layout(&s, &view(1000, Some(&go), None));
    let d = draw(&s, &view(1000, Some(&go), None), &before);
    assert!(texts(&d).contains(&"Return to Lobby".to_string()));

    s.frame(&view(1000, Some(&go), Some(&pending)));
    let d = draw(&s, &view(1000, Some(&go), Some(&pending)), &layout(&s, &view(1000, Some(&go), Some(&pending))));
    assert!(texts(&d).contains(&"Map A".to_string()));

    s.frame(&view(1000, Some(&go), Some(&cleared)));
    let d = draw(&s, &view(1000, Some(&go), Some(&cleared)), &layout(&s, &view(1000, Some(&go), Some(&cleared))));
    assert!(texts(&d).contains(&"Map A".to_string()), "the title stays");
}

#[test]
fn vote_marks_scale_to_the_largest_vote_and_sit_centred_under_the_label() {
    // Votes 3, 12, 0. Max 12, so the ratio is min(12, 8) / 12 = 0.6667. Marks: round(3 * 0.6667) = 2,
    // round(12 * 0.6667) = 8, 0 = none. Options from the top: A at y 316, B at 360, C at 404, each 40 high
    // (the close row is at 448, and each option takes 44). A's marks "· ·" are 15 px: x = 596 + 123 - 7 = 712,
    // y = 316 + 40 - 10 = 346. B's eight marks are 75 px: x = 596 + 123 - 37 = 682, y = 360 + 30 = 390.
    let go = {
        let mut g = game_over(1, EndScreenTab::Cards);
        g.is_ready_vote_available = true;
        g
    };
    let options = vec![vote(1, "A", 3), vote(2, "B", 12), vote(3, "C", 0)];
    let r = ready(options, None, false, false);
    let mut s = settled(EndScreenTab::Cards, Some(&go), Some(&r));
    let v = view(1000, Some(&go), Some(&r));
    s.click(Action::MoreOptions, &v);
    let l = layout(&s, &v);
    let rows: Vec<i32> = l.options.iter().map(|(_, r)| r.y).collect();
    assert_eq!(rows, vec![316, 360, 404]);

    let d = draw(&s, &v, &l);
    assert_eq!(base_text_at(&d, "· ·"), Some((712, 346)));
    let eight = vec!["·"; 8].join(" ");
    assert_eq!(base_text_at(&d, &eight), Some((682, 390)));
    assert!(d.iter().all(|x| !matches!(x, ScreenDraw::Base(Draw::Text { y: 434, text, .. }) if text.contains('·'))));
}

#[test]
fn a_chosen_vote_is_sent_and_no_longer_reacts_and_it_stops_the_pause() {
    let go = {
        let mut g = game_over(1, EndScreenTab::Cards);
        g.is_ready_vote_available = true;
        g
    };
    let options = vec![vote(1, "A", 0), vote(2, "B", 0)];
    let r = ready(options, None, false, false);
    let mut s = settled(EndScreenTab::Cards, Some(&go), Some(&r));
    let v = view(1000, Some(&go), Some(&r));
    s.click(Action::MoreOptions, &v);
    assert!(s.should_pause(true));

    let effects = s.click(Action::Ready(Some(1)), &v);
    assert_eq!(effects, vec![Effect::SendReady(Some(1))]);
    assert!(!s.should_pause(true), "the game is not paused once a ready vote was sent");
    let l = layout(&s, &v);
    let voted: Vec<Action> = hits(&s, &v, &l)
        .into_iter()
        .filter(|h| matches!(h.action, Action::Ready(_)))
        .map(|h| h.action)
        .collect();
    assert_eq!(voted, vec![Action::Ready(Some(2))]);
}

#[test]
fn the_duration_and_best_time_follow_the_rotated_title_rules() {
    // Duration 90 s is "01m 30s" (7 chars, 35 px). Best 100 s is "01m 40s" and the text "Best time: 01m 40s" is 18 chars
    // (90 px). Sidebar x = 584, so decoration x = max(584 * 0.2 - 20, 40) = 96.8. Anchor y 123 (see the title
    // rules), so the scale is (123 - 24) / 14 clamped to 2.0. The group is rotated -0.25 rad, the title is at half
    // scale 1.0 and y 18, the best line at y 30.
    let mut go = game_over(1, EndScreenTab::Cards);
    go.duration = JavaDuration { seconds: 90, nanos: 0 };
    go.prev_best_time = Some(JavaDuration { seconds: 100, nanos: 0 });
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let d = draw(&s, &v, &layout(&s, &v));

    match label_of(&d, "01m 30s") {
        ScreenDraw::Label { anchor, offset, scale, rotation, argb, shadow, .. } => {
            assert!(near(anchor.0, 96.8) && near(anchor.1, 123.0));
            assert_eq!(*offset, (-17.0, 0.0));
            assert_eq!(*scale, 2.0);
            assert!(near(*rotation, -0.25));
            // Kotlin's expOut(1) is 1 - 2^-10, so the settled interpolation is 0.99902 and the alpha 255 * that = 254.
            assert_eq!(*argb, 0xfea3_f5a3);
            assert!(*shadow);
        }
        other => panic!("{other:?}"),
    }
    match label_of(&d, "Game Duration") {
        ScreenDraw::Label { offset, scale, .. } => assert_eq!((*offset, *scale), ((-32.0, 18.0), 1.0)),
        other => panic!("{other:?}"),
    }
    match label_of(&d, "Best time: 01m 40s") {
        ScreenDraw::Label { offset, scale, argb, .. } => {
            assert_eq!(*offset, (-45.0, 30.0));
            assert_eq!(*scale, 1.0);
            assert_eq!(*argb, 0xfecc_a3f5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_new_best_time_grows_in_over_two_seconds_in_the_winning_colour() {
    // Duration 90 s beat 100 s, ended at 11 000 ms, drawn at 12 000 ms: 1 s in, progress (1000 / 2000)^2 = 0.25.
    // Best scale 2 - 0.25 = 1.75 on top of 2 * 0.5, so 1.75. Text "New best time! (-10s)" is 21 chars: 105 px, offset
    // -52. Alpha trunc(1 * 0.25 * 255) = 63 = 0x3F. Colour: second 12 mod 6 = 0, so 0xf5c0a3.
    let mut go = game_over(1, EndScreenTab::Cards);
    go.duration = JavaDuration { seconds: 90, nanos: 0 };
    go.prev_best_time = Some(JavaDuration { seconds: 100, nanos: 0 });
    go.is_best_time = true;
    go.ended_at_millis = 11_000;
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(12_000, Some(&go), None);
    let mut s = s;
    s.frame(&v);
    let d = draw(&s, &v, &layout(&s, &v));
    match label_of(&d, "New best time! (-10s)") {
        ScreenDraw::Label { offset, scale, argb, .. } => {
            assert_eq!(*offset, (-52.0, 30.0));
            assert!(near(*scale, 1.75));
            assert_eq!(*argb, 0x3ff5_c0a3);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_win_streak_star_turns_once_every_two_seconds_and_needs_a_win_above_one() {
    // Streak 3, won: the star is a 4 px square scaled by 4 * scale (2), so half side 16, centred at
    // (584 - 96.8, 123 + 12) = (487.2, 135). At 12 500 ms the angle is 500 / 2000 * 2 PI = PI / 2. The alpha is
    // the settled interpolation's 254 (see the duration test), so 254 * 30 / 255 = 29.
    let mut go = game_over(1, EndScreenTab::Cards);
    go.win_streak = Some(3);
    go.is_winner = true;
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(12_500, Some(&go), None);
    let d = draw(&s, &v, &layout(&s, &v));
    let star = d.iter().find_map(|x| match x {
        ScreenDraw::Square { cx, cy, half, rotation, argb } => Some((*cx, *cy, *half, *rotation, *argb)),
        _ => None,
    });
    let (cx, cy, half, rotation, argb) = star.expect("a star for a won streak of 3");
    assert!(near(cx, 487.2) && near(cy, 135.0));
    assert!(near(half, 16.0));
    assert!(near(rotation, std::f32::consts::FRAC_PI_2));
    assert_eq!(argb, 0x1df5_a3d6);

    let mut one = go.clone();
    one.win_streak = Some(1);
    let s = settled(EndScreenTab::Cards, Some(&one), None);
    let v = view(12_500, Some(&one), None);
    let d = draw(&s, &v, &layout(&s, &v));
    assert!(d.iter().all(|x| !matches!(x, ScreenDraw::Square { .. })), "a streak of one has no star");
}

#[test]
fn a_short_window_clamps_the_title_scale_to_one() {
    // Height 100: card top -16, title y min(-12, -38) + 12 = -26, anchor -46, so (-46 - 24) / 14 is clamped to 1.
    let mut go = game_over(1, EndScreenTab::Cards);
    go.duration = JavaDuration { seconds: 90, nanos: 0 };
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = MainView { height: 100, ..view(1000, Some(&go), None) };
    let d = draw(&s, &v, &layout(&s, &v));
    match label_of(&d, "01m 30s") {
        ScreenDraw::Label { anchor, scale, .. } => {
            assert!(near(anchor.1, -46.0));
            assert_eq!(*scale, 1.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_ready_subtitle_is_centred_over_the_sidebar_above_the_lowest_button() {
    // "Starting" is 8 chars, 40 px. x = 584 + 135 - 20 = 699, y = 448 - 9 = 439.
    let mut r = ready(vec![], None, false, false);
    r.subtitle = Some(Text::plain("Starting"));
    let s = settled(EndScreenTab::Cards, None, Some(&r));
    let v = view(1000, None, Some(&r));
    let d = draw(&s, &v, &layout(&s, &v));
    assert_eq!(base_text_at(&d, "Starting"), Some((699, 439)));
}

#[test]
fn the_buttons_take_their_sprite_by_state_and_hover() {
    let go = {
        let mut g = game_over(1, EndScreenTab::Cards);
        g.is_ready_vote_available = true;
        g
    };
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let inside = MainView { mouse: (600, 450), ..view(1000, Some(&go), None) };
    let names = nine_patch_names(&draw(&s, &inside, &layout(&s, &inside)));
    assert!(names.contains(&"bingo/button_game_generic_focused".to_string()), "hovered close");
    let outside = view(1000, Some(&go), None);
    let names = nine_patch_names(&draw(&s, &outside, &layout(&s, &outside)));
    assert!(names.contains(&"bingo/button_game_generic".to_string()));
    assert!(names.contains(&"bingo/button_game_continue_inactive".to_string()), "no ready timer");
    assert!(names.contains(&"bingo/game_sidebar_background".to_string()));
}

#[test]
fn the_primary_vote_button_is_inactive_once_its_option_was_chosen() {
    // With a vote pending, the primary button is the pending option. It is active until the client sends that option
    // (`clientSelected`), after which it does not react.
    let go = {
        let mut g = game_over(1, EndScreenTab::Cards);
        g.is_ready_vote_available = true;
        g
    };
    let pending = ready(vec![vote(7, "Map A", 0)], Some(7), false, true);
    let mut s = settled(EndScreenTab::Cards, Some(&go), Some(&pending));
    assert!(s.primary_active);
    let v = view(1000, Some(&go), Some(&pending));
    s.click(Action::Ready(Some(7)), &v);
    s.frame(&v);
    assert!(!s.primary_active);
}

/// Every flat fill the draw list makes, as (x, y, w, h, argb).
fn fills(d: &[ScreenDraw]) -> Vec<(i32, i32, i32, i32, u32)> {
    d.iter()
        .filter_map(|x| match x {
            ScreenDraw::Base(Draw::Fill { x, y, w, h, argb }) => Some((*x, *y, *w, *h, *argb)),
            _ => None,
        })
        .collect()
}

/// The anchor, colour and shadow of the plain label `text`, which has no offset, scale or turn.
fn plain_label(d: &[ScreenDraw], text: &str) -> (f32, f32, u32, bool) {
    match label_of(d, text) {
        ScreenDraw::Label { anchor, offset, scale, rotation, argb, shadow, .. } => {
            assert!(*offset == (0.0, 0.0) && *scale == 1.0 && *rotation == 0.0, "plain label {text:?}");
            (anchor.0, anchor.1, *argb, *shadow)
        }
        _ => unreachable!(),
    }
}

fn strip_of(width: i32, go: &GameOverPacket) -> Vec<Rect> {
    // Only the width changes; the sidebar is fully in after one frame at 1000 ms.
    let v = MainView { width, ..view(1000, Some(go), None) };
    let mut s = ScreenState::open(0, EndScreenTab::Cards, true);
    s.frame(&v);
    layout(&s, &v).tabs
}

#[test]
fn the_strip_is_menu_tab_bar_sized_and_centred_in_the_main_part() {
    // MenuTabBar: inner = min(400, main) - 28, each tab is inner / 2 rounded up to an even width, and the row starts at
    // round_up((main - inner) / 2) to an even number.
    let go = game_over(2, EndScreenTab::Cards);
    // Width 428: sidebar = 428 * 0.2 = 85 + 100 = 185, so main = 243. inner = 215, tab = round_up(107) = 108,
    // x0 = round_up(14) = 14: tabs at 14 and 122 (the Horizon 3x GUI of the end screen shows the same, x 42 and 366).
    assert_eq!(
        strip_of(428, &go),
        vec![Rect { x: 14, y: 0, w: 108, h: 24 }, Rect { x: 122, y: 0, w: 108, h: 24 }]
    );
    // Width 1280: sidebar = 256 + 100 = 356, main = 924. inner = min(400, 924) - 28 = 372 (the cap binds), tab = 186,
    // x0 = round_up((924 - 372) / 2 = 276) = 276.
    assert_eq!(
        strip_of(1280, &go),
        vec![Rect { x: 276, y: 0, w: 186, h: 24 }, Rect { x: 462, y: 0, w: 186, h: 24 }]
    );
}

#[test]
fn the_header_separator_runs_from_each_end_of_the_strip_in_two_rows() {
    // The strip of 584 holds tabs 106..478, so the separator runs 0..106 and 478..584 on rows 22 (white at 20%) and 23
    // (black at 75%), drawn before the tabs.
    let go = game_over(2, EndScreenTab::Cards);
    let s = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let d = draw(&s, &v, &layout(&s, &v));
    let f = fills(&d);
    for (x, w) in [(0, 106), (478, 106)] {
        let light = (x, 22, w, 1, 0x33_ffffff);
        let dark = (x, 23, w, 1, 0xbf_000000);
        assert!(f.contains(&light) && f.contains(&dark), "separator {x}..{}: {f:?}", x + w);
    }
    let first_tab_frame = position_of(
        &d,
        |s| matches!(s, ScreenDraw::NinePatch { name, .. } if name.starts_with("widget/tab")),
        "tab frame",
    );
    let first_separator = position_of(&d, |s| matches!(s, ScreenDraw::Base(Draw::Fill { argb: 0x33_ffffff, .. })), "separator");
    assert!(first_separator < first_tab_frame, "the separator is behind the tabs");
}

#[test]
fn the_open_tab_gets_a_menu_tile_and_a_text_wide_underline_and_the_other_gets_neither() {
    let go = game_over(2, EndScreenTab::Cards);
    let cards = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let f = fills(&draw(&cards, &v, &layout(&cards, &v)));
    // The tile is black at 25%, 2 px in from the left, top and right: x 108..290, y 2..24.
    assert!(f.contains(&(108, 2, 182, 22, 0x40_000000)), "{f:?}");
    // "Cards" is 25 px: the line is (186 - 25) / 2 = 80 past the tab's 106, 2 px above the bottom.
    assert!(f.contains(&(186, 22, 25, 1, 0xffff_ffff)), "{f:?}");
    assert!(!f.iter().any(|x| x.0 == 294 || x.0 == 370), "the plain tab has no tile or line: {f:?}");

    let go = game_over(2, EndScreenTab::Scores);
    let scores = settled(EndScreenTab::Scores, Some(&go), None);
    let f = fills(&draw(&scores, &v, &layout(&scores, &v)));
    // "Scores" is 30 px: (186 - 30) / 2 = 78 past 292, so x 370.
    assert!(f.contains(&(294, 2, 182, 22, 0x40_000000)), "{f:?}");
    assert!(f.contains(&(370, 22, 30, 1, 0xffff_ffff)), "{f:?}");
    assert!(!f.iter().any(|x| x.0 == 108), "the plain tab has no tile: {f:?}");
}

#[test]
fn both_tab_labels_are_white_with_a_shadow_and_the_open_one_sits_3_px_higher() {
    // acceptScrollingWithDefaultCenter: the text's x is the tab's middle less half its width, its y is
    // (top + bottom - 9) / 2 + 1 with top 0 when open and 3 when not, and bottom 24.
    // Cards: 106 + 93 - 25 / 2 = 187 at y (0 + 24 - 9) / 2 + 1 = 8. Scores: 292 + 93 - 30 / 2 = 370 at y 10.
    let go = game_over(2, EndScreenTab::Cards);
    let cards = settled(EndScreenTab::Cards, Some(&go), None);
    let v = view(1000, Some(&go), None);
    let d = draw(&cards, &v, &layout(&cards, &v));
    assert_eq!(plain_label(&d, "Cards"), (187.0, 8.0, 0xffff_ffff, true));
    assert_eq!(plain_label(&d, "Scores"), (370.0, 10.0, 0xffff_ffff, true));

    let go = game_over(2, EndScreenTab::Scores);
    let scores = settled(EndScreenTab::Scores, Some(&go), None);
    let d = draw(&scores, &v, &layout(&scores, &v));
    assert_eq!(plain_label(&d, "Cards"), (187.0, 10.0, 0xffff_ffff, true));
    assert_eq!(plain_label(&d, "Scores"), (370.0, 8.0, 0xffff_ffff, true));
}

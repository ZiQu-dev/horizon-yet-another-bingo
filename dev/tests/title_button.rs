//! The title screen's BINGO! button, pure part (`horizon_bingo::title_button`): the answers' hints, the folder rule,
//! the button's place and the pointer's hit test. Worked out by hand, not from the code under test.
use horizon_bingo::title_button::{
    button_rect, contains, hint_for, hint_text, label_text, lobby_folder_ok, Hint, WorldAnswer, BUTTON_H, BUTTON_W,
    wrap, HINT_CREATE, HINT_CREATE_KEY, LABEL_KEY,
};

fn no_lang(_: &str) -> Option<String> {
    None
}

#[test]
fn each_answer_says_what_the_player_needs() {
    // Queued opens the world, NotTitle and Busy are no news to the player, so no hint for any of them.
    assert_eq!(hint_for(WorldAnswer::Queued, "bingo"), None);
    assert_eq!(hint_for(WorldAnswer::NotTitle, "bingo"), None);
    assert_eq!(hint_for(WorldAnswer::Busy, "bingo"), None);
    assert_eq!(hint_for(WorldAnswer::Unknown, "bingo"), None);
    // A missing world is the one the player fixes: the hint names the folder to create.
    assert_eq!(
        hint_for(WorldAnswer::InvalidRequest, "bingo"),
        Some(Hint::CreateWorld { folder: "bingo".into() })
    );
    assert_eq!(hint_for(WorldAnswer::NotPermitted, "bingo"), Some(Hint::NotPermitted));
}

#[test]
fn the_create_hint_names_the_folder_in_english_and_in_a_translation() {
    let hint = Hint::CreateWorld { folder: "bingo".into() };
    assert_eq!(
        hint_text(&hint, &no_lang),
        "No world named \"bingo\" yet: create one, then press again."
    );
    let translated = |key: &str| (key == HINT_CREATE_KEY).then(|| "Utwórz świat „{folder}”".to_string());
    assert_eq!(hint_text(&hint, &translated), "Utwórz świat „bingo”");
    assert_eq!(label_text(&no_lang), "BINGO!");
    let named = |key: &str| (key == LABEL_KEY).then(|| "BINGO!!".to_string());
    assert_eq!(label_text(&named), "BINGO!!");
}

#[test]
fn a_folder_must_be_one_plain_segment() {
    assert!(lobby_folder_ok("bingo"));
    assert!(lobby_folder_ok(&"x".repeat(64)));
    assert!(!lobby_folder_ok(""));
    assert!(!lobby_folder_ok(&"x".repeat(65)));
    assert!(!lobby_folder_ok(".hidden"));
    assert!(!lobby_folder_ok("a/b"));
    assert!(!lobby_folder_ok("a\\b"));
    assert!(!lobby_folder_ok("c:d"));
    assert!(!lobby_folder_ok("tab\there"));
}

#[test]
fn the_button_sits_bottom_left_as_the_originals_does() {
    // The original adds it at x = 10, y = screen height - 70, 80 x 50, whatever the window's width.
    assert_eq!(button_rect(854.0, 480.0), [10.0, 410.0, BUTTON_W, BUTTON_H]);
    assert_eq!(button_rect(320.0, 240.0), [10.0, 170.0, 80.0, 50.0]);
    assert_eq!((BUTTON_W, BUTTON_H), (80.0, 50.0));
}

#[test]
fn a_click_is_on_the_button_or_not() {
    let rect = [531.0, 168.0, 98.0, 20.0];
    assert!(contains(rect, (531.0, 168.0)));
    assert!(contains(rect, (628.9, 187.9)));
    assert!(!contains(rect, (629.0, 170.0)), "the right edge is outside");
    assert!(!contains(rect, (530.9, 170.0)));
    assert!(!contains(rect, (600.0, 188.0)), "the bottom edge is outside");
}

#[test]
fn a_hint_wraps_to_the_button_width_word_by_word() {
    // 6 px a character; a 42 px line holds 7 characters.
    let six = |text: &str| 6 * text.chars().count() as i32;
    assert_eq!(wrap("one two three four", 42, &six), vec!["one two", "three", "four"]);
    // A word wider than a line stays whole on its own line.
    assert_eq!(wrap("a supercalifragilistic b", 42, &six), vec!["a", "supercalifragilistic", "b"]);
    // The hint on a 98 px button: lines of at most 16 characters, every word kept in order.
    let text = hint_text(&Hint::CreateWorld { folder: "bingo".into() }, &no_lang);
    assert_eq!(text, "No world named \"bingo\" yet: create one, then press again.");
    let lines = wrap(&text, 98, &six);
    assert_eq!(lines, vec!["No world named", "\"bingo\" yet:", "create one, then", "press again."]);
    assert!(lines.iter().all(|line| line.chars().count() <= 16));
    assert!(wrap("", 98, &six).is_empty());
    assert_eq!(HINT_CREATE, "No world named \"{folder}\" yet: create one, then press again.");
}

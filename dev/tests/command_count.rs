//! The held-book count that `ServerHost::command_result` reads from a `clear ... 0` run's feedback. The sentences are
//! vanilla 26.3's `commands.clear.test.*` and `clear.failed.*` (en_us), written out here as the server prints them.
use horizon_bingo::server::infobook::count_from_feedback;

fn lines(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| text.to_string()).collect()
}

#[test]
fn the_found_count_is_read_for_one_player_and_for_several() {
    assert_eq!(count_from_feedback(true, &lines(&["Found 3 matching item(s) on player Alice"])), Some(3));
    assert_eq!(count_from_feedback(true, &lines(&["Found 5 matching item(s) on 2 players"])), Some(5));
}

#[test]
fn no_match_is_a_failure_that_counts_zero() {
    assert_eq!(count_from_feedback(false, &lines(&["No items were found on player Alice"])), Some(0));
    assert_eq!(count_from_feedback(false, &lines(&["No items were found on 2 players"])), Some(0));
}

#[test]
fn a_refused_or_unknown_answer_has_no_count() {
    // A command that did not run, or whose feedback is not the count's sentence, leaves the caller to fall back.
    assert_eq!(count_from_feedback(false, &[]), None);
    assert_eq!(count_from_feedback(false, &lines(&["No player was found"])), None);
    assert_eq!(count_from_feedback(true, &lines(&["Removed 3 item(s) from player Alice"])), None);
}

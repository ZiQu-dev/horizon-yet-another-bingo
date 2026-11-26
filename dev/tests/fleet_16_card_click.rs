// Clicks on the card row (BingoCardsWidget.onMouseClicked): a card body selects its card, a tile falls through to
// the card (no JEI in Horizon), and nothing is selected where the row is not selectable.

use horizon_bingo::screen_cards::{Action, Hit};
use horizon_bingo::screen_controller::card_click_team;

fn hit(x: i32, y: i32, w: i32, h: i32, action: Action) -> Hit {
    Hit { x, y, w, h, action }
}

fn red() -> Option<String> {
    Some("red".into())
}

/// A selectable card at (100, 50), 120 x 80: tile 0 at (111, 71) to (131, 91), the rest of the box is the card.
fn selectable_card() -> Vec<Hit> {
    vec![
        hit(111, 71, 20, 20, Action::Tile { team: red(), index: 0 }),
        hit(100, 50, 121, 81, Action::SelectCard(red())),
    ]
}

#[test]
fn a_click_on_the_card_body_selects_that_card() {
    let hits = selectable_card();
    assert_eq!(card_click_team(&hits, (105, 55)), Some(red()));
}

#[test]
fn a_click_on_a_tile_falls_through_to_its_card_when_selectable() {
    let hits = selectable_card();
    assert_eq!(card_click_team(&hits, (115, 75)), Some(red()));
}

#[test]
fn a_click_on_a_tile_of_a_card_that_cannot_be_selected_selects_nothing() {
    // the end screen (or a single-card HUD) has tile hits but no card hits
    let hits = vec![hit(111, 71, 20, 20, Action::Tile { team: red(), index: 0 })];
    assert_eq!(card_click_team(&hits, (115, 75)), None);
}

#[test]
fn a_click_outside_every_hit_or_on_the_scroll_bar_selects_nothing() {
    let hits = selectable_card();
    assert_eq!(card_click_team(&hits, (10, 10)), None);
    let bar = vec![hit(90, 120, 140, 7, Action::ScrollBar), hit(100, 50, 121, 81, Action::SelectCard(red()))];
    assert_eq!(card_click_team(&bar, (95, 122)), None);
}

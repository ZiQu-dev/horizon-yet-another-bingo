//! Goal page of the lobby menu: the stalemate and win condition tiles carry the original's names and tooltip texts
//! (`bingo.options.win_behavior.stalemate.*` and `bingo.options.win_condition.*` in en_us.json).
use horizon_bingo::server::menu::{self, Page, View};
use horizon_bingo::server::options::Options;

fn goal_slot(index: usize) -> menu::Slot {
    let rules = Options::default();
    let view = View {
        rules: &rules,
        pregame: true,
        operator: true,
    };
    menu::contents(&Page::Goal, &view)[index]
        .clone()
        .expect("the goal page has a tile here")
}

#[test]
fn stalemate_tiles_use_the_original_names_and_tooltips() {
    let end = goal_slot(23);
    assert_eq!(end.name, "End Game");
    assert_eq!(
        end.lore[0],
        "If no team can reach the goal with the current card, then the game will end."
    );

    let reroll = goal_slot(24);
    assert_eq!(reroll.name, "Re-roll Card");
    assert_eq!(
        reroll.lore[0],
        "If no team can reach the goal with the current card, then the entire card will be replaced."
    );

    let nothing = goal_slot(25);
    assert_eq!(nothing.name, "Do Nothing");
    assert_eq!(
        nothing.lore[0],
        "Even if the goal is unreachable, the game will continue until it is manually stopped."
    );
}

#[test]
fn win_condition_tiles_use_the_original_names_and_tooltips() {
    let infinite = goal_slot(21);
    assert_eq!(infinite.name, "Infinite");
    assert_eq!(
        infinite.lore[0],
        "If checked, the game will continue to generate new cards indefinitely, until the time limit."
    );

    let replace = goal_slot(22);
    assert_eq!(replace.name, "Replace Goals");
    assert_eq!(
        replace.lore[0],
        "Once an item/line is scored, it is immediately replaced with new items. Scores will accumulate until the goal is reached."
    );
}

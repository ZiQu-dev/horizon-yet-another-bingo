//! The click sounds of the lobby panel's tiles, as the original's buttons play them (`Button.kt`): the wooden button's
//! click, `on` when the tile is on after the click and `off` otherwise. The number inputs' steppers are always on, and a
//! plain button (a page link, start, reroll, reset) is never on.

use super::lobby_scene::{Click, PanelAction, PanelState};
use super::options::Mode;
use super::texts::PanelMode;

/// The wooden button's click, when its tile is on after the click.
pub const CLICK_ON: &str = "minecraft:block.wooden_button.click_on";
/// The wooden button's click, when its tile is off after the click.
pub const CLICK_OFF: &str = "minecraft:block.wooden_button.click_off";

/// The click of a tile that is on (`true`) or off (`false`) after the click.
fn for_state(on: bool) -> &'static str {
    if on { CLICK_ON } else { CLICK_OFF }
}

/// The sound of a click on a panel tile, or `None` for a click that plays none (a team's pad, a wall that hits no tile).
/// `state` is the panel as it is before the click: every toggle's state after the click is the negation of its state now.
pub fn click_sound(click: &Click, state: &PanelState) -> Option<&'static str> {
    Some(match click {
        Click::Panel(action) => action_sound(*action, state),
        Click::AutoShuffle => for_state(!state.auto_shuffle),
        Click::TeamsFewer | Click::TeamsMore => CLICK_ON,
        Click::Open(_) | Click::Start | Click::Reroll | Click::Shuffle | Click::ResetDefaults => CLICK_OFF,
        Click::Join(_) | Click::Nothing => return None,
    })
}

/// The sound of a rule tile's click: the toggles by their state after the click, the difficulty presets (on once the card
/// has the preset), the steppers on, and the goal's kind (a plain button) off.
fn action_sound(action: PanelAction, state: &PanelState) -> &'static str {
    match action {
        PanelAction::Mode(PanelMode::Lockout) => for_state(state.mode != Mode::Lockout),
        PanelAction::Mode(PanelMode::Inventory) => for_state(state.mode != Mode::Inventory),
        PanelAction::Mode(PanelMode::HiddenItems) => for_state(!state.hidden_items),
        PanelAction::Mode(PanelMode::ConsumeItems) => for_state(!state.consume_items),
        PanelAction::Difficulty(_) => CLICK_ON,
        PanelAction::GoalFewer | PanelAction::GoalMore | PanelAction::TimeFewer | PanelAction::TimeMore => CLICK_ON,
        PanelAction::GoalKind => CLICK_OFF,
    }
}

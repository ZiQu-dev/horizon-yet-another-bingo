//! The HUD's keys and what the HUD draws, from `BingoHudController` (`ClientTickEvent.End` and `HudRenderEvent`).
//!
//! Pure: the host reads the keys and the screen, and carries out the actions. `HudKeys::tick` is the handler of
//! `ClientTickEvent.End`, and `hud_visibility` is the gate of `HudRenderEvent`.
//!
//! Edge triggering: Kotlin's `KeyMapping.wasPressed()` counts presses between two ticks. Here a key is given as
//! held or not at each tick, and a press is a change from not held to held, so a press shorter than one tick is
//! lost. A held key fires once, as `wasPressed` does.

use crate::ready_update::ReadyUpdatePacket;

/// What the host reads this tick. Keys are the live state of the key (`KeyMapping.isDown`, `isPressed`), not presses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyInput {
    /// The card key (Y by default) is held: `openCardKey.isPressed()`.
    pub open_card_down: bool,
    /// The HUD toggle key is held (unbound by default).
    pub toggle_hud_down: bool,
    /// Sneak is held: `options.keyShift.isDown` (`OptionsAccessor.isSneakPressed`).
    pub sneak_down: bool,
    /// The game is paused (`Minecraft.isPaused`). A paused tick changes nothing.
    pub paused: bool,
    /// The chat screen is open (`client.screen is ChatScreen`).
    pub chat_screen_open: bool,
    /// A left click on the card while the chat screen was open (`onAfterLeftClick` with `isCardFocused()`). It is
    /// latched until a tick with the chat screen open, as `cardHudWasClicked` is.
    pub card_hud_clicked: bool,
}

/// What the host does for one action of a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Open the card screen (`openCardScreen` with the selected card's team).
    OpenCard,
    /// Send `SetReadyPacket(is_ready)`: the ready toggle, `!state.ready.isReady`.
    SendReady(bool),
    /// Flip `config.client.enableHud` and save the config.
    ToggleHud,
}

/// The state that the Kotlin handler keeps between ticks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HudKeys {
    /// `openCardKeyAlreadyPressed`: the card key is held since a press that was already handled.
    open_card_already_pressed: bool,
    /// The toggle key was held at the last tick (its presses are edges).
    toggle_hud_was_down: bool,
    /// `cardHudWasClicked`.
    card_hud_was_clicked: bool,
}

impl HudKeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// One tick of `ClientTickEvent.End`. Returns the actions in the order the Kotlin does them: the ready toggle,
    /// then the card, then the HUD toggle. A paused tick returns nothing and keeps the state as it was.
    ///
    /// `ready` is the last `ReadyUpdatePacket` the client got (`state.ready`), None before the first one.
    pub fn tick(&mut self, input: KeyInput, ready: Option<&ReadyUpdatePacket>) -> Vec<KeyAction> {
        if input.paused {
            return Vec::new();
        }
        let mut send_ready: Option<bool> = None;
        let mut open_card = false;

        // A click on the card in chat opens the card, once a tick with the chat open comes.
        self.card_hud_was_clicked |= input.card_hud_clicked;
        if self.card_hud_was_clicked && input.chat_screen_open {
            open_card = true;
            self.card_hud_was_clicked = false;
        }

        // The card key opens the card, or with sneak readies up when the player may (`canSendReady`). The ready
        // packet goes out only while the timer runs; sneak with the timer stopped does nothing at all.
        if input.open_card_down && !self.open_card_already_pressed {
            let may_ready = ready.is_some_and(|r| r.can_send_ready);
            if may_ready && input.sneak_down {
                if let Some(r) = ready.filter(|r| r.is_running) {
                    send_ready = Some(!r.is_ready);
                }
            } else {
                open_card = true;
            }
            // Holding the ready key must not toggle the ready packet again and again.
            self.open_card_already_pressed = true;
        } else if !input.open_card_down {
            self.open_card_already_pressed = false;
        }

        let mut actions = Vec::new();
        if let Some(is_ready) = send_ready {
            actions.push(KeyAction::SendReady(is_ready));
        }
        if open_card {
            actions.push(KeyAction::OpenCard);
        }
        // The HUD toggle fires on each press, with no anti-repeat of its own.
        if input.toggle_hud_down && !self.toggle_hud_was_down {
            actions.push(KeyAction::ToggleHud);
        }
        self.toggle_hud_was_down = input.toggle_hud_down;
        actions
    }
}

/// What the HUD draws this frame: the cards, and the ready line and the tooltip (both drawn by `drawHudInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudVisibility {
    pub card: bool,
    pub ready: bool,
    pub tooltip: bool,
}

/// What the render gate reads. Config values are the defaults unless the player changed them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudView {
    /// `config.client.enableHud` (default true).
    pub enable_hud: bool,
    /// `config.client.hideOnF3` (default true).
    pub hide_on_f3: bool,
    /// `config.client.hideOnChat` (default false).
    pub hide_on_chat: bool,
    /// The F1 hide: `client.gui.hud.isHidden` (`NativeClientState.gui_hidden`).
    pub hud_hidden: bool,
    /// A screen of the mod's own is open (the card screen), which the Kotlin tests by its package.
    pub own_screen_open: bool,
    /// F3's debug screen (`NativeClientState.debug_screen`).
    pub debug_screen: bool,
    /// The chat screen is open.
    pub chat_open: bool,
    /// The player list key is held (`keyPlayerList.isDown`, Tab by default).
    pub tab_list_down: bool,
}

/// The render gate of `HudRenderEvent`, for a view. Nothing is drawn with the F1 hide or with a mod screen up.
/// The cards need `enableHud`, and not F3 with `hideOnF3`, and not the chat with `hideOnChat`. The ready line and
/// the tooltip need no F3, no chat and no Tab list.
pub fn hud_visibility(view: HudView) -> HudVisibility {
    if view.hud_hidden || view.own_screen_open {
        return HudVisibility { card: false, ready: false, tooltip: false };
    }
    let card = view.enable_hud
        && !(view.hide_on_f3 && view.debug_screen)
        && !(view.hide_on_chat && view.chat_open);
    let info = !view.debug_screen && !view.chat_open && !view.tab_list_down;
    HudVisibility { card, ready: info, tooltip: info }
}

/// `hud_visibility` with the default config (`hideOnF3` on, `hideOnChat` off), no chat, the F1 hide off and no mod
/// screen. Use `hud_visibility` when any of those is not so.
pub fn hud_visible(enable_hud: bool, debug_screen: bool, tab_list_down: bool) -> HudVisibility {
    hud_visibility(HudView {
        enable_hud,
        hide_on_f3: true,
        hide_on_chat: false,
        hud_hidden: false,
        own_screen_open: false,
        debug_screen,
        chat_open: false,
        tab_list_down,
    })
}

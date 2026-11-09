//! The HUD's keys and render gate, against what `BingoHudController.kt` does (lines 440-530 of the handler, and
//! `HudRenderEvent`). Every expectation below was worked out by hand from the Kotlin, not from the code.

use horizon_bingo::game::{GameState, GameStatus};
use horizon_bingo::hud_keys::{hud_visibility, hud_visible, HudKeys, HudView, HudVisibility, KeyAction, KeyInput};
use horizon_bingo::ready_update::ReadyUpdatePacket;

/// A ready packet with the three flags the handler reads. The rest is filler.
fn ready(can_send_ready: bool, is_running: bool, is_ready: bool) -> ReadyUpdatePacket {
    ReadyUpdatePacket {
        is_running,
        is_ready,
        state: GameState { identifier: "pregame".into(), status: GameStatus::Pregame },
        remaining_millis: 0,
        total_millis: 0,
        ready_players: 0,
        total_players: 0,
        title: None,
        subtitle: None,
        can_send_ready,
        vote_options: Vec::new(),
        vote_pending: None,
    }
}

/// Input with nothing held, the chat closed and the game running.
fn idle() -> KeyInput {
    KeyInput::default()
}

fn y_down() -> KeyInput {
    KeyInput { open_card_down: true, ..idle() }
}

fn y_sneak_down() -> KeyInput {
    KeyInput { open_card_down: true, sneak_down: true, ..idle() }
}

fn hud_down() -> KeyInput {
    KeyInput { toggle_hud_down: true, ..idle() }
}

// Plain card key.

#[test]
fn pressing_the_card_key_opens_the_card_once() {
    let mut keys = HudKeys::new();
    assert_eq!(keys.tick(y_down(), None), vec![KeyAction::OpenCard]);
}

#[test]
fn holding_the_card_key_opens_the_card_only_on_the_first_tick() {
    let mut keys = HudKeys::new();
    assert_eq!(keys.tick(y_down(), None), vec![KeyAction::OpenCard]);
    for _ in 0..5 {
        assert_eq!(keys.tick(y_down(), None), vec![]);
    }
}

#[test]
fn releasing_and_pressing_again_opens_the_card_again() {
    let mut keys = HudKeys::new();
    assert_eq!(keys.tick(y_down(), None), vec![KeyAction::OpenCard]);
    assert_eq!(keys.tick(idle(), None), vec![]);
    assert_eq!(keys.tick(y_down(), None), vec![KeyAction::OpenCard]);
}

#[test]
fn sneak_without_the_card_key_does_nothing() {
    let mut keys = HudKeys::new();
    let sneak = KeyInput { sneak_down: true, ..idle() };
    assert_eq!(keys.tick(sneak, Some(&ready(true, true, false))), vec![]);
}

// The card key with sneak readies up, when the player may and the timer runs.

#[test]
fn sneak_and_card_key_send_ready_true_when_not_ready() {
    let mut keys = HudKeys::new();
    let r = ready(true, true, false);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![KeyAction::SendReady(true)]);
}

#[test]
fn sneak_and_card_key_send_ready_false_when_already_ready() {
    let mut keys = HudKeys::new();
    let r = ready(true, true, true);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![KeyAction::SendReady(false)]);
}

#[test]
fn sneak_and_card_key_do_not_open_the_card_when_readying() {
    let mut keys = HudKeys::new();
    let r = ready(true, true, false);
    let actions = keys.tick(y_sneak_down(), Some(&r));
    assert!(!actions.contains(&KeyAction::OpenCard));
}

#[test]
fn holding_sneak_and_card_key_sends_ready_once() {
    let mut keys = HudKeys::new();
    let r = ready(true, true, false);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![KeyAction::SendReady(true)]);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![]);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![]);
}

#[test]
fn sneak_and_card_key_with_the_timer_stopped_do_nothing() {
    // canSendReady is set but the timer does not run: Kotlin neither sends nor opens the card.
    let mut keys = HudKeys::new();
    let r = ready(true, false, false);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![]);
}

#[test]
fn sneak_and_card_key_without_permission_open_the_card() {
    let mut keys = HudKeys::new();
    let r = ready(false, true, false);
    assert_eq!(keys.tick(y_sneak_down(), Some(&r)), vec![KeyAction::OpenCard]);
}

#[test]
fn sneak_and_card_key_before_any_ready_packet_open_the_card() {
    let mut keys = HudKeys::new();
    assert_eq!(keys.tick(y_sneak_down(), None), vec![KeyAction::OpenCard]);
}

// The HUD toggle.

#[test]
fn pressing_the_hud_key_toggles_the_hud_once() {
    let mut keys = HudKeys::new();
    assert_eq!(keys.tick(hud_down(), None), vec![KeyAction::ToggleHud]);
    assert_eq!(keys.tick(hud_down(), None), vec![]);
    assert_eq!(keys.tick(idle(), None), vec![]);
    assert_eq!(keys.tick(hud_down(), None), vec![KeyAction::ToggleHud]);
}

#[test]
fn hud_key_and_card_key_together_give_both_actions_in_order() {
    let mut keys = HudKeys::new();
    let both = KeyInput { open_card_down: true, toggle_hud_down: true, ..idle() };
    assert_eq!(keys.tick(both, None), vec![KeyAction::OpenCard, KeyAction::ToggleHud]);
}

#[test]
fn ready_then_card_then_toggle_order_when_all_fire_in_one_tick() {
    // A card click (chat open), sneak with the card key, and the HUD key, in one tick.
    let mut keys = HudKeys::new();
    let r = ready(true, true, false);
    let all = KeyInput {
        open_card_down: true,
        sneak_down: true,
        toggle_hud_down: true,
        chat_screen_open: true,
        card_hud_clicked: true,
        ..idle()
    };
    assert_eq!(
        keys.tick(all, Some(&r)),
        vec![KeyAction::SendReady(true), KeyAction::OpenCard, KeyAction::ToggleHud]
    );
}

// The card click in chat.

#[test]
fn clicking_the_card_in_chat_opens_the_card() {
    let mut keys = HudKeys::new();
    let click = KeyInput { card_hud_clicked: true, chat_screen_open: true, ..idle() };
    assert_eq!(keys.tick(click, None), vec![KeyAction::OpenCard]);
    // The click is used up.
    let chat = KeyInput { chat_screen_open: true, ..idle() };
    assert_eq!(keys.tick(chat, None), vec![]);
}

#[test]
fn a_card_click_waits_for_the_chat_screen_to_be_open() {
    let mut keys = HudKeys::new();
    let click = KeyInput { card_hud_clicked: true, ..idle() };
    assert_eq!(keys.tick(click, None), vec![]);
    // The flag stays set (Kotlin only clears it when the chat is open), so the next chat tick opens the card.
    let chat = KeyInput { chat_screen_open: true, ..idle() };
    assert_eq!(keys.tick(chat, None), vec![KeyAction::OpenCard]);
}

// Paused: the Kotlin returns before anything, so nothing changes.

#[test]
fn a_paused_tick_does_nothing() {
    let mut keys = HudKeys::new();
    let paused = KeyInput { paused: true, open_card_down: true, toggle_hud_down: true, ..idle() };
    assert_eq!(keys.tick(paused, None), vec![]);
}

#[test]
fn a_key_held_through_a_pause_fires_after_it() {
    // The card key was not handled while paused, so it is still a new press when the game runs again.
    let mut keys = HudKeys::new();
    let paused = KeyInput { paused: true, open_card_down: true, ..idle() };
    assert_eq!(keys.tick(paused, None), vec![]);
    assert_eq!(keys.tick(y_down(), None), vec![KeyAction::OpenCard]);
}

#[test]
fn the_hud_key_pressed_during_a_pause_is_not_lost() {
    let mut keys = HudKeys::new();
    let paused = KeyInput { paused: true, toggle_hud_down: true, ..idle() };
    assert_eq!(keys.tick(paused, None), vec![]);
    assert_eq!(keys.tick(hud_down(), None), vec![KeyAction::ToggleHud]);
}

// Visibility, from the render gate.

fn view() -> HudView {
    HudView {
        enable_hud: true,
        hide_on_f3: true,
        hide_on_chat: false,
        hud_hidden: false,
        own_screen_open: false,
        debug_screen: false,
        chat_open: false,
        tab_list_down: false,
    }
}

fn all(card: bool, ready: bool, tooltip: bool) -> HudVisibility {
    HudVisibility { card, ready, tooltip }
}

#[test]
fn everything_is_drawn_by_default() {
    assert_eq!(hud_visibility(view()), all(true, true, true));
}

#[test]
fn turning_the_hud_off_keeps_the_ready_line_and_tooltip() {
    let v = HudView { enable_hud: false, ..view() };
    assert_eq!(hud_visibility(v), all(false, true, true));
}

#[test]
fn f3_hides_the_cards_and_the_info_by_default() {
    let v = HudView { debug_screen: true, ..view() };
    assert_eq!(hud_visibility(v), all(false, false, false));
}

#[test]
fn f3_with_hide_on_f3_off_keeps_the_cards() {
    let v = HudView { debug_screen: true, hide_on_f3: false, ..view() };
    assert_eq!(hud_visibility(v), all(true, false, false));
}

#[test]
fn holding_tab_hides_only_the_ready_line_and_tooltip() {
    let v = HudView { tab_list_down: true, ..view() };
    assert_eq!(hud_visibility(v), all(true, false, false));
}

#[test]
fn an_open_chat_keeps_the_cards_unless_hide_on_chat() {
    let open = HudView { chat_open: true, ..view() };
    assert_eq!(hud_visibility(open), all(true, false, false));
    let hiding = HudView { chat_open: true, hide_on_chat: true, ..view() };
    assert_eq!(hud_visibility(hiding), all(false, false, false));
}

#[test]
fn the_f1_hide_hides_everything() {
    let v = HudView { hud_hidden: true, ..view() };
    assert_eq!(hud_visibility(v), all(false, false, false));
}

#[test]
fn a_mod_screen_open_hides_everything() {
    let v = HudView { own_screen_open: true, ..view() };
    assert_eq!(hud_visibility(v), all(false, false, false));
}

#[test]
fn the_shortcut_uses_the_default_config() {
    assert_eq!(hud_visible(true, false, false), all(true, true, true));
    assert_eq!(hud_visible(false, false, false), all(false, true, true));
    assert_eq!(hud_visible(true, true, false), all(false, false, false));
    assert_eq!(hud_visible(true, false, true), all(true, false, false));
}

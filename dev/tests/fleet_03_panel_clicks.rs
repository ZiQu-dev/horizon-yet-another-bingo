//! The lobby panel's click sounds, the reroll's primed TNT and spam (`CardReroll.kt`), and the goal's kind switch at a
//! full card. Expected values are written out by hand from the original's `Button.kt`, `CardReroll.kt` and `Goal.kt`.
use std::time::{Duration, Instant};

use horizon_bingo::server::lobby_scene::{Click, PanelAction, PanelState};
use horizon_bingo::server::options::{Goal, Mode};
use horizon_bingo::server::panel_clicks::{self, CLICK_OFF, CLICK_ON};
use horizon_bingo::server::reroll_spam::{self, Effect, RerollSpam};
use horizon_bingo::server::texts::PanelMode;

fn state() -> PanelState {
    PanelState::default()
}

#[test]
fn auto_shuffle_click_plays_on_when_it_turns_on_and_off_when_it_turns_off() {
    let mut off = state();
    off.auto_shuffle = false;
    assert_eq!(panel_clicks::click_sound(&Click::AutoShuffle, &off), Some("minecraft:block.wooden_button.click_on"));
    let mut on = state();
    on.auto_shuffle = true;
    assert_eq!(panel_clicks::click_sound(&Click::AutoShuffle, &on), Some("minecraft:block.wooden_button.click_off"));
}

#[test]
fn mode_toggle_sounds_follow_the_state_after_the_click() {
    let mut s = state();
    s.mode = Mode::Standard;
    // Turning lockout on: on after the click.
    assert_eq!(panel_clicks::click_sound(&Click::Panel(PanelAction::Mode(PanelMode::Lockout)), &s), Some(CLICK_ON));
    s.mode = Mode::Lockout;
    // Turning lockout off: off after the click.
    assert_eq!(panel_clicks::click_sound(&Click::Panel(PanelAction::Mode(PanelMode::Lockout)), &s), Some(CLICK_OFF));
    s.consume_items = true;
    assert_eq!(panel_clicks::click_sound(&Click::Panel(PanelAction::Mode(PanelMode::ConsumeItems)), &s), Some(CLICK_OFF));
}

#[test]
fn plain_buttons_and_steppers_play_the_off_and_on_clicks_of_the_original() {
    let s = state();
    assert_eq!(panel_clicks::click_sound(&Click::Start, &s), Some(CLICK_OFF));
    assert_eq!(panel_clicks::click_sound(&Click::Reroll, &s), Some(CLICK_OFF));
    assert_eq!(panel_clicks::click_sound(&Click::Open(horizon_bingo::server::lobby_scene::PanelPage::Features), &s), Some(CLICK_OFF));
    assert_eq!(panel_clicks::click_sound(&Click::Panel(PanelAction::GoalKind), &s), Some(CLICK_OFF));
    assert_eq!(panel_clicks::click_sound(&Click::Panel(PanelAction::GoalMore), &s), Some(CLICK_ON));
    assert_eq!(panel_clicks::click_sound(&Click::TeamsMore, &s), Some(CLICK_ON));
    assert_eq!(panel_clicks::click_sound(&Click::Panel(PanelAction::Difficulty(0)), &s), Some(CLICK_ON));
}

#[test]
fn a_join_click_plays_no_button_sound() {
    assert_eq!(panel_clicks::click_sound(&Click::Join("bingo_red"), &state()), None);
}

#[test]
fn a_second_reroll_within_a_second_plays_the_primed_tnt_louder_each_time() {
    let mut spam = RerollSpam::default();
    let now = Instant::now();
    // The first reroll is quiet: nothing to repeat yet.
    assert_eq!(spam.reroll(now, false), Effect::Quiet);
    // The first repeat: volume 1/100, pitch 0.5 + 0.5 * 1/100.
    match spam.reroll(now, false) {
        Effect::Sound { volume, pitch } => {
            assert!((volume - 0.01).abs() < 1e-6);
            assert!((pitch - 0.505).abs() < 1e-6);
        }
        other => panic!("expected a sound, got {other:?}"),
    }
}

#[test]
fn a_reroll_after_a_pause_is_quiet_and_restarts_the_count() {
    let mut spam = RerollSpam::default();
    let now = Instant::now();
    spam.reroll(now, false);
    spam.reroll(now, false);
    assert_eq!(spam.reroll(now + Duration::from_secs(2), false), Effect::Quiet);
    // The count restarted: the next repeat is the first again.
    match spam.reroll(now + Duration::from_secs(2), false) {
        Effect::Sound { volume, .. } => assert!((volume - 0.01).abs() < 1e-6),
        other => panic!("expected a sound, got {other:?}"),
    }
}

#[test]
fn the_hundred_and_third_fast_reroll_spams_tnt() {
    let mut spam = RerollSpam::default();
    let now = Instant::now();
    // Reroll 1 is quiet, rerolls 2..=102 are sounds (the 102nd at volume 1.01), reroll 103 spams.
    assert_eq!(spam.reroll(now, false), Effect::Quiet);
    for _ in 2..=102 {
        assert!(matches!(spam.reroll(now, false), Effect::Sound { .. }));
    }
    assert_eq!(spam.reroll(now, false), Effect::Spam);
    // After the spam the count is back at zero: the next repeat is a quiet-sounding first repeat.
    match spam.reroll(now, false) {
        Effect::Sound { volume, .. } => assert!((volume - 0.01).abs() < 1e-6),
        other => panic!("expected a sound, got {other:?}"),
    }
}

#[test]
fn preventing_lobby_chaos_turns_the_reroll_sound_and_spam_off() {
    let mut spam = RerollSpam::default();
    let now = Instant::now();
    for _ in 0..200 {
        assert_eq!(spam.reroll(now, true), Effect::Quiet);
    }
}

#[test]
fn a_spam_summons_twenty_tnt_with_fuses_from_ten_to_fifty() {
    let low = reroll_spam::spam_commands("Steve", || 0.0);
    let high = reroll_spam::spam_commands("Steve", || 0.999_999);
    assert_eq!(low.len(), 20);
    assert!(low.iter().all(|line| line.starts_with("execute as Steve at @s run summon minecraft:tnt ") && line.ends_with("{fuse:10}")));
    assert!(high.iter().all(|line| line.ends_with("{fuse:50}")));
}

#[test]
fn a_full_card_of_lines_turns_into_a_full_card_of_items() {
    let mut s = state();
    s.goal = Goal::Lines(11);
    assert_eq!(horizon_bingo::server::lobby_scene::panel_command(PanelAction::GoalKind, &s).as_deref(), Some("goal full_card"));
    s.goal = Goal::Lines(3);
    assert_eq!(horizon_bingo::server::lobby_scene::panel_command(PanelAction::GoalKind, &s).as_deref(), Some("goal items 3"));
}

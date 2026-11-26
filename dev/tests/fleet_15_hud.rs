//! HUD details of the client, checked against the Kotlin they come from. Each expected value is worked out by hand in
//! its comment from the original source, not read back from the code under test.
use horizon_bingo::card_draw::team_outlines_shown;
use horizon_bingo::fireworks::{hud_centre, hud_range};
use horizon_bingo::host::text_argb;

/// `ReadyHudRenderer` pulses the title's alpha, and a message fades its text's alpha: both must reach the screen.
/// A faded colour keeps its alpha byte; a colour without one (0x00RRGGBB) is opaque.
#[test]
fn text_keeps_the_alpha_it_is_drawn_with() {
    assert_eq!(text_argb(0x80ff_0000), 0x80ff_0000);
    assert_eq!(text_argb(0x05ff_ffff), 0x05ff_ffff);
    assert_eq!(text_argb(0x00ff_0000), 0xffff_0000);
}

/// `ClientCardBufferRenderer.drawCard`: `cardTeamOutlines && ((!isGui && !showMultipleCards) || cards.size <= 1)`.
#[test]
fn team_outlines_hidden_when_several_cards_are_shown_side_by_side() {
    assert!(!team_outlines_shown(true, true, false, 2));
    assert!(team_outlines_shown(true, true, false, 1));
    assert!(team_outlines_shown(true, false, false, 3));
}

#[test]
fn team_outlines_follow_their_own_option() {
    assert!(!team_outlines_shown(false, false, false, 1));
}

/// `(100 * cardScale).toInt()`: 100 * 0.555 is 55.5 as a Float, and toInt drops the half.
#[test]
fn hud_firework_range_is_truncated_from_the_card_scale() {
    assert_eq!(hud_range(0.555), 55.0);
    assert_eq!(hud_range(1.0), 100.0);
}

/// `centerX = x.toInt() + CARD_WIDTH / 2` = 10 + 61 = 71 and `centerY = y.toInt() + CARD_HEIGHT / 2` = 20 + 66 = 86,
/// whatever the card scale: the half size is the unscaled one, and the origin is truncated.
#[test]
fn hud_firework_centre_uses_the_unscaled_half_size_of_the_truncated_origin() {
    assert_eq!(hud_centre(10.0, 20.0), (71, 86));
    assert_eq!(hud_centre(10.9, 20.2), (71, 86));
}

//! The title screen's "BINGO!" button follows the `showQuickStartButton` option of `ClientConfig.kt` (default true,
//! `BingoWorldManager.kt:37`: the button is added only when the option is on). Expected values come from that file.
use horizon_bingo::config::ClientConfig;
use horizon_bingo::title_button::is_offered;

#[test]
fn button_is_offered_by_default() {
    assert!(is_offered(&ClientConfig::default()));
}

#[test]
fn button_is_hidden_when_the_option_is_off_in_the_config_file() {
    let config = ClientConfig::from_json(r#"{"showQuickStartButton": false}"#);
    assert!(!is_offered(&config));
}

#[test]
fn button_is_offered_when_the_option_is_on_in_the_config_file() {
    let config = ClientConfig::from_json(r#"{"showQuickStartButton": true}"#);
    assert!(is_offered(&config));
}

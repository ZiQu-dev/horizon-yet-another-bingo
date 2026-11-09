//! Reading the JSON text the mod sends beyond the exact pretty form. Expected values follow the
//! Kotlin defaults (the data class constructor), not the port's code.

use horizon_bingo::buf::DecodeError;
use horizon_bingo::update_settings::PlayerSettings;

#[test]
fn reads_compact_and_crlf_text_as_the_same_settings() {
    let want = PlayerSettings { bossbar: true, ..PlayerSettings::default() };
    let compact = r#"{"seenTutorial":false,"hideLobbyPrompt":false,"bossbar":true,"scoreboard":true,"scoreboardAutoHide":false,"leadingMessages":true,"scoreMessages":true,"itemMessages":true,"nightVision":true}"#;
    assert_eq!(PlayerSettings::from_json(compact), Ok(want.clone()));
    let crlf = "{\r\n\t\"bossbar\" :  true\r\n}";
    assert_eq!(PlayerSettings::from_json(crlf), Ok(want));
}

#[test]
fn unknown_keys_of_any_shape_are_skipped() {
    let text = r#"{
        "futureObject": {"a": "} still a string {", "b": [1, -2.5e3, null, {"c": "\"q\""}]},
        "bossbar": true,
        "futureFlag": false,
        "futureText": "}"
    }"#;
    let want = PlayerSettings { bossbar: true, ..PlayerSettings::default() };
    assert_eq!(PlayerSettings::from_json(text), Ok(want));
}

#[test]
fn missing_keys_take_the_kotlin_defaults() {
    assert_eq!(PlayerSettings::from_json("{}"), Ok(PlayerSettings::default()));
    let want = PlayerSettings { night_vision: false, ..PlayerSettings::default() };
    assert_eq!(PlayerSettings::from_json(r#"{"nightVision": false}"#), Ok(want));
}

#[test]
fn item_messages_defaults_to_the_decoded_score_messages() {
    let off = PlayerSettings { score_messages: false, item_messages: false, ..PlayerSettings::default() };
    assert_eq!(PlayerSettings::from_json(r#"{"scoreMessages": false}"#), Ok(off));
    let on = PlayerSettings { score_messages: false, item_messages: true, ..PlayerSettings::default() };
    assert_eq!(
        PlayerSettings::from_json(r#"{"scoreMessages": false, "itemMessages": true}"#),
        Ok(on)
    );
}

#[test]
fn malformed_text_is_rejected() {
    for bad in [
        r#"{"bossbar": 1}"#,
        r#"{"bossbar": true,}"#,
        r#"{"bossbar": true} x"#,
        r#"{"bossbar" true}"#,
        r#"{"bossbar": tru}"#,
        "",
    ] {
        assert!(
            matches!(PlayerSettings::from_json(bad), Err(DecodeError::InvalidJson(_))),
            "accepted {bad:?}"
        );
    }
}

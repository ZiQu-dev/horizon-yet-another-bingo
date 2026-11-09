//! Each row of oracle.tsv was written by the real `PlayerSettings.V2` converter over Minecraft's
//! FriendlyByteBuf (OracleUpdateSettings.java). The expected values below are typed in by hand
//! from the Kotlin data class, not computed by the port.

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::update_settings::PlayerSettings;

const TSV: &str = include_str!("oracle/update_settings.tsv");

/// Field order: seenTutorial, hideLobbyPrompt, bossbar, scoreboard, scoreboardAutoHide,
/// leadingMessages, scoreMessages, itemMessages, nightVision.
fn settings(v: [bool; 9]) -> PlayerSettings {
    PlayerSettings {
        seen_tutorial: v[0],
        hide_lobby_prompt: v[1],
        bossbar: v[2],
        scoreboard: v[3],
        scoreboard_auto_hide: v[4],
        leading_messages: v[5],
        score_messages: v[6],
        item_messages: v[7],
        night_vision: v[8],
    }
}

/// The value each oracle row was built from.
fn case(name: &str) -> PlayerSettings {
    let (f, t) = (false, true);
    let v = match name {
        "defaults" => [f, f, f, t, f, t, t, t, t],
        "all_false" => [f, f, f, f, f, f, f, f, f],
        "all_true" => [t, t, t, t, t, t, t, t, t],
        "flip_seenTutorial" => [t, f, f, t, f, t, t, t, t],
        "flip_hideLobbyPrompt" => [f, t, f, t, f, t, t, t, t],
        "flip_bossbar" => [f, f, t, t, f, t, t, t, t],
        "flip_scoreboard" => [f, f, f, f, f, t, t, t, t],
        "flip_scoreboardAutoHide" => [f, f, f, t, t, t, t, t, t],
        "flip_leadingMessages" => [f, f, f, t, f, f, t, t, t],
        "flip_scoreMessages" => [f, f, f, t, f, t, f, t, t],
        "flip_itemMessages" => [f, f, f, t, f, t, t, f, t],
        "flip_nightVision" => [f, f, f, t, f, t, t, t, f],
        "score_off_item_on" => [f, f, f, t, f, t, f, t, t],
        "score_on_item_off" => [f, f, f, t, f, t, t, f, t],
        other => panic!("no expected value for oracle row {other}"),
    };
    settings(v)
}

fn from_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "odd hex length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit"))
        .collect()
}

#[test]
fn oracle_has_every_case_and_no_more() {
    let names: Vec<&str> = TSV.lines().map(|l| l.split('\t').next().unwrap()).collect();
    assert_eq!(names.len(), 14);
    for name in names {
        case(name);
    }
}

#[test]
fn every_oracle_row_matches_the_port() {
    for line in TSV.lines() {
        let mut cols = line.split('\t');
        let name = cols.next().unwrap();
        let channel = cols.next().unwrap();
        let hex = cols.next().unwrap();
        let value = case(name);

        assert_eq!(channel, PlayerSettings::ID, "{name}: channel id");

        let bytes = encode(|w| value.write(w));
        assert_eq!(hex_string(&bytes), hex, "{name}: bytes written by the port");

        let oracle_bytes = from_hex(hex);
        assert_eq!(decode(&oracle_bytes, PlayerSettings::read), Ok(value), "{name}: read back");
    }
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

//! Checks the port against the real mod. Each row of `oracle.tsv` is `name TAB channel TAB hex`,
//! printed by `OracleCardReset.java` from the mod's own `CardResetPacket.V1` converter.

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::card_reset::CardResetPacket;

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd hex length in oracle row: {s:?}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("bad hex in oracle row"))
        .collect()
}

fn rows() -> Vec<(String, String, Vec<u8>)> {
    include_str!("oracle/card_reset.tsv")
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut cols = line.splitn(3, '\t');
            let name = cols.next().expect("name").to_string();
            let channel = cols.next().expect("channel").to_string();
            let hex = cols.next().expect("hex column present, may be empty");
            (name, channel, unhex(hex))
        })
        .collect()
}

/// The Rust value that each oracle row name stands for. An unknown name fails, so no row is skipped silently.
fn value_for(name: &str) -> CardResetPacket {
    match name {
        "card_reset" => CardResetPacket,
        other => panic!("oracle row {other:?} has no Rust value"),
    }
}

#[test]
fn oracle_has_the_card_reset_row() {
    assert_eq!(rows().len(), 1);
}

#[test]
fn write_matches_the_mod_bytes_and_channel() {
    for (name, channel, want) in rows() {
        let packet = value_for(&name);
        let got = encode(|w| packet.write(w));
        assert_eq!(got, want, "{name}: bytes differ from the mod");
        assert_eq!(CardResetPacket::ID, channel, "{name}: channel id differs from the mod");
    }
}

#[test]
fn read_returns_the_value_from_the_mod_bytes() {
    for (name, _channel, bytes) in rows() {
        let got = decode(&bytes, CardResetPacket::read);
        assert_eq!(got, Ok(value_for(&name)), "{name}: read does not give the value back");
    }
}

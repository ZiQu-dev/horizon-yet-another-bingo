//! Checks `CardShuffledPacket` against the rows the real Kotlin converter wrote (tests/oracle.tsv,
//! produced by OracleCardShuffled.java).

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::card_shuffled::CardShuffledPacket;

struct Row {
    name: String,
    channel: String,
    hex: String,
}

fn oracle_rows() -> Vec<Row> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle/card_shuffled.tsv");
    let text = std::fs::read_to_string(path).expect("tests/oracle/card_shuffled.tsv");
    text.lines()
        .map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next().expect("name").to_string();
            let channel = cols.next().expect("channel").to_string();
            let hex = cols.next().unwrap_or("").to_string();
            assert!(cols.next().is_none(), "row {name} has extra columns");
            Row { name, channel, hex }
        })
        .collect()
}

/// The value each oracle row was produced from.
fn case(name: &str) -> CardShuffledPacket {
    let team_key = match name {
        "none" => None,
        "empty_string" => Some(String::new()),
        "red" => Some("bingo_red".to_string()),
        "non_ascii" => Some("bingo_\u{e9}\u{8336}\u{1F642}".to_string()),
        "long_200" => Some(format!("bingo_{}", "x".repeat(194))),
        other => panic!("no Rust case for oracle row {other}"),
    };
    CardShuffledPacket { team_key }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn every_oracle_row_matches_the_kotlin_bytes_and_channel() {
    let rows = oracle_rows();
    assert_eq!(rows.len(), 5, "expected the five oracle rows");
    for row in &rows {
        let packet = case(&row.name);
        let bytes = encode(|w| packet.write(w));
        assert_eq!(to_hex(&bytes), row.hex, "bytes for {}", row.name);
        assert_eq!(CardShuffledPacket::ID, row.channel, "channel for {}", row.name);
    }
}

#[test]
fn oracle_bytes_read_back_to_the_same_value() {
    for row in oracle_rows() {
        let bytes: Vec<u8> = (0..row.hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&row.hex[i..i + 2], 16).expect("hex"))
            .collect();
        let packet = case(&row.name);
        assert_eq!(decode(&bytes, CardShuffledPacket::read), Ok(packet), "reading {}", row.name);
    }
}

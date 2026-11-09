//! Checks the Rust `CardOpenPacket` against the bytes the real mod's converter produced
//! (tests/oracle.tsv, written by OracleOpenCard.java). Each row is `name TAB channel TAB hex`.

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::open_card::CardOpenPacket;

const ORACLE: &str = include_str!("oracle/open_card.tsv");

/// The value each oracle case was made from, written here by hand from the case names.
fn case(name: &str) -> CardOpenPacket {
    let team_key = match name {
        "null_key" => None,
        "empty_key" => Some(String::new()),
        "ascii_key" => Some("bingo_red".to_string()),
        "non_ascii_key" => Some("bingo_\u{e9}\u{6f22}".to_string()),
        "long_key" => Some(format!("bingo_{}", "x".repeat(194))),
        other => panic!("no Rust value for oracle case {other:?}"),
    };
    CardOpenPacket { team_key }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit"))
        .collect()
}

#[test]
fn every_oracle_row_matches_the_real_converter() {
    let mut rows = 0;
    for line in ORACLE.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "malformed oracle row {line:?}");
        let (name, channel, hex) = (fields[0], fields[1], fields[2]);
        let packet = case(name);

        assert_eq!(CardOpenPacket::ID, channel, "channel id for {name}");

        let bytes = encode(|w| packet.write(w));
        assert_eq!(to_hex(&bytes), hex, "bytes written for {name}");

        let oracle_bytes = from_hex(hex);
        assert_eq!(decode(&oracle_bytes, CardOpenPacket::read), Ok(packet), "reading oracle bytes for {name}");
        rows += 1;
    }
    assert_eq!(rows, 5, "oracle rows read");
}

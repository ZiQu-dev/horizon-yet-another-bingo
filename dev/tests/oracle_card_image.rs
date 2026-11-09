//! Checks the port against bytes written by the real mod's CardImagePacket.V1 converter
//! (tests/oracle.tsv, made by OracleCardImage.java).

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::card_image::CardImagePacket;

/// Same formula as `OracleCardImage.pattern`: byte i is (i * 37 + 11) truncated to 8 bits.
fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 37 + 11) as u8).collect()
}

/// The value each oracle row was written from.
fn value_for(name: &str) -> CardImagePacket {
    let packet = |id: &str, image: Option<Vec<u8>>| CardImagePacket { id: id.to_string(), image };
    match name {
        "none_empty_id" => packet("", None),
        "empty_image" => packet("", Some(Vec::new())),
        "one_byte" => packet("card", Some(vec![0x5A])),
        "high_bit_bytes" => packet("b", Some(vec![0x00, 0x80, 0xFF, 0x7F])),
        "nonascii_id_absent" => packet("kartë ✓ 🙂", None),
        "image_128" => packet("map-1", Some(pattern(128))),
        "image_300" => packet("card-42", Some(pattern(300))),
        "image_16384" => packet("card-99", Some(pattern(16384))),
        other => panic!("no Rust value for oracle row {other}"),
    }
}

fn read_rows() -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle/card_image.tsv"))
        .expect("tests/oracle/card_image.tsv");
    text.lines()
        .map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next().expect("name").to_string();
            let channel = cols.next().expect("channel").to_string();
            let hex = cols.next().expect("hex").to_string();
            assert!(cols.next().is_none(), "row {name} has extra columns");
            (name, channel, hex)
        })
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &str) -> Vec<u8> {
    assert!(hex.len() % 2 == 0, "odd hex length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit"))
        .collect()
}

#[test]
fn every_oracle_row_is_written_identically() {
    let rows = read_rows();
    assert_eq!(rows.len(), 8, "oracle row count");
    for (name, channel, hex) in rows {
        let value = value_for(&name);
        assert_eq!(channel, CardImagePacket::ID, "channel of {name}");
        let bytes = encode(|w| value.write(w));
        assert_eq!(to_hex(&bytes), hex, "bytes of {name}");
    }
}

#[test]
fn every_oracle_row_reads_back_to_its_value() {
    for (name, _channel, hex) in read_rows() {
        let value = value_for(&name);
        let decoded = decode(&from_hex(&hex), CardImagePacket::read);
        assert_eq!(decoded, Ok(value), "reading the oracle bytes of {name}");
    }
}

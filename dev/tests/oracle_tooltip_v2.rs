//! `TooltipPacket` against the rows the mod's real Kotlin converter wrote (tests/oracle.tsv, produced by
//! OracleTooltipV2.java through the mod's own PacketBufImpl). Each expected value below is written by hand
//! from the Kotlin field list and the case name, not derived from the code under test.

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;
use horizon_bingo::tooltip_v2::{TooltipPacket, ID};

const TSV: &str = include_str!("oracle/tooltip_v2.tsv");

struct Row {
    name: &'static str,
    channel: &'static str,
    hex: &'static str,
}

fn oracle_rows() -> Vec<Row> {
    TSV.lines()
        .map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next().expect("name");
            let channel = cols.next().expect("channel");
            let hex = cols.next().unwrap_or("");
            assert!(cols.next().is_none(), "row {name} has extra columns");
            Row { name, channel, hex }
        })
        .collect()
}

fn plain(value: &str) -> Text {
    Text::plain(value)
}

fn styled(entries: Vec<(&str, Nbt)>) -> Text {
    Text(Nbt::compound(entries))
}

const TYPICAL_UUID: u128 = 0x123e_4567_e89b_12d3_a456_4266_1417_4000;

/// The value each oracle row was produced from.
fn case(name: &str) -> TooltipPacket {
    match name {
        "empty_list" => TooltipPacket { text: vec![], entity_id: TYPICAL_UUID },
        "one_plain" => TooltipPacket { text: vec![plain("hello")], entity_id: TYPICAL_UUID },
        "three_styled" => TooltipPacket {
            text: vec![
                // Yellow "hi": color before text, as the mod's codec writes it.
                styled(vec![("color", Nbt::string("yellow")), ("text", Nbt::string("hi"))]),
                plain("za\u{17c}\u{f3}\u{142}\u{107}"),
                // Bold "b".
                styled(vec![("text", Nbt::string("b")), ("bold", Nbt::Byte(1))]),
            ],
            entity_id: 1,
        },
        "empty_text_in_list" => TooltipPacket {
            text: vec![plain(""), plain("a"), plain("")],
            entity_id: TYPICAL_UUID,
        },
        "text_with_sibling" => TooltipPacket {
            // "a" followed by a bold "b" as an `extra` sibling.
            text: vec![styled(vec![
                ("extra", Nbt::list(vec![Nbt::compound(vec![
                    ("text", Nbt::string("b")),
                    ("bold", Nbt::Byte(1)),
                ])])),
                ("text", Nbt::string("a")),
            ])],
            entity_id: TYPICAL_UUID,
        },
        "high_bit_uuid" => TooltipPacket {
            text: vec![plain("x")],
            entity_id: 0x8000_0000_0000_0001_ffff_ffff_0000_0002,
        },
        "all_ones_uuid" => TooltipPacket { text: vec![plain("x")], entity_id: u128::MAX },
        "nil_uuid" => TooltipPacket { text: vec![], entity_id: 0 },
        other => panic!("no Rust case for oracle row {other}"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

#[test]
fn every_oracle_row_matches_the_kotlin_bytes_and_channel() {
    let rows = oracle_rows();
    assert_eq!(rows.len(), 8, "expected the eight oracle rows");
    for row in &rows {
        let packet = case(row.name);
        let bytes = encode(|w| packet.write(w).unwrap());
        assert_eq!(to_hex(&bytes), row.hex, "bytes for {}", row.name);
        assert_eq!(ID, row.channel, "channel for {}", row.name);
    }
}

#[test]
fn oracle_bytes_read_back_to_the_same_value() {
    for row in oracle_rows() {
        let bytes = from_hex(row.hex);
        assert_eq!(decode(&bytes, TooltipPacket::read), Ok(case(row.name)), "reading {}", row.name);
    }
}

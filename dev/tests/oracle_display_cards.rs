//! Each row of tests/oracle.tsv was written by the real mod's CardDisplayPacket.V1 (OracleDisplayCards.java over
//! the mod's PacketBufImpl). Every expected value below is typed from the Kotlin meaning, not from the code under test.

use horizon_bingo::display_cards::{CardDisplay, CardDisplayPacket, PlayerProfile, FORMATTING_NAMES, ID};
use horizon_bingo::buf::{decode, encode};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;

const TSV: &str = include_str!("oracle/display_cards.tsv");

const ALEX: u128 = 0xee305221_9b0e_3305_b140_17edb555b9fb;

fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    TSV.lines()
        .map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next().unwrap();
            let channel = cols.next().unwrap();
            let hex = cols.next().unwrap_or("");
            assert!(cols.next().is_none(), "row {name} has extra columns");
            (name, channel, hex)
        })
        .collect()
}

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn plain(value: &str) -> Text {
    Text::plain(value)
}

fn player(uuid: u128, name: &str) -> PlayerProfile {
    PlayerProfile { uuid, name: name.to_string() }
}

fn display(color: Option<&str>, name: Option<Text>, players: Vec<PlayerProfile>) -> CardDisplay {
    CardDisplay { team_color: color.map(str::to_string), team_name: name, players }
}

fn packet(entries: Vec<(Option<&str>, CardDisplay)>) -> CardDisplayPacket {
    CardDisplayPacket { display: entries.into_iter().map(|(key, d)| (key.map(str::to_string), d)).collect() }
}

/// The value each oracle row was produced from.
fn case(name: &str) -> CardDisplayPacket {
    match name {
        "empty_map" => packet(vec![]),
        "null_key_all_null" => packet(vec![(None, display(None, None, vec![]))]),
        "key_no_color_empty_text" => packet(vec![(Some("bingo_red"), display(None, Some(plain("")), vec![]))]),
        "red_plain_one_player" => packet(vec![(
            Some("bingo_red"),
            display(Some("RED"), Some(plain("Red Team")), vec![player(ALEX, "RegTest_control")]),
        )]),
        // writeText(null) is the empty component, so this reads back as an empty text.
        "absent_name_with_color" => packet(vec![(Some("bingo_blue"), display(Some("BLUE"), None, vec![]))]),
        // Compound entries in the order the mod wrote them: color, text, bold.
        "styled_text_color_bold" => packet(vec![(
            Some("bingo_gold"),
            display(
                Some("GOLD"),
                Some(Text(Nbt::compound(vec![
                    ("color", Nbt::string("red")),
                    ("text", Nbt::string("Gold")),
                    ("bold", Nbt::Byte(1)),
                ]))),
                vec![],
            ),
        )]),
        "non_ascii" => packet(vec![(
            Some("bingo_\u{e9}\u{8336}\u{1F642}"),
            display(
                Some("AQUA"),
                Some(plain("za\u{17c}\u{f3}\u{142}\u{107} \u{65e5}\u{672c} \u{1F600}")),
                vec![player(1, "Zo\u{eb}\u{8336}\u{1F642}")],
            ),
        )]),
        "uuid_high_bit" => packet(vec![(
            Some("bingo_red"),
            display(Some("RED"), Some(plain("x")), vec![player(0x8000_0000_0000_0000_FFFF_FFFF_FFFF_FFFF, "high")]),
        )]),
        "uuid_zero_empty_name" => packet(vec![(Some("bingo_red"), display(Some("RED"), Some(plain("x")), vec![player(0, "")]))]),
        "three_players" => packet(vec![(
            Some("bingo_green"),
            display(
                Some("GREEN"),
                Some(plain("Green")),
                vec![
                    player(ALEX, "Alex"),
                    player(0x0a1b_2c3d_4e5f_4a6b_8c7d_9e0f_1a2b_3c4d, "Bob"),
                    player(u128::MAX, "Carol"),
                ],
            ),
        )]),
        "three_entries_null_first" => packet(vec![
            (None, display(None, None, vec![])),
            (Some("bingo_red"), display(Some("RED"), Some(plain("Red")), vec![player(ALEX, "A")])),
            (Some("bingo_blue"), display(Some("BLUE"), Some(plain("Blue")), vec![])),
        ]),
        other => {
            let color = other.strip_prefix("format_").unwrap_or_else(|| panic!("no Rust case for oracle row {other}"));
            let key = format!("bingo_{}", color.to_lowercase());
            packet(vec![(Some(&key), display(Some(color), Some(plain(color)), vec![]))])
        }
    }
}

/// What a read gives back for a value: the wire has no absent text, so `None` reads as the empty component.
fn as_read(value: &CardDisplayPacket) -> CardDisplayPacket {
    CardDisplayPacket {
        display: value
            .display
            .iter()
            .map(|(key, d)| {
                let mut d = d.clone();
                if d.team_name.is_none() {
                    d.team_name = Some(Text::empty());
                }
                (key.clone(), d)
            })
            .collect(),
    }
}

#[test]
fn every_row_is_written_and_read_like_the_mod() {
    let rows = rows();
    assert_eq!(rows.len(), 33, "11 structural cases and one row per Formatting value");
    for (name, channel, hex) in rows {
        assert_eq!(channel, ID, "{name}: channel id");
        let value = case(name);
        let written = encode(|w| value.write(w).unwrap());
        assert_eq!(hex_of(&written), hex, "{name}: written bytes");
        let read_back = decode(&unhex(hex), CardDisplayPacket::read).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(read_back, as_read(&value), "{name}: read back");
    }
}

#[test]
fn the_oracle_has_every_formatting_value() {
    let mut from_rows: Vec<&str> =
        rows().iter().filter_map(|(name, _, _)| name.strip_prefix("format_")).collect();
    from_rows.sort_unstable();
    let mut known = FORMATTING_NAMES.to_vec();
    known.sort_unstable();
    assert_eq!(from_rows, known);
    assert_eq!(known.len(), 22);
}

#[test]
fn an_unknown_color_name_reads_as_no_color_and_the_rest_is_kept() {
    // The mod's `valueOf` throws on "PURPLE", which the converter turns into null. The bytes after it are read.
    // Expected rewrite: taken from the real mod's fromPacketBuf -> toPacketBuf (OracleDisplayCardsRead.java).
    let bytes = unhex("00000001000106505552504c4508000000000000");
    let value = decode(&bytes, CardDisplayPacket::read).unwrap();
    assert_eq!(value.display.len(), 1);
    assert_eq!(value.display[0].1.team_color, None);
    assert_eq!(hex_of(&encode(|w| value.write(w).unwrap())), "00000001000008000000000000");
}

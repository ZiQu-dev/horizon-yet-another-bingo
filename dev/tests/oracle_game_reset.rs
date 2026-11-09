//! Checks the Rust codec against the rows the real Kotlin converter wrote (tests/oracle.tsv,
//! produced by OracleGameReset.java). Each row is `name TAB channel-id TAB hex`.

use horizon_bingo::buf::{decode, encode, Reader};
use horizon_bingo::game_reset::GameResetPacket;

const ORACLE: &str = include_str!("oracle/game_reset.tsv");

struct Row<'a> {
    name: &'a str,
    channel: &'a str,
    hex: &'a str,
}

fn rows() -> Vec<Row<'static>> {
    ORACLE
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut parts = line.split('\t');
            let name = parts.next().expect("row has a name");
            let channel = parts.next().expect("row has a channel id");
            let hex = parts.next().expect("row has hex");
            assert!(parts.next().is_none(), "row has extra columns: {line:?}");
            Row { name, channel, hex }
        })
        .collect()
}

fn from_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "odd hex length in {hex:?}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex digit"))
        .collect()
}

fn row(name: &str) -> Row<'static> {
    rows().into_iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no oracle row {name}"))
}

#[test]
fn oracle_has_the_three_expected_rows() {
    let mut names: Vec<_> = rows().iter().map(|r| r.name).collect();
    names.sort_unstable();
    assert_eq!(names, ["game_reset", "game_reset_read_empty", "game_reset_read_trailing"]);
}

#[test]
fn channel_id_matches_the_oracle_and_the_hand_written_path() {
    // Hand-written from `MOD_ID_BINGO` + "game_reset" in the Kotlin source, as a second check.
    assert_eq!(GameResetPacket::ID, "yet-another-minecraft-bingo:game_reset");
    for r in rows() {
        assert_eq!(r.channel, GameResetPacket::ID, "channel of {}", r.name);
    }
}

#[test]
fn write_matches_the_oracle_bytes() {
    let r = row("game_reset");
    let bytes = encode(|w| GameResetPacket.write(w));
    assert_eq!(bytes, from_hex(r.hex), "bytes of {}", r.name);
    assert!(bytes.is_empty(), "the packet has no fields");
}

#[test]
fn read_accepts_the_oracle_empty_payload_and_returns_the_value() {
    let r = row("game_reset_read_empty");
    assert_eq!(decode(&from_hex(r.hex), GameResetPacket::read), Ok(GameResetPacket));
}

#[test]
fn read_ignores_trailing_bytes_like_the_kotlin_reader() {
    // The Kotlin converter reads nothing and does not check for leftovers. `decode` in buf.rs
    // is stricter (it rejects leftovers), so this test reads through `Reader` directly.
    let r = row("game_reset_read_trailing");
    let bytes = from_hex(r.hex);
    let mut reader = Reader::new(&bytes);
    assert_eq!(GameResetPacket::read(&mut reader), Ok(GameResetPacket));
}

#[test]
fn write_then_read_round_trips() {
    let packet = GameResetPacket;
    let bytes = encode(|w| packet.write(w));
    assert_eq!(decode(&bytes, GameResetPacket::read), Ok(packet));
}

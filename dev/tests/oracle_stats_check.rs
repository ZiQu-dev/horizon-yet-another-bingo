//! Checks the Rust `StatsCheckPacket` against the real mod. `oracle.tsv` is printed by
//! `OracleStatsCheck.java`, which calls the mod's own `StatsCheckPacket.V1` on Minecraft's `FriendlyByteBuf`.

use horizon_bingo::buf::{decode, encode, DecodeError, Reader};
use horizon_bingo::stats_check::StatsCheckPacket;

const TSV: &str = include_str!("oracle/stats_check.tsv");

/// The first 16 bytes of a `stats_check` packet the real server sent (130 bytes long).
const REAL_SERVER_PREFIX: &str = "80016366383365313335376565666238";

/// The value each oracle row was built from, written out again here rather than parsed from the TSV.
fn case_value(name: &str) -> String {
    match name {
        "stats_check_empty" => String::new(),
        "stats_check_one_char" => "a".to_string(),
        // SHA-512 of the empty input, from `sha512sum`.
        "stats_check_sha512_empty" => concat!(
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce",
            "47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        )
        .to_string(),
        "stats_check_len127" => "a".repeat(127),
        "stats_check_len128" => "a".repeat(128),
        "stats_check_len16384" => "b".repeat(16384),
        // 2-byte, 3-byte and 4-byte UTF-8 sequences.
        "stats_check_non_ascii" => "h\u{e9}llo \u{2603} \u{1D11E}".to_string(),
        other => panic!("no Rust value for oracle row {other}"),
    }
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

/// Splits one TSV row into (name, channel id, hex bytes).
fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    TSV.lines()
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 3, "malformed oracle row: {line}");
            (fields[0], fields[1], fields[2])
        })
        .collect()
}

fn oracle_bytes(name: &str) -> Vec<u8> {
    let (_, _, hex) = rows().into_iter().find(|(n, _, _)| *n == name).expect("row exists");
    from_hex(hex)
}

#[test]
fn every_oracle_row_is_written_identically_and_reads_back() {
    let rows = rows();
    assert_eq!(rows.len(), 7);
    for (name, channel, hex) in rows {
        let packet = StatsCheckPacket { hash_sha512: case_value(name) };
        assert_eq!(StatsCheckPacket::ID, channel, "{name}: channel id");

        let bytes = encode(|w| packet.write(w));
        assert_eq!(to_hex(&bytes), hex, "{name}: written bytes");

        assert_eq!(decode(&from_hex(hex), StatsCheckPacket::read), Ok(packet.clone()), "{name}: oracle bytes read back");
        assert_eq!(decode(&bytes, StatsCheckPacket::read), Ok(packet), "{name}: own bytes read back");
    }
}

#[test]
fn real_server_packet_header_is_a_varint_length_of_128() {
    // 0x80 0x01 is a VarInt of 128: the hex string after it is 128 bytes, so the packet is 2 + 128 = 130 bytes.
    let header = from_hex(REAL_SERVER_PREFIX);
    let mut reader = Reader::new(&header);
    assert_eq!(reader.read_varint(), Ok(128));
}

#[test]
fn a_130_byte_packet_with_the_real_prefix_reads_as_a_128_char_hex_digest() {
    // Only the first 16 bytes of the real packet are known. The 130-byte row that the mod
    // produced for the SHA-512 of the empty input starts with exactly those bytes.
    let bytes = oracle_bytes("stats_check_sha512_empty");
    assert_eq!(bytes.len(), 130);
    assert_eq!(to_hex(&bytes[..16]), REAL_SERVER_PREFIX);
    let packet = decode(&bytes, StatsCheckPacket::read).expect("reads");
    assert_eq!(packet.hash_sha512.len(), 128);
    assert!(packet.hash_sha512.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn truncated_string_is_rejected() {
    let bytes = oracle_bytes("stats_check_sha512_empty");
    assert_eq!(
        decode(&bytes[..100], StatsCheckPacket::read),
        Err(DecodeError::UnexpectedEnd { needed: 128, remaining: 98 })
    );
}

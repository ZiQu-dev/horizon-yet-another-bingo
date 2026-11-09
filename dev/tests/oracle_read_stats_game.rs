//! Reads hand-written payloads with the port and compares what it writes back with what the real
//! mod does. tests/oracle_read.tsv comes from OracleStatsGameRead.java: the mod's reader parses
//! each input, and the mod's writer prints the result (or the exception it throws).

use horizon_bingo::buf::{decode, Writer};
use horizon_bingo::stats_game::{StatsGamePacket, TooLong};

const ORACLE_READ: &str = include_str!("oracle/stats_game_read.tsv");

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_bytes(packet: &StatsGamePacket) -> Result<Vec<u8>, TooLong> {
    let mut w = Writer::default();
    packet.write(&mut w)?;
    Ok(w.into_bytes())
}

#[test]
fn every_read_row_matches_the_mods_read_then_write() {
    let mut rows = 0;
    for line in ORACLE_READ.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 4, "row shape: {line}");
        let (name, channel, input, expected) = (cols[0], cols[1], cols[2], cols[3]);
        assert_eq!(channel, StatsGamePacket::ID, "channel of {name}");
        rows += 1;

        let read = decode(&unhex(input), StatsGamePacket::read);
        match read {
            Err(err) => {
                assert!(expected.starts_with("THREW:"), "{name}: the port refused ({err}) but the mod read it");
            }
            Ok(packet) => {
                assert!(!expected.starts_with("THREW:"), "{name}: the mod refused but the port read it");
                let bytes = write_bytes(&packet).expect("a read packet is small enough to write");
                assert_eq!(hex(&bytes), expected, "{name}: written after reading");
            }
        }
    }
    assert_eq!(rows, 8);
}

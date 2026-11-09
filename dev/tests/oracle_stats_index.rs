//! Checks `StatsIndexPacket` against the bytes the real mod writes (tests/oracle.tsv, made by
//! OracleStatsIndex.java from the mod's own converter). Each row: name, channel id, hex.

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::stats_index::{Action, StatsIndexPacket};

const ORACLE: &str = include_str!("oracle/stats_index.tsv");

/// The value each oracle row was produced from. Mirrors the Java inputs in OracleStatsIndex.java.
fn case(name: &str) -> StatsIndexPacket {
    let first: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;
    let zero: u128 = 0;
    let high_bit: u128 = 0xfedc_ba98_7654_3210_8000_0000_0000_0001;
    match name {
        "stats_index_broadcast_empty" => StatsIndexPacket { action: Action::Broadcast, games: vec![] },
        "stats_index_request_empty" => StatsIndexPacket { action: Action::Request, games: vec![] },
        "stats_index_request_three" => StatsIndexPacket {
            action: Action::Request,
            games: vec![first, zero, high_bit],
        },
        "stats_index_broadcast_max_uuid" => StatsIndexPacket {
            action: Action::Broadcast,
            games: vec![u128::MAX],
        },
        "stats_index_broadcast_high_bit_only" => StatsIndexPacket {
            action: Action::Broadcast,
            games: vec![0x8000_0000_0000_0000_0000_0000_0000_0000],
        },
        other => panic!("oracle row {other:?} has no Rust case"),
    }
}

fn unhex(hex: &str) -> Vec<u8> {
    assert!(hex.len() % 2 == 0, "odd hex length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit"))
        .collect()
}

#[test]
fn every_oracle_row_matches_the_real_mod() {
    let mut rows = 0;
    for line in ORACLE.lines().filter(|line| !line.is_empty()) {
        let mut cols = line.split('\t');
        let name = cols.next().expect("name");
        let channel = cols.next().expect("channel id");
        let hex = cols.next().expect("hex");
        assert_eq!(cols.next(), None, "extra column in {name}");
        rows += 1;

        let value = case(name);
        assert_eq!(StatsIndexPacket::ID, channel, "channel id for {name}");

        let written = encode(|w| value.write(w));
        assert_eq!(written, unhex(hex), "bytes for {name}");

        let read_back = decode(&unhex(hex), StatsIndexPacket::read);
        assert_eq!(read_back, Ok(value), "reading the oracle bytes of {name}");
    }
    assert_eq!(rows, 5, "oracle row count");
}

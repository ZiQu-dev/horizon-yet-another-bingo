//! Checks the port against the real mod. `oracle.tsv` is printed by `OracleGameStatus.java`
//! running the real `GameStatusPacket.V1` converter over Minecraft's `FriendlyByteBuf`.
//! Each row's expected value is written out below in Rust, by hand, from the Java call on its
//! comment; the row's bytes must match, and the bytes must read back to the expected value.

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};

const ORACLE: &str = include_str!("oracle/game_status.tsv");

/// Number of rows the Java oracle prints. A missing or extra row fails the test.
const ROWS: usize = 13;

fn d(seconds: i64, nanos: i32) -> Option<JavaDuration> {
    Some(JavaDuration { seconds, nanos })
}

/// For each oracle row: the Rust value the Java call was given, and the value a reader gets back
/// from the bytes (they differ only where the wire drops precision: sub-millisecond parts, and
/// overflow, which is written as 0).
fn case(name: &str) -> (GameStatusPacket, GameStatusPacket) {
    let same = |ingame, remaining| {
        let p = GameStatusPacket { ingame_duration: ingame, remaining_duration: remaining };
        (p.clone(), p)
    };
    match name {
        // new GameStatusPacket(null, null)
        "default_both_null" => same(None, None),
        // new GameStatusPacket(Duration.ZERO, null)
        "ingame_zero_remaining_null" => same(d(0, 0), None),
        // new GameStatusPacket(null, Duration.ZERO)
        "ingame_null_remaining_zero" => same(None, d(0, 0)),
        // Duration.ofMillis(41893), null
        "ingame_41893ms_remaining_null" => same(d(41, 893_000_000), None),
        // null, Duration.ofMillis(124)
        "ingame_null_remaining_124ms" => same(None, d(0, 124_000_000)),
        // Duration.ofMillis(1500), Duration.ofMillis(300000)
        "both_present_ms" => same(d(1, 500_000_000), d(300, 0)),
        // Duration.ofMillis(-1), null: negative, floorDiv gives seconds -1
        "ingame_negative_1ms" => same(d(-1, 999_000_000), None),
        // Duration.ofMillis(Long.MAX_VALUE), Duration.ofMillis(Long.MIN_VALUE)
        "ingame_long_max_ms" => same(
            d(9_223_372_036_854_775, 807_000_000),
            d(-9_223_372_036_854_776, 192_000_000),
        ),
        // null, Duration.ofSeconds(-1, 500_000_000) = -0.5 s, written as -500 ms
        "remaining_negative_fraction" => same(None, d(-1, 500_000_000)),
        // Duration.ofNanos(1_999_999): sub-millisecond part truncated, reads back as 1 ms
        "ingame_sub_ms_truncates" => {
            let written = GameStatusPacket { ingame_duration: d(0, 1_999_999), remaining_duration: None };
            let back = GameStatusPacket { ingame_duration: d(0, 1_000_000), remaining_duration: None };
            (written, back)
        }
        // Duration.ofNanos(-1): truncates toward zero, writes 0
        "ingame_negative_sub_ms" => {
            let written = GameStatusPacket { ingame_duration: d(-1, 999_999_999), remaining_duration: None };
            let back = GameStatusPacket { ingame_duration: d(0, 0), remaining_duration: None };
            (written, back)
        }
        // Duration.ofSeconds(Long.MAX_VALUE): toMillis overflows, writes 0
        "ingame_overflow_writes_zero" => {
            let written = GameStatusPacket { ingame_duration: d(i64::MAX, 0), remaining_duration: None };
            let back = GameStatusPacket { ingame_duration: d(0, 0), remaining_duration: None };
            (written, back)
        }
        // Duration.ofSeconds(Long.MIN_VALUE): toMillis overflows, writes 0
        "remaining_overflow_negative_writes_zero" => {
            let written = GameStatusPacket { ingame_duration: None, remaining_duration: d(i64::MIN, 0) };
            let back = GameStatusPacket { ingame_duration: None, remaining_duration: d(0, 0) };
            (written, back)
        }
        other => panic!("oracle row {other} has no Rust case"),
    }
}

fn parse_hex(hex: &str) -> Vec<u8> {
    assert!(hex.len() % 2 == 0, "odd hex length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit"))
        .collect()
}

fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    ORACLE
        .lines()
        .map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next().expect("name");
            let channel = cols.next().expect("channel");
            let hex = cols.next().expect("hex");
            assert!(cols.next().is_none(), "row {name} has more than three columns");
            (name, channel, hex)
        })
        .collect()
}

#[test]
fn oracle_has_every_expected_row() {
    let rows = rows();
    assert_eq!(rows.len(), ROWS);
    for (name, _, _) in &rows {
        let _ = case(name);
    }
}

#[test]
fn rust_writes_the_same_bytes_and_channel_as_the_real_mod() {
    for (name, channel, hex) in rows() {
        let (written, _) = case(name);
        assert_eq!(channel, GameStatusPacket::ID, "channel of {name}");
        let bytes = encode(|w| written.write(w));
        let expected = parse_hex(hex);
        assert_eq!(bytes, expected, "bytes of {name}");
    }
}

#[test]
fn rust_reads_the_real_mods_bytes_back() {
    for (name, _, hex) in rows() {
        let (_, back) = case(name);
        let read = decode(&parse_hex(hex), GameStatusPacket::read);
        assert_eq!(read, Ok(back), "read-back of {name}");
    }
}

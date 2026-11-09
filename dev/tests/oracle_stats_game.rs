//! Checks the port against the real mod. Each row of tests/oracle.tsv was written by
//! OracleStatsGame.java calling the mod's own StatsGamePacket.V1 converter. The expected values
//! below are typed out by hand from the Java cases; they are not computed by the crate.

use horizon_bingo::buf::{decode, Writer};
use horizon_bingo::stats_game::{
    Duration, GameInfo, GamePlayerInfo, GameTeamInfo, Instant, PlayerGameSummary, StatsGamePacket, TooLong,
};

const ORACLE: &str = include_str!("oracle/stats_game.tsv");

const ZERO: u128 = 0;
/// UUID(0x0123456789abcdef, 0xfedcba9876543210)
const TYPICAL_GAME: u128 = 0x0123456789abcdef_fedcba9876543210;
/// UUID(0x1, 0x2)
const TYPICAL_HOST: u128 = 0x0000000000000001_0000000000000002;
/// UUID(0xfedcba9876543210, 0x8000000000000001)
const HIGH_BIT: u128 = 0xfedcba9876543210_8000000000000001;

const T0: Instant = Instant { epoch_second: 1_700_000_000, nano: 123_000_000 };
const T1: Instant = Instant { epoch_second: 1_700_003_600, nano: 0 };
const EPOCH: Instant = Instant { epoch_second: 0, nano: 0 };

/// The minimal fixture the single-field cases start from.
fn base() -> StatsGamePacket {
    StatsGamePacket {
        game: PlayerGameSummary {
            game: GameInfo {
                id: TYPICAL_GAME,
                bingo_options: "o".into(),
                bingo_options_hash: "h".into(),
                started_at: T0,
                ended_at: T1,
                duration: Duration { seconds: 1, nanos: 0 },
                player_count: 1,
                is_draw: false,
                is_forfeit: false,
                host_id: TYPICAL_HOST,
            },
            team: GameTeamInfo { id: "t".into(), game_id: TYPICAL_GAME, name: "n".into(), is_winner: false },
            player: GamePlayerInfo {
                team_id: "t".into(),
                game_id: TYPICAL_GAME,
                minecraft_id: TYPICAL_HOST,
                minecraft_name: "m".into(),
                captured_items: 1,
            },
        },
    }
}

/// The minimal fixture with one edit applied.
fn edit(f: impl FnOnce(&mut StatsGamePacket)) -> StatsGamePacket {
    let mut p = base();
    f(&mut p);
    p
}

/// Every row name in tests/oracle.tsv, with the value the Java case builds.
fn case(name: &str) -> Option<StatsGamePacket> {
    let empty_game = GameInfo {
        id: ZERO,
        bingo_options: String::new(),
        bingo_options_hash: String::new(),
        started_at: EPOCH,
        ended_at: EPOCH,
        duration: Duration { seconds: 0, nanos: 0 },
        player_count: 0,
        is_draw: false,
        is_forfeit: false,
        host_id: ZERO,
    };
    let empty_team = GameTeamInfo { id: String::new(), game_id: ZERO, name: String::new(), is_winner: false };
    let empty_player = GamePlayerInfo {
        team_id: String::new(),
        game_id: ZERO,
        minecraft_id: ZERO,
        minecraft_name: String::new(),
        captured_items: 0,
    };

    let packet = match name {
        "typical_all_fields" => StatsGamePacket {
            game: PlayerGameSummary {
                game: GameInfo {
                    id: TYPICAL_GAME,
                    bingo_options: r#"{"mode":"standard","size":5}"#.into(),
                    bingo_options_hash: "a1b2c3".into(),
                    started_at: T0,
                    ended_at: T1,
                    duration: Duration { seconds: 3723, nanos: 500_000_000 },
                    player_count: 4,
                    is_draw: false,
                    is_forfeit: true,
                    host_id: TYPICAL_HOST,
                },
                team: GameTeamInfo { id: "bingo_red".into(), game_id: TYPICAL_GAME, name: "Red".into(), is_winner: true },
                player: GamePlayerInfo {
                    team_id: "bingo_red".into(),
                    game_id: TYPICAL_GAME,
                    minecraft_id: HIGH_BIT,
                    minecraft_name: "Steve".into(),
                    captured_items: 17,
                },
            },
        },
        "zeros_and_empty" => StatsGamePacket {
            game: PlayerGameSummary { game: empty_game, team: empty_team, player: empty_player },
        },
        "negatives" => StatsGamePacket {
            game: PlayerGameSummary {
                game: GameInfo {
                    id: TYPICAL_GAME,
                    bingo_options: "o".into(),
                    bingo_options_hash: "h".into(),
                    started_at: Instant { epoch_second: -1, nano: 500_000_000 },
                    ended_at: Instant { epoch_second: -86_400, nano: 1 },
                    duration: Duration { seconds: -2, nanos: 500_000_000 },
                    player_count: -1,
                    is_draw: false,
                    is_forfeit: false,
                    host_id: TYPICAL_HOST,
                },
                team: GameTeamInfo { id: "bingo_x".into(), game_id: TYPICAL_GAME, name: "n".into(), is_winner: false },
                player: GamePlayerInfo {
                    team_id: "bingo_x".into(),
                    game_id: TYPICAL_GAME,
                    minecraft_id: TYPICAL_HOST,
                    minecraft_name: "m".into(),
                    captured_items: -5,
                },
            },
        },
        "int_max" => edit(|p| {
            p.game.game.duration = Duration { seconds: 360_000_000, nanos: 0 };
            p.game.game.player_count = i32::MAX;
            p.game.player.captured_items = i32::MAX;
        }),
        "int_min" => edit(|p| {
            p.game.game.duration = Duration { seconds: 60, nanos: 0 };
            p.game.game.player_count = i32::MIN;
            p.game.player.captured_items = i32::MIN;
        }),
        "non_ascii" => StatsGamePacket {
            game: PlayerGameSummary {
                game: GameInfo {
                    id: TYPICAL_GAME,
                    bingo_options: "ż 日本語 🎲".into(),
                    bingo_options_hash: "ÄÖ".into(),
                    started_at: T0,
                    ended_at: T1,
                    duration: Duration { seconds: 90, nanos: 0 },
                    player_count: 2,
                    is_draw: false,
                    is_forfeit: false,
                    host_id: TYPICAL_HOST,
                },
                team: GameTeamInfo { id: "bingo_czerwony".into(), game_id: TYPICAL_GAME, name: "Zażółć 🎲".into(), is_winner: true },
                player: GamePlayerInfo {
                    team_id: "bingo_czerwony".into(),
                    game_id: TYPICAL_GAME,
                    minecraft_id: HIGH_BIT,
                    minecraft_name: "Jürgen_ß".into(),
                    captured_items: 3,
                },
            },
        },
        "escapes" => StatsGamePacket {
            game: PlayerGameSummary {
                game: GameInfo {
                    bingo_options: "/slash/ \u{2028} sep \u{7f} del \"q\" back\\slash".into(),
                    ..base().game.game
                },
                team: GameTeamInfo {
                    id: "bingo_e".into(),
                    game_id: TYPICAL_GAME,
                    name: "quote\" back\\slash\ttab\nnl\rcr\u{1}ctl\u{1f}us".into(),
                    is_winner: false,
                },
                player: GamePlayerInfo {
                    team_id: "bingo_e".into(),
                    game_id: TYPICAL_GAME,
                    minecraft_id: TYPICAL_HOST,
                    minecraft_name: "\u{8}\u{c}<>&'".into(),
                    captured_items: 0,
                },
            },
        },
        "options_pretty_json" => edit(|p| {
            p.game.game.bingo_options = "{\n    \"cardSize\": 5,\n    \"items\": [\n        \"minecraft:dirt\"\n    ]\n}".into();
            p.game.game.duration = Duration { seconds: 0, nanos: 1_000_000 };
            p.game.team.id = "bingo_p".into();
            p.game.team.name = "p".into();
            p.game.player.team_id = "bingo_p".into();
            p.game.player.minecraft_name = "p".into();
        }),
        "dur_1ns" => with_duration(0, 1),
        "dur_1ms" => with_duration(0, 1_000_000),
        "dur_1500ns" => with_duration(0, 1500),
        "dur_1m" => with_duration(60, 0),
        "dur_1h" => with_duration(3600, 0),
        "dur_1h1m1s" => with_duration(3661, 0),
        "dur_neg_500ms" => with_duration(-1, 500_000_000),
        "dur_neg_1s" => with_duration(-1, 0),
        "dur_neg_1h30m" => with_duration(-5400, 0),
        "dur_1d" => with_duration(86_400, 0),
        "inst_whole" => with_instants(Instant { epoch_second: 1_700_000_000, nano: 0 }),
        "inst_millis" => with_instants(Instant { epoch_second: 1_700_000_000, nano: 120_000_000 }),
        "inst_micros" => with_instants(Instant { epoch_second: 1_700_000_000, nano: 123_456_000 }),
        "inst_nanos" => with_instants(Instant { epoch_second: 1_700_000_000, nano: 123_456_789 }),
        "inst_epoch_neg_frac" => with_instants(Instant { epoch_second: -1, nano: 999_999_999 }),
        "draw_only" => edit(|p| p.game.game.is_draw = true),
        "forfeit_only" => edit(|p| p.game.game.is_forfeit = true),
        "winner_only" => edit(|p| p.game.team.is_winner = true),
        "uuids_high_bit" => edit(|p| {
            p.game.game.id = HIGH_BIT;
            p.game.game.host_id = 0x8000000000000000_ffffffffffffffff;
            p.game.team.game_id = 0xffffffffffffffff_0000000000000000;
            p.game.player.game_id = HIGH_BIT;
            p.game.player.minecraft_id = 0x0000000000000008_0000000000000008;
        }),
        "options_utf16_at_cap_with_emoji" => with_options("x".repeat(30_013) + &"🎲".repeat(1000)),
        "options_utf16_over_cap_with_emoji" => with_options("x".repeat(30_014) + &"🎲".repeat(1000)),
        "options_ascii_over_cap" => with_options("x".repeat(32_014)),
        _ => return None,
    };
    Some(packet)
}

fn with_duration(seconds: i64, nanos: u32) -> StatsGamePacket {
    edit(|p| p.game.game.duration = Duration { seconds, nanos })
}

fn with_instants(instant: Instant) -> StatsGamePacket {
    edit(|p| {
        p.game.game.started_at = instant;
        p.game.game.ended_at = instant;
    })
}

fn with_options(options: String) -> StatsGamePacket {
    // The empty fixture with the options replaced: its JSON is the one the cap cases measure.
    let mut p = StatsGamePacket {
        game: PlayerGameSummary {
            game: GameInfo {
                id: ZERO,
                bingo_options: String::new(),
                bingo_options_hash: String::new(),
                started_at: EPOCH,
                ended_at: EPOCH,
                duration: Duration { seconds: 0, nanos: 0 },
                player_count: 0,
                is_draw: false,
                is_forfeit: false,
                host_id: ZERO,
            },
            team: GameTeamInfo { id: String::new(), game_id: ZERO, name: String::new(), is_winner: false },
            player: GamePlayerInfo {
                team_id: String::new(),
                game_id: ZERO,
                minecraft_id: ZERO,
                minecraft_name: String::new(),
                captured_items: 0,
            },
        },
    };
    p.game.game.bingo_options = options;
    p
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes the packet the way the mod's buffer does: the payload bytes, or the refusal.
fn write_bytes(packet: &StatsGamePacket) -> Result<Vec<u8>, TooLong> {
    let mut w = Writer::default();
    packet.write(&mut w)?;
    Ok(w.into_bytes())
}

fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    ORACLE
        .lines()
        .map(|line| {
            let mut cols = line.split('\t');
            (cols.next().unwrap(), cols.next().unwrap(), cols.next().unwrap())
        })
        .collect()
}

#[test]
fn every_oracle_row_is_a_case_this_test_builds() {
    let rows = rows();
    assert_eq!(rows.len(), 30);
    for (name, _, _) in rows {
        assert!(case(name).is_some(), "no Rust case for oracle row {name}");
    }
}

#[test]
fn every_oracle_row_names_the_mod_channel() {
    for (name, channel, _) in rows() {
        assert_eq!(channel, StatsGamePacket::ID, "channel of {name}");
    }
}

#[test]
fn written_bytes_equal_the_mods_bytes() {
    for (name, _, expected) in rows() {
        let packet = case(name).unwrap();
        match write_bytes(&packet) {
            Ok(bytes) => {
                assert!(!expected.starts_with("THREW:"), "{name}: the mod refused this, the port wrote {} bytes", bytes.len());
                assert_eq!(hex(&bytes), expected, "bytes of {name}");
            }
            Err(refused) => {
                assert!(expected.starts_with("THREW:"), "{name}: the port refused ({refused:?}) but the mod wrote bytes");
            }
        }
    }
}

#[test]
fn the_mods_bytes_read_back_to_the_same_value() {
    for (name, _, expected) in rows() {
        if expected.starts_with("THREW:") {
            continue;
        }
        let packet = case(name).unwrap();
        let read = decode(&unhex(expected), StatsGamePacket::read);
        assert_eq!(read, Ok(packet), "reading back {name}");
    }
}

#[test]
fn the_cap_is_counted_in_utf16_units() {
    // The emoji row is over 32767 UTF-8 bytes but exactly at the cap in chars, so it must be written.
    let at_cap = case("options_utf16_at_cap_with_emoji").unwrap();
    assert!(write_bytes(&at_cap).is_ok());
    let text = at_cap.to_json();
    assert_eq!(text.encode_utf16().count(), 32767);
    assert!(text.len() > 32767, "UTF-8 bytes are more than the chars, so the byte count would refuse it");
}

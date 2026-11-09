//! GameOverPacket against the real mod. Every row of tests/oracle.tsv came from OracleGameOver.java, which
//! runs the mod's own V1 and V2 converters over PacketBufImpl (write rows), and reads the same bytes back with
//! the mod's readers (read rows, whose 4th column is the summary the Kotlin reader produced).

use horizon_bingo::game_over::{EndScreenTab, GameOverPacket, ScoreRanking, V1_ID, V2_ID};
use horizon_bingo::buf::{decode, encode};
use horizon_bingo::game_status::JavaDuration;
use horizon_bingo::nbt::Nbt;
use horizon_bingo::score_update::TeamScore;
use horizon_bingo::text::Text;

const TSV: &str = include_str!("oracle/game_over.tsv");

/// Every case the oracle writes, in its order. Each has a `v1_` and a `v2_` write row and a `read_` row per version.
const CASES: [&str; 14] = [
    "minimal",
    "tab_scores",
    "winner_team",
    "no_winner",
    "nullables_present_zero",
    "nullables_mixed",
    "extreme_values",
    "scores_extreme",
    "duration_overflow",
    "ended_before_1970",
    "ended_after_2100",
    "ended_sub_second_before_epoch",
    "non_ascii",
    "styled_combined",
];

struct Row {
    name: String,
    channel: String,
    hex: String,
    /// Present on `read_` rows: the Kotlin reader's summary of these bytes.
    summary: Option<String>,
}

fn rows() -> Vec<Row> {
    TSV.lines()
        .map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            assert!(cols.len() == 3 || cols.len() == 4, "bad row: {line}");
            Row {
                name: cols[0].to_string(),
                channel: cols[1].to_string(),
                hex: cols[2].to_string(),
                summary: cols.get(3).map(|s| s.to_string()),
            }
        })
        .collect()
}

fn bytes_of(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("hex")).collect()
}

/// `(is_v1, case name)` from a write row's name such as `v2_winner_team`.
fn split_write_name(name: &str) -> (bool, &str) {
    if let Some(case) = name.strip_prefix("v1_") {
        (true, case)
    } else if let Some(case) = name.strip_prefix("v2_") {
        (false, case)
    } else {
        panic!("not a write row: {name}")
    }
}

/// The summary format of the oracle's `summary()`.
fn summary(p: &GameOverPacket) -> String {
    format!(
        "tab={};winner={};scores={};resumeFlag={};resumeDescNull={}",
        p.default_tab.name(),
        p.winner.as_deref().unwrap_or("null"),
        p.scores.len(),
        p.is_resume_available,
        p.resume_description.is_none(),
    )
}

// ---- The values the oracle was built from, typed by hand from OracleGameOver.java ----

fn dur(seconds: i64, nanos: i32) -> JavaDuration {
    JavaDuration { seconds, nanos }
}

fn plain(value: &str) -> Text {
    Text::plain(value)
}

/// A styled literal. Keys in the order the mod's codec wrote them (color, text, bold).
fn styled(entries: Vec<(&str, Nbt)>) -> Text {
    Text(Nbt::compound(entries))
}

fn s(value: &str) -> Nbt {
    Nbt::string(value)
}

fn ts(items: i32, lines: i32, cards: i32) -> TeamScore {
    TeamScore { items, lines, cards }
}

fn rank(index: i32, key: &str, name: Text, score: TeamScore, duration: Option<&str>) -> ScoreRanking {
    ScoreRanking { index, key: key.to_string(), name, score, duration: duration.map(str::to_string) }
}

fn base() -> GameOverPacket {
    GameOverPacket::default()
}

fn case(name: &str) -> GameOverPacket {
    match name {
        "minimal" => base(),
        "tab_scores" => GameOverPacket { default_tab: EndScreenTab::Scores, ..base() },
        "winner_team" => GameOverPacket {
            title: styled(vec![("color", s("gold")), ("text", s("Victory"))]),
            subtitle: plain("Red team wins"),
            winner: Some("bingo_red".into()),
            duration: dur(3723, 456_000_000),
            is_ready_vote_available: true,
            is_resume_available: true,
            resume_description: Some(plain("Resume the game")),
            is_winner: true,
            is_update: false,
            win_streak: Some(3),
            best_win_streak: Some(5),
            is_best_win_streak: true,
            captured_items: Some(120),
            best_captured_items: Some(150),
            is_best_captured_items: false,
            ended_at_millis: 1_794_000_000_123,
            is_best_time: true,
            prev_best_time: Some(dur(4000, 500_000_000)),
            seed: Some(123_456_789_012),
            scores: vec![
                rank(0, "bingo_red", plain("Red"), ts(10, 2, 1), Some("PT1M30S")),
                rank(1, "bingo_blue", styled(vec![("color", s("blue")), ("text", s("Blue"))]), ts(0, 0, 0), None),
                rank(2, "bingo_green", plain("Green"), ts(3, 0, 0), Some("PT0S")),
            ],
            default_tab: EndScreenTab::Scores,
            ..base()
        },
        "no_winner" => GameOverPacket {
            title: plain("Draw"),
            is_update: true,
            scores: vec![
                rank(0, "bingo_red", plain("Red"), ts(4, 1, 0), None),
                rank(1, "bingo_blue", plain("Blue"), ts(4, 1, 0), None),
            ],
            default_tab: EndScreenTab::Scores,
            ..base()
        },
        "nullables_present_zero" => GameOverPacket {
            winner: Some(String::new()),
            resume_description: Some(plain("")),
            win_streak: Some(0),
            best_win_streak: Some(0),
            captured_items: Some(0),
            best_captured_items: Some(0),
            prev_best_time: Some(dur(0, 0)),
            seed: Some(0),
            ..base()
        },
        "nullables_mixed" => GameOverPacket {
            winner: Some("bingo_red".into()),
            is_resume_available: true,
            win_streak: Some(7),
            best_captured_items: Some(7),
            prev_best_time: Some(dur(1, 0)),
            ..base()
        },
        "extreme_values" => GameOverPacket {
            duration: dur(-9_223_372_036_854_776, 192_000_000),
            is_ready_vote_available: true,
            is_winner: true,
            is_update: true,
            win_streak: Some(i64::MIN),
            best_win_streak: Some(i64::MAX),
            is_best_win_streak: true,
            captured_items: Some(i32::MIN),
            best_captured_items: Some(i32::MAX),
            is_best_captured_items: true,
            ended_at_millis: i64::MIN,
            is_best_time: true,
            prev_best_time: Some(dur(9_223_372_036_854_775, 807_000_000)),
            seed: Some(i64::MIN),
            default_tab: EndScreenTab::Scores,
            ..base()
        },
        "scores_extreme" => GameOverPacket {
            scores: vec![
                rank(i32::MIN, "", plain(""), ts(i32::MIN, i32::MAX, -1), Some("PT-0.5S")),
                rank(i32::MAX, "bingo_x", plain("x"), ts(0, 0, 0), Some("PT1H0.001S")),
            ],
            default_tab: EndScreenTab::Scores,
            ..base()
        },
        "duration_overflow" => GameOverPacket {
            // Duration.ofSeconds(Long.MAX_VALUE).toMillis() overflows, so writeDuration writes 0 for both fields.
            duration: dur(i64::MAX, 0),
            prev_best_time: Some(dur(i64::MAX, 0)),
            ..base()
        },
        "ended_before_1970" => GameOverPacket { ended_at_millis: -2_000_000_000_000, ..base() },
        "ended_after_2100" => GameOverPacket { ended_at_millis: 4_200_000_000_000, ..base() },
        "ended_sub_second_before_epoch" => GameOverPacket { ended_at_millis: -500, ..base() },
        "non_ascii" => GameOverPacket {
            title: plain("za\u{017c}\u{00f3}\u{0142}\u{0107} \u{65e5}\u{672c} \u{1F600}"),
            subtitle: plain("\u{00dc}n\u{00ef}c\u{00f6}d\u{00e9}"),
            winner: Some("bingo_\u{00e9}\u{8336}\u{1F642}".into()),
            is_resume_available: true,
            resume_description: Some(plain("\u{65e5}\u{672c}")),
            scores: vec![rank(
                0,
                "bingo_\u{00e9}\u{8336}\u{1F642}",
                plain("Zo\u{00eb}"),
                ts(1, 1, 1),
                Some("PT1S"),
            )],
            default_tab: EndScreenTab::Scores,
            ..base()
        },
        "styled_combined" => GameOverPacket {
            title: styled(vec![("color", s("red")), ("text", s("Bold red")), ("bold", Nbt::Byte(1))]),
            subtitle: styled(vec![("color", s("#FFAA00")), ("text", s("hex"))]),
            ..base()
        },
        other => panic!("no Rust case for oracle case {other}"),
    }
}

/// What a reader must give back for a value written by the given version: a field the version does not write
/// comes back as its default. `duration_overflow` is written as 0 (see the case).
fn expected_read(v1: bool, name: &str, value: GameOverPacket) -> GameOverPacket {
    let mut expected = value;
    if v1 {
        expected.resume_description = None;
    } else {
        expected.is_resume_available = false;
    }
    if name == "duration_overflow" {
        expected.duration = dur(0, 0);
        expected.prev_best_time = Some(dur(0, 0));
    }
    expected
}

fn write_value(v1: bool, value: &GameOverPacket) -> Vec<u8> {
    encode(|w| if v1 { value.write_v1(w) } else { value.write_v2(w) }.expect("the value writes"))
}

#[test]
fn every_case_is_in_the_oracle_for_both_versions() {
    let write_rows: Vec<Row> = rows().into_iter().filter(|r| !r.name.starts_with("read_")).collect();
    assert_eq!(write_rows.len(), CASES.len() * 2, "one v1 and one v2 row per case");
    for case_name in CASES {
        for prefix in ["v1_", "v2_"] {
            let name = format!("{prefix}{case_name}");
            assert!(write_rows.iter().any(|r| r.name == name), "missing row {name}");
        }
    }
}

#[test]
fn every_write_row_matches_the_mods_bytes_and_channel() {
    let write_rows: Vec<Row> = rows().into_iter().filter(|r| !r.name.starts_with("read_")).collect();
    for row in &write_rows {
        let (v1, case_name) = split_write_name(&row.name);
        let value = case(case_name);
        assert_eq!(write_value(v1, &value), bytes_of(&row.hex), "bytes for {}", row.name);
        assert_eq!(row.channel, if v1 { V1_ID } else { V2_ID }, "channel for {}", row.name);
    }
}

#[test]
fn oracle_bytes_read_back_to_the_value_that_was_written() {
    for row in rows().iter().filter(|r| !r.name.starts_with("read_")) {
        let (v1, case_name) = split_write_name(&row.name);
        let bytes = bytes_of(&row.hex);
        let expected = expected_read(v1, case_name, case(case_name));
        let decoded = if v1 {
            decode(&bytes, GameOverPacket::read_v1)
        } else {
            decode(&bytes, GameOverPacket::read_v2)
        };
        assert_eq!(decoded, Ok(expected), "reading back {}", row.name);
    }
}

#[test]
fn the_kotlin_readers_summary_matches_the_rust_reader() {
    let read_rows: Vec<Row> = rows().into_iter().filter(|r| r.name.starts_with("read_")).collect();
    // Every write row has one read row per version, plus the two unknown-tab rows.
    assert_eq!(read_rows.len(), CASES.len() * 2 + 2);
    for row in &read_rows {
        let v1 = row.channel == V1_ID;
        assert!(v1 || row.channel == V2_ID, "channel of {}", row.name);
        let bytes = bytes_of(&row.hex);
        let decoded = if v1 {
            decode(&bytes, GameOverPacket::read_v1)
        } else {
            decode(&bytes, GameOverPacket::read_v2)
        }
        .unwrap_or_else(|e| panic!("{}: {e}", row.name));
        assert_eq!(Some(summary(&decoded)), row.summary, "summary of {}", row.name);
    }
}

#[test]
fn an_unknown_tab_name_reads_as_cards_in_both_versions() {
    for row in rows().iter().filter(|r| r.name.ends_with("_unknown_tab")) {
        let bytes = bytes_of(&row.hex);
        let decoded = if row.channel == V1_ID {
            decode(&bytes, GameOverPacket::read_v1)
        } else {
            decode(&bytes, GameOverPacket::read_v2)
        }
        .expect("the packet decodes");
        assert_eq!(decoded.default_tab, EndScreenTab::Cards, "{}", row.name);
    }
}

#[test]
fn each_version_writes_only_its_own_resume_field() {
    // V1 writes isResumeAvailable where V2 writes resumeDescription. With its field absent (false / None),
    // each writes a single zero byte at that position, so the two packets are the same bytes.
    let value = case("winner_team");
    let v1_without_flag = write_value(true, &GameOverPacket { is_resume_available: false, ..value.clone() });
    let v2_without_description = write_value(false, &GameOverPacket { resume_description: None, ..value.clone() });
    assert_eq!(v1_without_flag, v2_without_description);
    // The description never reaches a V1 packet, so dropping it changes nothing.
    assert_eq!(
        write_value(true, &value),
        write_value(true, &GameOverPacket { resume_description: None, ..value.clone() })
    );
}

#[test]
fn an_empty_scores_list_is_a_zero_count_before_the_tab_name() {
    // ... scores: int count 0, then defaultTab as a string: VarInt 5, "CARDS".
    let minimal = write_value(false, &case("minimal"));
    let tail = &minimal[minimal.len() - 10..];
    assert_eq!(tail, [0, 0, 0, 0, 5, b'C', b'A', b'R', b'D', b'S']);
}

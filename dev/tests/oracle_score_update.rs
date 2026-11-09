use horizon_bingo::buf::{decode, encode};
use horizon_bingo::score_update::{ScoredItemPacket, TeamScore};

const ORACLE: &str = include_str!("oracle/score_update.tsv");

/// Every oracle row has a Rust case here. The values are written out by hand from the Java oracle's inputs.
enum Case {
    Packet(ScoredItemPacket),
    Score(TeamScore),
}

fn packet(is_viewer_on_team: bool, team: &str, items: i32, lines: i32, cards: i32) -> Case {
    Case::Packet(ScoredItemPacket {
        is_viewer_on_team,
        team: team.to_string(),
        score: TeamScore { items, lines, cards },
    })
}

fn case(name: &str) -> Option<Case> {
    Some(match name {
        "score_update_zero" => packet(false, "", 0, 0, 0),
        "score_update_viewer_on_team" => packet(true, "bingo_red", 3, 1, 2),
        "score_update_negative" => packet(false, "bingo_blue", -1, -2, -3),
        "score_update_int_extremes" => packet(true, "bingo_max", i32::MAX, i32::MIN, i32::MAX),
        "score_update_non_ascii" => packet(true, "bingo_żółw🙂", 1, 0, 0),
        "score_update_long_team" => packet(false, &format!("bingo_{}", "x".repeat(200)), 0, 7, 0),
        "team_score_zero" => Case::Score(TeamScore { items: 0, lines: 0, cards: 0 }),
        "team_score_mixed" => Case::Score(TeamScore { items: 5, lines: 0, cards: 12 }),
        "team_score_extremes" => Case::Score(TeamScore { items: i32::MIN, lines: -1, cards: i32::MAX }),
        _ => return None,
    })
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn bytes_of(hex: &str) -> Vec<u8> {
    assert!(hex.len() % 2 == 0, "odd hex length in {hex:?}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

#[test]
fn every_oracle_row_matches_the_mod_byte_for_byte_and_reads_back() {
    let mut rows = 0;
    for line in ORACLE.lines().filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "malformed oracle row {line:?}");
        let (name, channel, hex) = (fields[0], fields[1], fields[2]);
        let bytes = bytes_of(hex);

        match case(name).unwrap_or_else(|| panic!("no Rust case for oracle row {name}")) {
            Case::Packet(packet) => {
                assert_eq!(channel, ScoredItemPacket::ID, "{name}: channel id");
                assert_eq!(hex_of(&encode(|w| packet.write(w))), hex, "{name}: written bytes");
                assert_eq!(decode(&bytes, ScoredItemPacket::read), Ok(packet), "{name}: read back");
            }
            Case::Score(score) => {
                assert_eq!(channel, TeamScore::ID, "{name}: channel id");
                assert_eq!(hex_of(&encode(|w| score.write(w))), hex, "{name}: written bytes");
                assert_eq!(decode(&bytes, TeamScore::read), Ok(score), "{name}: read back");
            }
        }
        rows += 1;
    }
    assert_eq!(rows, 9, "the oracle file has a different number of rows than the cases");
}

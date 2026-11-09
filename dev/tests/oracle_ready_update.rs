//! `ReadyUpdatePacket` V1 and V2, with text and vote options, against the real mod.
//!
//! Every row of tests/oracle.tsv was written by the mod's own converters (OracleReadyUpdate.java, through the
//! mod's PacketBufImpl over bootstrapped registries). Each case below is built here from the values the Java
//! oracle was given, written out by hand, and compared byte for byte; then the oracle bytes are read back.
//! The rows of the older skeleton oracle (no text, no votes) are checked against the same code.

use horizon_bingo::ready_update::{ReadyUpdatePacket, VoteOption};
use horizon_bingo::buf::{decode, encode, DecodeError};
use horizon_bingo::game::{GameState, GameStatus};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;

const ORACLE: &str = include_str!("oracle/ready_update.tsv");
/// The skeleton's older oracle: the `ready_update_*` rows have no title, subtitle or vote options.
const SKELETON_ORACLE: &str = include_str!("oracle.tsv");

/// Most significant bit set.
const HIGH: u128 = 0xfedc_ba98_7654_3210_0123_4567_89ab_cdef;
/// Most significant bit clear.
const LOW: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;
const ZERO: u128 = 0;

fn state(identifier: &str, status: GameStatus) -> GameState {
    GameState { identifier: identifier.to_string(), status }
}

fn plain(value: &str) -> Text {
    Text::plain(value)
}

fn styled(entries: Vec<(&str, Nbt)>) -> Text {
    Text(Nbt::compound(entries))
}

/// `Component.literal(text).withStyle { color = GOLD }`: the colour, then the text.
fn gold(value: &str) -> Text {
    styled(vec![("color", Nbt::string("gold")), ("text", Nbt::string(value))])
}

/// `Component.literal(text).withStyle { bold = true }`: the text, then the flag.
fn bold(value: &str) -> Text {
    styled(vec![("text", Nbt::string(value)), ("bold", Nbt::Byte(1))])
}

/// `Component.literal(text).withStyle { color = 0xff8800 }`.
fn orange(value: &str) -> Text {
    styled(vec![("color", Nbt::string("#FF8800")), ("text", Nbt::string(value))])
}

fn vote(id: u128, title: Text, votes: i32, is_selected: bool) -> VoteOption {
    VoteOption { id, title, votes, is_selected }
}

/// The three vote options the oracle's `threeVotes()` builds.
fn three_votes() -> Vec<VoteOption> {
    vec![
        vote(HIGH, plain("Option A"), 0, false),
        vote(LOW, orange("C"), 5, true),
        vote(ZERO, plain(""), i32::MAX, false),
    ]
}

#[allow(clippy::too_many_arguments)]
fn packet(
    is_running: bool,
    is_ready: bool,
    state: GameState,
    remaining_millis: i64,
    total_millis: i64,
    ready_players: i32,
    total_players: i32,
    title: Option<Text>,
    subtitle: Option<Text>,
    can_send_ready: bool,
    vote_options: Vec<VoteOption>,
    vote_pending: Option<u128>,
) -> ReadyUpdatePacket {
    ReadyUpdatePacket {
        is_running,
        is_ready,
        state,
        remaining_millis,
        total_millis,
        ready_players,
        total_players,
        title,
        subtitle,
        can_send_ready,
        vote_options,
        vote_pending,
    }
}

/// Running, not ready, 1500 ms of 300000 ms, 2 of 4 ready, can send ready.
fn typical(
    state: GameState,
    title: Option<Text>,
    subtitle: Option<Text>,
    vote_options: Vec<VoteOption>,
    vote_pending: Option<u128>,
) -> ReadyUpdatePacket {
    packet(true, false, state, 1500, 300_000, 2, 4, title, subtitle, true, vote_options, vote_pending)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Version {
    V1,
    V2,
}

struct Case {
    version: Version,
    /// The value handed to the writer.
    packet: ReadyUpdatePacket,
    /// What reading the oracle bytes gives back. Differs from `packet` only where the version drops a field:
    /// V1 reads the status as PREGAME and has no vote fields.
    read_back: ReadyUpdatePacket,
}

fn same(version: Version, packet: ReadyUpdatePacket) -> Case {
    Case { version, read_back: packet.clone(), packet }
}

/// A V1 case whose read-back has the status PREGAME, with the identifier kept as the writer gave it (the
/// reader lower-cases it, so the identifiers used here are lower-case already).
fn v1_pregame_back(packet: ReadyUpdatePacket) -> Case {
    let identifier = packet.state.identifier.to_lowercase();
    let read_back = ReadyUpdatePacket {
        state: state(&identifier, GameStatus::Pregame),
        vote_options: Vec::new(),
        vote_pending: None,
        ..packet.clone()
    };
    Case { version: Version::V1, packet, read_back }
}

fn case(name: &str) -> Option<Case> {
    use GameStatus::*;
    Some(match name {
        // V2: one row per status. No text, no votes, no pending vote.
        "v2_status_pregame" => same(Version::V2, typical(state("pregame", Pregame), None, None, vec![], None)),
        "v2_status_starting" => same(Version::V2, typical(state("countdown", Starting), None, None, vec![], None)),
        "v2_status_playing" => same(Version::V2, typical(state("playing", Playing), None, None, vec![], None)),
        "v2_status_postgame" => same(Version::V2, typical(state("postgame", Postgame), None, None, vec![], None)),
        "v2_zero" => same(
            Version::V2,
            packet(false, false, state("pregame", Pregame), 0, 0, 0, 0, None, None, false, vec![], None),
        ),
        "v2_extremes" => same(
            Version::V2,
            packet(
                false, true, state("playing", Playing), -1, i64::MAX, i32::MIN, i32::MAX, None, None, false,
                vec![], Some(ZERO),
            ),
        ),
        // V2 text: absent, present, and present but empty (which is not the same as absent).
        "v2_title_only" => same(
            Version::V2,
            typical(state("pregame", Pregame), Some(plain("Waiting for players")), None, vec![], None),
        ),
        "v2_subtitle_only" => same(
            Version::V2,
            typical(state("pregame", Pregame), None, Some(plain("zażółć 日本 \u{1F600}")), vec![], None),
        ),
        "v2_title_and_subtitle" => same(
            Version::V2,
            typical(state("countdown", Starting), Some(gold("Round 1")), Some(bold("Good luck")), vec![], None),
        ),
        "v2_title_and_subtitle_empty" => same(
            Version::V2,
            typical(state("pregame", Pregame), Some(plain("")), Some(plain("")), vec![], None),
        ),
        // V2 vote options: a three-element list with and without a pending vote, and a pending vote alone.
        "v2_votes_three" => same(Version::V2, typical(state("pregame", Pregame), None, None, three_votes(), None)),
        "v2_votes_three_pending" => same(
            Version::V2,
            typical(state("pregame", Pregame), None, None, three_votes(), Some(LOW)),
        ),
        "v2_pending_no_votes" => same(
            Version::V2,
            typical(state("pregame", Pregame), None, None, vec![], Some(HIGH)),
        ),
        "v2_everything" => same(
            Version::V2,
            typical(
                state("playing", Playing),
                Some(gold("Round 2")),
                Some(bold("Go")),
                three_votes(),
                Some(HIGH),
            ),
        ),
        // V1: the identifier goes out upper-cased; the status and the vote fields are not written.
        "v1_zero" => v1_pregame_back(packet(
            false, false, state("pregame", Pregame), 0, 0, 0, 0, None, None, false, vec![], None,
        )),
        "v1_status_countdown" => v1_pregame_back(packet(
            true, true, state("countdown", Starting), 1000, 0, 0, 3, None, None, true, vec![], None,
        )),
        "v1_status_playing" => v1_pregame_back(packet(
            false, true, state("playing", Playing), 1500, 300_000, 2, 4, None, None, false, vec![], None,
        )),
        "v1_status_postgame" => v1_pregame_back(packet(
            true, false, state("postgame", Postgame), 5, 6, 7, 8, None, None, true, vec![], None,
        )),
        "v1_title_and_subtitle" => v1_pregame_back(packet(
            true, false, state("countdown", Starting), 1500, 300_000, 2, 4,
            Some(gold("Round 1")), Some(bold("Good luck")), true, vec![], None,
        )),
        "v1_title_empty" => v1_pregame_back(packet(
            false, false, state("pregame", Pregame), 0, 0, 0, 0, Some(plain("")), None, false, vec![], None,
        )),
        "v1_subtitle_only" => v1_pregame_back(packet(
            false, false, state("pregame", Pregame), 0, 0, 0, 0,
            None, Some(plain("zażółć 日本 \u{1F600}")), false, vec![], None,
        )),
        // The vote fields are set, but V1 has no place for them: they come back empty and absent.
        "v1_drops_votes" => v1_pregame_back(packet(
            false, false, state("pregame", Pregame), 0, 0, 0, 0, None, None, false, three_votes(), Some(HIGH),
        )),
        "v1_extremes" => v1_pregame_back(packet(
            true, true, state("starting_next_round", Postgame), -1, i64::MAX, i32::MIN, i32::MAX,
            None, None, true, vec![], None,
        )),
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

fn fields(line: &str) -> (&str, &str, &str) {
    let fields: Vec<&str> = line.split('\t').collect();
    assert_eq!(fields.len(), 3, "malformed oracle row {line:?}");
    (fields[0], fields[1], fields[2])
}

fn write_packet(version: Version, packet: &ReadyUpdatePacket) -> Vec<u8> {
    match version {
        Version::V1 => encode(|w| packet.write_v1(w).unwrap()),
        Version::V2 => encode(|w| packet.write_v2(w).unwrap()),
    }
}

#[test]
fn every_oracle_row_matches_the_mod_byte_for_byte_and_reads_back() {
    let mut rows = 0;
    for line in ORACLE.lines().filter(|l| !l.is_empty()) {
        let (name, channel, hex) = fields(line);
        if name == "vote_option_converter_id" {
            continue;
        }
        let case = case(name).unwrap_or_else(|| panic!("no Rust case for oracle row {name}"));
        let bytes = bytes_of(hex);
        let expected_channel = match case.version {
            Version::V1 => ReadyUpdatePacket::ID_V1,
            Version::V2 => ReadyUpdatePacket::ID_V2,
        };
        assert_eq!(channel, expected_channel, "{name}: channel id");
        assert_eq!(hex_of(&write_packet(case.version, &case.packet)), hex, "{name}: written bytes");
        let read = match case.version {
            Version::V1 => decode(&bytes, ReadyUpdatePacket::read_v1),
            Version::V2 => decode(&bytes, ReadyUpdatePacket::read_v2),
        };
        assert_eq!(read, Ok(case.read_back), "{name}: read back");
        rows += 1;
    }
    assert_eq!(rows, 23, "packet rows in the oracle");
}

#[test]
fn vote_option_converter_id_matches_the_mod() {
    let line = ORACLE
        .lines()
        .find(|l| l.starts_with("vote_option_converter_id\t"))
        .expect("the converter id row");
    assert_eq!(fields(line).1, VoteOption::ID);
}

#[test]
fn skeleton_rows_without_text_or_votes_still_reproduce() {
    // exp-skeleton/tests/oracle.tsv, the `ready_update_*` rows: absent title, subtitle and vote list.
    let mut rows = 0;
    for line in SKELETON_ORACLE.lines().filter(|l| l.starts_with("ready_update_")) {
        let (name, channel, hex) = fields(line);
        let bytes = bytes_of(hex);
        let (version, read) = match channel {
            ReadyUpdatePacket::ID_V1 => (Version::V1, decode(&bytes, ReadyUpdatePacket::read_v1)),
            ReadyUpdatePacket::ID_V2 => (Version::V2, decode(&bytes, ReadyUpdatePacket::read_v2)),
            other => panic!("{name}: unexpected channel {other}"),
        };
        let packet = read.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(packet.title, None, "{name}: title");
        assert_eq!(packet.subtitle, None, "{name}: subtitle");
        assert!(packet.vote_options.is_empty(), "{name}: vote options");
        assert_eq!(hex_of(&write_packet(version, &packet)), hex, "{name}: written bytes");
        rows += 1;
    }
    assert_eq!(rows, 6, "ready_update rows in the skeleton oracle");
}

#[test]
fn a_negative_vote_list_count_is_an_error() {
    // The `v2_zero` bytes with the voteOptions count (offset 41..45) set to -1.
    let mut bytes = bytes_of(
        ORACLE
            .lines()
            .find(|l| l.starts_with("v2_zero\t"))
            .map(|l| fields(l).2)
            .expect("the v2_zero row"),
    );
    bytes[41..45].copy_from_slice(&(-1i32).to_be_bytes());
    assert_eq!(decode(&bytes, ReadyUpdatePacket::read_v2), Err(DecodeError::NegativeLength(-1)));
}

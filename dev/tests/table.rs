use horizon_bingo::channels::{id, Dir, Status, CHANNELS};
use std::collections::HashSet;

/// The mod's registrations, read from `common/.../event/packet/ServerPacketEvents.kt`: each `registerS2C` /
/// `registerC2S` call resolved to its packet's `Identifier.of(MOD_ID_BINGO, "<path>")`. Twenty paths are only
/// server to client, two only client to server, and four (the settings and the three stats packets) both ways.
const MOD_CHANNELS: [(&str, Dir); 26] = [
    ("card_reset", Dir::S2c),
    ("display_cards", Dir::S2c),
    ("update_card_tiles", Dir::S2c),
    ("card_image", Dir::S2c),
    ("open_card", Dir::S2c),
    ("card_shuffled", Dir::S2c),
    ("card_completed", Dir::S2c),
    ("game_over_v2", Dir::S2c),
    ("game_over", Dir::S2c),
    ("score_update", Dir::S2c),
    ("scored_item_lost", Dir::S2c),
    ("game_status", Dir::S2c),
    ("game_message", Dir::S2c),
    ("game_message_clear", Dir::S2c),
    ("timer", Dir::S2c),
    ("countdown", Dir::S2c),
    ("tooltip_v2", Dir::S2c),
    ("game_reset", Dir::S2c),
    ("ready_timer_update", Dir::S2c),
    ("ready_timer_update_v2", Dir::S2c),
    ("set_ready", Dir::C2s),
    ("set_ready_v2", Dir::C2s),
    ("update_settings", Dir::Both),
    ("stats_check", Dir::Both),
    ("stats_game", Dir::Both),
    ("stats_index", Dir::Both),
];

#[test]
fn channel_table_matches_the_mods_registrations_path_by_path_and_direction() {
    let mut want: Vec<(&str, Dir)> = MOD_CHANNELS.to_vec();
    let mut have: Vec<(&str, Dir)> = CHANNELS.iter().map(|c| (c.path, c.dir)).collect();
    want.sort_by_key(|(path, _)| *path);
    have.sort_by_key(|(path, _)| *path);
    assert_eq!(have, want);
}

#[test]
fn channel_table_has_no_duplicate_paths() {
    let ids: HashSet<_> = CHANNELS.iter().map(|c| c.path).collect();
    assert_eq!(ids.len(), CHANNELS.len());
}

#[test]
fn every_done_channel_has_an_oracle_row_with_the_same_id() {
    let mut tsv = String::from(include_str!("oracle.tsv"));
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle")).unwrap() {
        tsv.push_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
        tsv.push('\n');
    }
    for c in CHANNELS.iter().filter(|c| c.status == Status::Done) {
        let want = id(c.path);
        assert!(
            tsv.lines().any(|l| l.split('\t').nth(1) == Some(want.as_str())),
            "{want} is marked Done but has no oracle row"
        );
    }
}

//! Saved data: round options, history lines and per-player settings. Expected values are written by hand from the
//! formats (32 hex digits per uuid, `mm:ss` for the best win), and the JSON keys are checked with an independent parse.
use std::collections::HashMap;

use horizon_bingo::server::options::{EndWhen, Goal, Mode, Options};
use horizon_bingo::server::store::{
    entry_to_line, options_from_json, options_to_json, parse_history, player_stats, settings_from_json,
    settings_to_json, stats_text, HistoryEntry, HistoryPlayer, HistoryTeam, PlayerStats,
};
use horizon_bingo::update_settings::PlayerSettings;
use serde_json::Value;

const ALICE: u128 = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef;
const BOB: u128 = 0xfedc_ba98_7654_3210_fedc_ba98_7654_3210;
const ALICE_HEX: &str = "0123456789abcdef0123456789abcdef";
const BOB_HEX: &str = "fedcba9876543210fedcba9876543210";

fn options(goal: Goal, mode: Mode, end_when: EndWhen) -> Options {
    Options { goal, mode, end_when, ..Options::default() }
}

fn round_trip(o: &Options) -> Options {
    options_from_json(&options_to_json(o))
}

#[test]
fn options_round_trip_for_several_variants() {
    let default = Options::default();
    assert_eq!(round_trip(&default), default);

    let items = Options {
        pvp: false,
        ..options(Goal::Items(7), Mode::Lockout, EndWhen::TeamsWin(2))
    };
    assert_eq!(round_trip(&items), items);

    let full = Options {
        keep_inventory: true,
        elytra: true,
        night_vision: true,
        spawn_distance: 40,
        ..options(Goal::FullCard, Mode::Inventory, EndWhen::AllWin)
    };
    assert_eq!(round_trip(&full), full);

    let never = options(Goal::Lines(3), Mode::Standard, EndWhen::Never);
    assert_eq!(round_trip(&never), never);
}

#[test]
fn options_json_uses_the_documented_keys() {
    let o = options(Goal::Items(7), Mode::Lockout, EndWhen::TeamsWin(2));
    let v: Value = serde_json::from_str(&options_to_json(&o)).unwrap();
    assert_eq!(v["goal"]["type"], "items");
    assert_eq!(v["goal"]["n"], 7);
    assert_eq!(v["mode"], "lockout");
    assert_eq!(v["endWhen"]["type"], "teams_win");
    assert_eq!(v["endWhen"]["n"], 2);
    assert_eq!(v["pvp"], true);
    assert_eq!(v["keepInventory"], false);
    assert_eq!(v["elytra"], false);
    assert_eq!(v["nightVision"], false);
    assert_eq!(v["spawnDistance"], 25);
}

#[test]
fn options_from_empty_or_broken_text_is_default() {
    assert_eq!(options_from_json(""), Options::default());
    assert_eq!(options_from_json("{}"), Options::default());
    assert_eq!(options_from_json("not json"), Options::default());
    assert_eq!(options_from_json("[1, 2]"), Options::default());
}

#[test]
fn zero_count_takes_the_default_goal() {
    let o = options_from_json(r#"{"goal":{"type":"lines","n":0}}"#);
    assert_eq!(o.goal, Goal::Lines(1));
    assert_eq!(o, Options::default());

    let o = options_from_json(r#"{"goal":{"type":"items","n":0}}"#);
    assert_eq!(o.goal, Goal::Lines(1));
}

#[test]
fn bad_fields_fall_back_one_by_one() {
    let o = options_from_json(
        r#"{"mode":"lockout","pvp":"yes","spawnDistance":-3,"endWhen":{"type":"teams_win","n":0},"elytra":true}"#,
    );
    assert_eq!(o.mode, Mode::Lockout);
    assert!(o.pvp, "a string is not a flag, so pvp keeps its default true");
    assert_eq!(o.spawn_distance, 25);
    assert_eq!(o.end_when, EndWhen::FirstWin);
    assert!(o.elytra);
    assert_eq!(o.goal, Goal::Lines(1));
}

fn two_team_entry() -> HistoryEntry {
    HistoryEntry {
        id: 0x1f,
        ended_at_ms: 1_700_000_000_000,
        duration_ms: 205_500,
        winner: Some("red".to_owned()),
        forfeit: false,
        teams: vec![
            HistoryTeam { key: "red".into(), name: "Red".into(), items: 9, lines: 1, won: true },
            HistoryTeam { key: "blue".into(), name: "Blue".into(), items: 4, lines: 0, won: false },
        ],
        players: vec![
            HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: "red".into(), items: 5 },
            HistoryPlayer { uuid: BOB, name: "Bob".into(), team: "blue".into(), items: 4 },
        ],
        rules: None,
    }
}

#[test]
fn entry_round_trips_through_one_line() {
    let e = two_team_entry();
    let line = entry_to_line(&e);
    assert!(!line.contains('\n'));
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["players"][0]["uuid"], ALICE_HEX);
    assert_eq!(parse_history(&line), vec![e]);
}

#[test]
fn history_skips_blank_and_broken_lines_and_keeps_order() {
    let first = two_team_entry();
    let second = HistoryEntry {
        id: 0x2,
        winner: None,
        duration_ms: 60_000,
        ..two_team_entry()
    };
    let text = format!(
        "{}\n\n   \n{{broken\n{{\"id\":\"zz\"}}\n{}\n",
        entry_to_line(&first),
        entry_to_line(&second)
    );
    assert_eq!(parse_history(&text), vec![first, second]);
    assert!(parse_history("").is_empty());
}

/// Alice's rounds, counted by hand: a win in 200000 ms with 5 items, a loss in 90000 ms with 3 items (its short time
/// must not count as the best win), a win in 150000 ms with 4 items, and a round without Alice.
fn alice_history() -> Vec<HistoryEntry> {
    let round = |id: u128, winner: &str, duration_ms: i64, alice_team: &str, alice_items: i32| HistoryEntry {
        id,
        duration_ms,
        winner: Some(winner.to_owned()),
        players: vec![
            HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: alice_team.into(), items: alice_items },
            HistoryPlayer { uuid: BOB, name: "Bob".into(), team: "other".into(), items: 1 },
        ],
        ..two_team_entry()
    };
    vec![
        round(1, "red", 200_000, "red", 5),
        round(2, "blue", 90_000, "red", 3),
        round(3, "blue", 150_000, "blue", 4),
        HistoryEntry {
            id: 4,
            winner: Some("red".to_owned()),
            players: vec![HistoryPlayer { uuid: BOB, name: "Bob".into(), team: "red".into(), items: 30 }],
            ..two_team_entry()
        },
    ]
}

#[test]
fn player_stats_count_games_wins_best_win_and_items() {
    let stats = player_stats(&alice_history(), ALICE);
    assert_eq!(
        stats,
        PlayerStats { games: 3, wins: 2, best_win_ms: Some(150_000), items: 12 }
    );
    assert_eq!(stats_text("Alice", &stats), "Alice: 3 games, 2 wins, best win 02:30, 12 items.");
}

#[test]
fn stats_without_wins_say_none() {
    let losses: Vec<HistoryEntry> = alice_history().into_iter().filter(|e| e.id == 2).collect();
    let stats = player_stats(&losses, ALICE);
    assert_eq!(stats.wins, 0);
    assert_eq!(stats.best_win_ms, None);
    assert_eq!(stats_text("Alice", &stats), "Alice: 1 games, 0 wins, best win none, 3 items.");
}

#[test]
fn stats_for_an_unknown_player_are_empty() {
    let stats = player_stats(&alice_history(), 0x77);
    assert_eq!(stats, PlayerStats::default());
    assert_eq!(stats_text("Nobody", &stats), "Nobody: 0 games, 0 wins, best win none, 0 items.");
}

#[test]
fn settings_round_trip_with_defaults_and_changes() {
    let changed = PlayerSettings {
        bossbar: true,
        night_vision: false,
        leading_messages: false,
        ..PlayerSettings::default()
    };
    let mut m = HashMap::new();
    m.insert(ALICE, PlayerSettings::default());
    m.insert(BOB, changed.clone());
    assert_eq!(settings_from_json(&settings_to_json(&m)), m);

    let text = settings_to_json(&m);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v[BOB_HEX]["bossbar"], true);
    assert_eq!(v[BOB_HEX]["nightVision"], false);
    assert_eq!(v[ALICE_HEX]["scoreboard"], true);
    assert!(text.find(ALICE_HEX) < text.find(BOB_HEX), "keys are sorted");
}

#[test]
fn settings_ignore_unknown_keys_and_skip_broken_entries() {
    let text = format!(
        r#"{{"{ALICE_HEX}": {{"bossbar": true, "noSuchSetting": 3}}, "{BOB_HEX}": {{"bossbar": "yes"}}, "xyz": {{}}}}"#
    );
    let m = settings_from_json(&text);
    assert_eq!(m.len(), 1);
    assert_eq!(m[&ALICE], PlayerSettings { bossbar: true, ..PlayerSettings::default() });
    assert!(settings_from_json("not json").is_empty());
}

#[test]
fn a_round_with_no_winner_is_no_win_for_anyone() {
    // Alice's team would be the winner if the round had one, but nobody won: it is a game, not a win.
    let draw = HistoryEntry {
        id: 5,
        duration_ms: 1_000,
        winner: None,
        players: vec![HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: "red".into(), items: 2 }],
        ..two_team_entry()
    };
    assert_eq!(player_stats(&[draw], ALICE), PlayerStats { games: 1, wins: 0, best_win_ms: None, items: 2 });
}

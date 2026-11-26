//! Singleplayer best time: the original keeps one best-time row per rules and kind of game (`BestTime` keyed by
//! `is_singleplayer`), so a faster multiplayer round does not set the singleplayer best. Expected values are the
//! durations written in the test.
use horizon_bingo::server::stats;
use horizon_bingo::server::store::{HistoryEntry, HistoryPlayer, HistoryTeam};

const ALICE: u128 = 0xa1;
const BOB: u128 = 0xb0;
const RULES: &str = "aaaaaaaaaaaaaaaa";

/// A round of rules `RULES` that `red` won, with Alice on red and, when `two_players`, Bob on blue.
fn round(id: u128, duration_ms: i64, two_players: bool) -> HistoryEntry {
    let mut players = vec![HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: "red".into(), items: 0 }];
    if two_players {
        players.push(HistoryPlayer { uuid: BOB, name: "Bob".into(), team: "blue".into(), items: 0 });
    }
    HistoryEntry {
        id,
        ended_at_ms: 1_700_000_000_000,
        duration_ms,
        winner: Some("red".into()),
        forfeit: false,
        teams: vec![
            HistoryTeam { key: "red".into(), name: "Red".into(), items: 0, lines: 0, won: true },
            HistoryTeam { key: "blue".into(), name: "Blue".into(), items: 0, lines: 0, won: false },
        ],
        players,
        rules: Some(RULES.into()),
    }
}

#[test]
fn the_singleplayer_best_is_the_shortest_one_player_round_not_a_faster_multiplayer_one() {
    // Alone 900 s, then a two-player round of 100 ms: the singleplayer best stays the 900 ms round.
    let history = [round(1, 900_000, false), round(2, 100_000, true)];
    assert_eq!(stats::best_time(&history, RULES, true), Some(900_000));
    assert_eq!(stats::best_time(&history, RULES, false), Some(100_000));
}

#[test]
fn a_new_singleplayer_round_is_a_new_best_only_against_the_singleplayer_rounds() {
    // A two-player round of 100 s does not set the singleplayer best, so a one-player round of 400 s does.
    let history = [round(1, 900_000, false), round(2, 100_000, true)];
    let end = stats::end_stats(&history, &round(3, 400_000, false), None);
    assert!(end.is_best_time);
    assert_eq!(end.prev_best_time_ms, Some(900_000));
}

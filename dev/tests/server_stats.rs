//! Statistics math of the history: the end screen's win streak, best time and best items, and the `bingo stats`
//! numbers. Each expected value is counted by hand from the rounds written in the test, not from the code.
use horizon_bingo::server::options::{EndWhen, Goal, Mode, Options};
use horizon_bingo::server::stats::{self, Captures};
use horizon_bingo::server::store::{HistoryEntry, HistoryPlayer, HistoryTeam};
use serde_json::json;

const ALICE: u128 = 0xa1;
const BOB: u128 = 0xb0;
const CAROL: u128 = 0xc4;
const RULES_A: &str = "aaaaaaaaaaaaaaaa";
const RULES_B: &str = "bbbbbbbbbbbbbbbb";
/// The start of the test clock, in ms: 2023-11-14 22:13:20 UTC.
const CLOCK: i64 = 1_700_000_000_000;

/// One round: `winner` (a team key or none), who is where, and the items of Alice and Bob. Every round lasts `duration_ms`
/// and ends at `ended_at_ms`, which is what the month filter reads.
struct Round {
    id: u128,
    winner: Option<&'static str>,
    forfeit: bool,
    duration_ms: i64,
    ended_at_ms: i64,
    /// Teams of the round: `red` for Alice, `blue` for Bob; Carol is on no team when `carol` is set.
    alice: &'static str,
    bob: &'static str,
    carol: bool,
    alice_items: i32,
    bob_items: i32,
    rules: Option<&'static str>,
}

impl Round {
    fn won(id: u128, winner: &'static str, duration_ms: i64) -> Self {
        Round {
            id,
            winner: Some(winner),
            forfeit: false,
            duration_ms,
            ended_at_ms: CLOCK,
            alice: "red",
            bob: "blue",
            carol: false,
            alice_items: 0,
            bob_items: 0,
            rules: Some(RULES_A),
        }
    }

    fn build(&self) -> HistoryEntry {
        let teams = vec![
            HistoryTeam { key: "red".into(), name: "Red".into(), items: 0, lines: 0, won: self.winner == Some("red") },
            HistoryTeam { key: "blue".into(), name: "Blue".into(), items: 0, lines: 0, won: self.winner == Some("blue") },
        ];
        let mut players = vec![
            HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: self.alice.into(), items: self.alice_items },
            HistoryPlayer { uuid: BOB, name: "Bob".into(), team: self.bob.into(), items: self.bob_items },
        ];
        if self.carol {
            players.push(HistoryPlayer { uuid: CAROL, name: "Carol".into(), team: "red".into(), items: 0 });
        }
        HistoryEntry {
            id: self.id,
            ended_at_ms: self.ended_at_ms,
            duration_ms: self.duration_ms,
            winner: self.winner.map(str::to_string),
            forfeit: self.forfeit,
            teams,
            players,
            rules: self.rules.map(str::to_string),
        }
    }
}

/// A round Alice's red team won, Bob on blue, in rules A.
fn win(id: u128, duration_ms: i64) -> HistoryEntry {
    Round::won(id, "red", duration_ms).build()
}

/// A round Alice lost (blue won), in rules A.
fn loss(id: u128) -> HistoryEntry {
    Round::won(id, "blue", 1000).build()
}

/// A draw: nobody won, two teams.
fn draw(id: u128) -> HistoryEntry {
    Round { winner: None, ..Round::won(id, "red", 1000) }.build()
}

/// A forfeit: ended by `end`, no winner.
fn forfeit(id: u128, duration_ms: i64) -> HistoryEntry {
    Round { winner: None, forfeit: true, ..Round::won(id, "red", duration_ms) }.build()
}

fn streak_of(rounds: &[HistoryEntry]) -> i64 {
    stats::current_streak(rounds, ALICE)
}

#[test]
fn the_streak_counts_the_wins_since_the_latest_loss_draw_or_nothing() {
    assert_eq!(streak_of(&[win(1, 1000), win(2, 1000), loss(3), win(4, 1000), win(5, 1000), win(6, 1000)]), 3);
    assert_eq!(streak_of(&[win(1, 1000), loss(2), win(3, 1000)]), 1);
    assert_eq!(streak_of(&[win(1, 1000), draw(2), win(3, 1000)]), 1, "a draw ends a streak in the original's query");
    assert_eq!(streak_of(&[]), 0);
    assert_eq!(streak_of(&[loss(1), loss(2)]), 0);
}

#[test]
fn a_forfeit_keeps_the_streak_and_a_round_without_alice_does_not_count() {
    assert_eq!(streak_of(&[win(1, 1000), forfeit(2, 500), win(3, 1000)]), 2);
    let bob_only = Round { alice: "red", bob: "red", ..Round::won(9, "blue", 1000) };
    // Alice's team lost that round, so the streak is 0 after a win before it.
    assert_eq!(streak_of(&[win(1, 1000), bob_only.build()]), 0);
    let carol_round = Round { carol: true, ..Round::won(10, "red", 1000) };
    assert_eq!(streak_of(&[win(1, 1000), carol_round.build()]), 2, "a player on the team counts; Carol's entry is not Alice's");
}

#[test]
fn the_best_streak_is_the_highest_streak_after_a_round_and_none_without_a_win() {
    assert_eq!(stats::best_streak(&[win(1, 1000), win(2, 1000), loss(3), win(4, 1000), win(5, 1000), win(6, 1000)], ALICE), Some(3));
    assert_eq!(stats::best_streak(&[win(1, 1000), loss(2), win(3, 1000)], ALICE), Some(1));
    assert_eq!(stats::best_streak(&[loss(1), draw(2)], ALICE), None);
    assert_eq!(stats::best_streak(&[], ALICE), None);
    assert_eq!(stats::best_streak(&[win(1, 1000), forfeit(2, 9)], ALICE), Some(1));
}

#[test]
fn the_best_time_is_the_shortest_win_of_the_rules_and_skips_draws_forfeits_and_other_rules() {
    let other_rules = Round { rules: Some(RULES_B), ..Round::won(7, "red", 10) }.build();
    let rounds = [
        win(1, 300_000),
        win(2, 200_000),
        draw(3),
        forfeit(4, 50_000),
        other_rules,
    ];
    assert_eq!(stats::best_time(&rounds, RULES_A, false), Some(200_000));
    assert_eq!(stats::best_time(&rounds, RULES_B, false), Some(10), "a round of other rules has its own best");
    assert_eq!(stats::best_time(&rounds, "cccccccccccccccc", false), None);
}

#[test]
fn a_draw_or_a_forfeit_alone_gives_no_best_time() {
    assert_eq!(stats::best_time(&[draw(1), forfeit(2, 100)], RULES_A, false), None);
}

#[test]
fn an_old_round_without_rules_never_sets_a_best() {
    let old = HistoryEntry { rules: None, ..win(1, 100) };
    assert_eq!(stats::best_time(&[old.clone()], RULES_A, false), None);
    assert_eq!(stats::best_items(&[old], ALICE, RULES_A), None);
}

#[test]
fn the_singleplayer_query_answers_only_when_the_first_round_had_one_player() {
    // The first round had two players: the one-row table answers the singleplayer query with nothing.
    let two_players_first = [win(1, 300_000), Round { bob: "red", ..Round::won(2, "red", 100) }.build()];
    assert_eq!(stats::best_time(&two_players_first, RULES_A, true), None);
    assert_eq!(stats::best_time(&two_players_first, RULES_A, false), Some(100));
    // A first round alone, with the one player: its own times answer.
    let alone = |id: u128, ms: i64| HistoryEntry { players: vec![HistoryPlayer { uuid: ALICE, name: "Alice".into(), team: "red".into(), items: 0 }], ..win(id, ms) };
    assert_eq!(stats::best_time(&[alone(1, 900), alone(2, 400)], RULES_A, true), Some(400));
}

#[test]
fn a_new_best_time_is_strictly_shorter_than_the_previous_one() {
    assert!(stats::is_best_time(None, 5000, false, true), "no earlier best");
    assert!(stats::is_best_time(Some(6000), 5000, false, true));
    assert!(!stats::is_best_time(Some(5000), 5000, false, true), "a tie is no new best");
    assert!(!stats::is_best_time(Some(6000), 5000, true, true), "a forfeit is no best");
    assert!(!stats::is_best_time(None, 5000, false, false), "a round nobody won is no best");
}

#[test]
fn the_best_items_keep_forfeits_and_other_rules_apart() {
    let with_items = |id: u128, items: i32, rules: &'static str, forfeit_round: bool| {
        let mut round = Round::won(id, "red", 1000);
        round.alice_items = items;
        round.rules = Some(rules);
        round.forfeit = forfeit_round;
        if forfeit_round {
            round.winner = None;
        }
        round.build()
    };
    let rounds = [with_items(1, 4, RULES_A, false), with_items(2, 7, RULES_A, false), with_items(3, 9, RULES_A, true), with_items(4, 20, RULES_B, false)];
    assert_eq!(stats::best_items(&rounds, ALICE, RULES_A), Some(9), "a forfeit still counts its items");
    assert_eq!(stats::best_items(&rounds, ALICE, RULES_B), Some(20));
    assert_eq!(stats::best_items(&rounds, BOB, RULES_A), Some(0), "Bob's items are 0 in each round");
}

fn options_with(goal: Goal) -> Options {
    Options { goal, mode: Mode::Standard, end_when: EndWhen::FirstWin, ..Options::default() }
}

#[test]
fn a_round_with_a_win_streak_and_best_items_gives_the_end_screen_numbers() {
    // Prior: two wins in a row (best streak 2), best items 5. This round: Alice won, found 6 items, 40 s long.
    let prior = [
        Round { alice_items: 5, ..Round::won(1, "red", 90_000) }.build(),
        Round { alice_items: 3, ..Round::won(2, "red", 60_000) }.build(),
    ];
    let game = Round { alice_items: 6, ..Round::won(3, "red", 40_000) }.build();
    let mut all = prior.to_vec();
    all.push(game);
    let end = stats::end_stats(&prior, &all[2], Some(ALICE));
    assert!(end.is_best_time, "40 s is shorter than 60 s, the shortest before");
    assert_eq!(end.prev_best_time_ms, Some(60_000));
    let player = end.player.expect("Alice played");
    assert_eq!(player.win_streak, 3);
    assert_eq!(player.best_win_streak, Some(2));
    assert!(player.is_best_win_streak, "3 is above the best 2");
    assert_eq!(player.captured_items, 6);
    assert_eq!(player.best_captured_items, Some(5));
    assert!(player.is_best_captured_items, "6 is above the best 5");
}

#[test]
fn a_streak_equal_to_the_best_is_not_a_new_best_and_a_loss_shows_zero() {
    let prior = [win(1, 9000), win(2, 9000), loss(3)];
    let mut all = prior.to_vec();
    all.push(win(4, 9000));
    let won = stats::end_stats(&all[..3], &all[3], Some(ALICE)).player.expect("played");
    // Before the round: streak 0, best 2. This win makes 1.
    assert_eq!((won.win_streak, won.best_win_streak, won.is_best_win_streak), (1, Some(2), false));
    let lost_round = loss(5);
    let mut all2 = all.clone();
    all2.push(lost_round);
    let lost = stats::end_stats(&all2[..4], &all2[4], Some(ALICE)).player.expect("played");
    assert_eq!(lost.win_streak, 0);
    assert!(!lost.is_best_win_streak);
}

#[test]
fn a_draw_or_a_forfeit_keeps_the_viewers_streak_on_their_screen() {
    let prior = [win(1, 9000), win(2, 9000)];
    let mut all = prior.to_vec();
    all.push(draw(3));
    let on_draw = stats::end_stats(&prior, &all[2], Some(ALICE)).player.expect("played");
    assert_eq!(on_draw.win_streak, 2, "the draw's own screen keeps the streak");
    let mut all2 = prior.to_vec();
    all2.push(forfeit(4, 500));
    let on_forfeit = stats::end_stats(&prior, &all2[2], Some(ALICE)).player.expect("played");
    assert_eq!(on_forfeit.win_streak, 2);
    assert!(!stats::end_stats(&prior, &all2[2], Some(ALICE)).is_best_time, "a forfeit is no best time");
}

#[test]
fn a_round_nobody_won_with_one_team_is_a_loss_for_the_viewer() {
    // One team only: not a draw, and nobody won, so the streak resets to 0.
    let prior = [win(1, 9000)];
    let one_team = HistoryEntry {
        teams: vec![HistoryTeam { key: "red".into(), name: "Red".into(), items: 0, lines: 0, won: false }],
        winner: None,
        ..Round::won(2, "red", 4000).build()
    };
    let end = stats::end_stats(&prior, &one_team, Some(ALICE)).player.expect("played");
    assert_eq!(end.win_streak, 0);
}

#[test]
fn a_one_team_forfeit_keeps_the_streak_though_it_is_no_draw() {
    // One team: a forfeit is not a draw, so only the forfeit rule keeps the streak.
    let prior = [win(1, 9000), win(2, 9000)];
    let one_team_forfeit = HistoryEntry {
        teams: vec![HistoryTeam { key: "red".into(), name: "Red".into(), items: 0, lines: 0, won: false }],
        winner: None,
        forfeit: true,
        ..Round::won(3, "red", 500).build()
    };
    let end = stats::end_stats(&prior, &one_team_forfeit, Some(ALICE)).player.expect("played");
    assert_eq!(end.win_streak, 2);
}

#[test]
fn a_spectator_and_the_game_wide_part_have_no_personal_numbers() {
    let prior = [win(1, 9000)];
    let game = Round { carol: true, ..Round::won(2, "red", 4000) }.build();
    let spectator = stats::end_stats(&prior, &game, Some(0x77));
    assert!(spectator.player.is_none(), "a uuid on no team");
    assert!(spectator.is_best_time, "the best time is the game's, shown to everyone");
    assert_eq!(spectator.prev_best_time_ms, Some(9000));
    assert!(stats::end_stats(&prior, &game, None).player.is_none());
}

#[test]
fn the_month_of_a_time_in_utc() {
    assert_eq!(stats::year_month(CLOCK), (2023, 11));
    assert_eq!(stats::year_month(0), (1970, 1));
    assert_eq!(stats::year_month(1_706_745_600_000), (2024, 2), "2024-02-01 00:00 UTC");
    assert_eq!(stats::year_month(1_706_745_599_999), (2024, 1));
}

#[test]
fn totals_count_games_wins_losses_items_and_time_and_month() {
    let forfeit_round = Round { winner: None, forfeit: true, alice_items: 4, ..Round::won(4, "red", 3000) }.build();
    let rounds = [
        Round { alice_items: 2, ..Round::won(1, "red", 10_000) }.build(),
        Round { alice_items: 1, ..Round::won(2, "blue", 5000) }.build(),
        draw(3),
        forfeit_round,
        // Started in January 2024 (ends 1 s into February, lasts 2 s).
        Round { ended_at_ms: 1_706_745_601_000, duration_ms: 2000, alice_items: 7, ..Round::won(5, "red", 2000) }.build(),
    ];
    let all = stats::totals(&rounds, ALICE, None);
    // Games: the four rounds that are not forfeits (win, loss, draw, win-in-January). Wins 2, losses 1, draw neither.
    assert_eq!(all.games, 4);
    assert_eq!((all.wins, all.losses), (2, 1));
    // Items and time include the forfeit (4 items, 3 s) and every round.
    assert_eq!(all.items, 2 + 1 + 4 + 7);
    assert_eq!(all.playtime_ms, 10_000 + 5000 + 1000 + 3000 + 2000);
    // The month of the clock: the round that started in January is out. A round starts at its end minus its duration.
    let november = stats::totals(&rounds, ALICE, Some((2023, 11)));
    assert_eq!(november.games, 3);
    assert_eq!(november.items, 2 + 1 + 4);
    assert_eq!(november.playtime_ms, 10_000 + 5000 + 1000 + 3000);
}

#[test]
fn a_time_and_a_count_are_written_as_the_original_writes_them() {
    assert_eq!(stats::format_duration(6500), "00m 06s");
    assert_eq!(stats::format_duration(3_725_000), "1h 02m 05s");
    assert_eq!(stats::format_duration(90_061_000), "1d 1h 01m 01s");
    assert_eq!(stats::format_duration(0), "00m 00s");
    let all = stats::Totals { games: 1234, wins: 1000, losses: 234, items: 1_000_000, playtime_ms: 61_000 };
    let monthly = stats::Totals::default();
    assert_eq!(
        stats::summary_text("Alice", &all, &monthly),
        "Bingo stats for Alice:\nAll Time:\n  Games: 1,234, Win/Loss: 1,000/234, Items: 1,000,000\n  Playtime: 01m 01s\nMonthly:\n  Games: 0, Win/Loss: 0/0, Items: 0\n  Playtime: 00m 00s"
    );
}

#[test]
fn the_rules_key_is_sixteen_hex_digits_and_follows_the_options() {
    let base = options_with(Goal::Items(3));
    let key = stats::rules_key(&base);
    assert_eq!(key.len(), 16);
    assert!(key.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(key, stats::rules_key(&options_with(Goal::Items(3))));
    assert_ne!(key, stats::rules_key(&options_with(Goal::Items(4))));
    assert_ne!(key, stats::rules_key(&Options { mode: Mode::Lockout, ..base }));
}

#[test]
fn the_capture_ledger_follows_gains_and_losses_to_the_finder() {
    let mut captures = Captures::default();
    captures.gained("red", ALICE, &[0, 1]);
    captures.gained("red", BOB, &[2]);
    captures.gained("blue", ALICE, &[0]);
    assert_eq!((captures.items(ALICE), captures.items(BOB)), (3, 1));
    // Red loses square 1 (Alice's) and square 9 (never found): only Alice goes down.
    captures.lost("red", &[1, 9]);
    assert_eq!((captures.items(ALICE), captures.items(BOB)), (2, 1));
    // A square lost twice is taken once.
    captures.lost("red", &[1]);
    assert_eq!(captures.items(ALICE), 2);
    captures.clear();
    assert_eq!((captures.items(ALICE), captures.items(BOB)), (0, 0));
}

#[test]
fn the_ledger_survives_a_save_and_an_old_save_gives_none() {
    let mut captures = Captures::default();
    captures.gained("red", ALICE, &[3, 4]);
    captures.gained("blue", BOB, &[0]);
    let saved = captures.to_json();
    assert_eq!(saved["items"].as_object().map(|items| items.len()), Some(2));
    let mut restored = Captures::from_json(&saved);
    assert_eq!((restored.items(ALICE), restored.items(BOB)), (2, 1));
    // The finders came back too: losing square 4 takes it from Alice.
    restored.lost("red", &[4]);
    assert_eq!(restored.items(ALICE), 1);
    assert_eq!(Captures::from_json(&json!({})).items(ALICE), 0);
}

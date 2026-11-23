//! Teams and the lobby ready timer against the original mod (2.15.1), hand-traced.
//!
//! Sources, read from /home/mateusz/bingo-horizon-port/orig/common/src/main/:
//! - presets: resources `data/yet-another-minecraft-bingo/yet-another-minecraft-bingo/teams/bingo_*.json`
//! - team joins and spectators: `team/TeamService.kt` `joinTeam` (line 83-99), `joinSpectators` (line 101-120)
//! - shuffle: `team/TeamService.kt` `shuffleTeams` (line 131-180), the command `team/ShuffleTeamsCommand.kt` (`integer("number_of_teams", min = 1)`)
//! - ready timer: `ready/ReadyTimerState.kt` `duration()` (line 46-50), `ready/ReadyService.kt` `isConditionSatisfied`,
//!   `tick`, `setReady`, `ready/ReadyController.kt` completion (line 88-89), `ready/ReadyCommand.kt` `cancel` (line 58-71)
//! - defaults: `config/BingoConfig.kt` lines 50-59
//!
//! Timer ticks: one `tick` call is one server tick of 50 ms. The call that starts the countdown does not count, so
//! call n has elapsed `50 * (n - 1)` ms. The countdown ends on the first call where the remaining time is <= 0.
use horizon_bingo::server::card_gen::Rng;
use horizon_bingo::server::ready::{ReadyOptions, ReadyTimer};
use horizon_bingo::server::teams::{Teams, PRESETS};

const A: u128 = 1;
const B: u128 = 2;
const C: u128 = 3;
const D: u128 = 4;

fn keys(teams: &Teams) -> Vec<&'static str> {
    teams.teams()
}

/// Team sizes in the order of the presets, skipping empty teams.
fn sizes(teams: &Teams) -> Vec<usize> {
    let mut sizes: Vec<usize> = keys(teams).iter().map(|k| teams.members(k).len()).collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

// ---------- Teams ----------

#[test]
fn the_eight_presets_match_the_original_team_files() {
    // hand-traced from bingo_*.json: key, color, display name (the fallback text), symbol (🔥 = fire, ...).
    let expected = [
        ("bingo_red", "RED", "Red", "\u{1F525}"),
        ("bingo_blue", "BLUE", "Blue", "\u{1F30A}"),
        ("bingo_green", "GREEN", "Green", "\u{1F342}"),
        ("bingo_yellow", "YELLOW", "Yellow", "\u{1F41D}"),
        ("bingo_aqua", "AQUA", "Aqua", "\u{1F344}"),
        ("bingo_pink", "LIGHT_PURPLE", "Pink", "\u{1F33A}"),
        ("bingo_orange", "GOLD", "Orange", "\u{1F344}"),
        ("bingo_gray", "GRAY", "Gray", "\u{1FAA8}"),
    ];
    assert_eq!(PRESETS.len(), expected.len());
    for (preset, (key, color, name, symbol)) in PRESETS.iter().zip(expected) {
        assert_eq!((preset.key, preset.color, preset.name, preset.symbol), (key, color, name, symbol));
    }
}

#[test]
fn joining_a_team_takes_a_spectator_off_the_spectators() {
    // TeamService.kt:88 `state.playersSpectatingIds.remove(player.uuid)`.
    let mut teams = Teams::new();
    teams.spectate(A);
    assert!(teams.is_spectator(A));
    teams.join(A, "bingo_red").unwrap();
    assert!(!teams.is_spectator(A));
    assert_eq!(teams.team_of(A), Some("bingo_red"));
}

#[test]
fn joining_another_team_moves_the_player() {
    // TeamService.kt:86 removes the player from their old team before joining the new one.
    let mut teams = Teams::new();
    teams.join(A, "bingo_red").unwrap();
    teams.join(A, "bingo_blue").unwrap();
    assert!(teams.members("bingo_red").is_empty());
    assert_eq!(teams.members("bingo_blue"), vec![A]);
}

#[test]
fn joining_the_same_team_again_keeps_the_place_in_the_team() {
    // TeamService.kt:82 `if (getPlayerTeam(player)?.key == teamTemplate.key) return`: nothing changes, so A stays first.
    let mut teams = Teams::new();
    teams.join(A, "bingo_red").unwrap();
    teams.join(B, "bingo_red").unwrap();
    teams.join(A, "bingo_red").unwrap();
    assert_eq!(teams.members("bingo_red"), vec![A, B]);
}

#[test]
fn spectating_leaves_the_team() {
    // TeamService.kt:101-111: the team is left (`removePlayer`), then the player joins the spectators.
    let mut teams = Teams::new();
    teams.join(A, "bingo_red").unwrap();
    teams.spectate(A);
    assert_eq!(teams.team_of(A), None);
    assert!(teams.is_spectator(A));
    assert!(teams.members("bingo_red").is_empty());
}

#[test]
fn leaving_reports_whether_the_player_was_anywhere() {
    // TeamService.kt:118 emits TeamChanged only when `removePlayer` returned true.
    let mut teams = Teams::new();
    assert!(!teams.leave(A));
    teams.join(A, "bingo_red").unwrap();
    assert!(teams.leave(A));
    assert_eq!(teams.team_of(A), None);
}

#[test]
fn shuffle_of_six_into_three_gives_two_each() {
    // TeamService.kt:153-171: 6 players, 3 teams: minPlayersPerTeam = floor(6 / 3) = 2, three full chunks of 2, one
    // to each team: sizes 2, 2, 2.
    let mut teams = Teams::new();
    teams.shuffle(&[A, B, C, D, 5, 6], 3, &mut Rng::new(7)).unwrap();
    assert_eq!(sizes(&teams), vec![2, 2, 2]);
}

#[test]
fn shuffle_keeps_spectators_out_of_the_teams() {
    // TeamService.kt:165-166 `filter { !state.playersSpectatingIds.contains(it.uuid) }`.
    let mut teams = Teams::new();
    teams.spectate(D);
    teams.shuffle(&[A, B, C, D], 2, &mut Rng::new(3)).unwrap();
    assert_eq!(teams.team_of(D), None);
    assert!(teams.is_spectator(D));
    assert_eq!(sizes(&teams), vec![2, 1]);
}

#[test]
fn shuffle_clears_the_old_teams_first() {
    // TeamService.kt:132 `state.teams.clear()`: a player on a team before the shuffle is only on a new one afterwards.
    let mut teams = Teams::new();
    teams.join(A, "bingo_gray").unwrap();
    teams.shuffle(&[B, C], 1, &mut Rng::new(1)).unwrap();
    assert_eq!(teams.team_of(A), None);
    assert_eq!(sizes(&teams), vec![2]);
}

#[test]
fn team_count_below_one_is_refused() {
    // ShuffleTeamsCommand.kt: `integer("number_of_teams", min = 1)`.
    let mut teams = Teams::new();
    assert!(teams.shuffle(&[A, B], 0, &mut Rng::new(0)).is_err());
}

#[test]
fn shuffle_fills_teams_in_chunks_of_the_minimum_size() {
    // TeamService.kt:153-171: min = floor(8 / 3) = 2 gives chunks [2, 2, 2, 2]; chunk i goes to teams[i % 3], so
    // teams 0, 1, 2, 0 get them: sizes 4, 2, 2.
    let mut teams = Teams::new();
    teams.shuffle(&[1, 2, 3, 4, 5, 6, 7, 8], 3, &mut Rng::new(9)).unwrap();
    assert_eq!(sizes(&teams), vec![4, 2, 2]);
}

#[test]
fn shuffle_picks_presets_at_random() {
    // TeamService.kt:134-137: the presets are shuffled, then the first numTeams are taken. With 2 teams the original
    // can give any pair of presets, so some seed puts two players on a team that is not red and blue.
    let used_other_pair = (0..50u64).any(|seed| {
        let mut teams = Teams::new();
        teams.shuffle(&[A, B], 2, &mut Rng::new(seed)).unwrap();
        keys(&teams) != vec!["bingo_red", "bingo_blue"]
    });
    assert!(used_other_pair);
}

#[test]
#[ignore = "differs from the original: 9 teams are allowed there (temporary teams named temp_bingo_<i>), this port keeps teams as indexes into the fixed 8-entry PRESETS table and refuses more than 8"]
fn shuffle_accepts_more_teams_than_presets() {
    // TeamService.kt:145-158 creates `temp_bingo_<i>` teams until numTeams is reached.
    let mut teams = Teams::new();
    teams.shuffle(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 9, &mut Rng::new(0)).unwrap();
    assert_eq!(keys(&teams).len(), 9);
}

// ---------- Ready timer ----------

#[test]
fn default_countdown_is_600_seconds() {
    // BingoConfig.kt:50 `startWhenReadySeconds = 600`. Two players online, A ready: share 1 of 2 gives 300 s = 6000 ticks.
    // The countdown starts on call 1, call n has elapsed 50 * (n - 1) ms, so 300000 ms ends on call 6001.
    assert_eq!(ReadyOptions::default().timer_secs, 600);
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    let online = [A, B];
    let ends: Vec<u32> = (1..=6001u32).filter(|_| timer.tick(&online, true)).collect();
    assert_eq!(ends, vec![6001]);
}

#[test]
fn quarter_ready_gives_three_quarters_of_the_countdown() {
    // ReadyTimerState.kt:46-50: (1 - 1/4) * 600000 = 450000 ms = 9000 ticks, ends on call 9001.
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    let online = [A, B, C, D];
    let ends: Vec<u32> = (1..=9001u32).filter(|_| timer.tick(&online, true)).collect();
    assert_eq!(ends, vec![9001]);
}

#[test]
fn everyone_ready_ends_the_countdown_on_the_next_tick() {
    // All ready: duration 0. Call 1 starts it, call 2 has elapsed 50 ms, remaining -50 <= 0 (ReadyController.kt:88-89).
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    timer.set_ready(B, true);
    let online = [A, B];
    assert!(!timer.tick(&online, true));
    assert!(timer.tick(&online, true));
}

#[test]
fn a_lone_player_never_starts_the_countdown() {
    // ReadyService.kt:74 `players.size > 1`.
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    let ever_ended = (1..=20_000u32).any(|_| timer.tick(&[A], true));
    assert!(!ever_ended);
    assert!(!timer.update_packet(A, &[A], "lobby", horizon_bingo::game::GameStatus::Pregame).is_running);
}

#[test]
fn the_countdown_waits_until_every_player_is_on_a_team() {
    // ReadyService.kt:74-75 and BingoConfig.kt:52 `startWhenReadyWaitsForTeams = true`: while `eligible` is false the
    // timer does not run, even with two players and one ready.
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    for _ in 0..100 {
        timer.tick(&[A, B], false);
    }
    assert!(!timer.update_packet(A, &[A, B], "lobby", horizon_bingo::game::GameStatus::Pregame).is_running);
    timer.tick(&[A, B], true);
    assert!(timer.update_packet(A, &[A, B], "lobby", horizon_bingo::game::GameStatus::Pregame).is_running);
}

#[test]
fn losing_the_conditions_resets_the_countdown_and_readiness() {
    // ReadyService.kt:153-155 `reset()` when the condition fails; ReadyTimerState.reset() clears who is ready.
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    timer.tick(&[A, B], true);
    timer.tick(&[A, B], true);
    timer.tick(&[A, B], false);
    let packet = timer.update_packet(A, &[A, B], "lobby", horizon_bingo::game::GameStatus::Pregame);
    assert!(!packet.is_running);
    assert!(!packet.is_ready);
}

#[test]
fn cancel_stops_the_countdown_until_someone_readies_again() {
    // ReadyCommand.kt:58-71: `/ready cancel` resets while running and sets isCancelled; ReadyService.kt
    // `setReady` clears it through `startTimer`.
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    timer.set_ready(A, true);
    timer.tick(&[A, B], true);
    timer.cancel();
    for _ in 0..100 {
        timer.tick(&[A, B], true);
    }
    assert!(!timer.update_packet(A, &[A, B], "lobby", horizon_bingo::game::GameStatus::Pregame).is_running);
    timer.set_ready(B, true);
    timer.tick(&[A, B], true);
    assert!(timer.update_packet(A, &[A, B], "lobby", horizon_bingo::game::GameStatus::Pregame).is_running);
}

#[test]
fn countdown_starts_with_nobody_ready() {
    // ReadyService.kt `isFirstVoteSatisfied`: PREGAME `!config.startWhenReadyWaitsForFirstVote` is true by default
    // (BingoConfig.kt:54). With nobody ready the share is 0, so 600 s = 12000 ticks: ends on call 12001.
    let mut timer = ReadyTimer::new(ReadyOptions::default());
    let online = [A, B];
    let ends: Vec<u32> = (1..=12_001u32).filter(|_| timer.tick(&online, true)).collect();
    assert_eq!(ends, vec![12_001]);
}

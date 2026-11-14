//! Server teams: preset lookup, joining, spectating, leaving, member order and the shuffle. Expected values are counted
//! by hand. Shuffle outcomes for fixed seeds come from a separate SplitMix64 reference (checked against its published
//! first output for seed 0, 0xE220A8397B1DCDAF), not from the code under test.
use horizon_bingo::server::card_gen::Rng;
use horizon_bingo::server::teams::{preset, Teams, PRESETS};

const RED: &str = "bingo_red";
const BLUE: &str = "bingo_blue";
const GREEN: &str = "bingo_green";
const YELLOW: &str = "bingo_yellow";

#[test]
fn presets_are_found_with_or_without_prefix_in_any_case() {
    assert_eq!(preset("bingo_red").unwrap().name, "Red");
    assert_eq!(preset("red").unwrap().key, RED);
    assert_eq!(preset("BINGO_BLUE").unwrap().key, BLUE);
    assert_eq!(preset("Green").unwrap().key, GREEN);
    // The key, not the colour, is what is looked up: "pink" is bingo_pink, whose colour is LIGHT_PURPLE.
    assert_eq!(preset("pink").unwrap().key, "bingo_pink");
    assert_eq!(preset("light_purple"), None);
    assert_eq!(preset("pink").unwrap().color, "LIGHT_PURPLE");
    assert_eq!(preset("orange").unwrap().color, "GOLD");
    assert_eq!(preset("gray").unwrap().color, "GRAY");
}

#[test]
fn unknown_presets_are_none() {
    assert_eq!(preset("bingo_"), None);
    assert_eq!(preset("purple"), None);
    assert_eq!(preset(""), None);
    assert_eq!(preset("spectators"), None);
}

#[test]
fn join_puts_player_on_team_and_moving_takes_them_off_the_old_one() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    assert_eq!(teams.team_of(1), Some(RED));
    assert_eq!(teams.members("bingo_red"), vec![1]);

    teams.join(1, "BLUE").unwrap();
    assert_eq!(teams.team_of(1), Some(BLUE));
    assert!(teams.members(RED).is_empty());
    assert_eq!(teams.teams(), vec![BLUE]);
}

#[test]
fn joining_takes_a_player_out_of_spectators() {
    let mut teams = Teams::new();
    teams.spectate(1);
    teams.join(1, "bingo_red").unwrap();
    assert!(!teams.is_spectator(1));
    assert_eq!(teams.team_of(1), Some(RED));
}

#[test]
fn spectating_leaves_the_team() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    teams.spectate(1);
    assert!(teams.is_spectator(1));
    assert_eq!(teams.team_of(1), None);
    assert!(teams.members(RED).is_empty());
}

#[test]
fn unknown_team_is_an_error_and_changes_nothing() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    let err = teams.join(1, "purple").unwrap_err();
    assert!(err.contains("purple"), "error should name the team: {err}");
    assert_eq!(teams.team_of(1), Some(RED));

    // "spectators" is a command for spectate, not a team.
    assert!(teams.join(2, "spectators").is_err());
    assert_eq!(teams.team_of(2), None);
    assert!(!teams.is_spectator(2));
}

#[test]
fn leave_reports_whether_it_removed_anything() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    assert!(teams.leave(1));
    assert_eq!(teams.team_of(1), None);
    assert!(!teams.leave(1));
    assert!(!teams.leave(99));

    teams.spectate(2);
    assert!(teams.leave(2));
    assert!(!teams.is_spectator(2));
}

#[test]
fn rejoining_the_same_team_keeps_the_place_in_its_order() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    teams.join(2, "red").unwrap();
    teams.join(3, "red").unwrap();
    teams.join(1, "red").unwrap();
    assert_eq!(teams.members(RED), vec![1, 2, 3]);
}

#[test]
fn members_are_in_join_order_and_moved_players_go_last() {
    let mut teams = Teams::new();
    teams.join(30, "red").unwrap();
    teams.join(10, "red").unwrap();
    teams.join(20, "red").unwrap();
    assert_eq!(teams.members(RED), vec![30, 10, 20]);

    teams.join(30, "blue").unwrap();
    teams.join(30, "red").unwrap();
    assert_eq!(teams.members(RED), vec![10, 20, 30]);
}

#[test]
fn members_accepts_names_with_or_without_prefix_and_is_empty_for_unknown_team() {
    let mut teams = Teams::new();
    teams.join(1, "green").unwrap();
    assert_eq!(teams.members("bingo_green"), vec![1]);
    assert_eq!(teams.members("GREEN"), vec![1]);
    assert!(teams.members("purple").is_empty());
}

#[test]
fn teams_lists_only_non_empty_teams_in_preset_order() {
    let mut teams = Teams::new();
    assert!(teams.teams().is_empty());

    // Joined gray first, aqua second; aqua comes before gray in the presets.
    teams.join(1, "gray").unwrap();
    teams.join(2, "aqua").unwrap();
    assert_eq!(teams.teams(), vec!["bingo_aqua", "bingo_gray"]);

    teams.leave(2);
    assert_eq!(teams.teams(), vec!["bingo_gray"]);
}

#[test]
fn unassigned_lists_players_with_no_team_and_no_spectating_in_given_order() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    teams.spectate(2);
    assert_eq!(teams.unassigned(&[3, 1, 4, 2, 5]), vec![3, 4, 5]);
    assert_eq!(teams.unassigned(&[4, 3]), vec![4, 3]);
}

#[test]
fn clear_removes_teams_and_keeps_spectators() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    teams.spectate(2);
    teams.clear();
    assert_eq!(teams.team_of(1), None);
    assert!(teams.teams().is_empty());
    assert!(teams.is_spectator(2));
}

#[test]
fn shuffle_rejects_team_counts_outside_1_to_8_and_keeps_the_old_teams() {
    let mut teams = Teams::new();
    teams.join(1, "red").unwrap();
    assert!(teams.shuffle(&[1, 2], 0, &mut Rng::new(0)).is_err());
    assert!(teams.shuffle(&[1, 2], 9, &mut Rng::new(0)).is_err());
    assert_eq!(teams.team_of(1), Some(RED));
    assert_eq!(teams.team_of(2), None);
}

#[test]
fn one_player_into_three_teams_lands_on_the_first_preset() {
    // Hand-counted: one player is never swapped; round-robin deals them to index 0, bingo_red.
    let mut teams = Teams::new();
    teams.shuffle(&[7], 3, &mut Rng::new(12345)).unwrap();
    assert_eq!(teams.team_of(7), Some(RED));
    assert_eq!(teams.teams(), vec![RED]);
}

#[test]
fn shuffle_clears_the_previous_assignment() {
    let mut teams = Teams::new();
    teams.join(9, "blue").unwrap();
    teams.shuffle(&[1], 1, &mut Rng::new(0)).unwrap();
    assert_eq!(teams.team_of(9), None);
    assert_eq!(teams.team_of(1), Some(RED));
}

#[test]
fn shuffle_skips_spectators_and_counts_repeats_once() {
    let mut teams = Teams::new();
    teams.spectate(5);
    teams.shuffle(&[5, 1, 2, 1], 2, &mut Rng::new(0)).unwrap();
    assert!(teams.is_spectator(5));
    assert_eq!(teams.team_of(5), None);
    assert_eq!(teams.members(RED).len() + teams.members(BLUE).len(), 2);
}

#[test]
fn two_players_shuffle_to_known_teams_for_known_seeds() {
    // Seed 0 gives the order [1, 2]; seed 2 gives [2, 1]. Round-robin over 2 teams.
    let mut teams = Teams::new();
    teams.shuffle(&[1, 2], 2, &mut Rng::new(0)).unwrap();
    assert_eq!(teams.team_of(1), Some(RED));
    assert_eq!(teams.team_of(2), Some(BLUE));

    teams.shuffle(&[1, 2], 2, &mut Rng::new(2)).unwrap();
    assert_eq!(teams.team_of(2), Some(RED));
    assert_eq!(teams.team_of(1), Some(BLUE));
}

#[test]
fn four_players_shuffle_to_known_order_for_seed_0() {
    // Seed 0 gives the order [3, 2, 1, 4]. Two teams: red gets 3 then 1, blue gets 2 then 4.
    let mut teams = Teams::new();
    teams.shuffle(&[1, 2, 3, 4], 2, &mut Rng::new(0)).unwrap();
    assert_eq!(teams.members(RED), vec![3, 1]);
    assert_eq!(teams.members(BLUE), vec![2, 4]);

    // Four teams: 3 red, 2 blue, 1 green, 4 yellow.
    teams.shuffle(&[1, 2, 3, 4], 4, &mut Rng::new(0)).unwrap();
    assert_eq!(teams.team_of(3), Some(RED));
    assert_eq!(teams.team_of(2), Some(BLUE));
    assert_eq!(teams.team_of(1), Some(GREEN));
    assert_eq!(teams.team_of(4), Some(YELLOW));
}

#[test]
fn shuffle_is_deterministic_for_a_seed_and_different_seeds_can_differ() {
    let players: Vec<u128> = (1..=13).collect();
    let snapshot = |seed: u64| {
        let mut teams = Teams::new();
        teams.shuffle(&players, 8, &mut Rng::new(seed)).unwrap();
        PRESETS.iter().map(|p| teams.members(p.key)).collect::<Vec<_>>()
    };
    assert_eq!(snapshot(7), snapshot(7));
    // Seeds 0 and 1 deal four players differently (see the four-player test for seed 0).
    let four = |seed: u64| {
        let mut teams = Teams::new();
        teams.shuffle(&[1, 2, 3, 4], 4, &mut Rng::new(seed)).unwrap();
        (teams.team_of(1), teams.team_of(2), teams.team_of(3), teams.team_of(4))
    };
    assert_ne!(four(0), four(1));
}

#[test]
fn shuffle_deals_everyone_once_into_near_equal_teams() {
    let players: Vec<u128> = (100..113).collect(); // 13 players
    for n in 1..=8usize {
        for seed in 0..5u64 {
            let mut teams = Teams::new();
            teams.shuffle(&players, n, &mut Rng::new(seed)).unwrap();

            // Hand-counted sizes: 13 split into n teams gives floor(13 / n) or one more.
            let lo = 13 / n;
            let hi = lo + usize::from(13 % n != 0);
            let mut seen = Vec::new();
            for (i, preset) in PRESETS.iter().enumerate() {
                let members = teams.members(preset.key);
                if i < n {
                    assert!(
                        (lo..=hi).contains(&members.len()),
                        "n={n} seed={seed} team {} has {} players",
                        preset.key,
                        members.len()
                    );
                } else {
                    assert!(members.is_empty(), "n={n}: preset {} should be unused", preset.key);
                }
                seen.extend(members);
            }
            seen.sort_unstable();
            assert_eq!(seen, players, "n={n} seed={seed}: every player exactly once");
        }
    }
}

#[test]
fn shuffle_with_more_teams_than_players_leaves_extra_teams_out_of_teams() {
    let mut teams = Teams::new();
    teams.shuffle(&[1, 2], 5, &mut Rng::new(3)).unwrap();
    assert_eq!(teams.teams().len(), 2);
    assert!(teams.unassigned(&[1, 2]).is_empty());
}

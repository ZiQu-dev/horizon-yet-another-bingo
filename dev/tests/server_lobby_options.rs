//! Session side of `difficulty`, `reroll`, `teams` and `autoshuffle`: the counts of the card the game starts with, the
//! refusals once play begins, the saved options, and the teams the auto shuffle makes at the start. The reroll seed is
//! SplitMix64 from seed 7, the first output computed by hand with an independent implementation: 7191089600892374487.
use std::collections::HashMap;

use horizon_bingo::server::card_gen::{generate_card, load_pool};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use serde_json::Value;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;
const DAVE: u128 = 4;

/// First SplitMix64 output for seed 7 (the session's shuffle generator starts from the seed).
const REROLL_SEED: u64 = 7191089600892374487;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    files: HashMap<String, String>,
    commands: Vec<String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn command(&mut self, line: &str) {
        self.commands.push(line.to_string());
    }

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }
}

fn session(seed: u64) -> Session {
    Session::new(SessionOptions {
        seed,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ..SessionOptions::default()
    })
}

fn players(host: &mut FakeHost, session: &mut Session, list: &[(u128, &str)]) {
    host.online = list.iter().map(|&(uuid, name)| Player { uuid, name: name.to_string() }).collect();
    for player in host.online.clone() {
        session.on_join(host, &player);
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Two players on Red and Blue, the usual small game.
fn two_teams(host: &mut FakeHost, session: &mut Session) {
    players(host, session, &[(ALICE, "Alice"), (BOB, "Bob")]);
    assert!(run(session, host, Some(ALICE), "join red").starts_with("You joined"));
    assert!(run(session, host, Some(BOB), "join blue").starts_with("You joined"));
}

fn snapshot(session: &Session) -> Value {
    serde_json::from_str(&session.snapshot_json()).expect("the snapshot is JSON")
}

/// The card's squares as (item, tier name), in card order.
fn card_squares(session: &Session) -> Vec<(String, String)> {
    snapshot(session)["card"]
        .as_array()
        .expect("a playing game has a card")
        .iter()
        .map(|square| (square["item"].as_str().unwrap().to_string(), square["tier"].as_str().unwrap().to_string()))
        .collect()
}

/// How many squares the card has of S, A, B, C and D.
fn tier_counts(session: &Session) -> [usize; 5] {
    let mut counts = [0usize; 5];
    for (_, tier) in card_squares(session) {
        let index = ["S", "A", "B", "C", "D"].iter().position(|name| *name == tier).expect("a known tier");
        counts[index] += 1;
    }
    counts
}

#[test]
fn the_default_card_is_easy_and_the_options_text_does_not_name_it() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    let text = run(&mut s, &mut host, Some(ALICE), "options");
    assert!(!text.contains("Difficulty"), "{text}");
    assert_eq!(run(&mut s, &mut host, Some(ALICE), "start"), "Game started.");
    assert_eq!(tier_counts(&s), [0, 0, 0, 9, 16]);
}

#[test]
fn a_preset_sets_the_counts_the_game_starts_with() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    let reply = run(&mut s, &mut host, Some(ALICE), "difficulty impossible");
    assert!(reply.contains("Difficulty: impossible."), "{reply}");
    run(&mut s, &mut host, Some(ALICE), "start");
    assert_eq!(tier_counts(&s), [5, 7, 6, 5, 2]);
}

#[test]
fn custom_counts_are_named_in_the_reply_and_applied_at_the_start() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    let reply = run(&mut s, &mut host, Some(ALICE), "difficulty 10 0 0 5 10");
    assert!(reply.contains("Difficulty: custom S 10 A 0 B 0 C 5 D 10."), "{reply}");
    run(&mut s, &mut host, Some(ALICE), "start");
    assert_eq!(tier_counts(&s), [10, 0, 0, 5, 10]);
}

#[test]
fn difficulty_is_refused_once_the_game_has_started() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    run(&mut s, &mut host, Some(ALICE), "start");
    assert_eq!(
        run(&mut s, &mut host, Some(ALICE), "difficulty hard"),
        "The difficulty can be changed only before the game starts."
    );
    assert_eq!(tier_counts(&s), [0, 0, 0, 9, 16]);
}

#[test]
fn a_saved_difficulty_is_read_back_by_a_new_session() {
    let mut host = FakeHost::default();
    let mut first = session(7);
    two_teams(&mut host, &mut first);
    run(&mut first, &mut host, Some(ALICE), "difficulty medium");
    let saved = host.files.get("options.json").cloned().expect("options.json is written");
    assert!(saved.contains("\"tierCounts\""), "{saved}");

    let mut second = session(7);
    let text = run(&mut second, &mut host, Some(ALICE), "options");
    assert!(text.contains("Difficulty: medium."), "{text}");
}

#[test]
fn easy_is_the_default_so_choosing_it_writes_no_key() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    run(&mut s, &mut host, Some(ALICE), "difficulty easy");
    let saved = host.files.get("options.json").cloned().unwrap_or_default();
    assert!(!saved.contains("tierCounts"), "{saved}");
}

#[test]
fn reroll_draws_the_seed_from_the_shuffle_generator_and_the_card_follows_it() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    assert_eq!(run(&mut s, &mut host, Some(ALICE), "reroll"), "Card re-rolled.");
    assert_eq!(snapshot(&s)["seed"], Value::from(REROLL_SEED));

    run(&mut s, &mut host, Some(ALICE), "start");
    let expected: Vec<(String, String)> = generate_card(REROLL_SEED, &load_pool(), [0, 0, 0, 9, 16])
        .squares
        .into_iter()
        .map(|square| (square.item, square.tier.name().to_string()))
        .collect();
    assert_eq!(card_squares(&s), expected);
}

#[test]
fn a_reroll_gives_a_different_card_than_the_seed_alone() {
    let mut host = FakeHost::default();
    let mut plain = session(7);
    two_teams(&mut host, &mut plain);
    run(&mut plain, &mut host, Some(ALICE), "start");
    let plain_card = card_squares(&plain);

    let mut host2 = FakeHost::default();
    let mut rerolled = session(7);
    two_teams(&mut host2, &mut rerolled);
    run(&mut rerolled, &mut host2, Some(ALICE), "reroll");
    run(&mut rerolled, &mut host2, Some(ALICE), "start");
    assert_ne!(card_squares(&rerolled), plain_card);
}

#[test]
fn reroll_is_refused_once_the_game_has_started() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    run(&mut s, &mut host, Some(ALICE), "start");
    let before = snapshot(&s)["seed"].clone();
    assert_eq!(run(&mut s, &mut host, Some(ALICE), "reroll"), "The card can be rerolled only before the game starts.");
    assert_eq!(snapshot(&s)["seed"], before);
}

#[test]
fn team_count_is_named_only_with_the_auto_shuffle() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    two_teams(&mut host, &mut s);
    let reply = run(&mut s, &mut host, Some(ALICE), "teams 2");
    assert!(!reply.contains("Auto shuffle"), "{reply}");
    let reply = run(&mut s, &mut host, Some(ALICE), "autoshuffle on");
    assert!(reply.contains("Auto shuffle: on, 2 teams."), "{reply}");
}

#[test]
fn the_auto_shuffle_deals_every_player_into_the_chosen_teams_at_the_start() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    players(&mut host, &mut s, &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol"), (DAVE, "Dave")]);
    run(&mut s, &mut host, Some(ALICE), "teams 2");
    run(&mut s, &mut host, Some(ALICE), "autoshuffle on");
    assert_eq!(run(&mut s, &mut host, Some(ALICE), "start"), "Game started.");

    let adds: Vec<&String> = host.commands.iter().filter(|c| c.starts_with("team add ")).collect();
    assert_eq!(adds, [&"team add bingo_red".to_string(), &"team add bingo_blue".to_string()]);
    let joins: Vec<&String> = host.commands.iter().filter(|c| c.starts_with("team join ")).collect();
    assert_eq!(joins.len(), 4, "{joins:?}");
    let red = joins.iter().filter(|c| c.starts_with("team join bingo_red ")).count();
    let blue = joins.iter().filter(|c| c.starts_with("team join bingo_blue ")).count();
    assert_eq!((red, blue), (2, 2), "{joins:?}");
}

#[test]
fn without_the_auto_shuffle_players_on_no_team_still_stop_the_start() {
    let mut host = FakeHost::default();
    let mut s = session(7);
    players(&mut host, &mut s, &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol"), (DAVE, "Dave")]);
    run(&mut s, &mut host, Some(ALICE), "teams 2");
    let refused = run(&mut s, &mut host, Some(ALICE), "start");
    assert!(refused.starts_with("4 player(s) are not on a team."), "{refused}");
}

//! A difficulty chosen on the lobby menu's Items page sets the tier mix of the next card the round deals. Each test
//! clicks the page's preset slot, starts the round and counts the squares of each tier on the card the snapshot holds.
//! The expected counts are the original's `TierLabel.DIFFICULTY_PRESETS`, written out here (not read from the code).
use std::collections::HashMap;

use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::menu::{self, Outcome, Page, View, ROUND_STARTED};
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use serde_json::Value;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const SEED: u64 = 7;

/// The Items page's preset slots, each with its preset's name and the squares per tier S, A, B, C, D.
const PRESETS: [(usize, &str, [usize; 5]); 5] = [
    (11, "easy", [0, 0, 0, 9, 16]),
    (12, "medium", [0, 0, 3, 10, 12]),
    (14, "hard", [0, 1, 5, 10, 9]),
    (15, "extreme", [0, 5, 7, 7, 6]),
    (16, "impossible", [5, 7, 6, 5, 2]),
];

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    files: HashMap<String, String>,
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

    fn command(&mut self, _line: &str) {}

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }
}

/// A lobby with Alice on red and Bob on blue, both online, as a round starts with.
fn lobby() -> (FakeHost, Session) {
    let mut host = FakeHost {
        online: vec![
            Player {
                uuid: ALICE,
                name: "Alice".into(),
            },
            Player {
                uuid: BOB,
                name: "Bob".into(),
            },
        ],
        ..FakeHost::default()
    };
    let mut session = Session::new(SessionOptions {
        seed: SEED,
        game: GameOptions {
            countdown_secs: 0,
            time_limit_secs: None,
        },
        ..SessionOptions::default()
    });
    for player in host.online.clone() {
        session.on_join(&mut host, &player);
    }
    assert!(session
        .command(&mut host, Some(ALICE), "join red")
        .starts_with("You joined"));
    assert!(session
        .command(&mut host, Some(BOB), "join blue")
        .starts_with("You joined"));
    (host, session)
}

/// Clicks `slot` of the Items page as an operator, and runs what the click names.
fn click_items(host: &mut FakeHost, session: &mut Session, slot: usize) -> Outcome {
    let rules = session.rules().clone();
    let view = View {
        rules: &rules,
        pregame: session.is_pregame(),
        operator: true,
    };
    let outcome = menu::click(&Page::Items, slot, &view);
    if let Outcome::Run(line) = &outcome {
        session.command(host, Some(ALICE), line);
    }
    outcome
}

/// The squares per tier of the round's first card, S A B C D, counted from the snapshot.
fn tier_mix(session: &Session) -> [usize; 5] {
    let snapshot: Value =
        serde_json::from_str(&session.snapshot_json()).expect("the snapshot is JSON");
    let card = snapshot["card"]
        .as_array()
        .expect("a card once the round has started");
    let mut counts = [0usize; 5];
    for square in card {
        let index = ["S", "A", "B", "C", "D"]
            .iter()
            .position(|tier| Some(*tier) == square["tier"].as_str())
            .expect("a tier name");
        counts[index] += 1;
    }
    counts
}

#[test]
fn each_preset_on_the_items_page_gives_the_card_its_tier_counts() {
    for (slot, name, counts) in PRESETS {
        let (mut host, mut session) = lobby();
        let outcome = click_items(&mut host, &mut session, slot);
        assert_eq!(
            outcome,
            Outcome::Run(format!("difficulty {name}")),
            "slot {slot}"
        );
        assert_eq!(
            session.rules().tier_counts,
            counts,
            "{name} is stored on the round"
        );

        assert_eq!(session.command(&mut host, None, "start"), "Game started.");
        assert_eq!(
            tier_mix(&session),
            counts,
            "{name}: the card holds its preset's squares per tier"
        );
        assert_eq!(counts.iter().sum::<usize>(), 25);
    }
}

#[test]
fn a_difficulty_changed_from_the_items_page_is_the_one_the_next_card_uses() {
    // Easy first, then hard before the start: the card the start deals is hard, not easy.
    let (mut host, mut session) = lobby();
    assert!(matches!(
        click_items(&mut host, &mut session, 11),
        Outcome::Run(_)
    ));
    assert!(matches!(
        click_items(&mut host, &mut session, 14),
        Outcome::Run(_)
    ));
    assert_eq!(session.rules().tier_counts, [0, 1, 5, 10, 9]);

    // A reroll deals another card with the same difficulty.
    assert_eq!(
        session.command(&mut host, Some(ALICE), "reroll"),
        "Card re-rolled."
    );
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    assert_eq!(tier_mix(&session), [0, 1, 5, 10, 9]);
}

#[test]
fn a_difficulty_click_after_the_start_is_refused_and_the_card_keeps_its_tiers() {
    let (mut host, mut session) = lobby();
    assert!(matches!(
        click_items(&mut host, &mut session, 14),
        Outcome::Run(_)
    ));
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    assert_eq!(tier_mix(&session), [0, 1, 5, 10, 9]);

    // The menu refuses the click once the round has started, so no command runs and the rules keep the hard preset.
    let outcome = click_items(&mut host, &mut session, 15);
    assert_eq!(outcome, Outcome::Tell(ROUND_STARTED.to_string()));
    assert_eq!(session.rules().tier_counts, [0, 1, 5, 10, 9]);
    assert_eq!(tier_mix(&session), [0, 1, 5, 10, 9]);
}

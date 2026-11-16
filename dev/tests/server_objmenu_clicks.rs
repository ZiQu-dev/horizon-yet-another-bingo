//! The lobby menu driven like a player's chest: each test clicks slots of a page as `menu::click` gives them, runs the
//! command a click names through the session (as the adapter does) and checks what the options, the seed and the reply
//! are. Covers the Root page's reroll, team count and auto shuffle, the Features page's starting kit, and the operator
//! and round gates. Expected values are the ones the slots promise, and the seed after a reroll is the first SplitMix64
//! output of seed 7, computed independently (see `server_lobby_options.rs`).
use std::collections::HashMap;

use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::menu::{self, Outcome, Page, View, NOT_OPERATOR, ROUND_STARTED};
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts;
use horizon_bingo::server::types::Player;
use serde_json::Value;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const SEED: u64 = 7;
/// First SplitMix64 output for seed 7: the seed a reroll gives the card.
const REROLL_SEED: u64 = 7191089600892374487;

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

fn session() -> Session {
    Session::new(SessionOptions {
        seed: SEED,
        game: GameOptions {
            countdown_secs: 0,
            time_limit_secs: None,
        },
        ..SessionOptions::default()
    })
}

/// Players online and joined to the server, as the server reports them on join.
fn join_all(host: &mut FakeHost, session: &mut Session, list: &[(u128, &str)]) {
    host.online = list
        .iter()
        .map(|&(uuid, name)| Player {
            uuid,
            name: name.to_string(),
        })
        .collect();
    for player in host.online.clone() {
        session.on_join(host, &player);
    }
}

/// Alice on red and Bob on blue, both online: a round can start.
fn two_teams(host: &mut FakeHost, session: &mut Session) {
    join_all(host, session, &[(ALICE, "Alice"), (BOB, "Bob")]);
    assert!(session
        .command(host, Some(ALICE), "join red")
        .starts_with("You joined"));
    assert!(session
        .command(host, Some(BOB), "join blue")
        .starts_with("You joined"));
}

/// The shown items of a page for the session's rules, as the chest holds them.
fn shown(session: &Session, page: &Page, operator: bool) -> Vec<Option<menu::Slot>> {
    let rules = session.rules().clone();
    let view = View {
        rules: &rules,
        pregame: session.is_pregame(),
        operator,
    };
    menu::contents(page, &view)
}

/// A click on `slot` of `page` by a player (`operator` says whether they may run operator commands), as the adapter
/// handles it: the command the click names runs through the session. Returns what the click means and the reply.
fn click(
    session: &mut Session,
    host: &mut FakeHost,
    page: &Page,
    slot: usize,
    operator: bool,
) -> (Outcome, String) {
    let rules = session.rules().clone();
    let view = View {
        rules: &rules,
        pregame: session.is_pregame(),
        operator,
    };
    let outcome = menu::click(page, slot, &view);
    let reply = match &outcome {
        Outcome::Run(line) => session.command(host, Some(ALICE), line),
        _ => String::new(),
    };
    (outcome, reply)
}

/// The snapshot of the session as JSON (the saved game state, which holds the seed, the rules and the first card).
fn snapshot(session: &Session) -> Value {
    serde_json::from_str(&session.snapshot_json()).expect("the snapshot is JSON")
}

fn seed_of(session: &Session) -> u64 {
    snapshot(session)["seed"].as_u64().expect("a seed")
}

/// The options line of `bingo options`.
fn options_line(session: &mut Session, host: &mut FakeHost) -> String {
    session.command(host, Some(ALICE), "options")
}

#[test]
fn reroll_card_button_draws_a_new_seed_for_an_operator_before_the_start() {
    let mut host = FakeHost::default();
    let mut session = session();
    two_teams(&mut host, &mut session);
    assert_eq!(seed_of(&session), SEED);

    // A player who is not an operator is told so, and the seed stays.
    let (outcome, reply) = click(&mut session, &mut host, &Page::Root, 10, false);
    assert_eq!(outcome, Outcome::Tell(NOT_OPERATOR.to_string()));
    assert_eq!(reply, "");
    assert_eq!(seed_of(&session), SEED);

    // The operator's click runs `reroll`, which gives the card the next seed of the round's generator.
    let (outcome, reply) = click(&mut session, &mut host, &Page::Root, 10, true);
    assert_eq!(outcome, Outcome::Run("reroll".to_string()));
    assert_eq!(reply, "Card re-rolled.");
    assert_eq!(seed_of(&session), REROLL_SEED);

    // Once the round has started the button is refused and the seed stays.
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    let (outcome, _) = click(&mut session, &mut host, &Page::Root, 10, true);
    assert_eq!(outcome, Outcome::Tell(ROUND_STARTED.to_string()));
    assert_eq!(seed_of(&session), REROLL_SEED);
}

#[test]
fn a_card_rerolled_from_the_menu_is_the_card_of_the_new_seed() {
    let mut host = FakeHost::default();
    let mut rerolled = session();
    two_teams(&mut host, &mut rerolled);
    assert_eq!(
        click(&mut rerolled, &mut host, &Page::Root, 10, true).1,
        "Card re-rolled."
    );
    assert_eq!(rerolled.command(&mut host, None, "start"), "Game started.");

    // A session started on the rerolled seed directly deals the same card, and the unrolled one another.
    let mut direct = Session::new(SessionOptions {
        seed: REROLL_SEED,
        game: GameOptions {
            countdown_secs: 0,
            time_limit_secs: None,
        },
        ..SessionOptions::default()
    });
    let mut fresh_host = FakeHost::default();
    two_teams(&mut fresh_host, &mut direct);
    assert_eq!(
        direct.command(&mut fresh_host, None, "start"),
        "Game started."
    );

    let mut plain = session();
    let mut plain_host = FakeHost::default();
    two_teams(&mut plain_host, &mut plain);
    assert_eq!(
        plain.command(&mut plain_host, None, "start"),
        "Game started."
    );

    let card = snapshot(&rerolled)["card"].clone();
    assert_eq!(card, snapshot(&direct)["card"]);
    assert_ne!(card, snapshot(&plain)["card"]);
    assert_eq!(card.as_array().map(Vec::len), Some(25));
}

#[test]
fn team_count_buttons_step_the_count_within_one_and_eight() {
    let mut host = FakeHost::default();
    let mut session = session();
    join_all(&mut host, &mut session, &[(ALICE, "Alice")]);
    assert_eq!(session.rules().team_count, 3);
    assert_eq!(
        shown(&session, &Page::Root, true)[15]
            .as_ref()
            .unwrap()
            .name,
        "Teams: 3"
    );

    // More teams (slot 16) runs `teams 4`, and the count the slot shows follows.
    let (outcome, reply) = click(&mut session, &mut host, &Page::Root, 16, true);
    assert_eq!(outcome, Outcome::Run("teams 4".to_string()));
    assert!(
        reply.starts_with("Goal: "),
        "the options line answers: {reply}"
    );
    assert_eq!(session.rules().team_count, 4);
    assert_eq!(
        shown(&session, &Page::Root, true)[15]
            .as_ref()
            .unwrap()
            .name,
        "Teams: 4"
    );

    // Fewer teams (slot 14) runs `teams 3`, and twice more reaches one team, where the button only shows.
    assert_eq!(
        click(&mut session, &mut host, &Page::Root, 14, true).0,
        Outcome::Run("teams 3".to_string())
    );
    click(&mut session, &mut host, &Page::Root, 14, true);
    click(&mut session, &mut host, &Page::Root, 14, true);
    assert_eq!(session.rules().team_count, 1);
    let (outcome, reply) = click(&mut session, &mut host, &Page::Root, 14, true);
    assert_eq!(outcome, Outcome::Nothing);
    assert_eq!(reply, "");
    assert_eq!(
        shown(&session, &Page::Root, true)[14]
            .as_ref()
            .unwrap()
            .lore,
        vec!["At least one team.".to_string()]
    );

    // Up to eight teams, the most the team presets have, and then the button only shows.
    for _ in 0..7 {
        click(&mut session, &mut host, &Page::Root, 16, true);
    }
    assert_eq!(session.rules().team_count, 8);
    assert_eq!(
        click(&mut session, &mut host, &Page::Root, 16, true).0,
        Outcome::Nothing
    );
    assert_eq!(session.rules().team_count, 8);
    assert_eq!(
        shown(&session, &Page::Root, true)[16]
            .as_ref()
            .unwrap()
            .lore,
        vec!["At most 8 teams.".to_string()]
    );

    // A player who is not an operator is refused and the count stays.
    let (outcome, _) = click(&mut session, &mut host, &Page::Root, 14, false);
    assert_eq!(outcome, Outcome::Tell(NOT_OPERATOR.to_string()));
    assert_eq!(session.rules().team_count, 8);
}

#[test]
fn auto_shuffle_button_switches_the_teams_and_the_options_line_names_the_count() {
    let mut host = FakeHost::default();
    let mut session = session();
    join_all(&mut host, &mut session, &[(ALICE, "Alice")]);
    assert!(!session.rules().team_auto_shuffle);
    assert_eq!(
        shown(&session, &Page::Root, true)[12]
            .as_ref()
            .unwrap()
            .name,
        "Auto shuffle: off"
    );

    let (outcome, reply) = click(&mut session, &mut host, &Page::Root, 12, true);
    assert_eq!(outcome, Outcome::Run("autoshuffle on".to_string()));
    assert!(reply.ends_with(" Auto shuffle: on, 3 teams."), "{reply}");
    assert!(session.rules().team_auto_shuffle);
    assert_eq!(
        shown(&session, &Page::Root, true)[12]
            .as_ref()
            .unwrap()
            .name,
        "Auto shuffle: on"
    );

    click(&mut session, &mut host, &Page::Root, 16, true);
    let line = options_line(&mut session, &mut host);
    assert!(line.ends_with(" Auto shuffle: on, 4 teams."), "{line}");

    let (_, reply) = click(&mut session, &mut host, &Page::Root, 12, true);
    assert!(!reply.contains("Auto shuffle"), "{reply}");
    assert!(!session.rules().team_auto_shuffle);
}

#[test]
fn auto_shuffle_at_the_start_deals_the_players_into_the_counted_teams() {
    let mut host = FakeHost::default();
    let mut session = session();
    let players: Vec<(u128, String)> = (1..=8u128).map(|i| (i, format!("P{i}"))).collect();
    let names: Vec<(u128, &str)> = players
        .iter()
        .map(|(uuid, name)| (*uuid, name.as_str()))
        .collect();
    join_all(&mut host, &mut session, &names);

    click(&mut session, &mut host, &Page::Root, 12, true);
    for _ in 0..2 {
        click(&mut session, &mut host, &Page::Root, 16, true);
    }
    assert_eq!(session.rules().team_count, 5);

    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    let status = session.command(&mut host, None, "status");
    let team_lines = status
        .lines()
        .filter(|line| line.contains(" players, "))
        .count();
    assert_eq!(team_lines, 5, "the auto shuffle makes five teams: {status}");
}

#[test]
fn starting_kit_button_clears_the_kit_for_an_operator_and_is_refused_once_started() {
    let mut host = FakeHost::default();
    let mut session = session();
    two_teams(&mut host, &mut session);

    // Without a kit the slot shows the kit as off, and a click does nothing.
    assert_eq!(
        shown(&session, &Page::Features, true)[4]
            .as_ref()
            .unwrap()
            .name,
        "Starting kit: off"
    );
    assert_eq!(
        click(&mut session, &mut host, &Page::Features, 4, true).0,
        Outcome::Nothing
    );

    assert!(session
        .command(&mut host, Some(ALICE), "kit add diamond 2")
        .contains("diamond"));
    let on = shown(&session, &Page::Features, true)[4].clone().unwrap();
    assert_eq!(on.name, "Starting kit: on");
    assert!(
        on.lore
            .contains(&"1 kind(s) of item are given to team players at the start.".to_string()),
        "{:?}",
        on.lore
    );

    // A player who is not an operator is refused, and the kit stays.
    let (outcome, _) = click(&mut session, &mut host, &Page::Features, 4, false);
    assert_eq!(outcome, Outcome::Tell(NOT_OPERATOR.to_string()));
    assert!(!session.rules().kit.is_empty());

    // An operator's click runs `kit clear`, and the slot shows the kit off again.
    let (outcome, reply) = click(&mut session, &mut host, &Page::Features, 4, true);
    assert_eq!(outcome, Outcome::Run("kit clear".to_string()));
    assert_eq!(reply, texts::KIT_CLEARED);
    assert!(session.rules().kit.is_empty());
    assert_eq!(
        shown(&session, &Page::Features, true)[4]
            .as_ref()
            .unwrap()
            .name,
        "Starting kit: off"
    );

    // With a kit again and the round started, the click is refused and the kit stays.
    session.command(&mut host, Some(ALICE), "kit add diamond 2");
    assert_eq!(session.command(&mut host, None, "start"), "Game started.");
    let (outcome, _) = click(&mut session, &mut host, &Page::Features, 4, true);
    assert_eq!(outcome, Outcome::Tell(ROUND_STARTED.to_string()));
    assert!(!session.rules().kit.is_empty());
}

#[test]
fn the_rules_book_lists_the_objectives_setting_after_a_click_on_it() {
    let mut host = FakeHost::default();
    let mut session = session();
    join_all(&mut host, &mut session, &[(ALICE, "Alice")]);
    let book = shown(&session, &Page::Root, true)[13].clone().unwrap();
    assert_eq!(book.lore.last().unwrap(), "Objectives: off");
    assert!(session
        .command(&mut host, Some(ALICE), "objectives on")
        .ends_with(" Objectives: on."));
    let book = shown(&session, &Page::Root, true)[13].clone().unwrap();
    assert_eq!(book.lore.last().unwrap(), "Objectives: on");
    assert!(session.rules().objectives);
}

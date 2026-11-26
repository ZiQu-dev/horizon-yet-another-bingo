//! Chat lines of a round's scoring: the wait line of a team that reached the goal while the game goes on (aqua, and the
//! end rule for operators in gray), the line of a lost scored item (named by its team), and hidden items mode, which
//! keeps a find's and a loss's item name from the players on no team or on another team. The texts are the original's
//! English (`bingo.game.not_ending_complete*`, `bingo.game.message.lost_item`); the item names are the port's own.
use std::collections::{HashMap, HashSet};

use horizon_bingo::server::card_gen::Pool;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::hidden_consume::HiddenConsume;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::options::{EndWhen, Goal, Mode, Options};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts::Span;
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const SEED: u64 = 7;

const WAIT: &str = "You've finished the game! Wait for other teams to complete their cards...";

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    broadcasts: Vec<String>,
    /// Plain lines sent to one player, and the spans of the styled ones.
    tells: Vec<(u128, String)>,
    spans: Vec<(u128, Vec<Span>)>,
    operators: HashSet<u128>,
    files: HashMap<String, String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}

    fn tell(&mut self, player: u128, line: &str) {
        self.tells.push((player, line.to_string()));
    }

    fn tell_spans(&mut self, player: u128, spans: &[Span]) {
        self.spans.push((player, spans.to_vec()));
    }

    fn broadcast(&mut self, line: &str) {
        self.broadcasts.push(line.to_string());
    }

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

    fn stat_value(&self, _player: u128, _stat_type: &str, _stat_name: Option<&str>) -> Option<i64> {
        Some(0)
    }

    fn has_advancement(&self, _player: u128, _id: &str) -> bool {
        false
    }

    fn is_operator(&self, player: u128) -> bool {
        self.operators.contains(&player)
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(rules: Options) -> SessionOptions {
    SessionOptions {
        seed: SEED,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        rules,
        ..SessionOptions::default()
    }
}

/// Every square of the card is one of these 25 plain items, so any of them is on Alice's card.
fn card_pool() -> Pool {
    let keys: Vec<String> = (0..25).map(|i| format!("minecraft:plain_{i}")).collect();
    Pool {
        tiers: [vec![], vec![], vec![], keys[..9].to_vec(), keys[9..].to_vec()],
        weights: [vec![], vec![], vec![], vec![1.0; 9], vec![1.0; 16]],
        groups: Vec::new(),
        skipped_tags: Vec::new(),
        tag_files: 0,
    }
}

/// Alice on red and Bob on blue, the game started. What the start sent is cleared.
fn start(host: &mut FakeHost, rules: Options) -> Session {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(rules));
    session.set_pool(card_pool());
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(session.command(host, Some(ALICE), "join red").starts_with("Moved player"));
    assert!(session.command(host, Some(BOB), "join blue").starts_with("Moved player"));
    host.broadcasts.clear();
    host.tells.clear();
    host.spans.clear();
    // The lockout rounds here have two teams and a lines goal, which the original refuses without `ignore_warnings`.
    assert_eq!(session.command(host, None, "start ignore_warnings"), "Starting the game...");
    for uuid in [ALICE, BOB] {
        session.on_inventory(host, uuid, &HashMap::new());
    }
    host.broadcasts.clear();
    host.tells.clear();
    host.spans.clear();
    session
}

fn inventory(items: &[&str]) -> HashMap<String, u32> {
    items.iter().map(|item| (item.to_string(), 1)).collect()
}

/// The plain text of the styled lines a player was sent, with the colour of each.
fn styled(host: &FakeHost, player: u128) -> Vec<(String, Option<&'static str>)> {
    host.spans
        .iter()
        .filter(|(uuid, _)| *uuid == player)
        .map(|(_, spans)| {
            let text: String = spans.iter().map(Span::plain).collect();
            let color = match spans.first() {
                Some(Span::Translate { color, .. }) | Some(Span::Text { color, .. }) => *color,
                None => None,
            };
            (text, color)
        })
        .collect()
}

#[test]
fn a_team_at_the_goal_waits_for_the_others_and_only_operators_get_the_end_rule() {
    let mut host = FakeHost { operators: HashSet::from([ALICE]), ..FakeHost::default() };
    let rules = Options { goal: Goal::Items(1), end_when: EndWhen::AllWin, ..Options::default() };
    let mut session = start(&mut host, rules);

    session.on_inventory(&mut host, ALICE, &inventory(&["minecraft:plain_0"]));

    // The game goes on: no team reached-the-goal broadcast, and the wait line goes to Alice's team only.
    assert!(host.broadcasts.iter().all(|line| !line.contains("reached the goal")));
    let alice = styled(&host, ALICE);
    assert!(alice.contains(&(WAIT.to_string(), Some("aqua"))), "wait line for Alice, got {alice:?}");
    let operator = "(End Game is set to 'All Teams Win'. Use '/bingo end' if you're ready to end the game.)";
    assert!(alice.contains(&(operator.to_string(), Some("gray"))), "operator line for Alice, got {alice:?}");
    assert!(styled(&host, BOB).iter().all(|(text, _)| text != WAIT), "Bob is on the other team");
}

#[test]
fn a_lost_item_is_announced_under_its_team_name() {
    let mut host = FakeHost::default();
    let rules = Options { goal: Goal::Items(25), mode: Mode::Inventory, ..Options::default() };
    let mut session = start(&mut host, rules);

    session.on_inventory(&mut host, ALICE, &inventory(&["minecraft:plain_0"]));
    assert!(host.broadcasts.iter().any(|line| line.starts_with("Alice got [")), "the find is announced");
    host.broadcasts.clear();

    session.on_inventory(&mut host, ALICE, &HashMap::new());
    let lost: Vec<&String> = host.broadcasts.iter().filter(|line| line.contains(" lost [")).collect();
    assert_eq!(lost.len(), 1, "one loss line, got {:?}", host.broadcasts);
    assert!(lost[0].starts_with("Red lost ["), "the team is named, got {}", lost[0]);
    assert!(lost[0].ends_with(']'), "the item is in brackets, got {}", lost[0]);
}

#[test]
fn hidden_items_keep_the_item_name_from_players_on_another_team() {
    let mut host = FakeHost::default();
    let rules = Options {
        goal: Goal::Items(25),
        hidden_consume: HiddenConsume { hidden_items: true, ..HiddenConsume::default() },
        ..Options::default()
    };
    let mut session = start(&mut host, rules);

    session.on_inventory(&mut host, ALICE, &inventory(&["minecraft:plain_0"]));

    let alice: Vec<&str> = host.tells.iter().filter(|(p, _)| *p == ALICE).map(|(_, l)| l.as_str()).collect();
    let bob: Vec<&str> = host.tells.iter().filter(|(p, _)| *p == BOB).map(|(_, l)| l.as_str()).collect();
    assert!(alice.iter().any(|line| line.starts_with("Alice got [")), "Alice sees the item, got {alice:?}");
    assert!(bob.contains(&"Alice got *****"), "Bob sees the hidden item, got {bob:?}");
    assert!(bob.iter().all(|line| !line.contains("plain_0")), "Bob never sees the item name, got {bob:?}");
    assert!(host.broadcasts.iter().all(|line| !line.contains("plain_0")), "no broadcast names the item");
}

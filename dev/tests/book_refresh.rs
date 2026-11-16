//! The tutorial book after a settings change (`bingo prefs`) and at the lobby's give, with the held count read through
//! `ServerHost::command_result`. The command strings are written out by hand: vanilla's `clear <player> <item predicate>
//! [max]` with `minecraft:custom_data~` matching the book's tag, and a max count of 0 only counts.
use std::collections::HashMap;

use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::infobook;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;

const COUNT_ALICE: &str = "clear Alice minecraft:written_book[minecraft:custom_data~{bingo_info_book:1b}] 0";
const CLEAR_ALICE: &str = "clear Alice minecraft:written_book[minecraft:custom_data~{bingo_info_book:1b}]";

/// A host that answers `command_result` from a table keyed by the exact command line, and records what it was asked and
/// what it was given.
#[derive(Default)]
struct Host {
    online: Vec<Player>,
    /// The count each command line answers with. A line not here answers `None`, as the adapter does.
    counts: HashMap<String, i32>,
    asked: Vec<String>,
    commands: Vec<String>,
    files: HashMap<String, String>,
    gives: Vec<(u128, String, String)>,
}

impl ServerHost for Host {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, _item: &str) -> Option<i32> {
        Some(1)
    }

    fn command(&mut self, line: &str) {
        self.commands.push(line.to_string());
    }

    fn command_result(&mut self, line: &str) -> Option<i32> {
        self.asked.push(line.to_string());
        self.counts.get(line).copied()
    }

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }

    fn give_item(&mut self, player: u128, item: &str, components: &str) -> bool {
        if !self.online.iter().any(|p| p.uuid == player) {
            return false;
        }
        self.gives.push((player, item.to_string(), components.to_string()));
        true
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn host_with_alice_holding(count: i32) -> Host {
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    host.counts.insert(COUNT_ALICE.to_string(), count);
    host
}

fn run(session: &mut Session, host: &mut Host, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

#[test]
fn the_count_and_clear_commands_match_the_book_tag() {
    assert_eq!(infobook::count_command("Alice"), COUNT_ALICE);
    assert_eq!(infobook::clear_command("Alice"), CLEAR_ALICE);
}

#[test]
fn a_settings_change_replaces_the_held_book_with_one_for_the_new_settings() {
    let mut host = host_with_alice_holding(1);
    let mut session = Session::new(SessionOptions::default());
    // The bossbar defaults to off, so this turns it on.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs bossbar"), "Set 'Bossbar' = On");

    assert!(host.asked.contains(&COUNT_ALICE.to_string()), "the held count is read first");
    assert_eq!(host.commands, vec![CLEAR_ALICE.to_string()], "the held copies are cleared");
    assert_eq!(host.gives.len(), 1, "exactly one refreshed book is given");
    let (player, item, components) = &host.gives[0];
    assert_eq!((*player, item.as_str()), (ALICE, "minecraft:written_book"));
    // The Bossbar line now shows the on mark (U+2611, dark green) in the book's settings page.
    assert!(components.contains("{text:\"\u{2611}\",color:\"dark_green\"}"), "{components}");
    assert!(components.ends_with("minecraft:custom_data={bingo_info_book:1b,bingo_ignore:1b,bingo_vanish:1b,bingo_keep:1b}"));
}

#[test]
fn no_book_is_refreshed_for_a_player_who_holds_none() {
    let mut host = host_with_alice_holding(0);
    let mut session = Session::new(SessionOptions::default());
    run(&mut session, &mut host, Some(ALICE), "prefs bossbar");
    assert!(host.commands.is_empty(), "nothing cleared");
    assert!(host.gives.is_empty(), "no book given to a player who had none");
}

#[test]
fn without_a_count_the_held_book_is_left_alone() {
    // The adapter's answer: no count, so neither clear nor give.
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    let mut session = Session::new(SessionOptions::default());
    run(&mut session, &mut host, Some(ALICE), "prefs bossbar");
    assert_eq!(host.asked, vec![COUNT_ALICE.to_string()]);
    assert!(host.commands.is_empty());
    assert!(host.gives.is_empty());
}

#[test]
fn a_player_who_already_holds_the_lobby_book_is_not_given_a_second_one() {
    // After a restart the session has given nothing yet: Alice holds a book from before, Bob holds none.
    let mut host = host_with_alice_holding(1);
    host.online.push(player(BOB, "Bob"));
    let mut session = Session::new(SessionOptions::default());
    session.tick(&mut host);
    let given: Vec<u128> = host.gives.iter().map(|(player, _, _)| *player).collect();
    assert_eq!(given, vec![BOB], "only the player without a book is given one");
    assert!(host.asked.contains(&COUNT_ALICE.to_string()));

    for _ in 0..3 {
        session.tick(&mut host);
    }
    assert_eq!(host.gives.len(), 1, "no book on later ticks either");
}

#[test]
fn the_lobby_gives_a_book_to_a_player_with_a_zero_count() {
    let mut host = host_with_alice_holding(0);
    let mut session = Session::new(SessionOptions::default());
    session.tick(&mut host);
    assert_eq!(host.gives.len(), 1);
    assert_eq!(host.gives[0].0, ALICE);
}

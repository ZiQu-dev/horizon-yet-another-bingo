//! The tutorial book after a settings change (`bingo prefs`), at the lobby's give and at `bingo book`. The held book is
//! replaced in the player's inventory (`ServerHost::replace_held_tagged`), so a player keeps one book, as the original's
//! `InfoBookService.updateBookItem` and `giveBookItem` do. The fake host keeps a count of the books each player holds.
use std::collections::HashMap;

use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::infobook;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;

/// A host that records what it was asked and given. Held books are a count per player; the replace keeps one of them.
#[derive(Default)]
struct Host {
    online: Vec<Player>,
    held: HashMap<u128, usize>,
    /// The SNBT of each replacement, by player.
    replaced: Vec<(u128, String)>,
    commands: Vec<String>,
    files: HashMap<String, String>,
    gives: Vec<(u128, String, String)>,
    /// Whether the host can read and write the inventory (the adapter's answer); false answers `None` and refuses.
    inventory: bool,
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
        *self.held.entry(player).or_insert(0) += 1;
        true
    }

    fn held_tagged(&self, player: u128, tag: &str) -> Option<usize> {
        assert_eq!(tag, infobook::HELD_TAG);
        self.inventory.then(|| self.held.get(&player).copied().unwrap_or(0))
    }

    fn replace_held_tagged(&mut self, player: u128, tag: &str, snbt: &str) -> bool {
        assert_eq!(tag, infobook::HELD_TAG);
        let held = self.held.get(&player).copied().unwrap_or(0);
        if !self.inventory || held == 0 {
            return false;
        }
        self.replaced.push((player, snbt.to_string()));
        self.held.insert(player, 1);
        true
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn host_with(holding: usize) -> Host {
    let mut host = Host { online: vec![player(ALICE, "Alice")], inventory: true, ..Host::default() };
    host.held.insert(ALICE, holding);
    host
}

#[test]
fn the_book_item_is_found_by_its_tag() {
    assert_eq!(infobook::HELD_TAG, "bingo_info_book");
    assert_eq!(infobook::ITEM, "minecraft:written_book");
}

#[test]
fn a_settings_change_replaces_the_held_book_in_place() {
    let mut host = host_with(1);
    let mut session = Session::new(SessionOptions::default());
    // The bossbar defaults to off, so this turns it on.
    assert_eq!(session.command(&mut host, Some(ALICE), "prefs bossbar"), "Set 'Bossbar' = On");

    assert!(host.commands.is_empty(), "nothing is cleared: the held book is replaced");
    assert!(host.gives.is_empty(), "no second book is given");
    assert_eq!(host.replaced.len(), 1, "the held book is replaced once");
    let (player, snbt) = &host.replaced[0];
    assert_eq!(*player, ALICE);
    assert!(snbt.starts_with("{id:\"minecraft:written_book\",count:1,components:{"), "{snbt}");
    // The Bossbar line now shows the on mark (U+2611, dark green) in the book's settings page.
    assert!(snbt.contains("{text:\"\u{2611}\",color:\"dark_green\"}"), "{snbt}");
    assert!(snbt.ends_with("\"minecraft:custom_data\":{bingo_info_book:1b,bingo_ignore:1b,bingo_vanish:1b,bingo_keep:1b}}}"), "{snbt}");
}

#[test]
fn no_book_is_replaced_for_a_player_who_holds_none() {
    let mut host = host_with(0);
    let mut session = Session::new(SessionOptions::default());
    session.command(&mut host, Some(ALICE), "prefs bossbar");
    assert!(host.replaced.is_empty(), "nothing replaced");
    assert!(host.commands.is_empty());
    assert!(host.gives.is_empty(), "no book given to a player who had none");
}

#[test]
fn without_an_inventory_read_the_held_book_is_left_alone() {
    // The adapter's answer when the host cannot read or write the inventory: nothing is replaced or given.
    let mut host = host_with(1);
    host.inventory = false;
    let mut session = Session::new(SessionOptions::default());
    assert_eq!(session.command(&mut host, Some(ALICE), "prefs bossbar"), "Set 'Bossbar' = On");
    assert!(host.replaced.is_empty());
    assert!(host.commands.is_empty());
    assert!(host.gives.is_empty());
}

#[test]
fn a_player_who_already_holds_the_lobby_book_is_not_given_a_second_one() {
    // After a restart the session has given nothing yet: Alice holds a book from before, Bob holds none.
    let mut host = host_with(1);
    host.online.push(player(BOB, "Bob"));
    let mut session = Session::new(SessionOptions::default());
    session.tick(&mut host);
    let given: Vec<u128> = host.gives.iter().map(|(player, _, _)| *player).collect();
    assert_eq!(given, vec![BOB], "only the player without a book is given one");
    assert_eq!(host.replaced.len(), 1, "Alice's book is refreshed, not replaced by a new one");

    for _ in 0..3 {
        session.tick(&mut host);
    }
    assert_eq!(host.gives.len(), 1, "no book on later ticks either");
}

#[test]
fn the_lobby_gives_a_book_to_a_player_with_none() {
    let mut host = host_with(0);
    let mut session = Session::new(SessionOptions::default());
    session.tick(&mut host);
    assert_eq!(host.gives.len(), 1);
    assert_eq!(host.gives[0].0, ALICE);
}

#[test]
fn bingo_book_refreshes_a_held_book_instead_of_giving_another() {
    let mut host = host_with(1);
    host.online.push(player(BOB, "Bob"));
    host.files.insert("options.json".to_string(), "{\"lobbyTutorialBook\": false}".to_string());
    let mut session = Session::new(SessionOptions::default());
    assert_eq!(session.command(&mut host, Some(ALICE), "book"), "You got the bingo book.");
    assert!(host.gives.is_empty(), "Alice keeps her one book");
    assert_eq!(host.replaced.len(), 1);

    assert_eq!(session.command(&mut host, Some(BOB), "book"), "You got the bingo book.");
    assert_eq!(host.gives.len(), 1, "Bob, with none, is given one");
    assert_eq!(host.gives[0].0, BOB);
}

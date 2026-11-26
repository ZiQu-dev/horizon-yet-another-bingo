//! The lobby's tutorial book is given again to a player who lost it (InfoBookController.onGameTick: every tick in the lobby,
//! each player without a book gets one). A player who still holds it is not given another.
use std::collections::HashMap;

use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::infobook;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;

#[derive(Default)]
struct Host {
    online: Vec<Player>,
    held: HashMap<u128, usize>,
    gives: Vec<u128>,
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

    fn give_item(&mut self, player: u128, _item: &str, _components: &str) -> bool {
        self.gives.push(player);
        *self.held.entry(player).or_insert(0) += 1;
        true
    }

    fn held_tagged(&self, player: u128, tag: &str) -> Option<usize> {
        assert_eq!(tag, infobook::HELD_TAG);
        Some(self.held.get(&player).copied().unwrap_or(0))
    }

    fn replace_held_tagged(&mut self, _player: u128, _tag: &str, _snbt: &str) -> bool {
        true
    }
}

#[test]
fn a_player_who_lost_the_lobby_book_is_given_a_new_one() {
    let mut host = Host { online: vec![Player { uuid: ALICE, name: "Alice".to_string() }], ..Host::default() };
    let mut session = Session::new(SessionOptions::default());

    session.tick(&mut host);
    assert_eq!(host.gives, vec![ALICE], "the first book is given in the lobby");

    session.tick(&mut host);
    assert_eq!(host.gives.len(), 1, "a player who holds the book is not given another");

    // Alice drops or loses the book: the next lobby tick gives her a new one.
    host.held.insert(ALICE, 0);
    session.tick(&mut host);
    assert_eq!(host.gives, vec![ALICE, ALICE], "the lost book is given back on the next tick");
}

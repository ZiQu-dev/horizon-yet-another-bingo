//! `bingo card`, the original's `BingoCardCommand` through `CardViewService.openCard`: the open-card packet goes to the
//! player who runs it, with the key of that player's team. A player on no team gets the packet with no team, which is
//! the client's lobby preview card; the original sends it to them too.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::open_card::CardOpenPacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;

#[derive(Default)]
struct Host {
    online: Vec<Player>,
    sent: Vec<(u128, String, Vec<u8>)>,
    files: HashMap<String, String>,
}

impl ServerHost for Host {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push((player, path.to_string(), payload));
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn boss_bar(&mut self, _player: u128, _id: &str, _title: &str, _progress: f32, _visible: bool) {}

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }
}

fn session() -> Session {
    Session::new(SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    })
}

/// The open-card packets sent to a player, decoded.
fn open_cards(host: &Host, player: u128) -> Vec<Option<String>> {
    host.sent
        .iter()
        .filter(|(to, path, _)| *to == player && path == "open_card")
        .map(|(_, _, payload)| decode(payload, CardOpenPacket::read).expect("open_card decodes").team_key)
        .collect()
}

#[test]
fn a_player_on_no_team_gets_the_open_card_packet_without_a_team() {
    let mut host = Host { online: vec![Player { uuid: ALICE, name: "Alice".into() }], ..Host::default() };
    let mut session = session();
    session.on_join(&mut host, &Player { uuid: ALICE, name: "Alice".into() });
    host.sent.clear();

    session.command(&mut host, Some(ALICE), "card");

    assert_eq!(open_cards(&host, ALICE), vec![None]);
}

#[test]
fn a_player_on_a_team_gets_the_open_card_packet_with_the_team_key() {
    let mut host = Host {
        online: vec![Player { uuid: ALICE, name: "Alice".into() }, Player { uuid: BOB, name: "Bob".into() }],
        ..Host::default()
    };
    let mut session = session();
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    let answer = session.command(&mut host, Some(BOB), "join blue");
    assert!(answer.starts_with("Moved player"), "{answer}");
    host.sent.clear();

    session.command(&mut host, Some(BOB), "card");

    assert_eq!(open_cards(&host, BOB), vec![Some("bingo_blue".to_string())]);
}

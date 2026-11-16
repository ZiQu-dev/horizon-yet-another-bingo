//! A player's saved settings survive a rejoin. The client sends its settings once its channels are registered, which is
//! on every join, and it sends the defaults: it keeps no copy of its own. The original's `PlayerSettingsService` keeps the
//! saved settings, and the client gets them from the server; the server does not take the client's packet over them.
//! Expected values are the Kotlin defaults of `PlayerSettings` and the chat lines of `BingoPrefsCommand`, written out here.
use std::collections::HashMap;

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_settings::PlayerSettings;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const SETTINGS_FILE: &str = "player-settings.json";

/// A host that records the packets sent to players and keeps the data files, as a restart reads them back.
#[derive(Default)]
struct Host {
    online: Vec<Player>,
    /// Each packet sent: the player, the channel path and the payload.
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

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
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

/// The settings saved in the host's file, by player.
fn saved(host: &Host) -> HashMap<u128, PlayerSettings> {
    store::settings_from_json(&host.files[SETTINGS_FILE])
}

/// The packet a client sends on join: its settings, which are the defaults for a client that keeps none.
fn join_packet(settings: &PlayerSettings) -> Vec<u8> {
    encode(|w| settings.write(w))
}

/// The settings of the last `update_settings` packet sent to a player.
fn last_settings_sent(host: &Host, uuid: u128) -> PlayerSettings {
    let (_, _, payload) = host
        .sent
        .iter()
        .rev()
        .find(|(to, path, _)| *to == uuid && path == "update_settings")
        .expect("an update_settings packet is sent to the player");
    decode(payload, PlayerSettings::read).expect("update_settings decodes")
}

/// Alice's settings after `prefs bossbar on`, `prefs scoreboard off` and `prefs night_vision off`, from the defaults:
/// the tutorial counts as seen after a change.
fn alice_changed() -> PlayerSettings {
    PlayerSettings {
        seen_tutorial: true,
        hide_lobby_prompt: false,
        bossbar: true,
        scoreboard: false,
        scoreboard_auto_hide: false,
        leading_messages: true,
        score_messages: true,
        item_messages: true,
        night_vision: false,
    }
}

/// The Kotlin defaults, written out.
fn kotlin_defaults() -> PlayerSettings {
    PlayerSettings {
        seen_tutorial: false,
        hide_lobby_prompt: false,
        bossbar: false,
        scoreboard: true,
        scoreboard_auto_hide: false,
        leading_messages: true,
        score_messages: true,
        item_messages: true,
        night_vision: true,
    }
}

/// Alice changes her settings, then leaves the server.
fn alice_changes_then_leaves(session: &mut Session, host: &mut Host) {
    session.on_join(host, &player(ALICE, "Alice"));
    session.command(host, Some(ALICE), "prefs bossbar on");
    session.command(host, Some(ALICE), "prefs scoreboard off");
    session.command(host, Some(ALICE), "prefs night_vision off");
    session.on_leave(host, ALICE);
}

#[test]
fn a_rejoin_keeps_the_saved_settings_when_the_client_sends_its_defaults() {
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    let mut session = session();
    alice_changes_then_leaves(&mut session, &mut host);
    assert_eq!(saved(&host)[&ALICE], alice_changed());

    // Alice's client joins again and sends its defaults.
    host.sent.clear();
    session.on_join(&mut host, &player(ALICE, "Alice"));
    session.on_packet(&mut host, ALICE, "update_settings", &join_packet(&kotlin_defaults()));

    assert_eq!(saved(&host)[&ALICE], alice_changed(), "the saved file keeps Alice's settings");
    assert_eq!(last_settings_sent(&host, ALICE), alice_changed(), "the client gets its saved settings back");
}

#[test]
fn a_restarted_server_answers_the_join_with_the_saved_settings() {
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    alice_changes_then_leaves(&mut session(), &mut host);

    // A new session on the same files, as after a restart.
    let mut restarted = session();
    host.sent.clear();
    restarted.on_join(&mut host, &player(ALICE, "Alice"));
    restarted.on_packet(&mut host, ALICE, "update_settings", &join_packet(&kotlin_defaults()));

    assert_eq!(last_settings_sent(&host, ALICE), alice_changed());
    assert_eq!(saved(&host)[&ALICE], alice_changed());
}

#[test]
fn the_saved_settings_are_not_overwritten_by_a_settings_packet_with_other_values() {
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    let mut session = session();
    alice_changes_then_leaves(&mut session, &mut host);
    let before = host.files[SETTINGS_FILE].clone();

    // A client that sends the opposite of everything Alice saved does not change the file.
    let opposite = PlayerSettings {
        seen_tutorial: false,
        hide_lobby_prompt: true,
        bossbar: false,
        scoreboard: true,
        scoreboard_auto_hide: true,
        leading_messages: false,
        score_messages: false,
        item_messages: false,
        night_vision: true,
    };
    session.on_packet(&mut host, ALICE, "update_settings", &join_packet(&opposite));
    assert_eq!(host.files[SETTINGS_FILE], before, "the file is not written again");
    assert_eq!(last_settings_sent(&host, ALICE), alice_changed());
}

#[test]
fn a_player_with_no_saved_settings_gets_the_defaults_and_they_are_saved_for_them() {
    let mut host = Host { online: vec![player(BOB, "Bob")], ..Host::default() };
    let mut session = session();
    // Bob's client sends a setting on, which the server does not take: it has none saved, so it gets the defaults.
    let client_values = PlayerSettings { bossbar: true, ..kotlin_defaults() };
    session.on_join(&mut host, &player(BOB, "Bob"));
    session.on_packet(&mut host, BOB, "update_settings", &join_packet(&client_values));

    assert_eq!(last_settings_sent(&host, BOB), kotlin_defaults());
    assert_eq!(saved(&host)[&BOB], kotlin_defaults());
    // A second join keeps what was saved, even if the client now sends something else.
    session.on_leave(&mut host, BOB);
    session.command(&mut host, Some(BOB), "prefs bossbar on");
    session.on_leave(&mut host, BOB);
    session.on_join(&mut host, &player(BOB, "Bob"));
    session.on_packet(&mut host, BOB, "update_settings", &join_packet(&kotlin_defaults()));
    assert!(saved(&host)[&BOB].bossbar, "Bob's change is kept");
    assert!(last_settings_sent(&host, BOB).bossbar);
}

#[test]
fn the_join_packet_still_gives_the_card_hud_and_leaving_takes_it_away() {
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    let mut session = session();
    alice_changes_then_leaves(&mut session, &mut host);

    let mut restarted = self::session();
    restarted.on_join(&mut host, &player(ALICE, "Alice"));
    assert!(!restarted.has_card_hud(ALICE), "no client packet yet: no HUD");
    restarted.on_packet(&mut host, ALICE, "update_settings", &join_packet(&kotlin_defaults()));
    assert!(restarted.has_card_hud(ALICE), "the client's settings packet means its channels are registered");
    restarted.on_leave(&mut host, ALICE);
    assert!(!restarted.has_card_hud(ALICE));
}

#[test]
fn a_packet_that_does_not_decode_is_not_answered_and_gives_no_hud() {
    let mut host = Host { online: vec![player(ALICE, "Alice")], ..Host::default() };
    let mut session = session();
    alice_changes_then_leaves(&mut session, &mut host);
    host.sent.clear();

    // A string that is not the settings' JSON: the JSON object is missing its closing brace.
    let broken = encode(|w| w.write_string("{\"bossbar\": true"));
    session.on_join(&mut host, &player(ALICE, "Alice"));
    session.on_packet(&mut host, ALICE, "update_settings", &broken);
    assert!(!session.has_card_hud(ALICE));
    assert!(!host.sent.iter().any(|(_, path, _)| path == "update_settings"), "nothing is answered");
}

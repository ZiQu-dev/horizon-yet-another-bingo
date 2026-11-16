//! `bingo prefs <setting> [on|off]`, the original's `BingoPrefsCommand`, and the settings it keeps (`PlayerSettings`,
//! `PlayerSettingsService`). The chat lines are the original's `bingo.options.notify_changed` text, `Set '%s' = %s`, with
//! the English names of `bingo.player_settings.*`; the defaults are the Kotlin constructor's. The click commands of the
//! book's settings page are read back out of the page's text components and parsed, so each one must set the value its
//! line shows the opposite of.
use std::collections::HashMap;

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::server::commands::{parse, requires_op, Command};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::infobook::{self, PageView, Part};
use horizon_bingo::server::prefs::Setting;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_settings::PlayerSettings;

const ALICE: u128 = 1;
const BOB: u128 = 2;

/// A host that records what the session sends, the boss bars and the files of the data folder.
#[derive(Default)]
struct Host {
    online: Vec<Player>,
    /// Each packet sent: the player, the channel path and the payload.
    sent: Vec<(u128, String, Vec<u8>)>,
    /// Each boss bar update: the player, the bar id, the title, the progress and whether it is visible.
    bars: Vec<(u128, String, String, f32, bool)>,
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

    /// The id of an item is the length of its name, so a test can predict it.
    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn boss_bar(&mut self, player: u128, id: &str, title: &str, progress: f32, visible: bool) {
        self.bars.push((player, id.to_string(), title.to_string(), progress, visible));
    }

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }
}

const SETTINGS_FILE: &str = "player-settings.json";

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

fn run(session: &mut Session, host: &mut Host, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// The settings saved in the host's file, by player.
fn saved(host: &Host) -> HashMap<u128, PlayerSettings> {
    store::settings_from_json(&host.files[SETTINGS_FILE])
}

/// The settings a settings packet carries.
fn settings_of(payload: &[u8]) -> PlayerSettings {
    decode(payload, PlayerSettings::read).expect("update_settings decodes")
}

// Parsing.

#[test]
fn each_setting_is_named_by_its_snake_case_word_and_takes_on_off_or_nothing() {
    let words = [
        ("bossbar", Setting::Bossbar),
        ("scoreboard", Setting::Scoreboard),
        ("scoreboard_auto_hide", Setting::ScoreboardAutoHide),
        ("leading_messages", Setting::LeadingMessages),
        ("score_messages", Setting::ScoreMessages),
        ("item_messages", Setting::ItemMessages),
        ("night_vision", Setting::NightVision),
        ("hide_lobby_prompt", Setting::HideLobbyPrompt),
    ];
    for (word, setting) in words {
        assert_eq!(parse(&format!("prefs {word} on")), Ok(Command::Prefs { setting, value: Some(true) }));
        assert_eq!(parse(&format!("prefs {word} false")), Ok(Command::Prefs { setting, value: Some(false) }));
        assert_eq!(parse(&format!("PREFS {} Off", word.to_uppercase())), Ok(Command::Prefs { setting, value: Some(false) }));
        if setting != Setting::HideLobbyPrompt {
            assert_eq!(parse(&format!("prefs {word}")), Ok(Command::Prefs { setting, value: None }));
        }
    }
}

#[test]
fn the_lobby_prompt_flag_takes_a_value_only_as_the_original_does() {
    assert!(parse("prefs hide_lobby_prompt").is_err());
    assert!(parse("prefs hide_lobby_prompt on").is_ok());
}

#[test]
fn bad_prefs_lines_are_refused_with_the_usage() {
    assert_eq!(parse("prefs"), Err("Usage: prefs <setting> [on|off]".to_string()));
    assert!(parse("prefs nosuch").unwrap_err().starts_with("Unknown setting 'nosuch'."));
    assert!(parse("prefs bossbar maybe").unwrap_err().contains("needs on or off, got 'maybe'"));
    assert!(parse("prefs bossbar on now").unwrap_err().starts_with("Too many arguments for 'prefs'."));
}

#[test]
fn prefs_is_for_every_player() {
    assert!(!requires_op(&Command::Prefs { setting: Setting::Bossbar, value: None }));
}

// The command.

#[test]
fn a_toggle_flips_the_setting_and_says_what_it_is_set_to() {
    let mut host = Host::default();
    let mut session = session();
    // The defaults: the bossbar is off, the scoreboard, the messages and the night vision are on.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs bossbar"), "Set 'Bossbar' = On");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs bossbar"), "Set 'Bossbar' = Off");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs scoreboard"), "Set 'Scoreboard' = Off");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs scoreboard_auto_hide"), "Set 'Scoreboard Auto Hide' = On");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs leading_messages"), "Set 'Leading Team' = Off");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs score_messages off"), "Set 'Line Scored' = Off");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs item_messages on"), "Set 'Item Scored' = On");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs night_vision false"), "Set 'Night Vision' = Off");
}

#[test]
fn the_console_cannot_change_a_player_setting() {
    let mut host = Host::default();
    let mut session = session();
    assert_eq!(run(&mut session, &mut host, None, "prefs bossbar on"), "Only a player can change the player settings.");
    assert!(!host.files.contains_key(SETTINGS_FILE));
    assert!(host.sent.is_empty());
}

#[test]
fn a_change_is_saved_for_that_player_only_and_marks_the_tutorial_seen() {
    let mut host = Host::default();
    let mut session = session();
    run(&mut session, &mut host, Some(ALICE), "prefs bossbar on");
    let settings = saved(&host);
    assert!(settings[&ALICE].bossbar);
    assert!(settings[&ALICE].seen_tutorial, "a change counts as having seen the tutorial");
    assert!(!settings.contains_key(&BOB), "Bob's settings are not written");
}

#[test]
fn a_saved_change_is_read_back_by_a_new_session() {
    let mut host = Host::default();
    run(&mut session(), &mut host, Some(ALICE), "prefs scoreboard off");
    run(&mut session(), &mut host, Some(ALICE), "prefs bossbar on");

    // A new session on the same files, as after a restart: Bossbar is on, so the toggle turns it off.
    let mut restarted = session();
    assert_eq!(run(&mut restarted, &mut host, Some(ALICE), "prefs bossbar"), "Set 'Bossbar' = Off");
    assert!(!saved(&host)[&ALICE].scoreboard);
}

#[test]
fn the_lobby_prompt_flag_is_saved_and_a_client_cannot_change_it() {
    let mut host = Host::default();
    let mut session = session();
    assert_eq!(
        run(&mut session, &mut host, Some(ALICE), "prefs hide_lobby_prompt on"),
        "Okay, you won't see this message again."
    );
    assert!(saved(&host)[&ALICE].hide_lobby_prompt);
    // A client that sends its settings with the flag off does not clear it: the server keeps its own.
    session.on_packet(&mut host, ALICE, "update_settings", &encode(|w| PlayerSettings::default().write(w)));
    assert!(saved(&host)[&ALICE].hide_lobby_prompt);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs hide_lobby_prompt off"), "");
    assert!(!saved(&host)[&ALICE].hide_lobby_prompt);
}

#[test]
fn a_change_goes_back_to_the_client_as_its_settings() {
    let mut host = Host::default();
    let mut session = session();
    run(&mut session, &mut host, Some(ALICE), "prefs bossbar on");
    let (player, path, payload) = host.sent.last().expect("the change is sent to the client").clone();
    assert_eq!((player, path.as_str()), (ALICE, "update_settings"));
    let sent = settings_of(&payload);
    assert!(sent.bossbar && sent.seen_tutorial);
    assert_eq!(sent.scoreboard, PlayerSettings::default().scoreboard, "the other settings are sent as they are");
}

// The settings page: each line runs the command that sets the opposite value.

/// The commands the book's settings page runs, read out of its text components, in the order of the page.
fn click_commands(page: &Part) -> Vec<String> {
    let snbt = page.snbt();
    snbt.split("command:\"")
        .skip(1)
        .map(|rest| rest[..rest.find('"').expect("the command ends")].replace("\\\"", "\""))
        .collect()
}

#[test]
fn every_line_of_the_settings_page_runs_the_command_that_sets_its_opposite() {
    let defaults = PlayerSettings::default();
    let page = &infobook::pages(&PageView { settings: &defaults, operator: false, hud: false })[1];
    let commands = click_commands(page);
    assert_eq!(
        commands,
        vec![
            "/bingo prefs bossbar true",
            "/bingo prefs scoreboard false",
            "/bingo prefs scoreboard_auto_hide true",
            "/bingo prefs leading_messages false",
            "/bingo prefs score_messages false",
            "/bingo prefs item_messages false",
            "/bingo prefs night_vision false",
        ]
    );
    // Each command, typed without its slash and `bingo`, parses to the setting with the value its line offers.
    for command in &commands {
        let line = command.strip_prefix("/bingo ").expect("the command runs bingo");
        let Ok(Command::Prefs { setting, value: Some(value) }) = parse(line) else {
            panic!("{command} is not a prefs command with a value");
        };
        assert_ne!(setting.get(&defaults), value, "{command} changes the setting");
    }
}

#[test]
fn the_auto_hide_line_is_clickable_only_without_the_card_hud() {
    let defaults = PlayerSettings::default();
    let with_hud = &infobook::pages(&PageView { settings: &defaults, operator: false, hud: true })[1];
    let commands = click_commands(with_hud);
    assert!(!commands.iter().any(|command| command.contains("scoreboard_auto_hide")));
    assert_eq!(commands.len(), 6);
}

#[test]
fn the_click_command_of_a_line_sets_that_line_after_a_change() {
    // A line that is on offers "false"; a click runs it and the setting is off, and the next book's line offers "true".
    let mut host = Host::default();
    let mut session = session();
    let on = PlayerSettings { night_vision: true, ..PlayerSettings::default() };
    // The settings page is the first page once the tutorial has been seen, the second before.
    let page = |settings: &PlayerSettings| {
        let pages = infobook::pages(&PageView { settings, operator: false, hud: false });
        pages[if settings.seen_tutorial { 0 } else { 1 }].clone()
    };
    let before = click_commands(&page(&on));
    assert!(before.contains(&"/bingo prefs night_vision false".to_string()));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs night_vision false"), "Set 'Night Vision' = Off");
    let after = saved(&host)[&ALICE].clone();
    assert!(!after.night_vision);
    assert!(click_commands(&page(&after)).contains(&"/bingo prefs night_vision true".to_string()));
}

// The bossbar's effect: a bossbar turned off during play is hidden at once, and the team's other players keep theirs.

#[test]
fn turning_the_bossbar_off_during_play_hides_it_for_that_player_only() {
    let mut host = Host { online: vec![player(ALICE, "Alice"), player(BOB, "Bob")], ..Host::default() };
    let mut session = session();
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    run(&mut session, &mut host, Some(ALICE), "join red");
    run(&mut session, &mut host, Some(BOB), "join blue");
    run(&mut session, &mut host, Some(ALICE), "prefs bossbar on");
    run(&mut session, &mut host, None, "timelimit 1");
    assert!(run(&mut session, &mut host, Some(ALICE), "start").starts_with("Game started"));
    host.bars.clear();

    for _ in 0..10 {
        session.tick(&mut host);
    }
    assert!(host.bars.iter().any(|bar| bar.0 == ALICE && bar.4), "Alice has her bar");
    assert!(host.bars.iter().any(|bar| bar.0 == BOB && bar.4), "Bob has his bar");

    host.bars.clear();
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "prefs bossbar off"), "Set 'Bossbar' = Off");
    assert_eq!(host.bars, vec![(ALICE, "bingo:bingo_red".to_string(), String::new(), 0.0, false)]);

    host.bars.clear();
    for _ in 0..10 {
        session.tick(&mut host);
    }
    assert!(!host.bars.iter().any(|bar| bar.0 == ALICE), "Alice's bar is not shown again");
    assert!(host.bars.iter().any(|bar| bar.0 == BOB && bar.4), "Bob still has his bar");
}

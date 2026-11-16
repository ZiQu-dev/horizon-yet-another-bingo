//! The lobby's tutorial book (`infobook`) and `bingo book`. The expected English is written by hand from the original's
//! `en_us.json` (the `bingo.intro.*` and `bingo.player_settings.*` keys), so the pages are checked against the text a
//! player reads, not against the code that builds them.
use std::collections::{HashMap, HashSet};

use horizon_bingo::server::commands::{parse, requires_op, Command};
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::infobook::{self, Part, PageView};
use horizon_bingo::server::options::Options;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::store;
use horizon_bingo::server::types::Player;
use horizon_bingo::update_settings::PlayerSettings;

const ALICE: u128 = 1;
const BOB: u128 = 2;

const CHECK_ON: &str = "\u{2611}";
const CHECK_OFF: &str = "\u{2610}";
const INFO: &str = "\u{1F6C8}";

/// A host that records the gives, the commands and the files, and knows which players are operators.
#[derive(Default)]
struct Host {
    online: Vec<Player>,
    operators: HashSet<u128>,
    commands: Vec<String>,
    files: HashMap<String, String>,
    /// Each item given: the player, the item id and the components.
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

    fn read_file(&mut self, name: &str) -> Option<String> {
        self.files.get(name).cloned()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
    }

    fn is_operator(&self, player: u128) -> bool {
        self.operators.contains(&player)
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

/// The page texts a player reads, in order.
fn plain_pages(view: PageView) -> Vec<String> {
    infobook::pages(&view).iter().map(Part::plain).collect()
}

/// Whether the SNBT (or the command's components) has its brackets and quotes balanced, and no raw line break: what the
/// game's reader needs to take it at all.
fn well_formed(text: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                (false, '\n') => return false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            '\n' => return false,
            _ => {}
        }
    }
    depth == 0 && !in_string
}

// The pages' English.

#[test]
fn a_new_player_reads_the_welcome_page_first_then_the_settings_and_the_hints() {
    let settings = PlayerSettings::default();
    let pages = plain_pages(PageView { settings: &settings, operator: false });
    assert_eq!(pages.len(), 3);
    assert_eq!(
        pages[0],
        "Welcome to Yet Another Bingo!\nv2.15.1\n\nRead how to play on the wiki.\n\n\
         Change your player settings on the next page.\n\nHave fun!"
    );
    assert!(pages[1].starts_with("Player Settings\n\n"));
    assert_eq!(
        pages[2],
        "Hints\n\nWhen holding a Bingo Card, press [key.use] to see its items.\n\n\
         Use \"/coords\" to keep track of coordinates and share them with your team."
    );
}

#[test]
fn a_player_who_has_seen_the_tutorial_reads_the_settings_page_first() {
    let settings = PlayerSettings { seen_tutorial: true, ..PlayerSettings::default() };
    let pages = plain_pages(PageView { settings: &settings, operator: false });
    assert!(pages[0].starts_with("Player Settings\n\n"));
    assert!(pages[1].starts_with("Welcome to Yet Another Bingo!"));
    assert!(pages[2].starts_with("Hints\n\n"));
}

#[test]
fn an_operator_is_told_to_use_the_menu_instead_of_the_settings_line() {
    let settings = PlayerSettings::default();
    let pages = plain_pages(PageView { settings: &settings, operator: true });
    assert_eq!(
        pages[0],
        "Welcome to Yet Another Bingo!\nv2.15.1\n\nUse the menu in front of you to configure the game.\n\n\
         Read how to play on the wiki.\n\nHave fun!"
    );
}

#[test]
fn the_settings_page_marks_each_setting_on_or_off_with_its_english_name() {
    // Night vision, the scoreboard and the message kinds are on by default; the bossbar and the auto hide are off.
    let settings = PlayerSettings::default();
    let pages = plain_pages(PageView { settings: &settings, operator: false });
    let expected = format!(
        "Player Settings\n\n{CHECK_OFF} Bossbar {INFO}\n\n{CHECK_ON} Scoreboard {INFO}\n{CHECK_OFF} Scoreboard Auto Hide {INFO}\n\n\
         Floating Messages:\n{CHECK_ON} Leading Team {INFO}\n{CHECK_ON} Line Scored {INFO}\n{CHECK_ON} Item Scored {INFO}\n\n\
         {CHECK_ON} Night Vision {INFO}"
    );
    assert_eq!(pages[1], expected);
}

#[test]
fn a_switched_off_setting_shows_the_off_mark_and_its_description_on_hover() {
    let settings = PlayerSettings { bossbar: true, night_vision: false, ..PlayerSettings::default() };
    let pages = plain_pages(PageView { settings: &settings, operator: false });
    assert!(pages[1].contains(&format!("{CHECK_ON} Bossbar {INFO}")));
    assert!(pages[1].contains(&format!("{CHECK_OFF} Night Vision {INFO}")));
    let snbt = infobook::components("Alice", &infobook::pages(&PageView { settings: &settings, operator: false }), false);
    // The description of the night vision setting is a hover text of its information mark.
    assert!(snbt.contains("\"Receive night vision when the feature is enabled, spectating, or post-game.\""));
}

// The book item.

#[test]
fn the_book_is_titled_and_signed_by_the_player_with_the_bingo_tags() {
    let settings = PlayerSettings::default();
    let pages = infobook::pages(&PageView { settings: &settings, operator: false });
    let components = infobook::components("Alice", &pages, true);
    assert!(components.starts_with(
        "minecraft:written_book_content={title:\"Yet Another Bingo\",author:\"Alice\",pages:["
    ));
    assert!(components.ends_with(
        "minecraft:custom_data={bingo_info_book:1b,bingo_ignore:1b,bingo_vanish:1b,bingo_keep:1b}"
    ));
    assert_eq!(infobook::ITEM, "minecraft:written_book");
    assert!(well_formed(&components));
}

#[test]
fn the_book_given_by_bingo_book_has_no_keep_tag() {
    let settings = PlayerSettings::default();
    let pages = infobook::pages(&PageView { settings: &settings, operator: false });
    let components = infobook::components("Bob", &pages, false);
    assert!(components.ends_with("minecraft:custom_data={bingo_info_book:1b,bingo_ignore:1b,bingo_vanish:1b}"));
}

#[test]
fn the_pages_are_text_components_with_the_wiki_links_and_a_bold_title() {
    let settings = PlayerSettings::default();
    let pages = infobook::pages(&PageView { settings: &settings, operator: false });
    let welcome = pages[0].snbt();
    assert!(welcome.contains("translate:\"bingo.intro.welcome\""));
    assert!(welcome.contains("bold:1b"));
    assert!(welcome.contains("click_event:{action:\"open_url\",url:\"https://horrific.dev/bingo/\"}"));
    assert!(welcome.contains("click_event:{action:\"open_url\",url:\"https://horrific.dev/bingo/what-is-bingo/\"}"));
    assert!(well_formed(&welcome));
    for page in &pages {
        assert!(well_formed(&page.snbt()));
        assert!(!page.snbt().contains('\n'), "a line break is written as the escape, never raw");
    }
}

#[test]
fn a_quote_in_a_name_is_escaped_in_the_book() {
    let settings = PlayerSettings::default();
    let pages = infobook::pages(&PageView { settings: &settings, operator: false });
    let components = infobook::components("Al\"ice", &pages, false);
    assert!(components.contains("author:\"Al\\\"ice\""));
    assert!(well_formed(&components));
}

// Parsing and the operator rule.

#[test]
fn bingo_book_is_a_command_for_everyone() {
    assert_eq!(parse("book"), Ok(Command::Book));
    assert_eq!(parse("BOOK"), Ok(Command::Book));
    assert!(parse("book now").is_err());
    assert!(!requires_op(&Command::Book));
}

// The option.

#[test]
fn the_lobby_book_is_on_by_default_and_written_only_when_off() {
    assert!(Options::default().lobby_tutorial_book);
    assert!(!store::options_to_json(&Options::default()).contains("lobbyTutorialBook"));
    let off = Options { lobby_tutorial_book: false, ..Options::default() };
    assert!(store::options_to_json(&off).contains("\"lobbyTutorialBook\": false"));
    assert!(!store::options_from_json(&store::options_to_json(&off)).lobby_tutorial_book);
    assert!(store::options_from_json("{}").lobby_tutorial_book);
}

// The session: the lobby gives the book once, and `bingo book`.

fn session() -> Session {
    Session::new(SessionOptions::default())
}

fn lobby_host() -> Host {
    Host { online: vec![player(ALICE, "Alice"), player(BOB, "Bob")], ..Host::default() }
}

#[test]
fn each_player_in_the_lobby_is_given_one_book_once() {
    let mut host = lobby_host();
    let mut session = session();
    session.tick(&mut host);
    let given: Vec<u128> = host.gives.iter().map(|(player, _, _)| *player).collect();
    assert_eq!(given, vec![ALICE, BOB], "the first tick gives each player one book");
    assert!(host.gives.iter().all(|(_, item, _)| item == "minecraft:written_book"));
    assert!(host.gives.iter().all(|(_, _, components)| components.ends_with("bingo_keep:1b}")), "the lobby's book is kept");

    for _ in 0..5 {
        session.tick(&mut host);
    }
    assert_eq!(host.gives.len(), 2, "no second book on later ticks");
}

#[test]
fn the_lobby_book_is_written_for_the_operator_and_the_player_separately() {
    let mut host = lobby_host();
    host.operators.insert(ALICE);
    let mut session = session();
    session.tick(&mut host);
    let alice = &host.gives[0].2;
    let bob = &host.gives[1].2;
    assert!(alice.contains("Use the menu in front of you to configure the game."));
    assert!(!alice.contains("Change your player settings on the next page."));
    assert!(bob.contains("Change your player settings on the next page."));
    assert!(!bob.contains("Use the menu in front of you"));
}

#[test]
fn no_book_is_given_in_the_lobby_when_the_option_is_off() {
    let mut host = lobby_host();
    host.files.insert("options.json".to_string(), "{\"lobbyTutorialBook\": false}".to_string());
    let mut session = session();
    session.tick(&mut host);
    assert!(host.gives.is_empty());
}

#[test]
fn bingo_book_gives_a_player_the_book_when_the_lobby_does_not() {
    let mut host = lobby_host();
    host.files.insert("options.json".to_string(), "{\"lobbyTutorialBook\": false}".to_string());
    let mut session = session();
    let reply = session.command(&mut host, Some(ALICE), "book");
    assert_eq!(reply, "You got the bingo book.");
    assert_eq!(host.gives.len(), 1);
    assert_eq!(host.gives[0].0, ALICE);
    assert!(host.gives[0].2.ends_with("minecraft:custom_data={bingo_info_book:1b,bingo_ignore:1b,bingo_vanish:1b}"));
}

#[test]
fn bingo_book_is_refused_while_the_lobby_gives_the_book() {
    let mut host = lobby_host();
    let mut session = session();
    let reply = session.command(&mut host, Some(ALICE), "book");
    assert_eq!(reply, "bingo book is off: the lobby gives the tutorial book (lobby_tutorial_book is on).");
    assert!(host.gives.is_empty());
}

#[test]
fn bingo_book_needs_a_player_and_the_console_gets_no_book() {
    let mut host = lobby_host();
    host.files.insert("options.json".to_string(), "{\"lobbyTutorialBook\": false}".to_string());
    let mut session = session();
    assert_eq!(session.command(&mut host, None, "book"), "Only a player can get the book.");
    assert!(host.gives.is_empty());
}

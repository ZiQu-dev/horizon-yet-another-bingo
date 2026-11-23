//! The lobby menu (`bingo menu`): which slots each page shows, what a click means for operators, for other players and
//! once the round has started, the menus a player has open, and the item SNBT the adapter reads. Expected values are
//! written out by hand from the original's pages (`menu/MenuPage.kt`) and the port's commands.
use horizon_bingo::server::commands::parse;
use horizon_bingo::server::menu::{
    self, Action, Menus, Outcome, Page, Slot, View, BACK_SLOT, CLOSE_SLOT, NOT_OPERATOR,
    ROUND_STARTED, SLOTS,
};
use horizon_bingo::server::options::{EndWhen, Goal, Mode, Options};

fn view(rules: &Options, pregame: bool, operator: bool) -> View<'_> {
    View {
        rules,
        pregame,
        operator,
    }
}

fn slot_at(
    page: &Page,
    rules: &Options,
    pregame: bool,
    operator: bool,
    index: usize,
) -> Option<Slot> {
    menu::contents(page, &view(rules, pregame, operator))[index].clone()
}

fn indexes(page: &Page, rules: &Options) -> Vec<usize> {
    menu::slots(page, &view(rules, true, true))
        .into_iter()
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn every_page_has_its_slots_at_the_documented_places() {
    let rules = Options::default();
    assert_eq!(SLOTS, 36);
    assert_eq!(BACK_SLOT, 27);
    assert_eq!(CLOSE_SLOT, 35);
    assert_eq!(
        { let mut got = indexes(&Page::Root, &rules); got.sort(); got },
        vec![0, 1, 2, 3, 4, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 22, 24, 35]
    );
    assert_eq!(
        indexes(&Page::Features, &rules),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 27, 35]
    );
    assert_eq!(
        indexes(&Page::Goal, &rules),
        vec![
            0, 1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24, 25, 27,
            35
        ]
    );
    assert_eq!(indexes(&Page::Items, &rules), vec![13, 11, 12, 14, 15, 16, 27, 35]);
    assert_eq!(
        indexes(&Page::Warnings("Nobody to play.".to_string()), &rules),
        vec![13, 21, 27, 35]
    );
}

#[test]
fn root_shows_the_default_round_and_its_rules() {
    let rules = Options::default();
    let standard = slot_at(&Page::Root, &rules, true, true, 0).unwrap();
    assert_eq!(standard.name, "Standard");
    assert_eq!(
        standard.lore,
        vec![
            "An item found stays found.".to_string(),
            "Selected.".to_string()
        ]
    );
    assert_eq!(standard.action, Action::Run("mode standard".to_string()));
    let inventory = slot_at(&Page::Root, &rules, true, true, 1).unwrap();
    assert_eq!(inventory.lore[1], "Click to choose.");
    let hidden = slot_at(&Page::Root, &rules, true, true, 3).unwrap();
    assert_eq!(hidden.name, "Hidden items: off");
    assert_eq!(
        hidden.action,
        Action::Run("mode hidden_items on".to_string())
    );
    let rules_book = slot_at(&Page::Root, &rules, true, true, 13).unwrap();
    assert_eq!(rules_book.name, "Current rules");
    assert_eq!(rules_book.lore[0], "Goal: 1 line(s)");
    assert_eq!(rules_book.lore[1], "Mode: standard");
    assert_eq!(rules_book.lore[2], "End: first win");
    assert_eq!(rules_book.lore[3], "Play to: 1 card(s)");
    assert_eq!(rules_book.lore[5], "PvP: on");
    assert_eq!(rules_book.lore[9], "Spawn distance: 25 chunks");
    assert_eq!(rules_book.action, Action::None);
}

#[test]
fn root_reflects_changed_rules() {
    let mut rules = Options::default();
    rules.mode = Mode::Lockout;
    rules.hidden_consume.hidden_items = true;
    assert_eq!(
        slot_at(&Page::Root, &rules, true, true, 2).unwrap().lore[1],
        "Selected."
    );
    assert_eq!(
        slot_at(&Page::Root, &rules, true, true, 0).unwrap().lore[1],
        "Click to choose."
    );
    let hidden = slot_at(&Page::Root, &rules, true, true, 3).unwrap();
    assert_eq!(hidden.name, "Hidden items: on");
    assert_eq!(
        hidden.action,
        Action::Run("mode hidden_items off".to_string())
    );
}

#[test]
fn goal_page_marks_the_current_goal_end_win_and_stalemate() {
    let mut rules = Options::default();
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 0).unwrap().name,
        "1 line"
    );
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 0).unwrap().lore[1],
        "Selected."
    );
    // Default end is first win (slot 14), win one card (slot 18), stalemate end game (slot 23).
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 14).unwrap().lore[1],
        "Selected."
    );
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 18).unwrap().lore[1],
        "Selected."
    );
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 23).unwrap().lore[1],
        "Selected."
    );
    rules.goal = Goal::Items(10);
    rules.end_when = EndWhen::TeamsWin(2);
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 4).unwrap().lore[1],
        "Selected."
    );
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 16).unwrap().lore[1],
        "Selected."
    );
    assert_eq!(
        slot_at(&Page::Goal, &rules, true, true, 14).unwrap().lore[1],
        "Click to choose."
    );
}

#[test]
fn features_page_toggles_and_spawn_presets() {
    let mut rules = Options::default();
    let pvp = slot_at(&Page::Features, &rules, true, true, 0).unwrap();
    assert_eq!(pvp.name, "PvP: on");
    assert_eq!(pvp.action, Action::Run("pvp off".to_string()));
    rules.pvp = false;
    assert_eq!(
        slot_at(&Page::Features, &rules, true, true, 0)
            .unwrap()
            .action,
        Action::Run("pvp on".to_string())
    );
    let spawn = slot_at(&Page::Features, &rules, true, true, 6).unwrap();
    assert_eq!(spawn.name, "Spawn distance: 25 chunks");
    assert_eq!(spawn.lore[1], "Selected.");
    assert_eq!(
        slot_at(&Page::Features, &rules, true, true, 5)
            .unwrap()
            .action,
        Action::Run("spawndistance 10".to_string())
    );
}

#[test]
fn clicks_on_settings_run_their_command_for_an_operator_before_the_start() {
    let rules = Options::default();
    assert_eq!(
        menu::click(&Page::Root, 1, &view(&rules, true, true)),
        Outcome::Run("mode inventory".to_string())
    );
    assert_eq!(
        menu::click(&Page::Goal, 4, &view(&rules, true, true)),
        Outcome::Run("goal items 10".to_string())
    );
    assert_eq!(
        menu::click(&Page::Goal, 10, &view(&rules, true, true)),
        Outcome::Run("timelimit 15".to_string())
    );
    assert_eq!(
        menu::click(&Page::Goal, 16, &view(&rules, true, true)),
        Outcome::Run("endwhen teams_win 2".to_string())
    );
    assert_eq!(
        menu::click(&Page::Features, 5, &view(&rules, true, true)),
        Outcome::Run("spawndistance 10".to_string())
    );
    assert_eq!(
        menu::click(&Page::Root, 22, &view(&rules, true, true)),
        Outcome::Run("start".to_string())
    );
}

#[test]
fn navigation_and_close_do_not_depend_on_rights_or_the_round() {
    let rules = Options::default();
    for (operator, pregame) in [(false, false), (false, true), (true, false)] {
        let v = view(&rules, pregame, operator);
        assert_eq!(menu::click(&Page::Root, 6, &v), Outcome::Show(Page::Goal));
        assert_eq!(
            menu::click(&Page::Root, 7, &v),
            Outcome::Show(Page::Features)
        );
        assert_eq!(menu::click(&Page::Root, 8, &v), Outcome::Show(Page::Items));
        assert_eq!(
            menu::click(&Page::Goal, BACK_SLOT, &v),
            Outcome::Show(Page::Root)
        );
        assert_eq!(menu::click(&Page::Features, CLOSE_SLOT, &v), Outcome::Close);
        assert_eq!(
            menu::click(&Page::Warnings("x".to_string()), BACK_SLOT, &v),
            Outcome::Show(Page::Root)
        );
    }
}

#[test]
fn a_player_who_is_not_an_operator_sees_the_values_but_cannot_change_them() {
    let rules = Options::default();
    let read = view(&rules, true, false);
    assert_eq!(
        menu::click(&Page::Root, 0, &read),
        Outcome::Tell(NOT_OPERATOR.to_string())
    );
    assert_eq!(
        menu::click(&Page::Root, 22, &read),
        Outcome::Tell(NOT_OPERATOR.to_string())
    );
    assert_eq!(
        menu::click(&Page::Goal, 10, &read),
        Outcome::Tell(NOT_OPERATOR.to_string())
    );
    let standard = slot_at(&Page::Root, &rules, true, false, 0).unwrap();
    assert_eq!(
        standard.lore.last().unwrap(),
        "Only operators can change this."
    );
    assert_eq!(standard.name, "Standard");
    assert_eq!(
        slot_at(&Page::Root, &rules, true, false, 13).unwrap().lore[0],
        "Goal: 1 line(s)"
    );
}

#[test]
fn once_the_round_has_started_settings_are_read_only_for_everyone() {
    let rules = Options::default();
    let op = view(&rules, false, true);
    assert_eq!(
        menu::click(&Page::Root, 1, &op),
        Outcome::Tell(ROUND_STARTED.to_string())
    );
    assert_eq!(
        menu::click(&Page::Features, 0, &op),
        Outcome::Tell(ROUND_STARTED.to_string())
    );
    assert_eq!(
        menu::click(&Page::Root, 22, &op),
        Outcome::Tell(ROUND_STARTED.to_string())
    );
    // Navigation still works, so the rules can be read.
    assert_eq!(menu::click(&Page::Root, 6, &op), Outcome::Show(Page::Goal));
    let lore = slot_at(&Page::Root, &rules, false, true, 0).unwrap().lore;
    assert_eq!(
        lore.last().unwrap(),
        "The round has started, so it is read only."
    );
}

#[test]
fn empty_slots_and_info_slots_do_nothing() {
    let rules = Options::default();
    let v = view(&rules, true, true);
    assert_eq!(menu::click(&Page::Root, 5, &v), Outcome::Nothing);
    assert_eq!(menu::click(&Page::Root, 30, &v), Outcome::Nothing);
    assert_eq!(menu::click(&Page::Root, 13, &v), Outcome::Nothing);
    assert_eq!(menu::click(&Page::Items, 13, &v), Outcome::Nothing);
}

#[test]
fn items_page_offers_the_five_difficulty_presets() {
    // The presets of the difficulty command: slots 11, 12, 14, 15, 16 (13 is the card's info slot).
    let rules = Options::default();
    assert_eq!(indexes(&Page::Items, &rules), vec![13, 11, 12, 14, 15, 16, 27, 35]);
    assert_eq!(
        slot_at(&Page::Items, &rules, true, true, 13).unwrap().name,
        "Card difficulty"
    );
    let v = view(&rules, true, true);
    assert_eq!(menu::click(&Page::Items, 11, &v), Outcome::Run("difficulty easy".to_string()));
    assert_eq!(menu::click(&Page::Items, 14, &v), Outcome::Run("difficulty hard".to_string()));
    assert_eq!(menu::click(&Page::Items, 16, &v), Outcome::Run("difficulty impossible".to_string()));
    // The card's current preset says so; the others say they can be chosen.
    assert!(slot_at(&Page::Items, &rules, true, true, 11).unwrap().lore.contains(&"Selected.".to_string()));
    assert!(slot_at(&Page::Items, &rules, true, true, 12).unwrap().lore.contains(&"Click to choose.".to_string()));
}

#[test]
fn warnings_page_shows_the_refusal_and_starts_anyway_for_an_operator() {
    let rules = Options::default();
    let text = "Nobody to play.".to_string();
    let page = Page::Warnings(text.clone());
    let info = slot_at(&page, &rules, true, true, 13).unwrap();
    assert_eq!(info.name, "Start refused");
    assert_eq!(info.lore, vec!["Nobody to play.".to_string()]);
    assert_eq!(
        menu::click(&page, 21, &view(&rules, true, true)),
        Outcome::Run("start ignore_warnings".to_string())
    );
    assert_eq!(
        menu::click(&page, 21, &view(&rules, true, false)),
        Outcome::Tell(NOT_OPERATOR.to_string())
    );
    assert_eq!(
        menu::click(&page, 21, &view(&rules, false, true)),
        Outcome::Tell(ROUND_STARTED.to_string())
    );
}

#[test]
fn a_refused_start_is_a_start_line_with_another_reply() {
    assert!(!menu::start_refused("start", "Starting the game..."));
    assert!(menu::start_refused("start", "Nobody to play."));
    assert!(menu::start_refused(
        "start ignore_warnings",
        "2 player(s) are not on a team."
    ));
    assert!(!menu::start_refused("goal lines 1", "Goal: 1 line(s)."));
    // The success text is the one the session gives for a start.
    assert_eq!(menu::GAME_STARTED, "Starting the game...");
}

#[test]
fn wrap_breaks_text_at_forty_characters_on_word_boundaries() {
    assert_eq!(
        menu::wrap("Nobody to play."),
        vec!["Nobody to play.".to_string()]
    );
    assert_eq!(
        menu::wrap("alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu"),
        vec![
            "alpha beta gamma delta epsilon zeta eta".to_string(),
            "theta iota kappa lambda mu".to_string()
        ]
    );
    assert!(menu::wrap("   ").is_empty());
}

#[test]
fn every_click_line_of_every_page_parses_as_a_command() {
    let rules = Options::default();
    let pages = [
        Page::Root,
        Page::Features,
        Page::Goal,
        Page::Items,
        Page::Warnings("x".to_string()),
    ];
    for page in &pages {
        for (index, slot) in menu::slots(page, &view(&rules, true, true)) {
            if let Action::Run(line) = slot.action {
                assert!(
                    parse(&line).is_ok(),
                    "slot {index} of {page:?}: '{line}' does not parse"
                );
            }
        }
    }
}

#[test]
fn item_snbt_writes_id_name_and_lore_with_quotes_escaped() {
    let rules = Options::default();
    let standard = slot_at(&Page::Root, &rules, true, true, 0).unwrap();
    assert_eq!(
        menu::item_snbt(&standard),
        "{id:\"minecraft:paper\",count:1,components:{\"minecraft:custom_name\":\"Standard\",\"minecraft:lore\":[\"An item found stays found.\",\"Selected.\"]}}"
    );
    let close = slot_at(&Page::Features, &rules, true, true, CLOSE_SLOT).unwrap();
    assert_eq!(
        menu::item_snbt(&close),
        "{id:\"minecraft:barrier\",count:1,components:{\"minecraft:custom_name\":\"Close\"}}"
    );
    let tricky = Slot {
        item: "minecraft:paper",
        name: "say \"hi\" \\".to_string(),
        lore: Vec::new(),
        action: Action::None,
    };
    assert_eq!(menu::item_snbt(&tricky), "{id:\"minecraft:paper\",count:1,components:{\"minecraft:custom_name\":\"say \\\"hi\\\" \\\\\"}}");
}

#[test]
fn menus_track_one_menu_per_window_and_forget_it_on_close() {
    let mut menus = Menus::new();
    assert_eq!(menus.page(7, 3), None);
    menus.open(7, 3, Page::Root);
    assert_eq!(menus.page(7, 3), Some(Page::Root));
    assert_eq!(menus.page(7, 4), None, "another window is not the menu");
    assert!(menus.turn(7, 3, Page::Goal));
    assert_eq!(menus.page(7, 3), Some(Page::Goal));
    assert!(
        !menus.turn(7, 4, Page::Root),
        "a turn in another window changes nothing"
    );
    assert_eq!(menus.page(7, 3), Some(Page::Goal));
    assert!(
        !menus.close(7, 4),
        "closing another window is not this menu"
    );
    assert!(menus.close(7, 3));
    assert_eq!(menus.page(7, 3), None);
}

#[test]
fn a_new_menu_replaces_the_players_old_one() {
    let mut menus = Menus::new();
    menus.open(9, 1, Page::Root);
    menus.open(9, 2, Page::Features);
    assert_eq!(menus.page(9, 1), None);
    assert_eq!(menus.page(9, 2), Some(Page::Features));
    menus.clear();
    assert_eq!(menus.page(9, 2), None);
}

#[test]
fn a_warnings_page_carries_its_text_in_the_page_value() {
    assert_ne!(
        Page::Warnings("a".to_string()),
        Page::Warnings("b".to_string())
    );
}

#[test]
fn menu_is_a_command_open_to_every_player_and_listed_in_usage() {
    use horizon_bingo::server::commands::{requires_op, usage, Command};
    assert_eq!(parse("menu"), Ok(Command::Menu));
    assert_eq!(parse("MENU"), Ok(Command::Menu));
    assert!(parse("menu now").is_err());
    assert!(!requires_op(&Command::Menu));
    assert!(usage().contains("| menu"), "usage lacks menu: {}", usage());
}

struct QuietHost;

impl horizon_bingo::server::host::ServerHost for QuietHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}
    fn tell(&mut self, _player: u128, _line: &str) {}
    fn broadcast(&mut self, _line: &str) {}
    fn players(&self) -> Vec<horizon_bingo::server::types::Player> {
        Vec::new()
    }
    fn item_numeric_id(&self, _item: &str) -> Option<i32> {
        None
    }
}

#[test]
fn the_console_is_told_the_menu_is_for_a_player_and_the_rules_are_readable() {
    use horizon_bingo::server::commands::Command;
    use horizon_bingo::server::session::{Session, SessionOptions};
    let mut session = Session::new(SessionOptions::default());
    let mut host = QuietHost;
    assert_eq!(
        session.run(&mut host, None, Command::Menu),
        "The menu opens in a player's game chat: type bingo menu in game."
    );
    assert_eq!(session.rules(), &Options::default());
    assert!(session.is_pregame());
}

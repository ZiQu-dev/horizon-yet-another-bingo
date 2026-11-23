//! Every translation key the server's chat lines use has an English entry in the mod's `en_us.json`, the text a client
//! shows when it has no translation. The lines are built from `texts`' own constructors, so a new line is checked as
//! soon as it is added to the list below.
use horizon_bingo::server::texts::{self, Line};
use serde_json::{Map, Value};

fn every_line() -> Vec<Line> {
    vec![
        texts::item_captured("Ada", "copper_torch", "item.minecraft.copper_torch".to_owned()),
        texts::objective_captured("Ada", "Own a diamond"),
        texts::scored_line("Red", "2 lines"),
        texts::leading_team("Red", "3 items", false, false),
        texts::leading_team("Red", "3 items", false, true),
        texts::leading_team("Red", "3 items", true, false),
        texts::leading_team("Red", "3 items", true, true),
        texts::start_success(),
        texts::end_success(),
        texts::reset_success(),
        texts::shuffle_success(),
        texts::ready_cancelled(),
        texts::ready_true("Ada"),
        texts::ready_false("Ada"),
        texts::joined_team("Ada", "Red"),
        texts::team_reached_goal("Red"),
        texts::game_continues(),
        texts::stalemate_no_team_can_win(),
        texts::stalemate_lockout(),
        texts::stalemate_impossible_goal(),
        texts::stopped_reason("Ada"),
        texts::game_over_reason(),
        texts::already_starting(),
        texts::lobby_not_open(),
        texts::start_failed(true),
        texts::start_failed(false),
        texts::game_duration("1:02"),
        texts::players_ready(2, 5),
        texts::ready_up_command("/bingo ready"),
        texts::starting_in("0:10"),
        texts::waiting_to_start(),
        texts::started_subtitle(),
        texts::countdown_number(3),
        texts::click_to_join(),
        texts::lobby_menu_hint(),
        texts::lobby_menu_start(),
        texts::lobby_panel_title(),
        texts::lobby_panel_players(),
        texts::lobby_panel_teams(),
        texts::lobby_panel_settings(),
        texts::lobby_panel_reroll(),
        texts::lobby_panel_auto_shuffle(true),
        texts::lobby_panel_auto_shuffle(false),
        texts::lobby_panel_team_count(1),
        texts::lobby_panel_team_count(4),
        texts::lobby_panel_minus(),
        texts::lobby_panel_plus(),
        texts::lobby_panel_shuffle(),
        texts::lobby_panel_reset_defaults(),
        texts::ready_not_on_team("/spectator"),
        texts::start_title(),
        texts::start_subtitle(),
        texts::start_banner(),
        texts::completed_card("Red"),
        texts::bingo_reason(),
        texts::out_of_time(),
        texts::stalemate_reason(),
        texts::lockout_reason(),
        texts::soft_locked_reason(),
    ]
}

#[test]
fn every_key_the_server_lines_use_has_an_english_entry() {
    let english: Map<String, Value> = serde_json::from_str(include_str!("../../bingo-native/assets/bingo/lang/en_us.json")).unwrap();
    let missing: Vec<&str> = every_line().iter().map(|line| line.key).filter(|key| !english.contains_key(*key)).collect();
    assert_eq!(missing, Vec::<&str>::new(), "keys without an English entry in en_us.json");
}

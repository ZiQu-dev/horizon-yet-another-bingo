//! The lobby's in-world parts: which clicked block means what (the menu signs, the team pads), the displays that label
//! them, the countdown and start titles, the sounds of the lobby, and `ready` for a player on no team. Expected values are
//! written out by hand from the original's lobby POIs (`lobby_poi.json.gz`) and `CountdownService`.
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::lobby_scene::{self, Click, PanelState, Tile};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts::{self, Line};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    commands: Vec<String>,
    titles: Vec<(u128, String, String, Option<String>)>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}
    fn tell(&mut self, _player: u128, _line: &str) {}
    fn broadcast(&mut self, _line: &str) {}
    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }
    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }
    fn command(&mut self, line: &str) {
        self.commands.push(line.to_owned());
    }
    fn show_title(&mut self, player: u128, title: &Line, subtitle: &Line, color: Option<&str>) {
        self.titles.push((player, title.render(), subtitle.render(), color.map(str::to_owned)));
    }
}

fn session(countdown_secs: i32) -> (Session, FakeHost) {
    let mut session = Session::new(SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    });
    let mut host = FakeHost::default();
    host.online = vec![
        Player { uuid: ALICE, name: "Alice".to_owned() },
        Player { uuid: BOB, name: "Bob".to_owned() },
    ];
    for player in host.online.clone() {
        session.on_join(&mut host, &player);
    }
    (session, host)
}

#[test]
fn the_pad_block_and_the_carpet_above_it_join_that_pads_team_and_a_block_beside_does_not() {
    // The `team` POI of the blue pad is (16, 23, -9).
    let at = [16.5, 24.0, -8.5];
    assert_eq!(lobby_scene::click_at([16, 23, -9], at), Some(Click::Join("bingo_blue")));
    assert_eq!(lobby_scene::click_at([16, 24, -9], at), Some(Click::Join("bingo_blue")));
    assert_eq!(lobby_scene::click_at([17, 23, -9], at), None);
    assert_eq!(lobby_scene::click_at([16, 25, -9], at), None);
    assert_eq!(lobby_scene::click_at([-17, 24, 14], at), Some(Click::Join("bingo_red")));
}

#[test]
fn a_wall_click_on_the_start_tile_starts_and_on_the_reroll_tile_rerolls_whichever_wall_block_the_ray_ends_on() {
    // The ray ends on the wall's south face (z -4), whatever block of the wall that is.
    assert_eq!(lobby_scene::click_at([2, 18, -5], [2.0, 18.0, -4.0]), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([1, 19, -5], [1.5, 19.1, -4.0]), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([-1, 18, -5], [-0.9, 18.3, -4.0]), Some(Click::Reroll));
}

#[test]
fn each_new_left_column_tile_maps_to_its_own_click() {
    let at = |x: f64, y: f64| lobby_scene::click_at([0, 19, -5], [x, y, -4.0]);
    assert_eq!(at(-0.5, 20.1), Some(Click::AutoShuffle));
    assert_eq!(at(-1.6, 19.5), Some(Click::TeamsFewer));
    assert_eq!(at(0.5, 19.5), Some(Click::TeamsMore));
    assert_eq!(at(-0.5, 19.0), Some(Click::Shuffle));
    assert_eq!(at(-0.5, 17.3), Some(Click::ResetDefaults));
}

#[test]
fn wall_clicks_on_other_tiles_or_between_tiles_open_the_menu() {
    assert_eq!(lobby_scene::click_at([-2, 18, -5], [-1.5, 17.8, -4.0]), Some(Click::Menu), "settings");
    assert_eq!(lobby_scene::click_at([0, 19, -5], [-0.5, 19.5, -4.0]), Some(Click::Menu), "the team count");
    assert_eq!(lobby_scene::click_at([0, 20, -5], [0.5, 20.7, -4.0]), Some(Click::Menu), "title");
    assert_eq!(lobby_scene::click_at([0, 18, -5], [0.99, 18.0, -4.0]), Some(Click::Menu), "the gap between the columns");
    assert_eq!(lobby_scene::click_at([3, 20, -5], [3.5, 20.5, -4.0]), Some(Click::Menu), "off every tile");
}

#[test]
fn no_two_tiles_overlap_and_all_stand_on_the_wall_plane() {
    for (i, (a, ra)) in lobby_scene::TILES.iter().enumerate() {
        assert!(ra[0] >= -2.0 && ra[2] <= 3.0 && ra[1] >= 17.0 && ra[3] <= 21.0, "{a:?} is off the wall");
        for (b, rb) in &lobby_scene::TILES[i + 1..] {
            let apart = ra[2] <= rb[0] || rb[2] <= ra[0] || ra[3] <= rb[1] || rb[3] <= ra[1];
            assert!(apart, "{a:?} overlaps {b:?}");
        }
        // The middle of every tile maps back to it.
        assert_eq!(lobby_scene::tile_at((ra[0] + ra[2]) / 2.0, (ra[1] + ra[3]) / 2.0), Some(*a));
    }
}

#[test]
fn only_the_wall_behind_the_panel_is_clickable() {
    let on_start = [2.0, 18.0, -4.0];
    assert_eq!(lobby_scene::click_at([-2, 17, -5], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([3, 20, -5], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([4, 18, -5], on_start), None);
    assert_eq!(lobby_scene::click_at([-3, 18, -5], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 22, -5], on_start), None);
    // The wall's own face is one block behind the signs for the blocks beside them.
    assert_eq!(lobby_scene::click_at([2, 18, -6], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([0, 17, -4], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 17, -8], on_start), None);
}

#[test]
fn the_tile_under_a_point_has_its_edges_on_the_top_and_right_and_none_outside_the_panel() {
    assert_eq!(lobby_scene::tile_at(1.05, 17.03), Some(Tile::Start));
    assert_eq!(lobby_scene::tile_at(2.99, 20.37), Some(Tile::Start));
    assert_eq!(lobby_scene::tile_at(2.0, 20.40), None, "the gap above start");
    assert_eq!(lobby_scene::tile_at(2.0, 20.5), Some(Tile::Players));
    assert_eq!(lobby_scene::tile_at(3.0, 18.0), None);
    assert_eq!(lobby_scene::tile_at(-2.01, 18.0), None);
    assert_eq!(lobby_scene::tile_at(-1.0, 21.5), None);
}

#[test]
fn the_scene_summons_one_display_per_tile_and_two_per_pad() {
    let commands = lobby_scene::summon_commands(PanelState::default());
    assert_eq!(commands.len(), 11 + 2 * 8);
    assert!(commands.iter().all(|c| c.starts_with("execute in bingo:lobby run summon minecraft:text_display ")));
    assert!(commands.iter().all(|c| c.contains("Tags:[\"bingo_lobby_ui\"")));
    // The tiles are fixed, translucent, in front of the wall (z -3.9), centred on their rectangle's x: the start tile's is 2.025.
    let tiles = &commands[..11];
    assert!(tiles.iter().all(|c| c.contains("billboard:\"fixed\"") && c.contains(" -3.9 {") && !c.contains("background:0,")));
    assert!(tiles.iter().any(|c| c.contains(" 2.025 17.03 -3.9 ") && c.contains("\"bingo.native.lobby_menu_start\"")));
    assert!(tiles.iter().any(|c| c.contains(" -0.525 18.17 -3.9 ") && c.contains("\"bingo.native.lobby_panel_reroll\"")));
}

#[test]
fn the_live_tiles_show_the_state_and_a_refresh_replaces_only_them_by_tag() {
    let state = PanelState { auto_shuffle: true, teams: 5 };
    let commands = lobby_scene::refresh_commands(state);
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[0], "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_tile_auto_shuffle]");
    assert!(commands[1].contains("Tags:[\"bingo_lobby_ui\",\"bingo_lobby_tile_auto_shuffle\"]"));
    assert!(commands[1].contains("\"bingo.native.lobby_panel_auto_shuffle_on\"") && commands[1].contains("color:\"green\""));
    assert_eq!(commands[2], "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_tile_team_count]");
    assert!(commands[3].contains("\"bingo.game.team_count\"") && commands[3].contains("with:[\"5\"]"));
    assert!(commands[3].contains("Tags:[\"bingo_lobby_ui\",\"bingo_lobby_tile_team_count\"]"));
    // A refreshed panel matches a fresh one: the same commands for those tiles.
    let full = lobby_scene::summon_commands(state);
    assert!(full.contains(&commands[1]) && full.contains(&commands[3]));
    let off = lobby_scene::refresh_commands(PanelState { auto_shuffle: false, teams: 1 });
    assert!(off[1].contains("\"bingo.native.lobby_panel_auto_shuffle_off\""));
    assert!(off[3].contains("\"bingo.game.team_count_1\""));
}

#[test]
fn a_pad_has_its_name_in_brackets_in_the_team_colour_and_click_to_join_in_grey_at_scale_one() {
    let commands = lobby_scene::summon_commands(PanelState::default());
    // The blue pad (16, 23, -9): its name 1.4 above the block, `Click to join!` 1.1, both over the block's middle.
    let name = commands.iter().find(|c| c.contains(" 16.5 24.4 -8.5 ")).unwrap();
    assert!(name.contains("text:\"[") && name.contains("\"bingo.team.blue\"") && name.contains("color:\"blue\"") && name.contains("text:\"]\""));
    assert!(name.contains("scale:[1.000f,1.000f,1f]") && name.contains("billboard:\"center\""));
    let join = commands.iter().find(|c| c.contains(" 16.5 24.1 -8.5 ")).unwrap();
    assert!(join.contains("\"bingo.lobby.click_to_join\"") && join.contains("color:\"gray\"") && join.contains("scale:[1.000f,1.000f,1f]"));
    assert_eq!(
        lobby_scene::clear_command(),
        "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_ui]"
    );
}

#[test]
fn a_title_command_sets_the_times_then_the_subtitle_then_the_coloured_title() {
    let commands = lobby_scene::title_commands("Alice", &texts::start_title(), &texts::started_subtitle(), Some("gold"));
    assert_eq!(
        commands,
        vec![
            "title Alice times 5 40 10".to_owned(),
            "title Alice subtitle {\"translate\":\"bingo.game.announce.started\",\"fallback\":\"Game has started!\"}".to_owned(),
            "title Alice title {\"translate\":\"bingo.game.announce.bingo\",\"fallback\":\"BINGO\",\"color\":\"gold\"}".to_owned(),
        ]
    );
    let number = lobby_scene::title_commands("Bob", &texts::countdown_number(3), &texts::no_subtitle(), None);
    assert_eq!(number[0], "title Bob times 0 25 5");
    assert_eq!(number[1], "title Bob subtitle {\"text\":\"\"}");
    assert_eq!(
        number[2],
        "title Bob title {\"translate\":\"bingo.native.countdown_number\",\"fallback\":\"%s\",\"with\":[\"3\"]}"
    );
}

#[test]
fn a_start_with_a_three_second_countdown_titles_bingo_in_the_team_colour_then_2_1_and_game_has_started() {
    let (mut session, mut host) = session(3);
    session.command(&mut host, Some(ALICE), "join red");
    session.command(&mut host, Some(BOB), "join pink");
    host.titles.clear();
    session.command(&mut host, None, "start");
    let step = |host: &mut FakeHost| host.titles.drain(..).filter(|t| t.0 == ALICE).collect::<Vec<_>>();
    // The start itself: the starting title, in red for Alice.
    assert_eq!(step(&mut host), vec![(ALICE, "BINGO".to_owned(), "Game is starting!".to_owned(), Some("red".to_owned()))]);
    let mut seconds = Vec::new();
    for _ in 0..3 {
        for _ in 0..20 {
            session.tick(&mut host);
        }
        seconds.extend(step(&mut host));
    }
    assert_eq!(
        seconds,
        vec![
            (ALICE, "2".to_owned(), String::new(), None),
            (ALICE, "1".to_owned(), String::new(), None),
            (ALICE, "BINGO".to_owned(), "Game has started!".to_owned(), Some("red".to_owned())),
        ]
    );
}

#[test]
fn a_player_on_no_team_cannot_ready_up_and_one_on_a_team_can() {
    let (mut session, mut host) = session(3);
    assert_eq!(
        session.command(&mut host, Some(ALICE), "ready"),
        "You cannot ready up without joining a team! (use '/spectator' if you want to spectate)"
    );
    session.command(&mut host, Some(ALICE), "join red");
    assert_eq!(session.command(&mut host, Some(ALICE), "ready"), "Alice is ready!");
}

#[test]
fn joining_a_team_plays_the_lodestone_lock_to_that_player_and_a_reroll_plays_primed_tnt() {
    let (mut session, mut host) = session(3);
    host.commands.clear();
    session.command(&mut host, Some(BOB), "join green");
    assert_eq!(host.commands, vec!["playsound minecraft:item.lodestone_compass.lock master Bob".to_owned()]);
    host.commands.clear();
    session.command(&mut host, Some(ALICE), "reroll");
    assert_eq!(host.commands, vec!["playsound minecraft:entity.tnt.primed master Alice".to_owned()]);
}

#[test]
fn freeze_and_thaw_commands_name_only_slowness_and_jump_boost() {
    assert_eq!(
        lobby_scene::freeze_commands("Alice", 5),
        vec![
            "gamemode adventure Alice".to_owned(),
            "effect give Alice minecraft:slowness 5 255 true".to_owned(),
            "effect give Alice minecraft:jump_boost 5 128 true".to_owned(),
        ]
    );
    assert_eq!(
        lobby_scene::thaw_commands("Alice"),
        vec!["effect clear Alice minecraft:slowness".to_owned(), "effect clear Alice minecraft:jump_boost".to_owned()]
    );
}

#[test]
fn the_countdown_holds_team_players_for_its_length_and_lets_them_go_at_zero() {
    let (mut session, mut host) = session(5);
    session.command(&mut host, Some(ALICE), "join red");
    session.command(&mut host, Some(BOB), "join pink");
    host.commands.clear();
    session.command(&mut host, None, "start");
    let effects = |host: &FakeHost| host.commands.iter().filter(|c| c.starts_with("effect ")).cloned().collect::<Vec<_>>();
    assert_eq!(
        effects(&host),
        vec![
            "effect give Alice minecraft:slowness 5 255 true".to_owned(),
            "effect give Alice minecraft:jump_boost 5 128 true".to_owned(),
            "effect give Bob minecraft:slowness 5 255 true".to_owned(),
            "effect give Bob minecraft:jump_boost 5 128 true".to_owned(),
        ]
    );
    host.commands.clear();
    for _ in 0..4 * 20 {
        session.tick(&mut host);
    }
    assert_eq!(effects(&host), Vec::<String>::new(), "nothing is cleared before the countdown ends");
    for _ in 0..20 {
        session.tick(&mut host);
    }
    assert_eq!(
        effects(&host),
        vec![
            "effect clear Alice minecraft:slowness".to_owned(),
            "effect clear Alice minecraft:jump_boost".to_owned(),
            "effect clear Bob minecraft:slowness".to_owned(),
            "effect clear Bob minecraft:jump_boost".to_owned(),
        ]
    );
}

#[test]
fn a_reset_during_the_countdown_releases_the_players_and_a_zero_countdown_holds_nobody() {
    let (mut session, mut host) = session(5);
    session.command(&mut host, Some(ALICE), "join red");
    session.command(&mut host, Some(BOB), "join pink");
    session.command(&mut host, None, "start");
    host.commands.clear();
    session.command(&mut host, None, "reset");
    assert!(host.commands.contains(&"effect clear Alice minecraft:slowness".to_owned()));
    assert!(host.commands.contains(&"effect clear Alice minecraft:jump_boost".to_owned()));
    let (mut session, mut host) = session_zero();
    session.command(&mut host, Some(ALICE), "join red");
    session.command(&mut host, Some(BOB), "join pink");
    host.commands.clear();
    session.command(&mut host, None, "start");
    assert!(host.commands.iter().all(|c| !c.starts_with("effect ")));
}

fn session_zero() -> (Session, FakeHost) {
    session(0)
}

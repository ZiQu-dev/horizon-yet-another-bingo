//! The lobby's in-world parts: which clicked block means what (the menu signs, the team pads), the displays that label
//! them, the countdown and start titles, the sounds of the lobby, and `ready` for a player on no team. Expected values are
//! written out by hand from the original's lobby POIs (`lobby_poi.json.gz`) and `CountdownService`.
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::lobby_scene::{self, Click, PanelAction, PanelPage, PanelState, Tile};
use horizon_bingo::server::options::{Goal, Mode};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts::{self, Line, PanelMode};
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

// The panel's layout, written out by hand: five columns (x -2..-0.5, -0.44..0.76, 0.82..1.82, 1.88..2.98, 3.04..4.0) and six
// rows whose tiles are 0.36 high and 0.42 apart from y 19.5 down: row 0 is 19.14..19.5, row 1 18.72..19.08, row 2
// 18.30..18.66, row 3 17.88..18.24, row 4 17.46..17.82 and row 5 17.04..17.40. The Game Mode tiles are two rows high.
fn hit(x: f64, y: f64) -> Option<Click> {
    lobby_scene::click_at([0, 18, -5], [x, y, -4.0])
}

#[test]
fn every_tile_of_the_grid_maps_to_its_click_at_its_middle() {
    let panel = |action| Some(Click::Panel(action));
    // Game Mode: four icon tiles, two rows high, and Features under them.
    assert_eq!(hit(-1.64, 18.69), panel(PanelAction::Mode(PanelMode::Lockout)));
    assert_eq!(hit(-0.86, 18.69), panel(PanelAction::Mode(PanelMode::Inventory)));
    assert_eq!(hit(-1.64, 17.85), panel(PanelAction::Mode(PanelMode::HiddenItems)));
    assert_eq!(hit(-0.86, 17.85), panel(PanelAction::Mode(PanelMode::ConsumeItems)));
    assert_eq!(hit(-1.25, 17.22), Some(Click::Open(PanelPage::Features)));
    // Goal: `-` value `+`, the Lines/Items switch, the time limit's `-` value `+`, More...
    assert_eq!(hit(-0.32, 18.90), panel(PanelAction::GoalFewer));
    assert_eq!(hit(0.16, 18.90), None, "the goal value only shows");
    assert_eq!(hit(0.64, 18.90), panel(PanelAction::GoalMore));
    assert_eq!(hit(0.16, 18.48), panel(PanelAction::GoalKind));
    assert_eq!(hit(-0.32, 17.64), panel(PanelAction::TimeFewer));
    assert_eq!(hit(0.16, 17.64), None, "the time value only shows");
    assert_eq!(hit(0.64, 17.64), panel(PanelAction::TimeMore));
    assert_eq!(hit(0.16, 17.22), Some(Click::Open(PanelPage::Goal)));
    // Difficulty: four presets and Customize.
    assert_eq!(hit(1.32, 18.90), panel(PanelAction::Difficulty(0)));
    assert_eq!(hit(1.32, 18.48), panel(PanelAction::Difficulty(1)));
    assert_eq!(hit(1.32, 18.06), panel(PanelAction::Difficulty(2)));
    assert_eq!(hit(1.32, 17.64), panel(PanelAction::Difficulty(3)));
    assert_eq!(hit(1.32, 17.22), Some(Click::Open(PanelPage::Items)));
    // Shufflificator: auto shuffle, `-` count `+`, Shuffle!, a gap row, Re-roll Card.
    assert_eq!(hit(2.43, 18.90), Some(Click::AutoShuffle));
    assert_eq!(hit(2.00, 18.48), Some(Click::TeamsFewer));
    assert_eq!(hit(2.43, 18.48), None, "the team count only shows");
    assert_eq!(hit(2.86, 18.48), Some(Click::TeamsMore));
    assert_eq!(hit(2.43, 18.06), Some(Click::Shuffle));
    assert_eq!(hit(2.43, 17.64), None, "the empty row under Shuffle!");
    assert_eq!(hit(2.43, 17.22), Some(Click::Reroll));
    // The start column: best time and players show, START is three rows high, Reset Defaults under it.
    assert_eq!(hit(3.52, 19.32), None);
    assert_eq!(hit(3.52, 18.90), None);
    assert_eq!(hit(3.52, 18.66 - 0.01), Some(Click::Start), "the top of START");
    assert_eq!(hit(3.52, 17.46), Some(Click::Start), "the bottom of START");
    assert_eq!(hit(3.52, 17.22), Some(Click::ResetDefaults));
    // Headers show.
    assert_eq!(hit(-1.25, 19.32), None);
    assert_eq!(hit(1.32, 19.32), None);
    assert_eq!(hit(0.16, 18.0 + 0.0), None, "the time header row");
}

#[test]
fn a_click_between_tiles_or_off_the_panel_does_nothing_and_the_edges_belong_to_the_right_and_the_top() {
    assert_eq!(hit(-0.47, 18.9), None, "the gap between the first two columns");
    assert_eq!(hit(-1.25, 18.69), None, "the gap between the two mode tiles");
    assert_eq!(hit(1.32, 18.69), None, "the gap between two difficulty rows");
    assert_eq!(hit(4.0, 18.0), None, "x 4 is past the panel");
    assert_eq!(lobby_scene::click_at([3, 18, -5], [3.9, 20.0, -4.0]), None, "above the panel");
    assert_eq!(lobby_scene::tile_at(-2.0, 18.5), Some(Tile::Mode(PanelMode::Lockout)), "the left edge is in the tile");
    assert_eq!(lobby_scene::tile_at(-1.28, 18.5), None, "the right edge is not");
    assert_eq!(lobby_scene::tile_at(-1.22, 18.5), Some(Tile::Mode(PanelMode::Inventory)));
    assert_eq!(lobby_scene::tile_at(3.04, 18.0), Some(Tile::Start));
}

#[test]
fn only_the_wall_behind_the_panel_is_clickable() {
    let on_start = [3.52, 18.0, -4.0];
    // The wall's blocks are x -2..3 and y 16..21; the ray ends on whichever of them is behind the tile.
    assert_eq!(lobby_scene::click_at([3, 18, -5], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([-2, 17, -5], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([4, 18, -5], on_start), None);
    assert_eq!(lobby_scene::click_at([-3, 18, -5], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 22, -5], on_start), None);
    assert_eq!(lobby_scene::click_at([2, 18, -6], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([0, 17, -4], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 17, -8], on_start), None);
}

#[test]
fn the_panel_is_six_blocks_wide_about_two_and_a_half_high_and_its_tiles_never_overlap() {
    assert_eq!(lobby_scene::TILES.len(), 33);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (i, (a, ra)) in lobby_scene::TILES.iter().enumerate() {
        x0 = x0.min(ra[0]);
        y0 = y0.min(ra[1]);
        x1 = x1.max(ra[2]);
        y1 = y1.max(ra[3]);
        for (b, rb) in &lobby_scene::TILES[i + 1..] {
            let apart = ra[2] <= rb[0] || rb[2] <= ra[0] || ra[3] <= rb[1] || rb[3] <= ra[1];
            assert!(apart, "{a:?} overlaps {b:?}");
        }
        // The middle of every tile maps back to it.
        assert_eq!(lobby_scene::tile_at((ra[0] + ra[2]) / 2.0, (ra[1] + ra[3]) / 2.0), Some(*a));
    }
    assert!((x0 - -2.0).abs() < 1e-9 && (x1 - 4.0).abs() < 1e-9, "x {x0}..{x1}");
    assert!((y0 - 17.04).abs() < 1e-9 && (y1 - 19.5).abs() < 1e-9, "y {y0}..{y1}");
}

#[test]
fn a_rule_tile_runs_the_command_for_the_rules_as_they_are_and_nothing_at_the_end_of_a_range() {
    let command = |action, state: &PanelState| lobby_scene::panel_command(action, state);
    let base = PanelState::default();
    let with_goal = |goal| PanelState { goal, ..base };
    let with_time = |time_limit| PanelState { time_limit, ..base };
    assert_eq!(command(PanelAction::GoalMore, &with_goal(Goal::Lines(1))), Some("goal lines 2".to_owned()));
    assert_eq!(command(PanelAction::GoalFewer, &with_goal(Goal::Lines(3))), Some("goal lines 2".to_owned()));
    assert_eq!(command(PanelAction::GoalFewer, &with_goal(Goal::Lines(1))), None);
    assert_eq!(command(PanelAction::GoalMore, &with_goal(Goal::Lines(11))), None, "the original's most lines");
    assert_eq!(command(PanelAction::GoalMore, &with_goal(Goal::Items(24))), Some("goal full_card".to_owned()));
    assert_eq!(command(PanelAction::GoalMore, &with_goal(Goal::FullCard)), None);
    assert_eq!(command(PanelAction::GoalFewer, &with_goal(Goal::FullCard)), Some("goal items 24".to_owned()));
    assert_eq!(command(PanelAction::GoalKind, &with_goal(Goal::Lines(3))), Some("goal items 3".to_owned()));
    assert_eq!(command(PanelAction::GoalKind, &with_goal(Goal::Items(20))), Some("goal lines 11".to_owned()));
    assert_eq!(command(PanelAction::GoalKind, &with_goal(Goal::FullCard)), Some("goal lines 11".to_owned()));
    // The time limit goes by 15 minutes from off to 360, and back to off.
    assert_eq!(command(PanelAction::TimeMore, &with_time(None)), Some("timelimit 15".to_owned()));
    assert_eq!(command(PanelAction::TimeMore, &with_time(Some(45))), Some("timelimit 60".to_owned()));
    assert_eq!(command(PanelAction::TimeMore, &with_time(Some(50))), Some("timelimit 60".to_owned()));
    assert_eq!(command(PanelAction::TimeMore, &with_time(Some(360))), None);
    assert_eq!(command(PanelAction::TimeFewer, &with_time(Some(60))), Some("timelimit 45".to_owned()));
    assert_eq!(command(PanelAction::TimeFewer, &with_time(Some(50))), Some("timelimit 45".to_owned()));
    assert_eq!(command(PanelAction::TimeFewer, &with_time(Some(15))), Some("timelimit off".to_owned()));
    assert_eq!(command(PanelAction::TimeFewer, &with_time(None)), None);
    // The presets, in the original's order, and the four mode switches (a click turns the one shown on off).
    for (index, name) in ["easy", "medium", "hard", "extreme"].into_iter().enumerate() {
        assert_eq!(command(PanelAction::Difficulty(index), &base), Some(format!("difficulty {name}")));
    }
    assert_eq!(command(PanelAction::Difficulty(4), &base), None, "the panel shows four presets");
    let lockout = PanelState { mode: Mode::Lockout, ..base };
    assert_eq!(command(PanelAction::Mode(PanelMode::Lockout), &base), Some("mode lockout on".to_owned()));
    assert_eq!(command(PanelAction::Mode(PanelMode::Lockout), &lockout), Some("mode lockout off".to_owned()));
    assert_eq!(command(PanelAction::Mode(PanelMode::Inventory), &lockout), Some("mode inventory on".to_owned()));
    let both = PanelState { hidden_items: true, consume_items: false, ..base };
    assert_eq!(command(PanelAction::Mode(PanelMode::HiddenItems), &both), Some("mode hidden_items off".to_owned()));
    assert_eq!(command(PanelAction::Mode(PanelMode::ConsumeItems), &both), Some("mode consume_items on".to_owned()));
}

#[test]
fn the_buttons_that_are_slots_of_the_menus_root_page_go_through_those_slots() {
    assert_eq!(lobby_scene::root_slot(Click::Start), Some(22));
    assert_eq!(lobby_scene::root_slot(Click::Reroll), Some(10));
    assert_eq!(lobby_scene::root_slot(Click::AutoShuffle), Some(12));
    assert_eq!(lobby_scene::root_slot(Click::TeamsFewer), Some(14));
    assert_eq!(lobby_scene::root_slot(Click::TeamsMore), Some(16));
    assert_eq!(lobby_scene::root_slot(Click::Shuffle), Some(11));
    assert_eq!(lobby_scene::root_slot(Click::ResetDefaults), Some(24));
    assert_eq!(lobby_scene::root_slot(Click::Open(PanelPage::Goal)), None);
}

/// The position a summon command puts its display at.
fn pos_of(command: &str) -> [f64; 3] {
    let rest = command.split("minecraft:text_display ").nth(1).unwrap();
    let mut numbers = rest.split(' ').take(3).map(|n| n.parse::<f64>().unwrap());
    [numbers.next().unwrap(), numbers.next().unwrap(), numbers.next().unwrap()]
}

/// The `[x, y]` scale of a summon command.
fn scale_of(command: &str) -> [f64; 2] {
    let rest = command.split("scale:[").nth(1).unwrap();
    let mut numbers = rest.split(|c| c == ',' || c == ']').take(2).map(|n| n.trim_end_matches('f').parse::<f64>().unwrap());
    [numbers.next().unwrap(), numbers.next().unwrap()]
}

#[test]
fn the_scene_summons_one_small_display_per_tile_on_the_panel_plane_and_two_per_pad() {
    let commands = lobby_scene::summon_commands(&PanelState::default());
    assert_eq!(commands.len(), 33 + 2 * 8);
    assert!(commands.iter().all(|c| c.starts_with("execute in bingo:lobby run summon minecraft:text_display ")));
    assert!(commands.iter().all(|c| c.contains("Tags:[\"bingo_lobby_ui\"")));
    let tiles = &commands[..33];
    // Every tile is fixed, translucent, in front of the wall (z -3.9), at its rectangle's bottom middle, with text that is
    // small: a scale the same both ways (about a half) except START, which is drawn at about one.
    for (command, (tile, rect)) in tiles.iter().zip(lobby_scene::TILES) {
        assert!(command.contains("billboard:\"fixed\"") && !command.contains("background:0,"), "{tile:?}");
        let pos = pos_of(command);
        assert!((pos[0] - (rect[0] + rect[2]) / 2.0).abs() < 1e-9 && (pos[1] - rect[1]).abs() < 1e-9 && pos[2] == -3.9, "{tile:?} {pos:?}");
        let [sx, sy] = scale_of(command);
        assert!((sx - sy).abs() < 1e-6, "{tile:?} is stretched");
        let (low, high) = if tile == Tile::Start { (0.85, 1.2) } else { (0.2, 0.6) };
        assert!(sx >= low && sx <= high, "{tile:?} text scale {sx}");
    }
    // Start's display stands on x 3.52 (the middle of 3.04..4.0) at y 17.46, the bottom of its three rows.
    let start = tiles.iter().find(|c| c.contains("\"bingo.native.lobby_menu_start\"")).unwrap();
    let pos = pos_of(start);
    assert!((pos[0] - 3.52).abs() < 1e-9 && (pos[1] - 17.46).abs() < 1e-9);
    // The headers are green on dark green; the default difficulty (easy) is the light chosen tile with black text.
    let header = tiles.iter().find(|c| c.contains("\"bingo.options.game_mode\"")).unwrap();
    assert!(header.contains("color:\"#99FF99\"") && header.contains(&format!("background:{},", 0xB00E_2A12u32 as i32)));
    let easy = tiles.iter().find(|c| c.contains("\"bingo.card_difficulty.easy\"")).unwrap();
    assert!(easy.contains("color:\"#000000\"") && easy.contains(&format!("background:{},", 0xE0C4_C8C8u32 as i32)));
    let medium = tiles.iter().find(|c| c.contains("\"bingo.card_difficulty.medium\"")).unwrap();
    assert!(medium.contains("color:\"#A8A8A8\"") && medium.contains(&format!("background:{},", 0xA026_2B2Bu32 as i32)));
    // The five green headers of the original's columns.
    for key in ["bingo.options.game_mode", "bingo.options.goal", "bingo.options.time_limit", "bingo.options.card_difficulty", "bingo.options.teams"] {
        assert!(tiles.iter().any(|c| c.contains(&format!("\"{key}\"")) && c.contains("color:\"#99FF99\"")), "{key}");
    }
}

#[test]
fn the_live_tiles_show_the_rules() {
    let state = PanelState {
        auto_shuffle: true,
        teams: 5,
        goal: Goal::Lines(3),
        time_limit: Some(90),
        difficulty: Some(2),
        mode: Mode::Lockout,
        hidden_items: true,
        consume_items: false,
        players: 2,
        max_players: 8,
    };
    let commands = lobby_scene::summon_commands(&state);
    let find = |key: &str| commands[..33].iter().find(|c| c.contains(&format!("\"{key}\""))).unwrap_or_else(|| panic!("{key}"));
    assert!(find("bingo.game.line_count").contains("with:[\"3\"]"), "the goal");
    assert!(find("bingo.native.lobby_panel_time_value").contains("with:[\"1h 30m\"]"), "the time limit");
    assert!(find("bingo.game.team_count").contains("with:[\"5\"]"), "the team count");
    assert!(find("bingo.options.player_count").contains("with:[\"2 / 8\"]"), "the players");
    assert!(find("bingo.native.lobby_panel_goal_lines").contains("Lines"));
    let light = format!("background:{},", 0xE0C4_C8C8u32 as i32);
    // Chosen tiles are light and carry the filled marker; the others are dark with the hollow one.
    for (key, on) in [
        ("bingo.card_difficulty.easy", false),
        ("bingo.card_difficulty.hard", true),
        ("bingo.options.mode_lockout", true),
        ("bingo.options.mode_inventory", false),
        ("bingo.options.mode_hidden_items", true),
        ("bingo.options.mode_consume_items", false),
        ("bingo.options.teams_shuffle_auto", true),
    ] {
        let command = find(key);
        assert_eq!(command.contains(&light), on, "{key}");
        assert_eq!(command.contains("\u{25C6} "), on, "{key} filled marker");
        assert_eq!(command.contains("\u{25C7} "), !on, "{key} hollow marker");
    }
    let off = lobby_scene::summon_commands(&PanelState::default());
    assert!(off.iter().any(|c| c.contains("\"bingo.options.time_limit_off\"")), "off shows Off");
    assert!(off.iter().any(|c| c.contains("\"bingo.game.line_count_1\"")), "one line shows 1 line");
}

#[test]
fn a_refresh_replaces_only_the_tiles_whose_look_changed_by_their_tag() {
    let old = PanelState::default();
    let new = PanelState { goal: Goal::Lines(2), difficulty: Some(1), players: 2, ..old };
    let commands = lobby_scene::refresh_commands(&old, &new);
    // The goal value, the two difficulty tiles that swap their look, and the player count: a kill and a summon each.
    assert_eq!(commands.len(), 8);
    let kills: Vec<&String> = commands.iter().step_by(2).collect();
    assert_eq!(
        kills,
        [
            "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_tile_goalvalue]",
            "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_tile_difficulty_0]",
            "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_tile_difficulty_1]",
            "execute in bingo:lobby run kill @e[type=minecraft:text_display,tag=bingo_lobby_tile_players]",
        ]
    );
    assert!(commands[1].contains("Tags:[\"bingo_lobby_ui\",\"bingo_lobby_tile_goalvalue\"]") && commands[1].contains("with:[\"2\"]"));
    assert!(commands[7].contains("with:[\"2 / 8\"]"));
    // A refreshed panel matches a fresh one for those tiles, and an unchanged state refreshes nothing.
    let full = lobby_scene::summon_commands(&new);
    assert!(commands.iter().skip(1).step_by(2).all(|c| full.contains(c)));
    assert!(lobby_scene::refresh_commands(&new, &new).is_empty());
    // Toggling a mode changes that tile only.
    let toggled = lobby_scene::refresh_commands(&old, &PanelState { mode: Mode::Inventory, ..old });
    assert_eq!(toggled.len(), 2);
    assert!(toggled[0].ends_with("tag=bingo_lobby_tile_mode_inventory]"));
}

#[test]
fn a_pad_has_its_name_in_brackets_in_the_team_colour_and_click_to_join_in_grey_at_scale_one() {
    let commands = lobby_scene::summon_commands(&PanelState::default());
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
    // The first number waits 100 ticks (the original's start delay), then one a second.
    let mut seconds = Vec::new();
    for _ in 0..100 {
        session.tick(&mut host);
    }
    assert!(step(&mut host).is_empty(), "nothing is shown during the delay");
    session.tick(&mut host);
    seconds.extend(step(&mut host));
    for _ in 0..3 {
        for _ in 0..20 {
            session.tick(&mut host);
        }
        seconds.extend(step(&mut host));
    }
    assert_eq!(
        seconds,
        vec![
            (ALICE, "3".to_owned(), String::new(), None),
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
            "effect give Alice minecraft:slowness 11 255 true".to_owned(),
            "effect give Alice minecraft:jump_boost 11 128 true".to_owned(),
            "effect give Bob minecraft:slowness 11 255 true".to_owned(),
            "effect give Bob minecraft:jump_boost 11 128 true".to_owned(),
        ]
    );
    host.commands.clear();
    // Held through the 100 tick delay and the 5 numbers: play begins on tick 201.
    for _ in 0..200 {
        session.tick(&mut host);
    }
    assert_eq!(effects(&host), Vec::<String>::new(), "nothing is cleared before the countdown ends");
    for _ in 0..1 {
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

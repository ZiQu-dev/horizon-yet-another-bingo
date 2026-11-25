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

// The panel's layout, copied by hand from `docs/original-ledger/panel-dump.md`: each tile's centre (x, y) relative to the menu
// block (0, 17, -5), its width and height, and what a right click on it does. Add 17 to y for the world.
const TILE_TABLE: [((f64, f64), (f64, f64), Option<Click>); 33] = [
    ((-3.55, 1.65), (2.5, 0.3), None),
    ((-4.2, 1.05), (1.2, 0.7), Some(Click::Panel(PanelAction::Mode(PanelMode::Lockout)))),
    ((-2.9, 1.05), (1.2, 0.7), Some(Click::Panel(PanelAction::Mode(PanelMode::Inventory)))),
    ((-4.2, 0.25), (1.2, 0.7), Some(Click::Panel(PanelAction::Mode(PanelMode::HiddenItems)))),
    ((-2.9, 0.25), (1.2, 0.7), Some(Click::Panel(PanelAction::Mode(PanelMode::ConsumeItems)))),
    ((-3.55, -0.4), (2.5, 0.4), Some(Click::Open(PanelPage::Features))),
    ((-1.2, 1.65), (2.0, 0.3), None),
    ((-2.05, 1.25), (0.3, 0.3), Some(Click::Panel(PanelAction::GoalFewer))),
    ((-1.2, 1.25), (1.2, 0.3), None),
    ((-0.35, 1.25), (0.3, 0.3), Some(Click::Panel(PanelAction::GoalMore))),
    ((-1.2, 0.85), (2.0, 0.3), Some(Click::Panel(PanelAction::GoalKind))),
    ((-1.2, 0.45), (2.0, 0.3), None),
    ((-2.05, 0.05), (0.3, 0.3), Some(Click::Panel(PanelAction::TimeFewer))),
    ((-1.2, 0.05), (1.2, 0.3), None),
    ((-0.35, 0.05), (0.3, 0.3), Some(Click::Panel(PanelAction::TimeMore))),
    ((-1.2, -0.4), (2.0, 0.4), Some(Click::Open(PanelPage::Goal))),
    ((0.7, 1.65), (1.6, 0.3), None),
    ((0.7, 1.25), (1.6, 0.3), Some(Click::Panel(PanelAction::Difficulty(0)))),
    ((0.7, 0.85), (1.6, 0.3), Some(Click::Panel(PanelAction::Difficulty(1)))),
    ((0.7, 0.45), (1.6, 0.3), Some(Click::Panel(PanelAction::Difficulty(2)))),
    ((0.7, 0.05), (1.6, 0.3), Some(Click::Panel(PanelAction::Difficulty(3)))),
    ((0.7, -0.4), (1.6, 0.4), Some(Click::Open(PanelPage::Items))),
    ((2.6, 1.65), (2.0, 0.3), None),
    ((2.6, 1.25), (2.0, 0.3), Some(Click::AutoShuffle)),
    ((1.75, 0.85), (0.3, 0.3), Some(Click::TeamsFewer)),
    ((2.6, 0.85), (1.2, 0.3), None),
    ((3.45, 0.85), (0.3, 0.3), Some(Click::TeamsMore)),
    ((2.6, 0.45), (2.0, 0.3), Some(Click::Shuffle)),
    ((2.6, -0.4), (2.0, 0.4), Some(Click::Reroll)),
    ((4.75, 1.65), (2.1, 0.3), None),
    ((4.75, 1.25), (2.1, 0.3), None),
    ((4.75, 0.45), (2.1, 1.1), Some(Click::Start)),
    ((4.75, -0.4), (2.1, 0.4), Some(Click::ResetDefaults)),
];

/// A right click whose ray ends on the wall block behind the panel at world `(x, y)`, on the wall's face z -5.
fn hit(x: f64, y: f64) -> Option<Click> {
    lobby_scene::click_at([x.floor() as i32, y.floor() as i32, -6], [x, y, -5.0])
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_tile_table_has_the_dumps_numbers_for_eight_spot_checked_tiles() {
    assert_eq!(lobby_scene::TILES.len(), 33);
    let rect_of = |tile: Tile| lobby_scene::TILES.iter().find(|(t, _)| *t == tile).unwrap().1;
    // (tile, centre x, centre y rel POI, width, height) as written in the dump's tables.
    let spots = [
        (Tile::HeaderMode, -3.55, 1.65, 2.5, 0.3),
        (Tile::Mode(PanelMode::Lockout), -4.2, 1.05, 1.2, 0.7),
        (Tile::Features, -3.55, -0.4, 2.5, 0.4),
        (Tile::GoalMore, -0.35, 1.25, 0.3, 0.3),
        (Tile::TimeValue, -1.2, 0.05, 1.2, 0.3),
        (Tile::Difficulty(3), 0.7, 0.05, 1.6, 0.3),
        (Tile::Shuffle, 2.6, 0.45, 2.0, 0.3),
        (Tile::Start, 4.75, 0.45, 2.1, 1.1),
    ];
    for (tile, cx, cy, w, h) in spots {
        let [x0, y0, x1, y1] = rect_of(tile);
        assert!(near(x0, cx - w / 2.0) && near(x1, cx + w / 2.0), "{tile:?} x {x0}..{x1}");
        assert!(near(y0, 17.0 + cy - h / 2.0) && near(y1, 17.0 + cy + h / 2.0), "{tile:?} y {y0}..{y1}");
    }
}

#[test]
fn every_tile_of_the_dump_maps_to_its_click_at_its_middle_and_nothing_just_outside_it() {
    for ((cx, cy), (w, h), click) in TILE_TABLE {
        let y = 17.0 + cy;
        assert_eq!(hit(cx, y), click, "centre ({cx}, {cy})");
        // The tiles are 0.1 apart, so a point 0.02 outside a side is in no tile.
        for (x, y) in [(cx - w / 2.0 - 0.02, y), (cx + w / 2.0 + 0.02, y), (cx, y - h / 2.0 - 0.02), (cx, y + h / 2.0 + 0.02)] {
            assert_eq!(hit(x, y), None, "outside ({cx}, {cy}) at ({x}, {y})");
        }
        // Just inside the corners is still the tile.
        assert_eq!(hit(cx - w / 2.0 + 0.01, y - h / 2.0 + 0.01), click, "bottom left corner ({cx}, {cy})");
        assert_eq!(hit(cx + w / 2.0 - 0.01, y + h / 2.0 - 0.01), click, "top right corner ({cx}, {cy})");
    }
    // Every tile of the table is in the code's table once.
    for ((cx, cy), _, _) in TILE_TABLE {
        let at = |(_, r): &(Tile, [f64; 4])| r[0] < cx && cx < r[2] && r[1] < 17.0 + cy && 17.0 + cy < r[3];
        assert_eq!(lobby_scene::TILES.iter().filter(|t| at(t)).count(), 1, "({cx}, {cy})");
    }
}

#[test]
fn the_edges_of_a_tile_belong_to_the_tile_above_and_to_the_right() {
    // Lockout is x -4.8..-3.6, y 17.7..18.4.
    assert_eq!(lobby_scene::tile_at(-4.8, 17.7), Some(Tile::Mode(PanelMode::Lockout)), "the bottom left corner is in");
    assert_eq!(lobby_scene::tile_at(-3.6, 18.0), None, "the right edge is not");
    assert_eq!(lobby_scene::tile_at(-4.0, 18.4), None, "the top edge is not");
    assert_eq!(lobby_scene::tile_at(-5.0, 18.0), None, "left of the panel");
    assert_eq!(lobby_scene::tile_at(5.8, 17.5), None, "right of the panel (x 5.8 is the end)");
    assert_eq!(lobby_scene::tile_at(5.79, 18.65), Some(Tile::BestTime));
}

#[test]
fn only_the_wall_blocks_behind_the_panel_are_clickable() {
    let on_start = [4.75, 17.45, -5.0];
    // The wall covers x -5..5 and y 16..21 at z -7..-5: whichever block the ray ends on there maps the hit's x and y.
    assert_eq!(lobby_scene::click_at([4, 17, -6], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([-5, 16, -6], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([5, 21, -7], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([2, 17, -5], on_start), Some(Click::Start));
    assert_eq!(lobby_scene::click_at([6, 17, -6], on_start), None);
    assert_eq!(lobby_scene::click_at([-6, 17, -6], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 22, -6], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 15, -6], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 17, -4], on_start), None);
    assert_eq!(lobby_scene::click_at([0, 17, -8], on_start), None);
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

const HEADER_BLOCK: &str = "execute in bingo:lobby run summon minecraft:block_display -4.800 18.500 -5.001 \
{Tags:[\"bingo_lobby_ui\",\"bingo_lobby_tile_headermode\"],Rotation:[0f,0f],billboard:\"fixed\",\
block_state:{Name:\"minecraft:lime_concrete\"},brightness:{sky:3,block:0},view_range:1f,\
transformation:{left_rotation:[0f,0f,0f,1f],right_rotation:[0f,0f,0f,1f],translation:[0f,0f,0f],scale:[2.5f,0.3f,0.05f]}}";
const HEADER_LABEL: &str = "execute in bingo:lobby run summon minecraft:text_display -3.550 18.550 -4.950 \
{Tags:[\"bingo_lobby_ui\",\"bingo_lobby_tile_headermode\"],Rotation:[0f,0f],billboard:\"fixed\",alignment:\"center\",\
background:0,default_background:0b,shadow:false,see_through:false,line_width:200,view_range:1f,\
transformation:{left_rotation:[0f,0f,0f,1f],right_rotation:[0f,0f,0f,1f],translation:[0f,0f,0f],scale:[0.7f,0.7f,0.7f]},\
text:{translate:\"bingo.options.game_mode\",fallback:\"Game Mode\",color:\"#99FF99\"}}";

/// The summon commands of the panel's tiles (the pads' are last), for a state.
fn tile_commands(state: &PanelState) -> Vec<String> {
    let mut commands = lobby_scene::summon_commands(state);
    commands.truncate(commands.len() - 16);
    commands
}

/// The commands of the entities whose tags include the tile's `tag`.
fn of_tile<'a>(commands: &'a [String], tag: &str) -> Vec<&'a String> {
    commands.iter().filter(|c| c.contains(&format!("\"{tag}\""))).collect()
}

/// The three numbers after the entity type of a summon command.
fn pos_of(command: &str) -> [f64; 3] {
    let rest = command.split("summon minecraft:").nth(1).unwrap().split(' ').skip(1);
    let mut numbers = rest.take(3).map(|n| n.parse::<f64>().unwrap());
    [numbers.next().unwrap(), numbers.next().unwrap(), numbers.next().unwrap()]
}

/// What follows `key` in a command up to the next `,` or `}` or `]`.
fn field<'a>(command: &'a str, key: &str) -> &'a str {
    let rest = command.split(key).nth(1).unwrap_or_else(|| panic!("{key} in {command}"));
    rest.split(|c| c == ',' || c == '}' || c == ']').next().unwrap()
}

#[test]
fn the_scene_summons_the_dumps_entities_blocks_texts_and_two_labels_per_pad() {
    let commands = lobby_scene::summon_commands(&PanelState::default());
    // 33 tile blocks and 61 texts on the panel, like the original's 94; two texts for each of the 8 pads.
    assert_eq!(commands.iter().filter(|c| c.contains("minecraft:block_display")).count(), 33);
    assert_eq!(commands.iter().filter(|c| c.contains("minecraft:text_display")).count(), 61 + 16);
    assert_eq!(commands.len(), 94 + 16);
    assert!(commands.iter().all(|c| c.starts_with("execute in bingo:lobby run summon minecraft:")));
    assert!(commands.iter().all(|c| c.contains("Tags:[\"bingo_lobby_ui\"")));
    // Every panel entity also carries its tile's tag; the pads' do not.
    assert!(commands[..94].iter().all(|c| c.contains("\"bingo_lobby_tile_")));
    assert!(commands[94..].iter().all(|c| !c.contains("bingo_lobby_tile_")));
}

#[test]
fn a_header_tile_is_a_lime_block_at_sky_3_and_pale_green_text() {
    let commands = tile_commands(&PanelState::default());
    assert_eq!(commands[0], HEADER_BLOCK);
    assert_eq!(commands[1], HEADER_LABEL);
}

#[test]
fn a_stepper_and_a_value_tile_are_white_and_gray_and_the_value_text_has_a_shadow() {
    let commands = tile_commands(&PanelState::default());
    // The goal's `-`: 0.3 square at x -2.2..-1.9, y 18.1..18.4, white at sky 15, black text with line width 12.
    let minus = of_tile(&commands, "bingo_lobby_tile_goalfewer");
    assert_eq!(minus.len(), 2);
    assert!(minus[0].contains(" -2.200 18.100 -5.001 ") && minus[0].contains("minecraft:white_concrete") && minus[0].contains("sky:15,block:0"));
    assert!(minus[0].contains("scale:[0.3f,0.3f,0.05f]"));
    assert!(minus[1].contains(" -2.050 18.150 -4.950 ") && field(minus[1], "line_width:") == "12" && field(minus[1], "shadow:") == "false");
    assert!(minus[1].contains("text:{color:\"black\",extra:[\"-\"],text:\"\"}"));
    // The value: gray at sky 14, `1 line` in the original's key, white (no colour), with a shadow, line width 200.
    let value = of_tile(&commands, "bingo_lobby_tile_goalvalue");
    assert!(value[0].contains("minecraft:gray_concrete") && value[0].contains("sky:14,block:0") && value[0].contains("scale:[1.2f,0.3f,0.05f]"));
    assert!(value[1].contains("text:{translate:\"bingo.game.line_count_1\",fallback:\"1 line\"}"));
    assert_eq!(field(value[1], "shadow:"), "true");
    assert_eq!(field(value[1], "line_width:"), "200");
    // The time value, the team count, the players and the best time use the dump's texts.
    let text_of = |tag: &str| of_tile(&commands, tag).last().unwrap().split_once("},text:").unwrap().1.to_owned();
    assert_eq!(text_of("bingo_lobby_tile_timevalue"), "{translate:\"bingo.options.time_limit_off\",fallback:\"Off\"}}");
    assert_eq!(text_of("bingo_lobby_tile_teams"), "{translate:\"bingo.game.team_count_3\",fallback:\"3 teams\"}}");
    assert_eq!(
        text_of("bingo_lobby_tile_players"),
        "{translate:\"bingo.options.player_count\",fallback:\"Players: %s\",with:[{color:\"green\",text:\"0 / 8\"}],color:\"#99FF99\"}}"
    );
    assert_eq!(
        text_of("bingo_lobby_tile_besttime"),
        "{translate:\"bingo.stats.best_time\",fallback:\"Best time: %s\",\
         with:[{translate:\"bingo.stats.best_time.not_set\",fallback:\"[Not Set]\",color:\"gray\"}],color:\"#99FF99\"}}"
    );
}

#[test]
fn a_toggle_is_a_gray_tile_with_a_hollow_diamond_and_an_empty_filled_one_until_it_is_on() {
    let off = tile_commands(&PanelState::default());
    // Easy is the chosen difficulty: white, sky 15, black text, hollow diamond in black and the filled dark green one.
    let easy = of_tile(&off, "bingo_lobby_tile_difficulty_0");
    assert_eq!(easy.len(), 4);
    assert!(easy[0].contains(" -0.100 18.100 -5.001 ") && easy[0].contains("minecraft:white_concrete") && easy[0].contains("sky:15,block:0"));
    assert!(easy[1].contains(" 0.050 18.150 -4.945 ") && easy[1].contains("text:{color:\"black\",text:\"\u{25C7}\"}"));
    assert!(easy[2].contains(" 0.050 18.150 -4.950 ") && easy[2].contains("text:{color:\"dark_green\",text:\"\u{25C6}\"}"));
    assert!(easy[3].contains(" 0.700 18.150 -4.950 ") && field(easy[3], "line_width:") == "68");
    assert!(easy[3].contains("text:{color:\"black\",extra:[{translate:\"bingo.card_difficulty.easy\",fallback:\"Easy\"}],text:\"\"}"));
    // Medium is off: gray, sky 14, gray text, and the filled diamond's text is empty.
    let medium = of_tile(&off, "bingo_lobby_tile_difficulty_1");
    assert!(medium[0].contains("minecraft:gray_concrete") && medium[0].contains("sky:14,block:0"));
    assert!(medium[1].contains("text:{color:\"gray\",text:\"\u{25C7}\"}"));
    assert!(medium[2].contains("text:{color:\"dark_green\",text:\"\"}"));
    assert!(medium[3].contains("text:{color:\"gray\",extra:["));
}

#[test]
fn a_mode_tile_has_a_big_icon_a_small_caption_and_the_toggles_diamonds() {
    let commands = tile_commands(&PanelState::default());
    let lockout = of_tile(&commands, "bingo_lobby_tile_mode_lockout");
    assert_eq!(lockout.len(), 5);
    assert!(lockout[0].contains(" -4.800 17.700 -5.001 ") && lockout[0].contains("scale:[1.2f,0.7f,0.05f]"));
    assert!(lockout[1].contains(" -4.650 17.950 -4.945 "));
    assert!(lockout[2].contains(" -4.650 17.950 -4.950 "));
    // The icon: scale 1.6 at y 17.9; the caption: scale 0.4 at y 17.8 with line width 78.
    assert!(lockout[3].contains(" -4.200 17.900 -4.950 ") && lockout[3].contains("scale:[1.6f,1.6f,1.6f]"));
    assert!(lockout[3].contains("text:{color:\"gray\",text:\"\u{1F512}\"}"));
    assert!(lockout[4].contains(" -4.200 17.800 -4.950 ") && lockout[4].contains("scale:[0.4f,0.4f,0.4f]") && field(lockout[4], "line_width:") == "78");
    assert!(lockout[4].contains("extra:[{translate:\"bingo.options.mode_lockout\",fallback:\"Lockout\"}]"));
}

#[test]
fn a_button_has_its_corner_glyph_and_the_alt_ones_are_darker() {
    let commands = tile_commands(&PanelState::default());
    // Features: 2.5 x 0.4 at y -0.6, sky 8, the `...` glyph at x -4.65, line width 107.
    let features = of_tile(&commands, "bingo_lobby_tile_features");
    assert_eq!(features.len(), 3);
    assert!(features[0].contains("sky:8,block:0") && features[0].contains(" -4.800 16.400 -5.001 "));
    assert!(features[1].contains(" -4.650 16.500 -4.945 ") && features[1].contains("text:{color:\"gray\",text:\"\u{22EF}\"}"));
    assert!(features[2].contains(" -3.550 16.500 -4.950 ") && field(features[2], "line_width:") == "107");
    // START has no glyph: a 2.1 x 1.1 block and its label 0.45 above the block's bottom.
    let start = of_tile(&commands, "bingo_lobby_tile_start");
    assert_eq!(start.len(), 2);
    assert!(start[0].contains("scale:[2.1f,1.1f,0.05f]") && start[0].contains("sky:8,block:0"));
    assert!(start[1].contains(" 4.750 17.350 -4.950 ") && field(start[1], "line_width:") == "90");
    // Shuffle! is a plain button at sky 14 with no glyph.
    let shuffle = of_tile(&commands, "bingo_lobby_tile_shuffle");
    assert_eq!(shuffle.len(), 2);
    assert!(shuffle[0].contains("sky:14,block:0"));
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
    let commands = tile_commands(&state);
    let label = |tag: &str| of_tile(&commands, tag).last().unwrap().to_string();
    assert!(label("bingo_lobby_tile_goalvalue").contains("translate:\"bingo.game.line_count_3\",fallback:\"3 lines\""));
    assert!(label("bingo_lobby_tile_timevalue").contains("with:[\"1h 30m\"]"));
    assert!(label("bingo_lobby_tile_teams").contains("translate:\"bingo.game.team_count_5\",fallback:\"5 teams\""));
    assert!(label("bingo_lobby_tile_players").contains("text:\"2 / 8\""));
    assert!(label("bingo_lobby_tile_goalkind").contains("fallback:\"Lines\""));
    // A goal of ten lines has no numbered key: the generic one takes the number.
    let ten = tile_commands(&PanelState { goal: Goal::Lines(10), ..state });
    assert!(of_tile(&ten, "bingo_lobby_tile_goalvalue")[1].contains("translate:\"bingo.game.line_count\",fallback:\"%s lines\",with:[\"10\"]"));
    for (tag, on) in [
        ("bingo_lobby_tile_difficulty_0", false),
        ("bingo_lobby_tile_difficulty_2", true),
        ("bingo_lobby_tile_mode_lockout", true),
        ("bingo_lobby_tile_mode_inventory", false),
        ("bingo_lobby_tile_mode_hiddenitems", true),
        ("bingo_lobby_tile_mode_consumeitems", false),
        ("bingo_lobby_tile_autoshuffle", true),
    ] {
        let tile = of_tile(&commands, tag);
        assert_eq!(tile[0].contains("minecraft:white_concrete"), on, "{tag} block");
        assert_eq!(tile[2].contains("text:\"\u{25C6}\""), on, "{tag} filled diamond");
        assert_eq!(tile[1].contains("color:\"black\""), on, "{tag} ink");
    }
}

#[test]
fn a_refresh_rewrites_only_the_tiles_whose_state_changed_by_their_tag() {
    let old = PanelState::default();
    let new = PanelState { goal: Goal::Lines(2), difficulty: Some(1), players: 2, ..old };
    let commands = lobby_scene::refresh_commands(&old, &new);
    let kills: Vec<&String> = commands.iter().filter(|c| c.contains(" run kill ")).collect();
    assert_eq!(
        kills,
        [
            "execute in bingo:lobby run kill @e[type=!minecraft:player,tag=bingo_lobby_tile_goalvalue]",
            "execute in bingo:lobby run kill @e[type=!minecraft:player,tag=bingo_lobby_tile_difficulty_0]",
            "execute in bingo:lobby run kill @e[type=!minecraft:player,tag=bingo_lobby_tile_difficulty_1]",
            "execute in bingo:lobby run kill @e[type=!minecraft:player,tag=bingo_lobby_tile_players]",
        ]
    );
    // The goal value is a kill, a block and a label; each difficulty tile a kill and four entities; the players like the goal.
    assert_eq!(commands.len(), 3 + 5 + 5 + 3);
    assert!(commands[2].contains("translate:\"bingo.game.line_count_2\",fallback:\"2 lines\""));
    // The old difficulty (easy) is gray now and the new one white.
    assert!(commands[4].contains("minecraft:gray_concrete") && commands[9].contains("minecraft:white_concrete"));
    // The rewritten commands are the ones a fresh panel makes, and an unchanged state rewrites nothing.
    let fresh = lobby_scene::summon_commands(&new);
    assert!(commands.iter().filter(|c| !c.contains(" run kill ")).all(|c| fresh.contains(c)));
    assert!(lobby_scene::refresh_commands(&new, &new).is_empty());
    // A mode toggle rewrites that tile alone: its kill, block, two diamonds, icon and caption.
    let toggled = lobby_scene::refresh_commands(&old, &PanelState { mode: Mode::Inventory, ..old });
    assert_eq!(toggled.len(), 6);
    assert!(toggled[0].ends_with("tag=bingo_lobby_tile_mode_inventory]"));
    // The goal kind and the team count change their own tiles.
    let kind = lobby_scene::refresh_commands(&old, &PanelState { goal: Goal::Items(1), ..old });
    assert!(kind.iter().any(|c| c.ends_with("tag=bingo_lobby_tile_goalkind]")));
    assert!(kind.iter().any(|c| c.ends_with("tag=bingo_lobby_tile_goalvalue]")));
    let teams = lobby_scene::refresh_commands(&old, &PanelState { teams: 4, ..old });
    assert_eq!(teams.len(), 3);
    assert!(teams[2].contains("bingo.game.team_count_4"));
}

#[test]
fn a_pad_has_its_bracketed_name_and_click_to_join_as_vertical_billboards() {
    let commands = lobby_scene::summon_commands(&PanelState::default());
    // The blue pad (16, 23, -9): the name 1.5 above the block, `Click to join!` 1.0, both over the block's middle.
    let name = commands.iter().find(|c| c.contains(" 16.500 24.500 -8.500 ")).unwrap();
    assert!(name.contains("billboard:\"vertical\"") && name.contains("alignment:\"center\"") && name.contains("background:0,"));
    assert!(name.contains("shadow:true") && name.contains("line_width:200") && name.contains("scale:[1f,1f,1f]"));
    assert!(name.contains(
        "text:{translate:\"chat.square_brackets\",fallback:\"[%s]\",with:[{color:\"blue\",extra:[\"\u{1F30A} \",\
         {translate:\"bingo.team.name\",fallback:\"%s Team\",with:[{translate:\"bingo.team.blue\",fallback:\"Blue\"}]}],text:\"\"}]}"
    ));
    let join = commands.iter().find(|c| c.contains(" 16.500 24.000 -8.500 ")).unwrap();
    assert!(join.contains("billboard:\"vertical\"") && join.contains("shadow:false") && join.contains(&format!("background:{},", 0x4000_0000)));
    assert!(join.contains("text:{translate:\"bingo.lobby.click_to_join\",fallback:\"Click to join!\"}"));
    // The gold pad is the orange team with the mushroom (the dump's `[mushroom Orange Team]`), the gray one the rock.
    assert!(commands.iter().any(|c| c.contains(" -16.500 25.500 -3.500 ") && c.contains("color:\"gold\"") && c.contains("\u{1F344} ") && c.contains("bingo.team.orange")));
    assert!(commands.iter().any(|c| c.contains(" -1.500 22.500 18.500 ") && c.contains("\u{1FAA8} ") && c.contains("bingo.team.gray")));
    // The kill takes blocks, texts and interactions by the scene's tag.
    assert_eq!(lobby_scene::clear_command(), "execute in bingo:lobby run kill @e[type=!minecraft:player,tag=bingo_lobby_ui]");
}

#[test]
fn the_panels_entities_match_the_original_dump_in_place_size_and_brightness() {
    // The dump is read from the repository's docs; a checkout without it skips the comparison.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/original-ledger/panel-dump.json");
    let Ok(raw) = std::fs::read_to_string(path) else { return };
    let dump: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let r3 = |v: f64| (v * 1000.0).round() as i64;
    // (type, position and scale x in thousandths, then the text's line width or the block's sky) of every menu entity.
    type Key = (String, [i64; 3], i64, i64);
    let mut original: Vec<Key> = Vec::new();
    for e in dump.as_array().unwrap().iter().filter(|e| e["_group"] == "menu" && e["type"] != "minecraft:interaction") {
        let pos: Vec<f64> = e["Pos"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let scale = e["transformation"]["scale"][0].as_f64().unwrap();
        let extra = if e["type"] == "minecraft:block_display" { e["brightness"]["sky"].as_i64().unwrap() } else { e["line_width"].as_i64().unwrap() };
        original.push((e["type"].as_str().unwrap().to_owned(), [r3(pos[0]), r3(pos[1]), r3(pos[2])], r3(scale), extra));
    }
    let mut ours: Vec<Key> = tile_commands(&PanelState { players: 1, ..PanelState::default() })
        .iter()
        .map(|c| {
            let kind = if c.contains("block_display") { "minecraft:block_display" } else { "minecraft:text_display" };
            let p = pos_of(c);
            let scale: f64 = field(c, "scale:[").trim_end_matches('f').parse().unwrap();
            let extra = if kind == "minecraft:block_display" { field(c, "sky:").parse().unwrap() } else { field(c, "line_width:").parse().unwrap() };
            (kind.to_owned(), [r3(p[0]), r3(p[1]), r3(p[2])], r3(scale), extra)
        })
        .collect();
    original.sort();
    ours.sort();
    assert_eq!(ours.len(), original.len());
    for (a, b) in ours.iter().zip(&original) {
        assert_eq!(a, b);
    }
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

#[test]
fn the_maps_three_wall_signs_are_taken_off_the_panels_wall() {
    // The signs are at (0, 17..19, -5), where the original's menu is anchored; they would hide the tiles.
    assert_eq!(
        lobby_scene::sign_removal_commands(),
        vec![
            "execute in bingo:lobby run setblock 0 17 -5 minecraft:air".to_owned(),
            "execute in bingo:lobby run setblock 0 18 -5 minecraft:air".to_owned(),
            "execute in bingo:lobby run setblock 0 19 -5 minecraft:air".to_owned(),
        ]
    );
}

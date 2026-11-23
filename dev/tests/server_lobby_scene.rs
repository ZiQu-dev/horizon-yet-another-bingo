//! The lobby's in-world parts: which clicked block means what (the menu signs, the team pads), the displays that label
//! them, the countdown and start titles, the sounds of the lobby, and `ready` for a player on no team. Expected values are
//! written out by hand from the original's lobby POIs (`lobby_poi.json.gz`) and `CountdownService`.
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::lobby_scene::{self, Click};
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
    assert_eq!(lobby_scene::click_at([16, 23, -9]), Some(Click::Join("bingo_blue")));
    assert_eq!(lobby_scene::click_at([16, 24, -9]), Some(Click::Join("bingo_blue")));
    assert_eq!(lobby_scene::click_at([17, 23, -9]), None);
    assert_eq!(lobby_scene::click_at([16, 25, -9]), None);
    assert_eq!(lobby_scene::click_at([-17, 24, 14]), Some(Click::Join("bingo_red")));
}

#[test]
fn the_three_signs_of_the_menu_spot_open_the_menu_and_blocks_around_them_do_not() {
    assert_eq!(lobby_scene::click_at([0, 17, -5]), Some(Click::Menu));
    assert_eq!(lobby_scene::click_at([-1, 19, -6]), Some(Click::Menu));
    assert_eq!(lobby_scene::click_at([1, 18, -4]), Some(Click::Menu));
    assert_eq!(lobby_scene::click_at([2, 18, -5]), None);
    assert_eq!(lobby_scene::click_at([0, 20, -5]), None);
    assert_eq!(lobby_scene::click_at([0, 17, -3]), None);
}

#[test]
fn the_scene_summons_the_menu_text_and_two_displays_per_pad_at_the_originals_heights() {
    let commands = lobby_scene::summon_commands();
    assert_eq!(commands.len(), 1 + 2 * 8);
    assert!(commands.iter().all(|c| c.starts_with("execute in bingo:lobby run summon minecraft:text_display ")));
    assert!(commands.iter().all(|c| c.contains("Tags:[\"bingo_lobby_ui\"]")));
    // The blue pad (16, 23, -9): its name at 1.5 above the block, `Click to join!` at 1.0, both over the block's middle.
    assert!(commands.iter().any(|c| c.contains(" 16.5 24.5 -8.5 ") && c.contains("\"bingo.team.blue\"") && c.contains("color:\"blue\"")));
    assert!(commands.iter().any(|c| c.contains(" 16.5 24 -8.5 ") && c.contains("\"bingo.lobby.click_to_join\"")));
    assert!(commands[0].contains(" 0.5 20.5 -4.5 ") && commands[0].contains("bold:true") && commands[0].contains("color:\"green\""));
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

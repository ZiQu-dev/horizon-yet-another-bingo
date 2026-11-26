//! The players' map items (`map_items`): who holds which map and when it is given, removed and replaced; the preview in the
//! lobby; the memento after a game; the map ids kept for a server's life (and in `map-ids.json`); the repaint of the
//! changed squares, throttled to one repaint per 10 ticks; flashing squares; hidden squares. A fake host records the maps
//! made, the pictures painted (with the tick and the dirty rectangle), the items given and the commands queued.
//! Expected values are counted by hand: a card square at (row, column) covers the 22 x 22 rectangle that starts at
//! 5 + 24 * column, 5 + 24 * row (the map border is 5 pixels, a tile is 24 pixels apart).
use std::collections::HashMap;

use horizon_bingo::buf::{decode, encode};
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::hidden_consume::TileView;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::map_items::{card_view, want_for, MapItems, SquareState, Want, MAP_IDS_FILE};
use horizon_bingo::server::map_render::{DirtyRect, IconSource, Overlays, RgbaImage};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::game::GameStatus;
use horizon_bingo::update_card_tiles::CardTilesPacket;
use horizon_bingo::update_settings::PlayerSettings;
use serde_json::{json, Value};

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;
const DAVE: u128 = 4;
const EVE: u128 = 5;

/// The item the server takes from a player to remove their map.
const CLEAR_MAPS: &str = "clear {} minecraft:filled_map[minecraft:custom_data~{bingo_card:1b}]";

/// The map ids of the first scenario: Alice's team (red) is made first, then Bob's (blue), then the lobby preview.
const RED: i32 = 1;
const BLUE: i32 = 2;
const PREVIEW: i32 = 3;

/// The SNBT of a team card made as the server gives it (`Bingo Card (Red)`, the team's colour).
fn red_card(id: i32) -> String {
    format!(
        "{{id:\"minecraft:filled_map\",count:1,components:{{\"minecraft:map_id\":{id},\"minecraft:custom_name\":{{translate:\"bingo.card.team_card\",with:[\"Red\"],color:\"red\",italic:1b}},\"minecraft:map_color\":16733525,\"minecraft:custom_data\":{{bingo_card:1b}},\"minecraft:tooltip_display\":{{hide_additional_tooltip:1b}}}}}}"
    )
}

fn blue_card(id: i32) -> String {
    format!(
        "{{id:\"minecraft:filled_map\",count:1,components:{{\"minecraft:map_id\":{id},\"minecraft:custom_name\":{{translate:\"bingo.card.team_card\",with:[\"Blue\"],color:\"blue\",italic:1b}},\"minecraft:map_color\":5592575,\"minecraft:custom_data\":{{bingo_card:1b}},\"minecraft:tooltip_display\":{{hide_additional_tooltip:1b}}}}}}"
    )
}

fn preview_card(id: i32) -> String {
    format!(
        "{{id:\"minecraft:filled_map\",count:1,components:{{\"minecraft:map_id\":{id},\"minecraft:custom_name\":{{translate:\"bingo.card.preview_card\",italic:1b}},\"minecraft:custom_data\":{{bingo_card:1b}},\"minecraft:tooltip_display\":{{hide_additional_tooltip:1b}}}}}}"
    )
}

fn blue_memento_lost(id: i32) -> String {
    format!(
        "{{id:\"minecraft:filled_map\",count:1,components:{{\"minecraft:map_id\":{id},\"minecraft:custom_name\":{{translate:\"bingo.card.team_card_memento_lost\",color:\"blue\",italic:1b}},\"minecraft:map_color\":5592575,\"minecraft:custom_data\":{{bingo_card:1b}},\"minecraft:tooltip_display\":{{hide_additional_tooltip:1b}}}}}}"
    )
}

fn red_memento_lost(id: i32) -> String {
    format!(
        "{{id:\"minecraft:filled_map\",count:1,components:{{\"minecraft:map_id\":{id},\"minecraft:custom_name\":{{translate:\"bingo.card.team_card_memento_lost\",color:\"red\",italic:1b}},\"minecraft:map_color\":16733525,\"minecraft:custom_data\":{{bingo_card:1b}},\"minecraft:tooltip_display\":{{hide_additional_tooltip:1b}}}}}}"
    )
}

#[derive(Debug, Clone)]
struct Sent {
    player: u128,
    path: String,
    payload: Vec<u8>,
}

/// The fake server: players, the console commands, the files of the data folder, and the maps. A map's picture is kept
/// by id, as the world keeps it, so a second host made from this one's maps is a restarted server.
#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<Sent>,
    commands: Vec<String>,
    files: HashMap<String, String>,
    /// The tick the test is at: `step` counts it up before each session tick, as the session counts its own ticks.
    clock: u64,
    next_id: i32,
    /// The picture of each map, by id.
    pictures: HashMap<i32, Vec<u8>>,
    /// Ids of the maps made, in order.
    made: Vec<i32>,
    /// Repaints as `(tick, map id, dirty rectangle)`.
    paints: Vec<(u64, i32, DirtyRect)>,
    /// Pictures sent to a player: `(player, map id)`.
    map_sends: Vec<(u128, i32)>,
    /// Items given: `(player, SNBT)`.
    gives: Vec<(u128, String)>,
}

impl FakeHost {
    fn take_sent(&mut self) -> Vec<Sent> {
        std::mem::take(&mut self.sent)
    }

    fn take_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.commands)
    }

    fn take_gives(&mut self) -> Vec<(u128, String)> {
        std::mem::take(&mut self.gives)
    }

    fn paints_of(&self, id: i32) -> Vec<(u64, DirtyRect)> {
        self.paints.iter().filter(|p| p.1 == id).map(|p| (p.0, p.2)).collect()
    }

    fn picture(&self, id: i32) -> &[u8] {
        &self.pictures[&id]
    }
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push(Sent { player, path: path.to_string(), payload });
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
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

    fn new_map(&mut self, pixels: &[u8]) -> Option<i32> {
        self.next_id += 1;
        self.pictures.insert(self.next_id, pixels.to_vec());
        self.made.push(self.next_id);
        Some(self.next_id)
    }

    fn paint_map(&mut self, map_id: i32, pixels: &[u8], dirty: DirtyRect) -> bool {
        if !self.pictures.contains_key(&map_id) {
            return false;
        }
        self.pictures.insert(map_id, pixels.to_vec());
        self.paints.push((self.clock, map_id, dirty));
        true
    }

    fn send_map(&mut self, player: u128, map_id: i32, _pixels: &[u8]) -> bool {
        self.map_sends.push((player, map_id));
        true
    }

    fn give_map(&mut self, player: u128, snbt: &str) -> bool {
        self.gives.push((player, snbt.to_string()));
        true
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(seed: u64) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Has each listed player join the server, and puts each one on the team `join` names. Drains the commands, so a test
/// starts from a known point.
fn lobby(host: &mut FakeHost, session: &mut Session, online: &[(u128, &str)], teams: &[(u128, &str)]) {
    host.online = online.iter().map(|&(uuid, name)| player(uuid, name)).collect();
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    for &(uuid, team) in teams {
        let answer = run(session, host, Some(uuid), &format!("join {team}"));
        assert!(answer.starts_with("Moved player"), "joining {team}: {answer}");
    }
    host.take_commands();
}

/// One server tick: the clock of the host moves with the session's own count.
fn step(session: &mut Session, host: &mut FakeHost) {
    host.clock += 1;
    session.tick(host);
}

fn steps(session: &mut Session, host: &mut FakeHost, n: u64) {
    for _ in 0..n {
        step(session, host);
    }
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

/// The 25 squares' items of the card a two-team round of `seed` plays, as a player sees it with hidden items off.
fn card_names(seed: u64) -> Vec<String> {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(seed));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    let sent = host.take_sent();
    let payload = sent
        .iter()
        .find(|s| s.player == ALICE && s.path == "update_card_tiles")
        .expect("the card is sent at the start")
        .payload
        .clone();
    let tiles = decode(&payload, CardTilesPacket::read).expect("update_card_tiles decodes");
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every square has an item")).collect()
}

/// The 22 x 22 rectangle of the square at `row`, `column`.
fn square_rect(row: usize, column: usize) -> DirtyRect {
    let x0 = 5 + 24 * column;
    let y0 = 5 + 24 * row;
    DirtyRect { x0, y0, x1: x0 + 22, y1: y0 + 22 }
}

/// The map colour at the middle of a square.
fn centre(picture: &[u8], row: usize, column: usize) -> u8 {
    let x = 5 + 24 * column + 11;
    let y = 5 + 24 * row + 11;
    picture[y * 128 + x]
}

/// Every square drawn as the same picture: a solid icon, so a shown square is not the background.
struct SolidIcons;

impl IconSource for SolidIcons {
    fn icon(&self, _item: &str) -> Option<RgbaImage> {
        Some(RgbaImage::from_rgba(16, 16, [255u8, 0, 0, 255].repeat(16 * 16)))
    }
}

// ---- Pure rules ----

#[test]
fn who_holds_what_depends_on_the_phase_the_team_and_the_hud() {
    let red = Some("bingo_red");
    let team = |key: &str| Some(Want::Team { key: key.to_string() });
    let memento = |won: bool| Some(Want::Memento { key: "bingo_red".to_string(), won });

    // Before the game, a player on a team holds the team's card, and one on no team the preview.
    assert_eq!(want_for(GameStatus::Pregame, red, false, false, false), team("bingo_red"));
    assert_eq!(want_for(GameStatus::Pregame, None, false, false, false), Some(Want::Preview));
    // The countdown and the game: the team's card; a player on no team has none.
    assert_eq!(want_for(GameStatus::Starting, red, false, false, false), team("bingo_red"));
    assert_eq!(want_for(GameStatus::Playing, red, false, false, false), team("bingo_red"));
    assert_eq!(want_for(GameStatus::Playing, None, false, false, false), None);
    // After the game, the memento, with the result.
    assert_eq!(want_for(GameStatus::Postgame, red, false, false, true), memento(true));
    assert_eq!(want_for(GameStatus::Postgame, red, false, false, false), memento(false));
    // A spectator and a player with the client's HUD get none.
    assert_eq!(want_for(GameStatus::Playing, red, true, false, false), None);
    assert_eq!(want_for(GameStatus::Playing, red, false, true, false), None);
    assert_eq!(want_for(GameStatus::Pregame, None, false, true, false), None);
}

#[test]
fn an_item_is_written_as_vanilla_reads_it_with_its_name_colour_and_marker() {
    let red = Want::Team { key: "bingo_red".to_string() };
    assert_eq!(horizon_bingo::server::map_items::item_snbt(1, &red), red_card(1));
    let blue = Want::Memento { key: "bingo_blue".to_string(), won: false };
    assert_eq!(
        horizon_bingo::server::map_items::item_snbt(2, &blue),
        blue_memento_lost(2)
    );
    assert_eq!(horizon_bingo::server::map_items::item_snbt(3, &Want::Preview), preview_card(3));
}

#[test]
fn map_ids_come_back_from_their_file_and_a_damaged_file_gives_none() {
    let mut maps = MapItems::new();
    maps.load_ids(r#"{"bingo_red": 3, "bingo_blue": 9, "preview": 7}"#);
    assert_eq!(maps.id_of("bingo_blue"), Some(9));

    let text = maps.ids_json();
    let mut again = MapItems::new();
    again.load_ids(&text);
    assert_eq!(again.id_of("bingo_red"), Some(3));
    assert_eq!(again.id_of("preview"), Some(7));
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), json!({"bingo_red": 3, "bingo_blue": 9, "preview": 7}));
    assert!(again.take_ids_to_save().is_none(), "loading the ids changes nothing to save");

    let mut damaged = MapItems::new();
    damaged.load_ids("[1, 2]");
    assert_eq!(damaged.id_of("bingo_red"), None);
}

#[test]
fn a_hidden_square_shows_no_item_and_a_square_another_team_took_is_locked_in_lockout() {
    let views = vec![
        TileView { index: 0, hidden: true, item: None, tier: Some(TierLabel::A) },
        TileView { index: 1, hidden: false, item: Some("minecraft:stone".to_string()), tier: Some(TierLabel::B) },
    ];
    let squares = vec![
        SquareState::default(),
        SquareState { found: false, achieved_by: vec!["RED".to_string()], flash_since: None },
    ];
    let view = card_view(Some("BLUE"), &views, &squares, true);
    assert_eq!(view.team_color.as_deref(), Some("BLUE"));
    assert!(view.tiles[0].hidden);
    assert_eq!(view.tiles[0].tier, Some(TierLabel::A));
    assert_eq!(view.tiles[0].item, None);
    assert!(!view.tiles[1].hidden);
    assert!(view.tiles[1].locked, "another team has it and this one does not");
    assert!(!view.tiles[1].achieved);

    let open = card_view(Some("BLUE"), &views, &squares, false);
    assert!(!open.tiles[1].locked, "no lock outside lockout");
}

// ---- Session: who is given what, and when ----

/// Two teams, a spectator, a player with the HUD on a team, and a player on no team, in the lobby. The first tick gives
/// each team's card to its members (in the lobby too, as the original does) and the preview to the player on no team.
fn lobby_with_everyone(seed: u64) -> (FakeHost, Session) {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(seed));
    lobby(
        &mut host,
        &mut session,
        &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol"), (DAVE, "Dave"), (EVE, "Eve")],
        &[(ALICE, "red"), (BOB, "blue"), (DAVE, "green")],
    );
    run(&mut session, &mut host, Some(CAROL), "spectator");
    let settings = encode(|w| PlayerSettings::default().write(w));
    session.on_packet(&mut host, DAVE, "update_settings", &settings);
    host.take_commands();
    (host, session)
}

#[test]
fn players_without_the_hud_get_their_team_card_or_the_preview_and_nobody_else_does() {
    let (mut host, mut session) = lobby_with_everyone(7);
    step(&mut session, &mut host);

    assert_eq!(host.made, vec![RED, BLUE, PREVIEW], "one map per team and one for the preview, made in order");
    assert_eq!(
        host.take_gives(),
        vec![(ALICE, red_card(RED)), (BOB, blue_card(BLUE)), (EVE, preview_card(PREVIEW))],
        "Dave runs the HUD and Carol watches: neither gets a map"
    );
    assert!(host.map_sends.contains(&(ALICE, RED)), "the picture is sent to the player it is given to");
}

#[test]
fn the_round_removes_the_preview_from_a_player_who_is_not_playing_and_the_card_stays_with_the_team() {
    let (mut host, mut session) = lobby_with_everyone(7);
    steps(&mut session, &mut host, 1);
    host.take_gives();
    host.take_commands();

    // Eve becomes a spectator at the start: she has no preview any more. Alice and Bob keep their cards.
    run(&mut session, &mut host, Some(ALICE), "start ignore_warnings");
    step(&mut session, &mut host);

    assert!(host.take_gives().is_empty(), "no team card is given again: the team already holds it");
    let commands = host.take_commands();
    assert!(commands.contains(&CLEAR_MAPS.replace("{}", "Eve")), "Eve's preview is taken: {commands:?}");
    assert!(!commands.iter().any(|c| c.starts_with("clear Alice") || c.starts_with("clear Bob")));
}

#[test]
fn after_the_game_each_team_card_is_taken_and_the_memento_with_the_result_is_given() {
    let (mut host, mut session) = lobby_with_everyone(7);
    steps(&mut session, &mut host, 1);
    run(&mut session, &mut host, Some(ALICE), "start ignore_warnings");
    steps(&mut session, &mut host, 1);
    host.take_gives();
    host.take_commands();

    // Nobody wins: the game ends with `end`. The team cards are taken first, the memento comes on the next tick.
    run(&mut session, &mut host, Some(ALICE), "end");
    step(&mut session, &mut host);
    let commands = host.take_commands();
    assert!(commands.contains(&CLEAR_MAPS.replace("{}", "Alice")), "{commands:?}");
    assert!(commands.contains(&CLEAR_MAPS.replace("{}", "Bob")), "{commands:?}");
    assert!(host.take_gives().is_empty(), "the memento is given after the card is gone");

    step(&mut session, &mut host);
    let blue_memento = blue_memento_lost(BLUE);
    assert_eq!(host.take_gives(), vec![(ALICE, red_memento_lost(RED)), (BOB, blue_memento)]);
}

#[test]
fn a_player_who_drops_their_map_gets_it_back_on_the_next_tick_with_the_same_id() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    step(&mut session, &mut host);
    assert_eq!(host.take_gives(), vec![(ALICE, red_card(RED))]);

    // The inventory no longer holds a filled map: the server gives one back.
    session.on_inventory(&mut host, ALICE, &HashMap::new());
    step(&mut session, &mut host);
    assert_eq!(host.take_gives(), vec![(ALICE, red_card(RED))]);
    assert_eq!(host.made, vec![RED], "the map is not made again");
}

#[test]
fn a_map_id_is_kept_across_rounds_and_saved_so_a_restarted_server_reuses_it() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    step(&mut session, &mut host);
    run(&mut session, &mut host, Some(ALICE), "start");
    steps(&mut session, &mut host, 1);
    run(&mut session, &mut host, Some(ALICE), "end");
    steps(&mut session, &mut host, 2);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "reset"), "Resetting the game...");
    steps(&mut session, &mut host, 2);
    host.take_gives();
    assert_eq!(host.made, vec![RED, BLUE], "no map is made for the second round");

    let saved: Value = serde_json::from_str(&host.files[MAP_IDS_FILE]).expect("map-ids.json is JSON");
    assert_eq!(saved, json!({"bingo_red": RED, "bingo_blue": BLUE}));

    // A new server on the same files: the teams' maps are painted again under their saved ids, nothing is made.
    let mut restarted = FakeHost {
        online: vec![player(ALICE, "Alice")],
        files: host.files.clone(),
        pictures: host.pictures.clone(),
        next_id: host.next_id,
        ..FakeHost::default()
    };
    let mut again = Session::new(options(7));
    again.on_join(&mut restarted, &player(ALICE, "Alice"));
    run(&mut again, &mut restarted, Some(ALICE), "join red");
    step(&mut again, &mut restarted);
    let first = restarted.take_commands();
    assert!(first.contains(&CLEAR_MAPS.replace("{}", "Alice")), "a map from before the restart is taken first: {first:?}");
    step(&mut again, &mut restarted);
    assert_eq!(restarted.take_gives(), vec![(ALICE, red_card(RED))]);
    assert!(restarted.made.is_empty(), "the saved id is painted, not made again");
    assert_eq!(restarted.paints_of(RED).first().map(|p| p.1), Some(DirtyRect::FULL), "a restarted map is painted whole");
}

// ---- Session: the picture ----

/// Alice (red) and Bob (blue) are playing; the first tick has made both maps. Returns the names of the card's squares.
fn playing(seed: u64, hidden_items: bool) -> (FakeHost, Session, Vec<String>) {
    let names = card_names(seed);
    let mut host = FakeHost::default();
    let mut session = Session::new(options(seed));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    if hidden_items {
        run(&mut session, &mut host, Some(ALICE), "mode hidden_items on");
    }
    run(&mut session, &mut host, Some(ALICE), "start");
    step(&mut session, &mut host);
    assert_eq!(host.made, vec![RED, BLUE]);
    host.take_gives();
    (host, session, names)
}

#[test]
fn a_scored_square_flashes_on_the_team_map_every_10_ticks_for_80_ticks_and_only_its_rectangle_is_painted() {
    let (mut host, mut session, names) = playing(7, false);
    steps(&mut session, &mut host, 1); // tick 2
    session.on_inventory(&mut host, ALICE, &held(&names[0..1])); // scored at tick 2, square (0, 0)
    steps(&mut session, &mut host, 130);

    // The first repaint is at the next 10-tick mark after the first tick, 11: the square turns to its team colour, then
    // it flashes on each 10-tick beat until 80 ticks have passed (the last beat that still changes is 91).
    let beats: Vec<u64> = host.paints_of(RED).iter().map(|p| p.0).collect();
    assert_eq!(beats, vec![11, 21, 31, 41, 51, 61, 71, 81, 91]);
    assert!(host.paints_of(RED).iter().all(|p| p.1 == square_rect(0, 0)), "only the scored square is painted");
    // Blue's map shows red's find as a stripe on that square, once; it does not flash.
    assert_eq!(host.paints_of(BLUE), vec![(11, square_rect(0, 0))]);
}

#[test]
fn repaints_are_throttled_to_one_per_beat_and_one_paint_covers_every_square_that_changed() {
    let (mut host, mut session, names) = playing(7, false);
    steps(&mut session, &mut host, 1); // tick 2
    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    steps(&mut session, &mut host, 3); // tick 5: a second square is scored inside the same beat
    session.on_inventory(&mut host, ALICE, &held(&[names[0].clone(), names[6].clone()]));
    steps(&mut session, &mut host, 10); // tick 15

    // Both squares changed in the beat after 2 and 5: one repaint at tick 11, with the rectangle of both.
    let early = host.paints_of(RED).into_iter().filter(|p| p.0 <= 15).collect::<Vec<_>>();
    assert_eq!(early.len(), 1, "one repaint in the beat: {early:?}");
    assert_eq!(early[0].0, 11);
    assert_eq!(early[0].1, DirtyRect { x0: 5, y0: 5, x1: 51, y1: 51 }, "the rectangle of squares (0,0) and (1,1)");
}

#[test]
fn with_hidden_items_a_square_shows_its_picture_only_once_the_team_has_found_it() {
    let (mut host, mut session, names) = playing(7, true);
    // Icons are drawn from now on: a found square shows its picture, a hidden one never does.
    session.set_map_assets(Box::new(SolidIcons), Overlays::default());
    steps(&mut session, &mut host, 1);
    let hidden = host.picture(RED).to_vec();
    assert_eq!(centre(&hidden, 0, 3), centre(&hidden, 0, 4), "two hidden squares look the same");
    let background = centre(&hidden, 0, 3);

    session.on_inventory(&mut host, ALICE, &held(&names[3..4]));
    steps(&mut session, &mut host, 10);
    let found = host.paints_of(RED);
    assert_eq!(found.len(), 1, "the find is painted once, at the next beat: {found:?}");
    assert_eq!(found[0].1, square_rect(0, 3), "only the found square's rectangle is painted");
    let picture = host.picture(RED);
    assert_ne!(centre(picture, 0, 3), background, "the found square shows its picture");
    assert_eq!(centre(picture, 0, 4), background, "a square not found stays hidden");
}

#[test]
fn a_player_on_no_team_alone_gets_the_preview_map() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session, &[(EVE, "Eve")], &[]);
    step(&mut session, &mut host);
    assert_eq!(host.take_gives(), vec![(EVE, preview_card(RED))], "the first map made is the preview");
    assert_eq!(host.made, vec![RED]);
}

//! Session: joins, the lobby (ready screen, start rules, team commands), the countdown, the card, scoring, chat, the end
//! of the game and commands, driven through a fake host that records every packet. Packets are decoded with the
//! client's own readers. Expected values are counted by hand: 20 ticks per second, a card of 25 squares, a 5-square row
//! is one line, and the ready timer lasts 600 s, shortened by the share of ready players.
use std::collections::{HashMap, HashSet};

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::display_cards::{CardDisplayPacket, PlayerProfile};
use horizon_bingo::game_over::{GameOverPacket, ScoreRanking};
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};
use horizon_bingo::open_card::CardOpenPacket;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::score_update::{ScoredItemPacket, TeamScore};
use horizon_bingo::server::card_gen::{generate_card, load_pool, DEFAULT_DISTRIBUTION};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::{Card, Player, CARD_SQUARES};
use horizon_bingo::text::Text;
use horizon_bingo::timer::{CountdownPacket, TimerPacket};
use horizon_bingo::update_card_tiles::CardTilesPacket;

const TIER_LIST: &str = horizon_bingo::data::items_tierlist_json::JSON;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

const READY_V1: [u8; 1] = [1];
const READY_V2: [u8; 2] = [1, 0];

#[derive(Debug, Clone)]
struct Sent {
    player: u128,
    path: String,
    payload: Vec<u8>,
}

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<Sent>,
    broadcasts: Vec<String>,
    commands: Vec<String>,
}

impl FakeHost {
    fn take(&mut self) -> Vec<Sent> {
        std::mem::take(&mut self.sent)
    }

    fn take_broadcasts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.broadcasts)
    }

    fn take_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.commands)
    }
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push(Sent { player, path: path.to_string(), payload });
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, line: &str) {
        self.broadcasts.push(line.to_string());
    }

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    /// The id of an item is the length of its name, so a test can predict it.
    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn command(&mut self, line: &str) {
        self.commands.push(line.to_string());
    }
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(seed: u64, countdown_secs: i32, time_limit_secs: Option<i32>) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs, time_limit_secs },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

/// Runs a command as `player` (or the console) and returns the answer.
fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Sets who is online, has each join the server, and puts each listed player on a team (`team` is a word for
/// `join <team>`). Drains everything sent so far, so a test starts from a known point.
fn lobby(host: &mut FakeHost, session: &mut Session, online: &[(u128, &str)], teams: &[(u128, &str)]) {
    host.online = online.iter().map(|&(uuid, name)| player(uuid, name)).collect();
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    for &(uuid, team) in teams {
        let answer = run(session, host, Some(uuid), &format!("join {team}"));
        assert!(answer.starts_with("Moved player"), "joining {team}: {answer}");
    }
    host.take();
    host.take_commands();
}

fn count(sent: &[Sent], player: u128, path: &str) -> usize {
    sent.iter().filter(|s| s.player == player && s.path == path).count()
}

fn payloads(sent: &[Sent], player: u128, path: &str) -> Vec<Vec<u8>> {
    sent.iter().filter(|s| s.player == player && s.path == path).map(|s| s.payload.clone()).collect()
}

/// The one payload of this kind sent to this player.
fn only(sent: &[Sent], player: u128, path: &str) -> Vec<u8> {
    let found = payloads(sent, player, path);
    assert_eq!(found.len(), 1, "one {path} for player {player}");
    found[0].clone()
}

fn tiles_of(payload: &[u8]) -> CardTilesPacket {
    decode(payload, CardTilesPacket::read).expect("update_card_tiles decodes")
}

fn score_of(payload: &[u8]) -> ScoredItemPacket {
    decode(payload, ScoredItemPacket::read).expect("score_update decodes")
}

fn status_of(payload: &[u8]) -> GameStatusPacket {
    decode(payload, GameStatusPacket::read).expect("game_status decodes")
}

fn seconds_of(payload: &[u8]) -> i32 {
    decode(payload, CountdownPacket::read).expect("countdown decodes").seconds_remaining
}

fn ready_of(payload: &[u8]) -> ReadyUpdatePacket {
    decode(payload, ReadyUpdatePacket::read_v2).expect("ready_timer_update_v2 decodes")
}

fn names_of(tiles: &CardTilesPacket) -> Vec<String> {
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

/// The item ids of a tier list tier, minus its tags.
fn tier_items(tier: &str) -> HashSet<String> {
    let root: serde_json::Value = serde_json::from_str(TIER_LIST).expect("tier list is JSON");
    root[tier]
        .as_array()
        .expect("tier is an array")
        .iter()
        .map(|entry| entry.as_str().expect("entry is a string").to_string())
        .filter(|id| !id.starts_with('#'))
        .collect()
}

/// A card as the server sends it before anything is found: 25 tiles, 9 of tier C and 16 of tier D (the default
/// distribution), every item from its tier's list and no item twice, nothing achieved. Returns the item names.
fn check_fresh_card(tiles: &CardTilesPacket, team: &str) -> Vec<String> {
    assert_eq!(tiles.team_key.as_deref(), Some(team));
    assert!(!tiles.should_notify, "the first card is not announced");
    assert_eq!(tiles.tiles.len(), CARD_SQUARES);
    let (c, d) = (tier_items("C"), tier_items("D"));
    let (mut c_count, mut d_count) = (0, 0);
    for (position, (index, tile)) in tiles.tiles.iter().enumerate() {
        assert_eq!(*index, position as i32);
        let name = tile.id.clone().expect("every tile has an item");
        let list = match tile.item_tier {
            Some(TierLabel::C) => {
                c_count += 1;
                &c
            }
            Some(TierLabel::D) => {
                d_count += 1;
                &d
            }
            other => panic!("tile {position} has tier {other:?}, expected C or D"),
        };
        assert!(list.contains(&name), "{name} is not in its tier's list");
        assert_eq!(tile.image.item.as_ref().map(|stack| stack.item), Some(name.len() as i32));
        assert!(!tile.image.is_achieved && tile.progress == 0.0 && tile.team_keys.is_empty());
    }
    assert_eq!((c_count, d_count), (9, 16));
    let names = names_of(tiles);
    assert_eq!(names.iter().collect::<HashSet<_>>().len(), CARD_SQUARES, "no item twice");
    names
}

/// A ranking row of one team in a game-over packet.
fn ranking(index: i32, key: &str, name: &str, items: i32, lines: i32, cards: i32, duration: &str) -> ScoreRanking {
    ScoreRanking {
        index,
        key: key.to_string(),
        name: Text::plain(name),
        score: TeamScore { items, lines, cards },
        duration: Some(duration.to_string()),
    }
}

fn names_of_generated(card: &Card) -> Vec<String> {
    card.squares.iter().map(|square| square.item.clone()).collect()
}

/// The card for the first team, with the names of its squares. The game must be Playing.
fn card_names(host: &mut FakeHost, player: u128) -> Vec<String> {
    let tiles = tiles_of(&only(&host.take(), player, "update_card_tiles"));
    names_of(&tiles)
}

#[test]
fn joining_before_play_gets_the_game_status_and_the_ready_screen() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    host.online = vec![player(ALICE, "Alice")];
    session.on_join(&mut host, &player(ALICE, "Alice"));

    let sent = host.take();
    assert_eq!(sent.len(), 2, "the game status and the ready screen");
    assert_eq!(count(&sent, ALICE, "game_status"), 1);
    let status = status_of(&only(&sent, ALICE, "game_status"));
    assert_eq!(status.ingame_duration, Some(JavaDuration { seconds: 0, nanos: 0 }));
    assert_eq!(status.remaining_duration, None);

    let ready = ready_of(&only(&sent, ALICE, "ready_timer_update_v2"));
    assert!(!ready.is_running && !ready.is_ready);
    assert_eq!((ready.ready_players, ready.total_players), (0, 1));
}

#[test]
fn full_game_from_start_to_game_over() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert_eq!(
        host.take_commands(),
        [
            "gamerule pvp true",
            "gamerule keep_inventory false",
            "team add bingo_red",
            "team modify bingo_red color red",
            "team join bingo_red Alice",
            "team join bingo_red Bob",
            "gamemode adventure Alice",
            "effect give Alice minecraft:slowness 3 255 true",
            "effect give Alice minecraft:jump_boost 3 128 true",
            "gamemode adventure Bob",
            "effect give Bob minecraft:slowness 3 255 true",
            "effect give Bob minecraft:jump_boost 3 128 true",
        ]
    );
    let start = host.take();
    assert_eq!(count(&start, ALICE, "game_status"), 1);
    assert_eq!(count(&start, BOB, "game_status"), 1);
    assert_eq!(count(&start, ALICE, "countdown"), 1);
    assert_eq!(seconds_of(&only(&start, ALICE, "countdown")), 3);
    assert_eq!(start.len(), 4);
    assert!(run(&mut session, &mut host, None, "status").contains("Starting"));

    // Ticks 1 to 59: the countdown shows 2 at tick 20 and 1 at tick 40; the game is still Starting.
    let mut countdown_values = Vec::new();
    for _ in 0..59 {
        session.tick(&mut host);
        for sent in host.take() {
            if sent.path == "countdown" && sent.player == ALICE {
                countdown_values.push(seconds_of(&sent.payload));
            }
        }
    }
    assert_eq!(countdown_values, vec![2, 1]);

    // Tick 60 begins play: countdown 0, the game status, and the card for each player.
    session.tick(&mut host);
    let begin = host.take();
    assert_eq!(seconds_of(&only(&begin, ALICE, "countdown")), 0);
    assert_eq!(status_of(&only(&begin, ALICE, "game_status")).ingame_duration, Some(JavaDuration { seconds: 0, nanos: 0 }));
    for who in [ALICE, BOB] {
        assert_eq!(count(&begin, who, "display_cards"), 1);
        assert_eq!(count(&begin, who, "update_card_tiles"), 1);
        assert_eq!(count(&begin, who, "open_card"), 1);
        assert_eq!(count(&begin, who, "score_update"), 1);
    }
    assert_eq!(begin.len(), 12, "6 packets for each of 2 players");

    let display = decode(&only(&begin, ALICE, "display_cards"), CardDisplayPacket::read).unwrap();
    assert_eq!(display.display.len(), 1);
    assert_eq!(display.display[0].0.as_deref(), Some("bingo_red"));
    let team = &display.display[0].1;
    assert_eq!(team.team_color.as_deref(), Some("RED"));
    assert_eq!(team.team_name, Some(Text::plain("Red")));
    assert_eq!(
        team.players,
        vec![
            PlayerProfile { uuid: ALICE, name: "Alice".to_string() },
            PlayerProfile { uuid: BOB, name: "Bob".to_string() },
        ]
    );

    let card_for_alice = only(&begin, ALICE, "update_card_tiles");
    let names = check_fresh_card(&tiles_of(&card_for_alice), "bingo_red");
    assert_eq!(only(&begin, BOB, "update_card_tiles"), card_for_alice, "both see the same card");

    let open = decode(&only(&begin, ALICE, "open_card"), CardOpenPacket::read).unwrap();
    assert_eq!(open.team_key.as_deref(), Some("bingo_red"));
    assert_eq!(score_of(&only(&begin, ALICE, "score_update")).score, TeamScore { items: 0, lines: 0, cards: 0 });

    // 100 ticks of play, then Alice holds the whole top row: one line, so the game ends.
    for _ in 0..100 {
        session.tick(&mut host);
    }
    assert!(host.take().is_empty(), "nothing happens on plain ticks during play");

    let row: Vec<String> = names[0..5].to_vec();
    let mut held: HashMap<String, u32> = row.iter().map(|name| (name.clone(), 1)).collect();
    held.insert("unlisted:held_item".to_string(), 3);
    session.on_inventory(&mut host, ALICE, &held);

    let finish = host.take();
    let broadcasts = host.take_broadcasts();
    assert_eq!(count(&finish, ALICE, "update_card_tiles"), 1);
    assert_eq!(count(&finish, BOB, "update_card_tiles"), 1);
    assert!(tiles_of(&only(&finish, ALICE, "update_card_tiles")).should_notify, "the find is announced");
    assert_eq!(count(&finish, ALICE, "score_update"), 1);
    assert_eq!(count(&finish, BOB, "score_update"), 1);
    let score = score_of(&only(&finish, BOB, "score_update"));
    assert_eq!(score.score, TeamScore { items: 5, lines: 1, cards: 1 });
    assert!(score.is_viewer_on_team);

    let expected_lines: Vec<String> = row
        .iter()
        .map(|name| format!("Alice got [{}]", name.strip_prefix("minecraft:").unwrap_or(name)))
        .chain(std::iter::once("Red won the game!".to_string()))
        .collect();
    assert_eq!(broadcasts, expected_lines);

    // Postgame for both, and the end screen: the team won, its ranking is the one row, elapsed 100 ticks = 5 s.
    assert_eq!(count(&finish, ALICE, "game_status"), 1);
    assert_eq!(count(&finish, BOB, "game_status"), 1);
    assert_eq!(status_of(&only(&finish, BOB, "game_status")).ingame_duration, Some(JavaDuration { seconds: 5, nanos: 0 }));
    for who in [ALICE, BOB] {
        assert_eq!(count(&finish, who, "game_over_v2"), 1);
        let over = decode(&only(&finish, who, "game_over_v2"), GameOverPacket::read_v2).unwrap();
        assert_eq!(over.winner.as_deref(), Some("bingo_red"));
        assert!(over.is_winner);
        assert!(!over.is_update, "the end screen opens");
        assert_eq!(over.scores, vec![ranking(0, "bingo_red", "Red", 5, 1, 1, "PT5S")]);
    }
    assert_eq!(finish.len(), 8, "tiles, score, game status and end screen for each of 2 players");
    assert!(run(&mut session, &mut host, None, "status").contains("Postgame"));

    // After the end, nothing more: ticks and further finds are silent and nothing is announced.
    session.tick(&mut host);
    session.on_inventory(&mut host, BOB, &HashMap::from([(names[5].clone(), 1)]));
    assert!(host.take().is_empty());
    assert!(host.take_broadcasts().is_empty());
}

#[test]
fn both_ready_players_start_the_game_on_the_second_tick() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);

    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    session.on_packet(&mut host, BOB, "set_ready_v2", &READY_V2);
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"), "nothing starts before a tick");

    // The first tick starts the timer, which is empty at once, so the second tick starts the game.
    session.tick(&mut host);
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    session.tick(&mut host);
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"));
}

#[test]
fn a_player_who_leaves_before_play_loses_their_ready_flag_and_their_team() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    host.take();

    host.online = vec![player(BOB, "Bob")];
    session.on_leave(&mut host, ALICE);
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    session.on_join(&mut host, &player(ALICE, "Alice"));
    let back = host.take();
    assert!(!ready_of(&only(&back, ALICE, "ready_timer_update_v2")).is_ready, "Alice came back without being ready");

    // Alice left her team in the lobby, so one player is on no team and the start is refused.
    let refused = run(&mut session, &mut host, Some(BOB), "start");
    assert!(refused.starts_with("1 player(s) are not on a team"), "{refused}");
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
}

#[test]
fn nobody_online_and_only_offline_ready_players_never_start_the_game() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert_eq!(run(&mut session, &mut host, None, "start"), "Nobody to play.");
    for _ in 0..30 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"), "no player online");

    // Bob is on a team and online, but Alice, who is ready, is offline: the ready count does not reach the timer.
    host.online = vec![player(BOB, "Bob")];
    run(&mut session, &mut host, Some(BOB), "join red");
    host.take();
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    for _ in 0..30 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"), "Alice is offline");
    assert!(host.take().iter().all(|s| s.path == "ready_timer_update_v2"));
}

#[test]
fn malformed_ready_payloads_and_unknown_channels_are_ignored() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);

    session.on_packet(&mut host, ALICE, "set_ready", &[]);
    session.on_packet(&mut host, ALICE, "set_ready_v2", &[1]);
    session.on_packet(&mut host, ALICE, "set_ready_v2", &[1, 0, 0]);
    session.on_packet(&mut host, ALICE, "set_ready", &[1, 0]);
    session.on_packet(&mut host, ALICE, "set_ready_v3", &READY_V1);
    session.on_packet(&mut host, ALICE, "game_status", &READY_V1);
    assert!(host.take().is_empty(), "nothing is sent for ignored packets");

    // None of the above counted: after ten ticks the ready screen shows nobody ready.
    for _ in 0..10 {
        session.tick(&mut host);
    }
    let screen = ready_of(&only(&host.take(), ALICE, "ready_timer_update_v2"));
    assert_eq!(screen.ready_players, 0);
    // The countdown runs even so: the original does not wait for a first vote by default.

    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert_eq!(ready_of(&only(&host.take(), ALICE, "ready_timer_update_v2")).ready_players, 1);
}

#[test]
fn a_line_needs_all_five_squares_and_only_then_ends_the_game() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    host.online = vec![player(ALICE, "Alice")];
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);

    // Four squares of the first row: a line is not complete yet.
    let held: HashMap<String, u32> = names[0..4].iter().map(|name| (name.clone(), 1)).collect();
    session.on_inventory(&mut host, ALICE, &held);
    let sent = host.take();
    assert_eq!(score_of(&only(&sent, ALICE, "score_update")).score, TeamScore { items: 4, lines: 0, cards: 0 });
    assert_eq!(count(&sent, ALICE, "game_over_v2"), 0);
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"));

    session.on_inventory(&mut host, ALICE, &HashMap::from([(names[4].clone(), 1)]));
    assert_eq!(count(&host.take(), ALICE, "game_over_v2"), 1);
    assert!(run(&mut session, &mut host, None, "status").contains("Postgame"));
}

#[test]
fn zero_counts_repeat_finds_and_items_off_the_card_score_nothing() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    host.online = vec![player(ALICE, "Alice")];
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);

    session.on_inventory(&mut host, ALICE, &HashMap::from([(names[0].clone(), 0), ("unlisted:x".to_string(), 9)]));
    assert!(host.take().is_empty());
    assert!(host.take_broadcasts().is_empty());

    let held = HashMap::from([(names[0].clone(), 1)]);
    session.on_inventory(&mut host, ALICE, &held);
    assert_eq!(host.take_broadcasts().len(), 1);
    host.take();
    session.on_inventory(&mut host, ALICE, &held);
    assert!(host.take().is_empty(), "an already found square is not found again");
    assert!(host.take_broadcasts().is_empty());
}

#[test]
fn inventory_before_play_does_nothing() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    host.online = vec![player(ALICE, "Alice")];
    run(&mut session, &mut host, Some(ALICE), "join red");
    host.take();
    session.on_inventory(&mut host, ALICE, &HashMap::from([("minecraft:stone".to_string(), 1)]));
    assert!(host.take().is_empty());
    assert!(host.take_broadcasts().is_empty());
}

#[test]
fn a_player_who_joins_during_play_gets_the_current_game_and_a_leaver_keeps_their_team() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take_commands();

    session.on_inventory(&mut host, ALICE, &HashMap::from([(names[0].clone(), 1)]));
    host.take();
    host.take_broadcasts();

    // Bob leaves during play, which keeps his team, and comes back.
    host.online = vec![player(ALICE, "Alice")];
    session.on_leave(&mut host, BOB);
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    session.on_join(&mut host, &player(BOB, "Bob"));
    let back = host.take();
    assert!(back.iter().all(|s| s.player == BOB), "only the returning player is sent to");
    assert_eq!(count(&back, BOB, "game_status"), 1);
    assert_eq!(count(&back, BOB, "display_cards"), 1);
    assert_eq!(count(&back, BOB, "open_card"), 1);
    assert_eq!(decode(&only(&back, BOB, "open_card"), CardOpenPacket::read).unwrap().team_key.as_deref(), Some("bingo_red"));
    let tiles = tiles_of(&only(&back, BOB, "update_card_tiles"));
    assert!(!tiles.should_notify);
    assert_eq!(names_of(&tiles), names, "the same card");
    assert!(tiles.tiles[0].1.image.is_achieved, "the square Alice found is still found");
    assert_eq!(tiles.tiles[0].1.progress, 1.0);
    assert_eq!(tiles.tiles[0].1.team_keys, Vec::<String>::new());
    assert_eq!(score_of(&only(&back, BOB, "score_update")).score, TeamScore { items: 1, lines: 0, cards: 0 });
    let display = decode(&only(&back, BOB, "display_cards"), CardDisplayPacket::read).unwrap();
    assert_eq!(display.display[0].1.players.len(), 2);

    // Carol is new and on no team: she sees the teams and the score, but no tiles, and no card of her own.
    host.online.push(player(CAROL, "Carol"));
    session.on_join(&mut host, &player(CAROL, "Carol"));
    let joined = host.take();
    assert_eq!(count(&joined, CAROL, "display_cards"), 1);
    assert_eq!(count(&joined, CAROL, "update_card_tiles"), 0);
    assert_eq!(decode(&only(&joined, CAROL, "open_card"), CardOpenPacket::read).unwrap().team_key, None);
    assert!(!score_of(&only(&joined, CAROL, "score_update")).is_viewer_on_team);
}

#[test]
fn reset_clears_the_card_and_the_next_card_uses_the_next_seed() {
    let pool = load_pool();
    let mut host = FakeHost::default();
    let mut session = Session::new(options(100, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    let first = card_names(&mut host, ALICE);
    assert_eq!(first, names_of_generated(&generate_card(100, &pool, DEFAULT_DISTRIBUTION)));
    host.take_commands();

    session.on_inventory(&mut host, ALICE, &HashMap::from([(first[0].clone(), 1)]));
    host.take();
    host.take_broadcasts();

    assert_eq!(run(&mut session, &mut host, None, "reset"), "Resetting the game...");
    let reset = host.take();
    assert_eq!(count(&reset, ALICE, "game_reset"), 1);
    assert_eq!(count(&reset, ALICE, "card_reset"), 1);
    assert_eq!(count(&reset, ALICE, "game_status"), 1);
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    assert_eq!(host.take_commands(), ["team remove bingo_red", "gamemode survival Alice"]);

    // The card is gone: a find now is silent.
    session.on_inventory(&mut host, ALICE, &HashMap::from([(first[1].clone(), 1)]));
    assert!(host.take().is_empty());

    // The team stays, so the next start uses the card of seed 101.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    let second = card_names(&mut host, ALICE);
    assert_eq!(second, names_of_generated(&generate_card(101, &pool, DEFAULT_DISTRIBUTION)));
    assert_ne!(second, first);
}

#[test]
fn reset_takes_everyone_out_of_ready() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(100, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    session.tick(&mut host);
    session.tick(&mut host);
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"));
    host.take();

    run(&mut session, &mut host, None, "reset");
    let after = host.take();
    assert!(!ready_of(&only(&after, ALICE, "ready_timer_update_v2")).is_ready);
    assert!(!ready_of(&only(&after, BOB, "ready_timer_update_v2")).is_ready);

    // Only Bob is ready now: the timer runs for him alone and does not start the game.
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    for _ in 0..5 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
}

#[test]
fn time_limit_ends_the_game_with_no_winner_at_tick_1200() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, Some(60)));
    host.online = vec![player(ALICE, "Alice")];
    run(&mut session, &mut host, Some(ALICE), "start");

    // Play begins at once with the full 60 seconds.
    let begin = host.take();
    assert_eq!(decode(&only(&begin, ALICE, "timer"), TimerPacket::read).unwrap().seconds_remaining, 60);

    // Ticks 1 to 1199: a timer packet at ticks 20, 40, ... 1180 (59 of them) and no end.
    let mut timers = Vec::new();
    for _ in 0..1199 {
        session.tick(&mut host);
        for sent in host.take() {
            if sent.path == "timer" {
                timers.push(decode(&sent.payload, TimerPacket::read).unwrap().seconds_remaining);
            }
        }
    }
    assert_eq!(timers, (1..=59).rev().collect::<Vec<i32>>());
    assert!(host.take_broadcasts().is_empty());

    // Tick 1200 is the end: timer 0, the game status Postgame, the end screen with no winner.
    session.tick(&mut host);
    let end = host.take();
    assert_eq!(decode(&only(&end, ALICE, "timer"), TimerPacket::read).unwrap().seconds_remaining, 0);
    assert_eq!(status_of(&only(&end, ALICE, "game_status")).ingame_duration, Some(JavaDuration { seconds: 60, nanos: 0 }));
    let over = decode(&only(&end, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner, None);
    assert!(!over.is_winner);
    assert_eq!(over.scores, vec![ranking(0, "bingo_red", "Red", 0, 0, 0, "PT1M")]);
    assert_eq!(host.take_broadcasts(), vec!["The game has ended.".to_string()]);
}

#[test]
fn commands_answer_with_text_and_report_errors_without_changing_the_game() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    host.online = vec![player(ALICE, "Alice")];

    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    assert!(run(&mut session, &mut host, None, "status").contains("Status: Pregame."));
    assert!(host.take().is_empty());

    let refused = run(&mut session, &mut host, None, "end");
    assert!(!refused.is_empty() && refused != "Stopped the game!", "ending in Pregame is an error: {refused}");
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    assert!(host.take().is_empty(), "ending a game that is not playing changes nothing");

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert!(run(&mut session, &mut host, None, "status").contains("Starting"));
    assert_eq!(count(&host.take(), ALICE, "countdown"), 1);

    let again = run(&mut session, &mut host, None, "start");
    assert!(!again.is_empty() && again != "Starting the game...", "a second start is an error: {again}");
    assert!(run(&mut session, &mut host, None, "status").contains("Starting"));
    assert!(host.take().is_empty(), "the failed start sent nothing");

    let unknown = run(&mut session, &mut host, None, "dance");
    for name in ["start", "end", "reset", "status"] {
        assert!(unknown.contains(name), "the reply lists {name}: {unknown}");
    }
    assert!(run(&mut session, &mut host, None, "").contains("start"), "an empty line gives the usage");

    assert_eq!(run(&mut session, &mut host, None, "reset"), "Resetting the game...");
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
}

#[test]
fn ending_by_command_sends_the_end_screen_without_a_winner() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    host.online = vec![player(ALICE, "Alice")];
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();

    assert_eq!(run(&mut session, &mut host, None, "end"), "Stopped the game!");
    let sent = host.take();
    let over = decode(&only(&sent, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner, None);
    assert_eq!(over.scores, vec![ranking(0, "bingo_red", "Red", 0, 0, 0, "PT0S")]);
    assert_eq!(host.take_broadcasts(), vec!["The game has ended.".to_string()]);
}

#[test]
fn before_play_the_ready_screen_is_sent_every_ten_ticks_and_nothing_else() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    host.online = vec![player(ALICE, "Alice")];
    for _ in 0..100 {
        session.tick(&mut host);
    }
    let sent = host.take();
    assert_eq!(sent.len(), 10, "ticks 10, 20, ..., 100");
    assert!(sent.iter().all(|s| s.path == "ready_timer_update_v2" && s.player == ALICE));
}

#[test]
fn resume_after_the_time_limit_gives_fifteen_more_minutes_from_the_elapsed_time() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, Some(60)));
    host.online = vec![player(ALICE, "Alice")];
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    for _ in 0..1200 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Postgame"));
    host.take();

    // The limit moves to 60 s played plus 15 minutes, so 900 s are left at the resume.
    assert_eq!(run(&mut session, &mut host, None, "resume"), "The game continues.");
    let resumed = host.take();
    assert_eq!(decode(&only(&resumed, ALICE, "timer"), TimerPacket::read).unwrap().seconds_remaining, 900);

    // Ten seconds later the game is still running; with no extension it would end on the first tick.
    for _ in 0..200 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"), "the resumed game does not end again");
}

/// Session options with a ready countdown of `timer_secs` and the mod's minimum of two players (the default).
fn ready_options(seed: u64, timer_secs: i32) -> SessionOptions {
    SessionOptions {
        ready: ReadyOptions { timer_secs, ..ReadyOptions::default() },
        ..options(seed, 0, None)
    }
}

#[test]
fn a_lone_ready_player_never_starts_the_countdown() {
    // ReadyService.kt:74 needs players.size > 1. Alice is ready on a team and waits 200 ticks with nobody else online.
    let mut host = FakeHost::default();
    let mut session = Session::new(ready_options(7, 10));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    host.take();

    for _ in 0..200 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"), "one player cannot start");
    let sent = host.take();
    let packets = payloads(&sent, ALICE, "ready_timer_update_v2");
    assert_eq!(packets.len(), 20, "ticks 10, 20, ..., 200");
    assert!(packets.iter().all(|p| !ready_of(p).is_running), "the countdown never runs");
}

#[test]
fn a_player_leaving_cancels_the_countdown_and_the_ready_flags_and_the_return_restarts_it() {
    // Timer 10 s. Ticks 1..=50 run the countdown (Alice ready, Bob online). Bob leaves before tick 51.
    let mut host = FakeHost::default();
    let mut session = Session::new(ready_options(7, 10));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    for _ in 0..50 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));

    host.online = vec![player(ALICE, "Alice")];
    session.on_leave(&mut host, BOB);
    host.take();
    // Tick 51 finds one player online: the countdown stops and Alice's ready flag goes with it.
    session.tick(&mut host);
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    for _ in 0..9 {
        session.tick(&mut host);
    }
    // Tick 60 sends the ready screen: not running, Alice not ready.
    let sent = host.take();
    let screen = ready_of(&only(&sent, ALICE, "ready_timer_update_v2"));
    assert!(!screen.is_running, "the countdown stopped");
    assert!(!screen.is_ready, "the reset cleared Alice's ready flag");
    assert_eq!(screen.total_players, 1);

    // Bob returns on his team, and Alice readies again. Tick 61 starts the countdown from zero (share 1 of 2: 5000 ms,
    // which is 100 ticks), so it ends on tick 161, not before.
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    session.on_join(&mut host, &player(BOB, "Bob"));
    assert!(run(&mut session, &mut host, Some(BOB), "join red").starts_with("Moved player"));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    host.take();
    for _ in 61..=160 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"), "tick 160 is 50 ms short");
    session.tick(&mut host);
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"), "tick 161 ends the countdown");
}

#[test]
fn an_operator_can_start_a_lone_player_without_any_ready_countdown() {
    // `bingo start` has no player-count check (GameCommands.kt:37-46). One player on no team joins bingo_red.
    let mut host = FakeHost::default();
    let mut session = Session::new(ready_options(7, 10));
    host.online = vec![player(ALICE, "Alice")];
    session.on_join(&mut host, &player(ALICE, "Alice"));
    host.take();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"));
    assert!(host.take_commands().contains(&"team add bingo_red".to_string()));
}

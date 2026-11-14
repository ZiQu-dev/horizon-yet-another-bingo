//! Session: joins, the ready screen, the countdown, the card, scoring, chat, the end of the game and commands, driven
//! through a fake host that records every packet. Packets are decoded with the client's own readers. Expected values
//! are counted by hand: 20 ticks per second, a card of 25 squares, a 5-square row is one line.
use std::collections::{HashMap, HashSet};

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::display_cards::{CardDisplayPacket, PlayerProfile};
use horizon_bingo::game_over::{GameOverPacket, ScoreRanking};
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};
use horizon_bingo::open_card::CardOpenPacket;
use horizon_bingo::score_update::{ScoredItemPacket, TeamScore};
use horizon_bingo::server::card_gen::{generate_card, load_pool, DEFAULT_DISTRIBUTION};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::{Player, CARD_SQUARES};
use horizon_bingo::text::Text;
use horizon_bingo::timer::{CountdownPacket, TimerPacket};
use horizon_bingo::update_card_tiles::CardTilesPacket;

const TIER_LIST: &str = include_str!("../../bingo-native/horizon-bingo/data/items.tierlist.json");

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

const READY_V1: [u8; 1] = [1];
const READY_V2: [u8; 2] = [1, 0];
const UNREADY_V2: [u8; 2] = [0, 0];

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
}

impl FakeHost {
    fn take(&mut self) -> Vec<Sent> {
        std::mem::take(&mut self.sent)
    }

    fn take_broadcasts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.broadcasts)
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
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

fn options(seed: u64, countdown_secs: i32, time_limit_secs: Option<i32>) -> SessionOptions {
    SessionOptions {
        seed,
        game: GameOptions { countdown_secs, time_limit_secs },
        team: "bingo_red".to_string(),
        team_color: "RED".to_string(),
        team_name: "Red".to_string(),
    }
}

fn count(sent: &[Sent], player: u128, path: &str) -> usize {
    sent.iter().filter(|s| s.player == player && s.path == path).count()
}

fn payloads(sent: &[Sent], player: u128, path: &str) -> Vec<Vec<u8>> {
    sent.iter().filter(|s| s.player == player && s.path == path).map(|s| s.payload.clone()).collect()
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
fn check_fresh_card(tiles: &CardTilesPacket) -> Vec<String> {
    assert_eq!(tiles.team_key.as_deref(), Some("bingo_red"));
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

fn ranking(items: i32, lines: i32, cards: i32, duration: &str) -> ScoreRanking {
    ScoreRanking {
        index: 0,
        key: "bingo_red".to_string(),
        name: Text::plain("Red"),
        score: TeamScore { items, lines, cards },
        duration: Some(duration.to_string()),
    }
}

/// Alice and Bob join, both get ready and the game starts with a countdown of `countdown` seconds. Returns once the
/// game is Playing (the tick that begins it has run), with the names of the card's squares.
fn start_two_players(host: &mut FakeHost, session: &mut Session, countdown: i32) -> Vec<String> {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    session.on_join(host, &player(ALICE, "Alice"));
    session.on_join(host, &player(BOB, "Bob"));
    session.on_packet(host, ALICE, "set_ready", &READY_V1);
    session.on_packet(host, BOB, "set_ready_v2", &READY_V2);
    for _ in 0..countdown * 20 {
        session.tick(host);
    }
    let names = names_of(&tiles_of(&payloads(&host.take(), ALICE, "update_card_tiles")[0]));
    names
}

#[test]
fn joining_before_play_gets_only_the_game_status() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    host.online = vec![player(ALICE, "Alice")];
    session.on_join(&mut host, &player(ALICE, "Alice"));

    let sent = host.take();
    assert_eq!(sent.len(), 1);
    assert_eq!(count(&sent, ALICE, "game_status"), 1);
    let status = status_of(&payloads(&sent, ALICE, "game_status")[0]);
    assert_eq!(status.ingame_duration, Some(JavaDuration { seconds: 0, nanos: 0 }));
    assert_eq!(status.remaining_duration, None);
}

#[test]
fn full_game_from_ready_to_game_over() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7, 3, None));

    session.on_join(&mut host, &player(ALICE, "Alice"));
    session.on_join(&mut host, &player(BOB, "Bob"));
    assert_eq!(host.take().len(), 2, "one game status each");

    // Alice alone is ready: nobody starts yet.
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(host.take().is_empty());
    assert!(session.command(&mut host, None, "status").contains("Pregame"));

    // Bob's ready packet (V2) starts the game: the status goes out as Starting, and the countdown shows 3.
    session.on_packet(&mut host, BOB, "set_ready_v2", &READY_V2);
    let start = host.take();
    assert_eq!(count(&start, ALICE, "game_status"), 1);
    assert_eq!(count(&start, BOB, "game_status"), 1);
    assert_eq!(count(&start, ALICE, "countdown"), 1);
    assert_eq!(seconds_of(&payloads(&start, ALICE, "countdown")[0]), 3);
    assert_eq!(start.len(), 4);

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
    assert!(session.command(&mut host, None, "status").contains("Starting"));

    // Tick 60 is the last of the countdown and begins play: countdown 0, the game status, and the card for each player.
    session.tick(&mut host);
    let begin = host.take();
    assert_eq!(count(&begin, ALICE, "countdown"), 1);
    assert_eq!(seconds_of(&payloads(&begin, ALICE, "countdown")[0]), 0);
    assert_eq!(count(&begin, ALICE, "game_status"), 1);
    assert_eq!(status_of(&payloads(&begin, ALICE, "game_status")[0]).ingame_duration, Some(JavaDuration { seconds: 0, nanos: 0 }));
    for who in [ALICE, BOB] {
        assert_eq!(count(&begin, who, "display_cards"), 1);
        assert_eq!(count(&begin, who, "update_card_tiles"), 1);
        assert_eq!(count(&begin, who, "open_card"), 1);
        assert_eq!(count(&begin, who, "score_update"), 1);
    }
    assert_eq!(begin.len(), 12, "6 packets for each of 2 players");

    let display = decode(&payloads(&begin, ALICE, "display_cards")[0], CardDisplayPacket::read).unwrap();
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

    let card_for_alice = payloads(&begin, ALICE, "update_card_tiles")[0].clone();
    let names = check_fresh_card(&tiles_of(&card_for_alice));
    assert_eq!(payloads(&begin, BOB, "update_card_tiles")[0], card_for_alice, "both see the same card");

    let open = decode(&payloads(&begin, ALICE, "open_card")[0], CardOpenPacket::read).unwrap();
    assert_eq!(open.team_key.as_deref(), Some("bingo_red"));
    assert_eq!(score_of(&payloads(&begin, ALICE, "score_update")[0]).score, TeamScore { items: 0, lines: 0, cards: 0 });

    // 100 ticks of play, then Alice holds the whole top row: one line, so the game ends.
    for _ in 0..100 {
        session.tick(&mut host);
    }
    assert!(host.take().is_empty(), "nothing happens on plain ticks during play");

    let row: Vec<String> = names[0..5].to_vec();
    let mut held = HashMap::new();
    for name in &row {
        held.insert(name.clone(), 1);
    }
    held.insert("unlisted:held_item".to_string(), 3);
    session.on_inventory(&mut host, ALICE, &held);

    let finish = host.take();
    let broadcasts = host.take_broadcasts();
    assert_eq!(count(&finish, ALICE, "update_card_tiles"), 1);
    assert_eq!(count(&finish, BOB, "update_card_tiles"), 1);
    assert!(tiles_of(&payloads(&finish, ALICE, "update_card_tiles")[0]).should_notify, "the find is announced");
    assert_eq!(count(&finish, ALICE, "score_update"), 1);
    assert_eq!(count(&finish, BOB, "score_update"), 1);
    let score = score_of(&payloads(&finish, BOB, "score_update")[0]);
    assert_eq!(score.score, TeamScore { items: 5, lines: 1, cards: 1 });
    assert!(score.is_viewer_on_team);

    let expected_lines: Vec<String> = row
        .iter()
        .map(|name| format!("Alice found {}", name.strip_prefix("minecraft:").unwrap_or(name)))
        .chain(std::iter::once("Red won the game!".to_string()))
        .collect();
    assert_eq!(broadcasts, expected_lines);

    // The game is over: Postgame for both, the end screen with the team's ranking, elapsed time 100 ticks = 5 s.
    assert_eq!(count(&finish, ALICE, "game_status"), 1);
    assert_eq!(count(&finish, BOB, "game_status"), 1);
    let frozen = status_of(&payloads(&finish, BOB, "game_status")[0]);
    assert_eq!(frozen.ingame_duration, Some(JavaDuration { seconds: 5, nanos: 0 }));
    for who in [ALICE, BOB] {
        assert_eq!(count(&finish, who, "game_over_v2"), 1);
        let over = decode(&payloads(&finish, who, "game_over_v2")[0], GameOverPacket::read_v2).unwrap();
        assert_eq!(over.winner.as_deref(), Some("bingo_red"));
        assert!(over.is_winner);
        assert!(!over.is_update, "the end screen opens");
        assert_eq!(over.scores, vec![ranking(5, 1, 1, "PT5S")]);
    }
    assert_eq!(finish.len(), 8, "tiles, score, game status and end screen for each of 2 players");
    assert!(session.command(&mut host, None, "status").contains("Postgame"));

    // After the end, nothing more: ticks and further finds are silent and nothing is announced.
    session.tick(&mut host);
    session.on_inventory(&mut host, BOB, &HashMap::from([(names[5].clone(), 1)]));
    assert!(host.take().is_empty());
    assert!(host.take_broadcasts().is_empty());
}

#[test]
fn one_ready_of_two_does_not_start_the_game() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(host.take().is_empty());
    assert!(session.command(&mut host, None, "status").contains("Pregame"));

    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Playing"), "both ready starts at once with countdown 0");
}

#[test]
fn taking_back_ready_stops_the_start() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7, 3, None));
    session.on_packet(&mut host, ALICE, "set_ready_v2", &READY_V2);
    session.on_packet(&mut host, ALICE, "set_ready_v2", &UNREADY_V2);
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    assert!(host.take().is_empty(), "Alice is not ready");

    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Starting"));
}

#[test]
fn a_player_who_leaves_is_no_longer_ready_when_they_come_back() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7, 3, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);

    host.online = vec![player(BOB, "Bob")];
    session.on_leave(&mut host, ALICE);
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    session.on_join(&mut host, &player(ALICE, "Alice"));
    host.take();

    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    assert!(host.take().is_empty(), "Alice came back without being ready");
}

#[test]
fn nobody_online_or_only_offline_ready_players_never_start_the_game() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Pregame"), "no player online");

    host.online = vec![player(BOB, "Bob")];
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Pregame"), "Alice is offline");
    assert!(host.take().is_empty());
}

#[test]
fn malformed_ready_payloads_and_unknown_channels_are_ignored() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(7, 3, None));

    session.on_packet(&mut host, ALICE, "set_ready", &[]);
    session.on_packet(&mut host, ALICE, "set_ready_v2", &[1]);
    session.on_packet(&mut host, ALICE, "set_ready_v2", &[1, 0, 0]);
    session.on_packet(&mut host, ALICE, "set_ready", &[1, 0]);
    session.on_packet(&mut host, ALICE, "set_ready_v3", &READY_V1);
    session.on_packet(&mut host, ALICE, "game_status", &READY_V1);
    assert!(host.take().is_empty());

    // None of the above counted: Bob alone being ready does not start the game.
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    assert!(host.take().is_empty());
    assert!(session.command(&mut host, None, "status").contains("Pregame"));

    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Starting"));
}

#[test]
fn a_line_needs_all_five_squares_and_only_then_ends_the_game() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    let names = names_of(&tiles_of(&payloads(&host.take(), ALICE, "update_card_tiles")[0]));

    // Four squares of the first row: a line is not complete yet.
    let held: HashMap<String, u32> = names[0..4].iter().map(|name| (name.clone(), 1)).collect();
    session.on_inventory(&mut host, ALICE, &held);
    let sent = host.take();
    assert_eq!(score_of(&payloads(&sent, ALICE, "score_update")[0]).score, TeamScore { items: 4, lines: 0, cards: 0 });
    assert_eq!(count(&sent, ALICE, "game_over_v2"), 0);
    assert!(session.command(&mut host, None, "status").contains("Playing"));

    session.on_inventory(&mut host, ALICE, &HashMap::from([(names[4].clone(), 1)]));
    assert_eq!(count(&host.take(), ALICE, "game_over_v2"), 1);
    assert!(session.command(&mut host, None, "status").contains("Postgame"));
}

#[test]
fn zero_counts_repeat_finds_and_items_off_the_card_score_nothing() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    let names = names_of(&tiles_of(&payloads(&host.take(), ALICE, "update_card_tiles")[0]));

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
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 3, None));
    session.on_inventory(&mut host, ALICE, &HashMap::from([("minecraft:stone".to_string(), 1)]));
    assert!(host.take().is_empty());
    assert!(host.take_broadcasts().is_empty());
}

#[test]
fn a_player_who_joins_during_play_gets_the_current_card_and_progress() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    let names = start_two_players(&mut host, &mut session, 0);

    session.on_inventory(&mut host, ALICE, &HashMap::from([(names[0].clone(), 1)]));
    host.take();
    host.take_broadcasts();

    host.online.push(player(CAROL, "Carol"));
    session.on_join(&mut host, &player(CAROL, "Carol"));
    let joined = host.take();
    assert!(joined.iter().all(|s| s.player == CAROL), "only the new player is sent to");
    assert_eq!(count(&joined, CAROL, "game_status"), 1);
    assert_eq!(count(&joined, CAROL, "display_cards"), 1);
    assert_eq!(count(&joined, CAROL, "update_card_tiles"), 1);
    assert_eq!(count(&joined, CAROL, "open_card"), 1);
    assert_eq!(count(&joined, CAROL, "score_update"), 1);

    let card = tiles_of(&payloads(&joined, CAROL, "update_card_tiles")[0]);
    assert!(!card.should_notify);
    assert_eq!(names_of(&card), names, "the same card");
    assert!(card.tiles[0].1.image.is_achieved, "the square found earlier is still found");
    assert_eq!(card.tiles[0].1.progress, 1.0);
    assert_eq!(card.tiles[0].1.team_keys, vec!["bingo_red".to_string()]);
    assert!(card.tiles[1..].iter().all(|(_, tile)| !tile.image.is_achieved && tile.progress == 0.0));

    assert_eq!(score_of(&payloads(&joined, CAROL, "score_update")[0]).score, TeamScore { items: 1, lines: 0, cards: 0 });
    let display = decode(&payloads(&joined, CAROL, "display_cards")[0], CardDisplayPacket::read).unwrap();
    assert_eq!(display.display[0].1.players.len(), 3);
}

#[test]
fn reset_clears_the_card_and_the_next_card_uses_the_next_seed() {
    let pool = load_pool();
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(100, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    let first = tiles_of(&payloads(&host.take(), ALICE, "update_card_tiles")[0]);
    assert_eq!(names_of(&first), names_of_generated(&generate_card(100, &pool, DEFAULT_DISTRIBUTION)));

    session.on_inventory(&mut host, ALICE, &HashMap::from([(names_of(&first)[0].clone(), 1)]));
    host.take();
    host.take_broadcasts();

    assert_eq!(session.command(&mut host, None, "reset"), "Game reset.");
    let reset = host.take();
    assert_eq!(count(&reset, ALICE, "game_reset"), 1);
    assert_eq!(count(&reset, BOB, "card_reset"), 1);
    assert_eq!(count(&reset, ALICE, "game_status"), 1);
    assert_eq!(reset.len(), 6, "game status, game reset and card reset for 2 players");
    assert!(session.command(&mut host, None, "status").contains("Pregame"));

    // The card is gone: a find now is silent.
    session.on_inventory(&mut host, ALICE, &HashMap::from([(names_of(&first)[1].clone(), 1)]));
    assert!(host.take().is_empty());

    // The reset cleared the ready flags: both ready up again to start the next game with the card of seed 101.
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(host.take().is_empty(), "one ready of two starts nothing");
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    let round = host.take();
    let second = tiles_of(&payloads(&round, ALICE, "update_card_tiles")[0]);
    assert_eq!(names_of(&second), names_of_generated(&generate_card(101, &pool, DEFAULT_DISTRIBUTION)));
    assert_ne!(names_of(&second), names_of(&first));
    assert_eq!(score_of(&payloads(&round, ALICE, "score_update")[0]).score, TeamScore { items: 0, lines: 0, cards: 0 });
}

fn names_of_generated(card: &horizon_bingo::server::types::Card) -> Vec<String> {
    card.squares.iter().map(|square| square.item.clone()).collect()
}

#[test]
fn time_limit_ends_the_game_with_no_winner_at_tick_1200() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 0, Some(60)));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);

    // Play begins at once with the full 60 seconds.
    let begin = host.take();
    assert_eq!(count(&begin, ALICE, "timer"), 1);
    assert_eq!(decode(&payloads(&begin, ALICE, "timer")[0], TimerPacket::read).unwrap().seconds_remaining, 60);

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
    assert_eq!(decode(&payloads(&end, ALICE, "timer")[0], TimerPacket::read).unwrap().seconds_remaining, 0);
    assert_eq!(status_of(&payloads(&end, ALICE, "game_status")[0]).ingame_duration, Some(JavaDuration { seconds: 60, nanos: 0 }));
    let over = decode(&payloads(&end, ALICE, "game_over_v2")[0], GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner, None);
    assert!(!over.is_winner);
    assert_eq!(over.scores, vec![ranking(0, 0, 0, "PT1M")]);
    assert_eq!(host.take_broadcasts(), vec!["The game has ended.".to_string()]);
}

#[test]
fn commands_answer_with_text_and_report_errors_without_changing_the_game() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 3, None));

    assert!(session.command(&mut host, None, "status").contains("Pregame"));
    assert!(session.command(&mut host, None, "status").contains("0 of 25"));
    assert!(host.take().is_empty());

    let refused = session.command(&mut host, None, "end");
    assert!(!refused.is_empty() && refused != "Game ended.", "ending in Pregame is an error: {refused}");
    assert!(session.command(&mut host, None, "status").contains("Pregame"));
    assert!(host.take().is_empty(), "ending a game that is not playing changes nothing");

    assert_eq!(session.command(&mut host, Some(ALICE), "start"), "Game started.");
    assert!(session.command(&mut host, None, "status").contains("Starting"));
    let starting = host.take();
    assert_eq!(count(&starting, ALICE, "countdown"), 1);

    let again = session.command(&mut host, None, "start");
    assert!(!again.is_empty() && again != "Game started.", "a second start is an error: {again}");
    assert!(session.command(&mut host, None, "status").contains("Starting"));
    assert!(host.take().is_empty(), "the failed start sent nothing");

    let unknown = session.command(&mut host, None, "dance");
    for name in ["start", "end", "reset", "status"] {
        assert!(unknown.contains(name), "the reply lists {name}: {unknown}");
    }
    assert_eq!(session.command(&mut host, None, ""), unknown);

    assert_eq!(session.command(&mut host, None, "reset"), "Game reset.");
    assert!(session.command(&mut host, None, "status").contains("Pregame"));
}

#[test]
fn ending_by_command_sends_the_end_screen_without_a_winner() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    host.take();

    assert_eq!(session.command(&mut host, None, "end"), "Game ended.");
    let sent = host.take();
    let over = decode(&payloads(&sent, ALICE, "game_over_v2")[0], GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner, None);
    assert_eq!(over.scores, vec![ranking(0, 0, 0, "PT0S")]);
    assert_eq!(host.take_broadcasts(), vec!["The game has ended.".to_string()]);
}

#[test]
fn pregame_is_silent_on_ticks() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice")];
    let mut session = Session::new(options(7, 3, None));
    for _ in 0..100 {
        session.tick(&mut host);
    }
    assert!(host.take().is_empty());
}

#[test]
fn reset_takes_everyone_out_of_ready_so_one_late_ready_does_not_restart_the_game() {
    let mut host = FakeHost::default();
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    let mut session = Session::new(options(100, 0, None));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Playing"));

    session.command(&mut host, None, "reset");
    host.take();
    // Only Bob is ready now. Before the fix Alice's old ready still counted and this started a new game.
    session.on_packet(&mut host, BOB, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Pregame"));
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    assert!(session.command(&mut host, None, "status").contains("Playing"));
}

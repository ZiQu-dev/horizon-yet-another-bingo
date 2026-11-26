//! Session with several teams: the lobby rules (start, warnings, spectators, shuffles, ready timer), the team commands
//! sent to the server, one card shared by all teams with a progress per team, the win and the ranking, the boss bar and
//! the commands outside their state. Expected values are counted by hand: 20 ticks per second, a line is five squares,
//! and the ready timer lasts 600 s, shortened by the share of ready players.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::display_cards::CardDisplayPacket;
use horizon_bingo::game_over::{GameOverPacket, ScoreRanking};
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};
use horizon_bingo::open_card::CardOpenPacket;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::score_update::{ScoredItemPacket, TeamScore};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::text::Text;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

const READY_V1: [u8; 1] = [1];

#[derive(Debug, Clone)]
struct Sent {
    player: u128,
    path: String,
    payload: Vec<u8>,
}

/// One boss bar update: player, bar id, title, progress, visible.
type Bar = (u128, String, String, f32, bool);

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<Sent>,
    broadcasts: Vec<String>,
    commands: Vec<String>,
    bars: Vec<Bar>,
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

    fn take_bars(&mut self) -> Vec<Bar> {
        std::mem::take(&mut self.bars)
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

    fn boss_bar(&mut self, player: u128, id: &str, title: &str, progress: f32, visible: bool) {
        self.bars.push((player, id.to_string(), title.to_string(), progress, visible));
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

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Sets who is online, has each join the server, and puts each listed player on a team with `join <team>`. Drains what
/// was sent and the commands, so a test starts from a known point.
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

fn payloads(sent: &[Sent], player: u128, path: &str) -> Vec<Vec<u8>> {
    sent.iter().filter(|s| s.player == player && s.path == path).map(|s| s.payload.clone()).collect()
}

fn count(sent: &[Sent], player: u128, path: &str) -> usize {
    payloads(sent, player, path).len()
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

fn ready_of(payload: &[u8]) -> ReadyUpdatePacket {
    decode(payload, ReadyUpdatePacket::read_v2).expect("ready_timer_update_v2 decodes")
}

fn names_of(tiles: &CardTilesPacket) -> Vec<String> {
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

/// The names of the squares of the card, as seen by one player. The game must be Playing and the player must have
/// the card's packets in the host.
fn card_names(host: &mut FakeHost, player: u128) -> Vec<String> {
    names_of(&tiles_of(&only(&host.take(), player, "update_card_tiles")))
}

fn ranking(index: i32, key: &str, name: &str, items: i32, lines: i32, cards: i32, duration: &str) -> ScoreRanking {
    ScoreRanking {
        index,
        key: key.to_string(),
        name: Text::plain(name),
        score: TeamScore { items, lines, cards },
        duration: (duration != "DNF").then(|| duration.to_string()),
    }
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

/// `minecraft:oak_log` as the chat shows it.
fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

#[test]
fn two_teams_share_one_card_and_each_sees_the_others_finds_as_taken_by_them() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert_eq!(
        host.take_commands(),
        [
            "gamerule pvp true",
            "gamerule keep_inventory false",
            "team add bingo_red",
            "team modify bingo_red color red",
            "team join bingo_red Alice",
            "team add bingo_blue",
            "team modify bingo_blue color blue",
            "team join bingo_blue Bob",
            "gamemode survival Alice",
            "gamemode survival Bob",
        ]
    );

    // Each player sees only their own team's card (the original lists only the cards a player views).
    let begin = host.take();
    let display = decode(&only(&begin, ALICE, "display_cards"), CardDisplayPacket::read).unwrap();
    let keys: Vec<Option<String>> = display.display.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(keys, [Some("bingo_red".to_string())]);
    assert_eq!(display.display[0].1.team_color.as_deref(), Some("RED"));
    assert_eq!(display.display[0].1.players.iter().map(|p| p.uuid).collect::<Vec<_>>(), [ALICE]);
    let bob = decode(&only(&begin, BOB, "display_cards"), CardDisplayPacket::read).unwrap();
    assert_eq!(bob.display.len(), 1);
    assert_eq!(bob.display[0].0.as_deref(), Some("bingo_blue"));
    assert_eq!(bob.display[0].1.team_color.as_deref(), Some("BLUE"));
    assert_eq!(bob.display[0].1.players.iter().map(|p| p.uuid).collect::<Vec<_>>(), [BOB]);

    assert_eq!(decode(&only(&begin, ALICE, "open_card"), CardOpenPacket::read).unwrap().team_key.as_deref(), Some("bingo_red"));
    assert_eq!(decode(&only(&begin, BOB, "open_card"), CardOpenPacket::read).unwrap().team_key.as_deref(), Some("bingo_blue"));
    let alice_tiles = tiles_of(&only(&begin, ALICE, "update_card_tiles"));
    let bob_tiles = tiles_of(&only(&begin, BOB, "update_card_tiles"));
    assert_eq!(alice_tiles.team_key.as_deref(), Some("bingo_red"));
    assert_eq!(bob_tiles.team_key.as_deref(), Some("bingo_blue"));
    assert_eq!(names_of(&alice_tiles), names_of(&bob_tiles), "one card for both teams");

    // Each team starts with a score of its own, and a score is for its own team only.
    let scores = payloads(&begin, ALICE, "score_update");
    assert_eq!(scores.len(), 2);
    assert_eq!(score_of(&scores[0]).team, "bingo_red");
    assert!(score_of(&scores[0]).is_viewer_on_team);
    assert_eq!(score_of(&scores[1]).team, "bingo_blue");
    assert!(!score_of(&scores[1]).is_viewer_on_team);

    let names = names_of(&alice_tiles);

    // Alice's team finds the first square: Alice has it achieved, and Bob sees that the red team took it.
    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    let sent = host.take();
    let alice_after = tiles_of(&only(&sent, ALICE, "update_card_tiles"));
    let bob_after = tiles_of(&only(&sent, BOB, "update_card_tiles"));
    let (_, own) = &alice_after.tiles[0];
    assert!(own.image.is_achieved && own.progress == 1.0 && own.team_keys.is_empty());
    assert!(alice_after.should_notify, "the finding team is told");
    let (_, theirs) = &bob_after.tiles[0];
    assert!(!theirs.image.is_achieved && theirs.progress == 0.0);
    assert_eq!(theirs.team_keys, ["bingo_red"]);
    assert!(!bob_after.should_notify, "the other team is not told");
    let bob_score = score_of(&only(&sent, BOB, "score_update"));
    assert_eq!((bob_score.team.as_str(), bob_score.is_viewer_on_team, bob_score.score.items), ("bingo_red", false, 1));
    assert_eq!(host.take_broadcasts(), [format!("Alice got [{}]", short(&names[0]))]);

    // Bob's team finds the same square: now both teams have it, each as its own achievement and the other's key.
    session.on_inventory(&mut host, BOB, &held(&names[0..1]));
    let sent = host.take();
    let alice_after = tiles_of(&only(&sent, ALICE, "update_card_tiles"));
    let bob_after = tiles_of(&only(&sent, BOB, "update_card_tiles"));
    assert!(alice_after.tiles[0].1.image.is_achieved);
    assert_eq!(alice_after.tiles[0].1.team_keys, ["bingo_blue"]);
    assert!(bob_after.tiles[0].1.image.is_achieved);
    assert_eq!(bob_after.tiles[0].1.team_keys, ["bingo_red"]);
    assert!(bob_after.should_notify && !alice_after.should_notify);
    let alice_score = score_of(&only(&sent, ALICE, "score_update"));
    assert_eq!((alice_score.team.as_str(), alice_score.is_viewer_on_team, alice_score.score.items), ("bingo_blue", false, 1));
    assert_eq!(host.take_broadcasts(), [format!("Bob got [{}]", short(&names[0]))]);
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"), "no line yet, so no winner");
}

#[test]
fn a_team_that_completes_a_line_wins_and_the_ranking_lists_every_team() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    for _ in 0..100 {
        session.tick(&mut host);
    }
    host.take();

    // Alice holds the whole top row: red has a line, so red wins, after 100 ticks = 5 s.
    session.on_inventory(&mut host, ALICE, &held(&names[0..5]));
    let finish = host.take();
    let broadcasts = host.take_broadcasts();
    let mut expected: Vec<String> = names[0..5].iter().map(|name| format!("Alice got [{}]", short(name))).collect();
    assert_eq!(broadcasts, expected);

    let scores = [ranking(0, "bingo_red", "Red", 5, 1, 1, "PT5S"), ranking(1, "bingo_blue", "Blue", 0, 0, 0, "DNF")];
    let alice = decode(&only(&finish, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    let bob = decode(&only(&finish, BOB, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(alice.winner.as_deref(), Some("bingo_red"));
    assert!(alice.is_winner, "Alice is on the winning team");
    assert!(!bob.is_winner, "Bob is on the other team");
    assert_eq!(alice.scores, scores);
    assert_eq!(bob.scores, scores);
    assert_eq!(alice.duration.to_millis_or_zero(), 5_000);
    assert_eq!(decode(&only(&finish, BOB, "game_status"), GameStatusPacket::read).unwrap().ingame_duration, Some(JavaDuration { seconds: 5, nanos: 0 }));
    assert!(run(&mut session, &mut host, None, "status").contains("Postgame"));
}

#[test]
fn a_player_on_no_team_blocks_the_start_until_ignore_warnings_makes_them_a_spectator() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(
        &mut host,
        &mut session,
        &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol")],
        &[(ALICE, "red"), (BOB, "blue")],
    );

    let refused = run(&mut session, &mut host, Some(ALICE), "start");
    assert!(refused.starts_with("1 player(s) are not on a team"), "{refused}");
    assert!(refused.contains("join") && refused.contains("shuffleteams") && refused.contains("ignore_warnings"), "{refused}");
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    assert!(host.take_commands().is_empty(), "a refused start changes nothing");
    assert!(host.take().is_empty());

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start ignore_warnings"), "Starting the game...");
    let commands = host.take_commands();
    assert_eq!(commands.iter().filter(|c| c.starts_with("team ")).count(), 6, "Carol is not added to any team");
    assert!(commands.contains(&"gamemode spectator Carol".to_string()), "Carol watches in spectator mode");

    // Carol watches: both teams, no tiles, no card of her own, and no team in her scores.
    let begin = host.take();
    assert_eq!(decode(&only(&begin, CAROL, "display_cards"), CardDisplayPacket::read).unwrap().display.len(), 2);
    assert_eq!(count(&begin, CAROL, "update_card_tiles"), 0);
    assert_eq!(decode(&only(&begin, CAROL, "open_card"), CardOpenPacket::read).unwrap().team_key, None);
    assert!(payloads(&begin, CAROL, "score_update").iter().all(|p| !score_of(p).is_viewer_on_team));
}

#[test]
fn ignore_warnings_with_nobody_on_a_team_is_nobody_to_play() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start ignore_warnings"), "Nobody to play.");
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    assert!(host.take_commands().is_empty());
    assert!(host.take().is_empty(), "the spectators made here are not kept after a refusal");
}

#[test]
fn shuffleteams_deals_the_players_to_distinct_teams_and_leaves_spectators_out() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob"), (CAROL, "Carol")], &[]);
    assert_eq!(run(&mut session, &mut host, Some(CAROL), "spectator"), "You are now a spectator.");

    assert_eq!(run(&mut session, &mut host, None, "shuffleteams 0"), "shuffleteams needs a whole number of at least 1, got '0'");
    assert_eq!(run(&mut session, &mut host, None, "shuffleteams 9"), "Team count must be between 1 and 8, not 9.");
    assert_eq!(run(&mut session, &mut host, None, "shuffleteams 2"), "Teams have been shuffled!");

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    let commands = host.take_commands();
    assert_eq!(commands.iter().filter(|c| c.starts_with("team ")).count(), 6);
    assert_eq!(commands.iter().filter(|c| c.starts_with("team add")).count(), 2);
    let joined: Vec<&str> = commands.iter().filter_map(|c| c.strip_prefix("team join ")).collect();
    assert_eq!(joined.len(), 2);
    // Two random presets (as in the original), one player on each.
    let mut teams: Vec<&str> = joined.iter().map(|j| j.split(' ').next().unwrap()).collect();
    teams.sort_unstable();
    teams.dedup();
    assert_eq!(teams.len(), 2);
    let mut names: Vec<&str> = joined.iter().map(|j| j.rsplit(' ').next().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(names, ["Alice", "Bob"], "every player who is not a spectator is on a team");
}

#[test]
fn shuffleteams_clears_the_ready_flag_only_of_players_whose_team_changed() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(5, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    host.take();

    // One team, drawn at random: seed 5 draws red, so everyone goes to red. Alice stays on red, so she keeps her ready flag; Bob moves, so his goes.
    assert_eq!(run(&mut session, &mut host, None, "shuffleteams 1"), "Teams have been shuffled!");
    let sent = host.take();
    assert!(ready_of(&only(&sent, ALICE, "ready_timer_update_v2")).is_ready);
    assert!(!ready_of(&only(&sent, BOB, "ready_timer_update_v2")).is_ready);
}

#[test]
fn the_ready_timer_counts_ticks_and_starts_the_game_when_its_share_runs_out() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);
    session.on_packet(&mut host, ALICE, "set_ready", &READY_V1);
    host.take();

    // The first tick starts the timer. After ten ticks the screen has run 9 ticks (450 ms) of the length it has at a
    // half ready share: 600 s * (1 - 1/2) = 300 s. So 300 000 - 450 = 299 550 ms remain.
    for _ in 0..10 {
        session.tick(&mut host);
    }
    let sent = host.take();
    let bob_screen = ready_of(&only(&sent, BOB, "ready_timer_update_v2"));
    assert!(bob_screen.is_running && !bob_screen.is_ready);
    assert_eq!((bob_screen.ready_players, bob_screen.total_players), (1, 2));
    assert_eq!(bob_screen.remaining_millis, 299_550);
    assert!(ready_of(&only(&sent, ALICE, "ready_timer_update_v2")).is_ready);

    // The timer reaches zero on the 6001st tick (the first counted as the start), so the game starts then.
    for _ in 10..6000 {
        session.tick(&mut host);
    }
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"), "still 50 ms to go at tick 6000");
    session.tick(&mut host);
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"), "started on tick 6001");
}

#[test]
fn the_ready_command_toggles_the_callers_flag_and_refuses_the_console() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "red")]);

    assert_eq!(run(&mut session, &mut host, None, "ready"), "Only a player can ready up.");
    assert!(host.take().is_empty());

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "ready"), "Alice is ready!");
    assert!(ready_of(&only(&host.take(), ALICE, "ready_timer_update_v2")).is_ready);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "ready"), "Alice is no longer ready!");
    assert!(!ready_of(&only(&host.take(), ALICE, "ready_timer_update_v2")).is_ready);
}

#[test]
fn start_sends_the_team_commands_and_spreads_the_players_when_there_are_two_teams() {
    let mut host = FakeHost::default();
    let mut session = Session::new(SessionOptions { spread: Some((25, 10)), ..options(7, 0, None) });
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);

    run(&mut session, &mut host, Some(ALICE), "start");
    assert_eq!(
        host.take_commands(),
        [
            "gamerule pvp true",
            "gamerule keep_inventory false",
            "team add bingo_red",
            "team modify bingo_red color red",
            "team join bingo_red Alice",
            "team add bingo_blue",
            "team modify bingo_blue color blue",
            "team join bingo_blue Bob",
            "tag Alice add bingo_spread",
            "tag Bob add bingo_spread",
            "spreadplayers 0 0 400 800 false @a[tag=bingo_spread]",
            "tp @a[team=bingo_red,tag=!bingo_spread] Alice",
            "tp @a[team=bingo_blue,tag=!bingo_spread] Bob",
            "tag Alice remove bingo_spread",
            "tag Bob remove bingo_spread",
            "gamemode survival Alice",
            "gamemode survival Bob",
        ]
    );

    // The reset takes the teams that the start added back off the server.
    run(&mut session, &mut host, None, "reset");
    assert_eq!(host.take_commands(), ["team remove bingo_red", "team remove bingo_blue", "gamemode adventure Alice", "gamemode adventure Bob"]);
}

#[test]
fn one_team_is_not_spread_and_a_pink_team_gets_its_vanilla_colour() {
    let mut host = FakeHost::default();
    let mut session = Session::new(SessionOptions { spread: Some((25, 10)), ..options(7, 0, None) });
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "pink")]);

    run(&mut session, &mut host, Some(ALICE), "start");
    assert_eq!(
        host.take_commands(),
        ["gamerule pvp true", "gamerule keep_inventory false", "team add bingo_pink", "team modify bingo_pink color light_purple", "team join bingo_pink Alice", "gamemode survival Alice"]
    );
}

#[test]
fn the_boss_bar_of_each_team_shows_the_time_left_every_ten_ticks_and_is_hidden_at_the_end() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "timelimit 1");
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    host.take_bars();

    for _ in 0..10 {
        session.tick(&mut host);
    }
    // Ten ticks of play are 500 ms: 59 500 ms of the 60 000 ms limit remain.
    let bars = host.take_bars();
    assert_eq!(bars.len(), 2, "one bar for each member");
    let (player_a, id_a, title_a, progress_a, visible_a) = &bars[0];
    assert_eq!((*player_a, id_a.as_str(), title_a.as_str(), *visible_a), (ALICE, "bingo:bingo_red", "00:59 left", true));
    assert!((progress_a - 59_500.0 / 60_000.0).abs() < 1e-6);
    let (player_b, id_b, _, progress_b, visible_b) = &bars[1];
    assert_eq!((*player_b, id_b.as_str(), *visible_b), (BOB, "bingo:bingo_blue", true));
    assert!((progress_b - 59_500.0 / 60_000.0).abs() < 1e-6);

    // Nothing between the tens, and the next ten shows 59 000 ms.
    for _ in 0..9 {
        session.tick(&mut host);
    }
    assert!(host.take_bars().is_empty());
    session.tick(&mut host);
    let later = host.take_bars();
    assert_eq!(later.len(), 2);
    assert!((later[0].3 - 59_000.0 / 60_000.0).abs() < 1e-6);

    // The end hides both bars.
    run(&mut session, &mut host, None, "end");
    let hidden = host.take_bars();
    assert_eq!(
        hidden,
        [
            (ALICE, "bingo:bingo_red".to_string(), String::new(), 0.0, false),
            (BOB, "bingo:bingo_blue".to_string(), String::new(), 0.0, false),
        ]
    );
}

#[test]
fn commands_outside_their_state_fail_and_change_nothing() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    host.take_commands();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "join blue"), "Teams can be changed only before the game starts.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "shuffleteams 2"), "Teams can be changed only before the game starts.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "spectator"), "Teams can be changed only before the game starts.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "ready"), "Ready is possible only before the game starts.");
    assert_eq!(
        run(&mut session, &mut host, Some(ALICE), "ready cancel"),
        "The ready timer can be cancelled only before the game starts."
    );
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "The game is not in Pregame (Playing).");
    assert!(host.take().is_empty(), "the refused commands sent nothing");
    assert!(host.take_commands().is_empty());

    // The time limit can still change during play.
    assert!(run(&mut session, &mut host, Some(ALICE), "timelimit 5").starts_with("Time limit set"));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "end"), "Stopped the game!");
    host.take();
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "timelimit 5"), "The time limit can be changed only in Pregame or Playing.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "end"), "The game is not Playing.");
    assert!(host.take().is_empty());
}

#[test]
fn a_console_or_unknown_input_is_refused_before_play() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[]);

    assert_eq!(run(&mut session, &mut host, None, "join red"), "Only a player can join a team.");
    assert_eq!(run(&mut session, &mut host, None, "spectator"), "Only a player can become a spectator.");
    assert_eq!(run(&mut session, &mut host, None, "card"), "Only a player can open a card.");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "join nope"), "Team 'nope' does not exist!");
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "join"), "Usage: join <team>");
    assert!(run(&mut session, &mut host, None, "dance").starts_with("Unknown command 'dance'"));
    assert!(host.take().is_empty());
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
    assert!(run(&mut session, &mut host, None, "status").contains("Pregame"));
}

#[test]
fn card_opens_the_callers_team_card() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "card"), "Opening your card");
    let sent = host.take();
    assert_eq!(decode(&only(&sent, ALICE, "open_card"), CardOpenPacket::read).unwrap().team_key.as_deref(), Some("bingo_red"));
    assert_eq!(count(&sent, BOB, "open_card"), 0, "only the caller is sent the card");

    assert_eq!(run(&mut session, &mut host, Some(BOB), "card"), "Opening your card");
    assert_eq!(decode(&only(&host.take(), BOB, "open_card"), CardOpenPacket::read).unwrap().team_key.as_deref(), Some("bingo_blue"));
}

#[test]
fn status_lists_the_state_and_each_team_with_its_players_items_and_lines() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);

    assert_eq!(
        run(&mut session, &mut host, None, "status"),
        "Status: Pregame.\nRed: 1 players, 0 items, 0 lines\nBlue: 1 players, 0 items, 0 lines"
    );

    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    assert_eq!(
        run(&mut session, &mut host, None, "status"),
        "Status: Playing.\nRed: 1 players, 2 items, 0 lines\nBlue: 1 players, 0 items, 0 lines"
    );
}

#[test]
fn the_ranking_orders_teams_by_lines_then_items_and_ties_keep_the_preset_order() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();

    // Blue has one square, red none: blue ranks first.
    session.on_inventory(&mut host, BOB, &held(&names[0..1]));
    host.take();
    host.take_broadcasts();
    run(&mut session, &mut host, None, "end");
    let over = decode(&only(&host.take(), ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner, None);
    assert!(!over.is_winner);
    assert_eq!(over.scores, [ranking(0, "bingo_blue", "Blue", 1, 0, 0, "DNF"), ranking(1, "bingo_red", "Red", 0, 0, 0, "DNF")]);
}

#[test]
fn a_tie_in_the_ranking_shares_a_place_and_keeps_the_order_of_the_presets() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(BOB, "blue"), (ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();

    run(&mut session, &mut host, None, "end");
    let over = decode(&only(&host.take(), BOB, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.scores, [ranking(0, "bingo_red", "Red", 0, 0, 0, "DNF"), ranking(0, "bingo_blue", "Blue", 0, 0, 0, "DNF")]);
    assert_eq!(over.winner, None);
}

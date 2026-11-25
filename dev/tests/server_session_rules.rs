//! Round rules of the server session: the goal, the card mode, the end condition, the toggles and the spawn distance as
//! set before play; what they do during play (the goal, the lockout, the inventory mode); the vote and the resume after
//! the game ends; joining a running game; and the reset when nobody is online. Expected values are counted from the
//! rules by hand: 20 ticks per second, a countdown of 3 s is 60 ticks after a 100 tick start delay, and a game with nobody online resets after 100
//! ticks.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::score_update::ScoredItemPacket;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

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

fn names_of(tiles: &CardTilesPacket) -> Vec<String> {
    tiles.tiles.iter().map(|(_, tile)| tile.id.clone().expect("every tile has an item")).collect()
}

/// The names of the squares of the card, as seen by one player. The game must be Playing and the player must have
/// the card's packets in the host. Every square of this card has its own item, so the names are distinct.
fn card_names(host: &mut FakeHost, player: u128) -> Vec<String> {
    names_of(&tiles_of(&only(&host.take(), player, "update_card_tiles")))
}

fn held(names: &[String]) -> HashMap<String, u32> {
    names.iter().map(|name| (name.clone(), 1)).collect()
}

/// `minecraft:oak_log` as the chat shows it.
fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn status(session: &mut Session, host: &mut FakeHost) -> String {
    run(session, host, None, "status")
}

#[test]
fn goal_items_three_is_not_won_with_two_squares_and_is_won_with_three() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "goal items 3").starts_with("Goal: 3 item(s)"));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    // Two squares: red has 2 of the 3 items it needs, so the game goes on.
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    host.take();
    assert_eq!(
        host.take_broadcasts(),
        [format!("Alice got [{}]", short(&names[0])), format!("Alice got [{}]", short(&names[1]))]
    );
    let text = status(&mut session, &mut host);
    assert!(text.contains("Playing") && text.contains("Red: 1 players, 2 items"), "{text}");

    // The third square: red wins, and the winner is announced.
    session.on_inventory(&mut host, ALICE, &held(&names[0..3]));
    let finish = host.take();
    assert_eq!(
        host.take_broadcasts(),
        [format!("Alice got [{}]", short(&names[2]))]
    );
    assert!(status(&mut session, &mut host).contains("Postgame"));
    let over = decode(&only(&finish, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(over.winner.as_deref(), Some("bingo_red"));
    assert!(over.is_winner);
}

#[test]
fn goal_full_card_needs_all_twenty_five_squares() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "goal full_card").contains("Goal: full card"));
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    // 24 squares: the goal is not reached, so nothing ends.
    session.on_inventory(&mut host, ALICE, &held(&names[0..24]));
    host.take();
    let broadcasts = host.take_broadcasts();
    assert_eq!(broadcasts.len(), 24, "one find line per square");
    assert!(!broadcasts.iter().any(|line| line.contains("won") || line.contains("reached")), "{broadcasts:?}");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Playing") && text.contains("Red: 1 players, 24 items"), "{text}");

    // The 25th square completes the card: red reached the goal and is the winner.
    session.on_inventory(&mut host, ALICE, &held(&names));
    host.take();
    assert_eq!(
        host.take_broadcasts(),
        [format!("Alice got [{}]", short(&names[24]))]
    );
    assert!(status(&mut session, &mut host).contains("Postgame"));
}

#[test]
fn endwhen_never_announces_the_goal_but_does_not_end_until_end() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "goal items 1");
    assert!(run(&mut session, &mut host, Some(ALICE), "endwhen never").contains("End: never"));
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    host.take();
    assert_eq!(
        host.take_broadcasts(),
        [format!("Alice got [{}]", short(&names[0])), "Red reached the goal!".to_string()]
    );
    assert!(status(&mut session, &mut host).contains("Playing"));

    // More squares keep the game going, and the goal is not announced again.
    session.on_inventory(&mut host, ALICE, &held(&names[0..3]));
    host.take();
    let more = host.take_broadcasts();
    assert_eq!(more.len(), 2, "two more finds, no second goal line: {more:?}");
    assert!(status(&mut session, &mut host).contains("Playing"));

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "end"), "Stopped the game!");
    host.take();
    assert!(host.take_broadcasts().is_empty());
    assert!(status(&mut session, &mut host).contains("Postgame"));
}

#[test]
fn endwhen_all_win_lets_the_first_team_to_reach_the_goal_play_on() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "goal items 1");
    assert!(run(&mut session, &mut host, Some(ALICE), "endwhen all_win").contains("End: all teams win"));
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    // Red reaches its goal first: the game goes on.
    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    host.take();
    assert_eq!(
        host.take_broadcasts(),
        [format!("Alice got [{}]", short(&names[0])), "Red reached the goal!".to_string()]
    );
    assert!(status(&mut session, &mut host).contains("Playing"));

    // Blue reaches its goal too: every team has won, so the game ends, and red, which was first, is the winner.
    session.on_inventory(&mut host, BOB, &held(&names[0..1]));
    let finish = host.take();
    assert_eq!(
        host.take_broadcasts(),
        [format!("Bob got [{}]", short(&names[0]))]
    );
    assert!(status(&mut session, &mut host).contains("Postgame"));
    let alice = decode(&only(&finish, ALICE, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    let bob = decode(&only(&finish, BOB, "game_over_v2"), GameOverPacket::read_v2).unwrap();
    assert_eq!(alice.winner.as_deref(), Some("bingo_red"));
    assert!(alice.is_winner && !bob.is_winner);
}

#[test]
fn lockout_gives_a_square_to_the_first_team_only() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "mode lockout").contains("Mode: lockout"));
    // Lockout with the lines goal and two teams is refused by the original's start (`CommandStartLockoutWithLinesGoal`)
    // unless warnings are ignored, so this round starts with `ignore_warnings`.
    run(&mut session, &mut host, Some(ALICE), "start ignore_warnings");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    // Red finds square 0.
    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    host.take();
    assert_eq!(host.take_broadcasts(), [format!("Alice got [{}]", short(&names[0]))]);

    // Blue holds the same item: square 0 is red's, so blue gets nothing, not even a find line or a score.
    session.on_inventory(&mut host, BOB, &held(&names[0..1]));
    assert!(host.take().is_empty(), "a refused find sends nothing");
    assert!(host.take_broadcasts().is_empty(), "no find line for a refused square");
    let text = status(&mut session, &mut host);
    assert!(text.contains("Blue: 1 players, 0 items, 0 lines"), "{text}");

    // Blue finds square 1 next to it: that one is blue's.
    session.on_inventory(&mut host, BOB, &held(&names[0..2]));
    host.take();
    assert_eq!(host.take_broadcasts(), [format!("Bob got [{}]", short(&names[1]))]);
    assert_eq!(
        status(&mut session, &mut host),
        "Status: Playing.\nRed: 1 players, 1 items, 0 lines\nBlue: 1 players, 1 items, 0 lines"
    );
}

#[test]
fn inventory_mode_takes_a_square_back_when_the_item_leaves_the_inventory() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    assert!(run(&mut session, &mut host, Some(ALICE), "mode inventory").contains("Mode: inventory"));
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    let found = host.take();
    assert_eq!(score_of(&only(&found, ALICE, "score_update")).score.items, 1);
    assert_eq!(host.take_broadcasts(), [format!("Alice got [{}]", short(&names[0]))]);

    // The item is gone from the inventory: the score goes back to 0, and nobody is told of a find.
    session.on_inventory(&mut host, ALICE, &HashMap::new());
    let lost = host.take();
    let alice = score_of(&only(&lost, ALICE, "score_update"));
    assert_eq!((alice.team.as_str(), alice.is_viewer_on_team, alice.score.items), ("bingo_red", true, 0));
    assert_eq!(score_of(&only(&lost, BOB, "score_update")).score.items, 0);
    assert!(host.take_broadcasts().is_empty());
    assert!(status(&mut session, &mut host).contains("Red: 1 players, 0 items, 0 lines"));
}

#[test]
fn round_options_are_listed_with_their_defaults_and_refused_once_the_game_starts() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);

    let defaults = run(&mut session, &mut host, None, "options");
    assert!(defaults.contains("Goal: 1 line(s)") && defaults.contains("Mode: standard"), "{defaults}");
    assert!(run(&mut session, &mut host, Some(ALICE), "goal items 5").contains("Goal: 5 item(s)"));
    assert!(run(&mut session, &mut host, None, "options").contains("Goal: 5 item(s)"));

    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    host.take_commands();
    let before = run(&mut session, &mut host, None, "options");

    for line in ["goal items 3", "goal full_card", "mode lockout", "endwhen never", "pvp off", "spawndistance 5"] {
        let answer = run(&mut session, &mut host, Some(ALICE), line);
        assert!(answer.contains("only before the game starts"), "{line}: {answer}");
    }
    assert_eq!(run(&mut session, &mut host, None, "options"), before, "nothing changed");
    assert!(host.take_commands().is_empty());
    assert!(host.take().is_empty());
    assert!(status(&mut session, &mut host).contains("Playing"));
}

#[test]
fn toggles_set_before_start_reach_the_server_commands_and_the_players_at_countdown_end() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 3, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);

    assert!(run(&mut session, &mut host, Some(ALICE), "pvp off").contains("PvP: off"));
    assert!(run(&mut session, &mut host, Some(ALICE), "keepinventory on").contains("Keep inventory: on"));
    assert!(run(&mut session, &mut host, Some(ALICE), "elytra on").contains("Elytra: on"));
    assert!(run(&mut session, &mut host, Some(ALICE), "nightvision on").contains("Night vision: on"));

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    // The game rules go out at the start, the player's gear when play begins after the 100 tick start delay and the 3 s countdown (60 ticks): play begins on tick 161.
    for _ in 0..160 {
        session.tick(&mut host);
    }
    assert!(status(&mut session, &mut host).contains("Starting"));
    session.tick(&mut host);
    assert!(status(&mut session, &mut host).contains("Playing"));

    let commands = host.take_commands();
    for expected in [
        "gamerule pvp false",
        "gamerule keep_inventory true",
        "give Alice elytra",
        "effect give Alice night_vision infinite 0 true",
    ] {
        assert!(commands.iter().any(|c| c == expected), "{expected} missing from {commands:?}");
    }
}

#[test]
fn vote_is_refused_before_the_game_ends() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote next"), "Voting is possible only after the game ends.");
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote lobby"), "Voting is possible only after the game ends.");
    assert!(status(&mut session, &mut host).contains("Playing"));
}

#[test]
fn a_lone_player_who_votes_lobby_sends_the_game_back_to_pregame() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    run(&mut session, &mut host, None, "end");
    assert!(status(&mut session, &mut host).contains("Postgame"));
    host.take();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote lobby"), "The vote is over: back to the lobby.");
    let sent = host.take();
    assert_eq!(count(&sent, ALICE, "game_reset"), 1, "the reset goes to the player");
    assert!(status(&mut session, &mut host).contains("Pregame"));
}

#[test]
fn a_lone_player_who_votes_keep_plays_the_same_card_again() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice")], &[(ALICE, "red")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    run(&mut session, &mut host, None, "end");
    host.take();
    host.take_commands();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote keep"), "The game continues.");
    let resumed = host.take();
    assert_eq!(count(&resumed, ALICE, "game_status"), 1);
    assert!(status(&mut session, &mut host).contains("Playing"));

    // The card is the same one: a find on square 0 shows the same names on the same squares.
    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    let after = host.take();
    assert_eq!(names_of(&tiles_of(&only(&after, ALICE, "update_card_tiles"))), names);
}

#[test]
fn with_two_players_the_vote_waits_for_the_second_one() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    run(&mut session, &mut host, None, "end");
    host.take();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote next"), "Vote recorded (1 of 2 votes).");
    assert!(status(&mut session, &mut host).contains("Postgame"), "Bob has not voted yet");
    host.take();

    assert_eq!(run(&mut session, &mut host, Some(BOB), "vote next"), "The vote is over: back to the lobby.");
    assert!(status(&mut session, &mut host).contains("Pregame"));
}

#[test]
fn a_tie_between_keep_and_next_goes_back_to_the_lobby_and_a_keep_majority_resumes() {
    for (bob_vote, expect) in [("vote next", "Pregame"), ("vote keep", "Playing")] {
        let mut host = FakeHost::default();
        let mut session = Session::new(options(7, 0, None));
        lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
        run(&mut session, &mut host, Some(ALICE), "start");
        run(&mut session, &mut host, None, "end");
        host.take();

        run(&mut session, &mut host, Some(ALICE), "vote keep");
        run(&mut session, &mut host, Some(BOB), bob_vote);
        assert!(status(&mut session, &mut host).contains(expect), "keep vs {bob_vote}");
    }
}

#[test]
fn resume_after_a_first_win_turns_the_end_condition_off() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "goal items 1");
    run(&mut session, &mut host, Some(ALICE), "start");
    let names = card_names(&mut host, ALICE);
    host.take();
    host.take_commands();

    session.on_inventory(&mut host, ALICE, &held(&names[0..1]));
    host.take();
    assert_eq!(host.take_broadcasts(), [format!("Alice got [{}]", short(&names[0]))]);
    assert!(status(&mut session, &mut host).contains("Postgame"));

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "resume"), "The game continues.");
    host.take();
    assert_eq!(host.take_broadcasts(), ["The game continues."]);
    assert!(status(&mut session, &mut host).contains("Playing"));
    assert!(run(&mut session, &mut host, None, "options").contains("End: never"));

    // Another square: the goal was already reached, and with no end condition the game does not end again.
    session.on_inventory(&mut host, ALICE, &held(&names[0..2]));
    host.take();
    assert_eq!(host.take_broadcasts(), [format!("Alice got [{}]", short(&names[1]))]);
    assert!(status(&mut session, &mut host).contains("Playing"));
}

#[test]
fn a_player_who_joins_a_running_game_watches_until_they_join_a_playing_team() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    host.take_commands();

    let carol = player(CAROL, "Carol");
    host.online.push(carol.clone());
    session.on_join(&mut host, &carol);
    assert!(host.take_commands().contains(&"gamemode spectator Carol".to_string()));

    // A team that does not play in this game is refused.
    assert!(run(&mut session, &mut host, Some(CAROL), "join green").contains("is not playing"));
    assert!(host.take_commands().is_empty());

    assert_eq!(run(&mut session, &mut host, Some(CAROL), "join red"), "Moved player Carol onto team Red!");
    assert_eq!(
        host.take_commands(),
        ["team join bingo_red Carol", "gamemode survival Carol", "tp Carol Alice"]
    );
}

#[test]
fn a_game_in_progress_with_nobody_online_keeps_playing() {
    // The original's `ResetOnLeaveController` resets only an ended game (POSTGAME) that nobody is in. A game in progress
    // waits for its players, so the round and its saved state survive an empty server.
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    host.take();
    host.take_commands();
    assert!(status(&mut session, &mut host).contains("Playing"));

    host.online.clear();
    for _ in 0..200 {
        session.tick(&mut host);
    }
    assert!(status(&mut session, &mut host).contains("Playing"), "an empty server does not reset a game in progress");
}

#[test]
fn an_ended_game_with_nobody_online_goes_back_to_pregame_after_one_hundred_ticks() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    run(&mut session, &mut host, Some(ALICE), "end");
    host.take();
    host.take_commands();
    assert!(status(&mut session, &mut host).contains("Postgame"));

    host.online.clear();
    for _ in 0..99 {
        session.tick(&mut host);
    }
    assert!(status(&mut session, &mut host).contains("Postgame"), "99 empty ticks are not yet a reset");

    session.tick(&mut host);
    assert!(status(&mut session, &mut host).contains("Pregame"), "the 100th empty tick resets the ended game");
}

#[test]
fn a_reset_forgets_the_votes_of_the_last_game() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    run(&mut session, &mut host, None, "end");
    host.take();

    // Alice votes in the first game; Bob has not, so the vote waits.
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote next"), "Vote recorded (1 of 2 votes).");

    // Reset, then a second game that ends. Alice's old vote must not count for her now.
    run(&mut session, &mut host, None, "reset");
    run(&mut session, &mut host, None, "start");
    run(&mut session, &mut host, None, "end");
    host.take();
    assert_eq!(run(&mut session, &mut host, Some(BOB), "vote keep"), "Vote recorded (1 of 2 votes).");
    assert!(status(&mut session, &mut host).contains("Postgame"), "only Bob has voted in this game");
}

#[test]
fn a_resume_forgets_the_votes_before_it() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7, 0, None));
    lobby(&mut host, &mut session, &[(ALICE, "Alice"), (BOB, "Bob")], &[(ALICE, "red"), (BOB, "blue")]);
    run(&mut session, &mut host, Some(ALICE), "start");
    run(&mut session, &mut host, None, "end");
    host.take();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote keep"), "Vote recorded (1 of 2 votes).");
    assert_eq!(run(&mut session, &mut host, None, "resume"), "The game continues.");
    run(&mut session, &mut host, None, "end");
    host.take();

    // Alice's vote from before the resume is gone: Bob's vote alone is 1 of 2.
    assert_eq!(run(&mut session, &mut host, Some(BOB), "vote keep"), "Vote recorded (1 of 2 votes).");
    assert!(status(&mut session, &mut host).contains("Postgame"));
}

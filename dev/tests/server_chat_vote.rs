//! Plain chat votes: `vote lobby`, `vote next` and `vote keep` typed without `bingo`. Which lines count as one, and that
//! the vote a chat line makes is the vote the command makes, and only counts in Postgame. Expected values are written by
//! hand from the vote rules.
use horizon_bingo::server::commands::{Command, VoteChoice, chat_shortcut};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
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

/// Both players online, each on a team, the lobby left as it is.
fn lobby(host: &mut FakeHost, session: &mut Session) {
    host.online = vec![Player { uuid: ALICE, name: "Alice".to_string() }, Player { uuid: BOB, name: "Bob".to_string() }];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(run(session, host, Some(ALICE), "join red").starts_with("Moved player"));
    assert!(run(session, host, Some(BOB), "join blue").starts_with("Moved player"));
}

fn status(session: &mut Session, host: &mut FakeHost) -> String {
    run(session, host, None, "status")
}

#[test]
fn the_three_votes_are_recognised_whatever_the_case_and_the_spaces_at_the_ends() {
    assert_eq!(chat_shortcut("vote next"), Some(Command::Vote(VoteChoice::NextRound)));
    assert_eq!(chat_shortcut("vote keep"), Some(Command::Vote(VoteChoice::KeepPlaying)));
    assert_eq!(chat_shortcut("vote lobby"), Some(Command::Vote(VoteChoice::ReturnToLobby)));
    assert_eq!(chat_shortcut("  Vote LOBBY  "), Some(Command::Vote(VoteChoice::ReturnToLobby)));
    assert_eq!(chat_shortcut("VOTE Keep\t"), Some(Command::Vote(VoteChoice::KeepPlaying)));
}

#[test]
fn every_other_line_is_left_to_the_game() {
    for line in ["", "vote", "vote return", "vote lobby now", "bingo vote lobby", "lobby", "keep", "votelobby", "vote next keep"] {
        assert_eq!(chat_shortcut(line), None, "{line:?} is not a chat vote");
    }
}

#[test]
fn a_chat_vote_is_the_same_vote_as_the_command() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert_eq!(run(&mut session, &mut host, None, "end"), "Stopped the game!");
    assert!(session.is_postgame());

    // Alice's chat line, as the server runs it after the tick: she votes for the lobby, Bob has not voted yet.
    let chat = chat_shortcut("  VOTE Lobby ").expect("a chat vote");
    assert_eq!(session.run(&mut host, Some(ALICE), chat), "Vote recorded (1 of 2 votes).");
    assert!(status(&mut session, &mut host).contains("Postgame"), "Bob has not voted yet");

    let chat = chat_shortcut("vote lobby").expect("a chat vote");
    assert_eq!(session.run(&mut host, Some(BOB), chat), "The vote is over: back to the lobby.");
    assert!(status(&mut session, &mut host).contains("Pregame"));
}

#[test]
fn a_chat_vote_keep_plays_the_game_on_when_both_keep() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    run(&mut session, &mut host, Some(ALICE), "start");
    run(&mut session, &mut host, None, "end");

    assert_eq!(session.run(&mut host, Some(ALICE), chat_shortcut("vote keep").unwrap()), "Vote recorded (1 of 2 votes).");
    assert_eq!(session.run(&mut host, Some(BOB), chat_shortcut("vote keep").unwrap()), "The game continues.");
    assert!(status(&mut session, &mut host).contains("Playing"));
}

#[test]
fn a_chat_vote_counts_only_in_postgame() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    lobby(&mut host, &mut session);
    assert!(!session.is_postgame(), "the lobby is not Postgame");

    run(&mut session, &mut host, Some(ALICE), "start");
    assert!(!session.is_postgame(), "a game in progress is not Postgame");
    assert_eq!(
        session.run(&mut host, Some(ALICE), chat_shortcut("vote lobby").unwrap()),
        "Voting is possible only after the game ends."
    );
    assert!(status(&mut session, &mut host).contains("Playing"));

    run(&mut session, &mut host, None, "end");
    assert!(session.is_postgame(), "the ended game is Postgame");
}

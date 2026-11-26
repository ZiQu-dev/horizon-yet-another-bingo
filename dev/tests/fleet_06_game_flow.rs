//! The end of a game as the postgame vote settles it: a next-round result starts the next round at once, and Keep
//! Playing names what it does (`GameResumeService.getResumeDescription`). Expected texts are the original's en_us.json
//! strings, written out by hand.
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::resume::{choose, description, Resume, Situation};
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<(u128, String)>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, _payload: Vec<u8>) {
        self.sent.push((player, path.to_string()));
    }

    fn tell(&mut self, _player: u128, _line: &str) {}

    fn broadcast(&mut self, _line: &str) {}

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn command(&mut self, _line: &str) {}
}

fn options() -> SessionOptions {
    SessionOptions {
        seed: 7,
        game: GameOptions { countdown_secs: 0, time_limit_secs: None },
        ready: ReadyOptions::default(),
        spread: None,
        ..SessionOptions::default()
    }
}

fn run(session: &mut Session, host: &mut FakeHost, player: Option<u128>, line: &str) -> String {
    session.command(host, player, line)
}

/// Alice on red and Bob on blue, both online, the game started and then ended by `end`.
fn ended_two_player_game() -> (Session, FakeHost) {
    let mut host = FakeHost::default();
    let mut session = Session::new(options());
    host.online = vec![Player { uuid: ALICE, name: "Alice".to_string() }, Player { uuid: BOB, name: "Bob".to_string() }];
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    assert!(run(&mut session, &mut host, Some(ALICE), "join red").starts_with("Moved player"));
    assert!(run(&mut session, &mut host, Some(BOB), "join blue").starts_with("Moved player"));
    assert_eq!(run(&mut session, &mut host, Some(ALICE), "start"), "Starting the game...");
    assert_eq!(run(&mut session, &mut host, None, "end"), "Stopped the game!");
    assert!(session.is_postgame());
    (session, host)
}

#[test]
fn both_players_voting_next_round_starts_the_next_round() {
    let (mut session, mut host) = ended_two_player_game();

    assert_eq!(run(&mut session, &mut host, Some(ALICE), "vote next"), "Vote recorded (1 of 2 votes).");
    assert!(session.is_postgame(), "Bob has not voted yet");
    assert_eq!(
        run(&mut session, &mut host, Some(BOB), "vote next"),
        "The vote is over: starting the next round."
    );
    assert!(!session.is_postgame(), "the next round is being played, not the lobby");
    assert!(run(&mut session, &mut host, None, "status").contains("Playing"));
}

#[test]
fn keep_playing_text_follows_the_resume_choice_of_the_original() {
    let all_resumable = Situation { no_team_finished: true, unfinished_teams: true, near_time_limit: false };
    assert_eq!(choose(all_resumable), Resume::KeepPlaying);
    assert_eq!(choose(Situation { near_time_limit: true, ..all_resumable }), Resume::IncreaseTimeLimit);
    // A team has finished and another has not: the game goes on until every team wins.
    let one_finished = Situation { no_team_finished: false, unfinished_teams: true, near_time_limit: false };
    assert_eq!(choose(one_finished), Resume::WaitUntilAllWin);
    // Every team has finished: infinite mode.
    let all_finished = Situation { unfinished_teams: false, ..one_finished };
    assert_eq!(choose(all_finished), Resume::InfiniteMode);

    assert_eq!(description(Resume::KeepPlaying, 900).render(), "Keep Playing");
    assert_eq!(description(Resume::IncreaseTimeLimit, 15 * 60).render(), "Extend time by 15m 00s");
    assert_eq!(description(Resume::WaitUntilAllWin, 900).render(), "Extend to 'All Teams Win'");
    assert_eq!(description(Resume::InfiniteMode, 900).render(), "Extend to Infinite Mode");
}

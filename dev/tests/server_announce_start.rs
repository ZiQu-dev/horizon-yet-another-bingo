//! The start of a game as the original's `GameService.start` announces it: a title to every player (`BINGO`, then `Game is
//! starting!`), and the banner with the options to each player on a team. Also the warnings that refuse a start: lockout
//! with a lines goal (which `ignore_warnings` passes) and a goal the card already meets (which nothing passes).
//!
//! Driven through a fake host that records titles, chat lines and packets. Expected lines are written out by hand.
use horizon_bingo::buf::decode;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::server::card_gen::Pool;
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::texts::Line;
use horizon_bingo::server::types::Player;

const ALICE: u128 = 1;
const BOB: u128 = 2;
const CAROL: u128 = 3;

const FLINT: &str = "objective:yet-another-minecraft-bingo:challenge/never_get_flint_and_steel";
const SKELETON: &str = "objective:yet-another-minecraft-bingo:challenge/never_killed_by_skeleton";

#[derive(Default)]
struct FakeHost {
    online: Vec<Player>,
    sent: Vec<(u128, String, Vec<u8>)>,
    titles: Vec<(u128, Line, Line)>,
    tells: Vec<(u128, String)>,
    broadcasts: Vec<String>,
}

impl ServerHost for FakeHost {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        self.sent.push((player, path.to_string(), payload));
    }

    fn tell(&mut self, player: u128, line: &str) {
        self.tells.push((player, line.to_string()));
    }

    fn broadcast(&mut self, line: &str) {
        self.broadcasts.push(line.to_string());
    }

    fn players(&self) -> Vec<Player> {
        self.online.clone()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        Some(item.len() as i32)
    }

    fn title(&mut self, player: u128, title: &Line, subtitle: &Line) {
        self.titles.push((player, title.clone(), subtitle.clone()));
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

/// Alice on red and Bob on blue, both online, with the join traffic drained.
fn two_teams(host: &mut FakeHost, session: &mut Session) {
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    for p in host.online.clone() {
        session.on_join(host, &p);
    }
    assert!(run(session, host, Some(ALICE), "join red").starts_with("You joined"));
    assert!(run(session, host, Some(BOB), "join blue").starts_with("You joined"));
    host.titles.clear();
    host.tells.clear();
    host.broadcasts.clear();
    host.sent.clear();
}

fn tells_of(host: &FakeHost, player: u128) -> Vec<String> {
    host.tells.iter().filter(|(to, _)| *to == player).map(|(_, line)| line.clone()).collect()
}

/// The last `ready_timer_update_v2` packet sent to a player.
fn last_ready(host: &FakeHost, player: u128) -> ReadyUpdatePacket {
    let payload = host
        .sent
        .iter()
        .rev()
        .find(|(to, path, _)| *to == player && path == "ready_timer_update_v2")
        .map(|(_, _, payload)| payload.clone())
        .expect("a ready packet was sent to the player");
    decode(&payload, ReadyUpdatePacket::read_v2).expect("ready_timer_update_v2 decodes")
}

#[test]
fn a_start_titles_every_player_and_banners_the_players_on_a_team() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    two_teams(&mut host, &mut session);
    // Carol is online with no team: `ignore_warnings` makes her a spectator, who gets the title but no banner.
    host.online.push(player(CAROL, "Carol"));
    assert_eq!(run(&mut session, &mut host, None, "start ignore_warnings"), "Game started.");

    let mut titled: Vec<u128> = host.titles.iter().map(|(player, _, _)| *player).collect();
    titled.sort_unstable();
    assert_eq!(titled, vec![ALICE, BOB, CAROL], "every player online gets the title");
    for (_, title, subtitle) in &host.titles {
        assert_eq!(title.key, "bingo.game.announce.bingo");
        assert_eq!(title.render(), "BINGO");
        assert_eq!(subtitle.key, "bingo.game.announce.starting");
        assert_eq!(subtitle.render(), "Game is starting!");
    }

    // The banner: a rule of 40 dashes, `BINGO Start!`, the options as the `options` command gives them, a rule.
    let rule = "┄".repeat(40);
    let options_line = run(&mut session, &mut host, None, "options");
    assert!(options_line.starts_with("Goal: 1 line(s)"), "{options_line}");
    let expected = vec![rule.clone(), "BINGO Start!".to_string(), options_line.clone(), rule.clone()];
    assert_eq!(tells_of(&host, ALICE), expected);
    assert_eq!(tells_of(&host, BOB), expected);
    assert!(tells_of(&host, CAROL).is_empty(), "a spectator gets no banner");
    assert!(host.broadcasts.is_empty(), "the start is told to the players, not broadcast");
}

#[test]
fn lockout_with_a_lines_goal_refuses_the_start_and_says_why() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    two_teams(&mut host, &mut session);
    assert!(run(&mut session, &mut host, None, "mode lockout").contains("Mode: lockout"));

    let refused = run(&mut session, &mut host, None, "start");
    assert_eq!(
        refused,
        "Lockout is enabled, but the card goal is 1 line - this might cause a stalemate. You may want this instead: stalemate do_nothing. Use start ignore_warnings to start anyway."
    );
    assert!(session.is_pregame(), "a refused start stays in the lobby");
    assert!(host.titles.is_empty() && host.tells.is_empty(), "nothing is announced for a refused start");

    // `ignore_warnings` starts the same round, and the title goes to both players.
    assert_eq!(run(&mut session, &mut host, None, "start ignore_warnings"), "Game started.");
    assert_eq!(host.titles.len(), 2);
}

#[test]
fn doing_nothing_on_a_stalemate_lets_lockout_with_a_lines_goal_start() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    two_teams(&mut host, &mut session);
    run(&mut session, &mut host, None, "mode lockout");
    run(&mut session, &mut host, None, "stalemate do_nothing");
    assert_eq!(run(&mut session, &mut host, None, "start"), "Game started.");
}

#[test]
fn lockout_with_one_team_starts_without_a_warning() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    host.online = vec![player(ALICE, "Alice"), player(BOB, "Bob")];
    for p in host.online.clone() {
        session.on_join(&mut host, &p);
    }
    assert!(run(&mut session, &mut host, Some(ALICE), "join red").starts_with("You joined"));
    assert!(run(&mut session, &mut host, Some(BOB), "join red").starts_with("You joined"));
    run(&mut session, &mut host, None, "mode lockout");
    assert_eq!(run(&mut session, &mut host, None, "start"), "Game started.");
}

/// Alice readies up and the lobby ticks for 250 seconds (5000 ticks): half of the 600 s timer is still left for one
/// ready player out of two. Returns whether her ready packet says the countdown runs.
fn countdown_runs(lockout: bool) -> bool {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    two_teams(&mut host, &mut session);
    if lockout {
        run(&mut session, &mut host, None, "mode lockout");
    }
    run(&mut session, &mut host, Some(ALICE), "ready");
    for _ in 0..5000 {
        session.tick(&mut host);
    }
    last_ready(&host, ALICE).is_running
}

#[test]
fn the_lockout_warning_keeps_the_ready_countdown_from_running() {
    assert!(countdown_runs(false), "control: with the standard mode the countdown runs");
    assert!(!countdown_runs(true), "lockout with two teams: the start would be refused, so no countdown");
}

/// The pool of a card with two inverse squares and 23 plain items: a 25-square card is then all of the pool, so both
/// inverse squares are on it. Their objectives are the original's inverse challenges: nobody holds the forbidden items
/// at the start, so both count as achieved before play.
fn pool_with_two_inverse_squares() -> Pool {
    let mut keys = vec![FLINT.to_string(), SKELETON.to_string()];
    keys.extend((0..23).map(|i| format!("minecraft:plain_{i}")));
    Pool {
        tiers: [vec![], vec![], vec![], keys[..9].to_vec(), keys[9..].to_vec()],
        weights: [vec![], vec![], vec![], vec![1.0; 9], vec![1.0; 16]],
        groups: Vec::new(),
        skipped_tags: Vec::new(),
        tag_files: 0,
    }
}

#[test]
fn a_goal_the_card_already_meets_refuses_the_start_even_with_ignore_warnings() {
    let mut host = FakeHost::default();
    let mut session = Session::new(options(7));
    session.set_pool(pool_with_two_inverse_squares());
    two_teams(&mut host, &mut session);

    run(&mut session, &mut host, None, "goal items 2");
    let refused = "The goal is set to 2 items, but the card already has 2 items completed!";
    assert_eq!(run(&mut session, &mut host, None, "start"), refused);
    assert_eq!(run(&mut session, &mut host, None, "start ignore_warnings"), refused, "nothing overrides it");
    assert!(session.is_pregame());
    assert!(host.titles.is_empty(), "no title for a refused start");

    // One item more than the card holds before play: the start goes ahead.
    run(&mut session, &mut host, None, "goal items 3");
    assert_eq!(run(&mut session, &mut host, None, "start"), "Game started.");
    assert_eq!(host.titles.len(), 2);
}

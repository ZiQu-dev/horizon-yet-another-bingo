//! Lobby ready timer. Expected tick numbers are counted by hand: `tick` call 1 starts the countdown, and call n has
//! elapsed `50 * (n - 1)` ms. The countdown ends on the first call where `(1 - ready/online) * timer_ms - elapsed <= 0`.
use horizon_bingo::buf::decode;
use horizon_bingo::game::GameStatus;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::server::ready::{encode_update_v2, ReadyOptions, ReadyTimer};
use horizon_bingo::text::Text;

const A: u128 = 1;
const B: u128 = 2;
const C: u128 = 3;
const D: u128 = 4;

fn timer(timer_secs: i32) -> ReadyTimer {
    ReadyTimer::new(ReadyOptions { timer_secs, min_players: 1 })
}

/// Calls `tick` for call numbers `from..=to` and returns the numbers of the calls that reported the end.
fn run(timer: &mut ReadyTimer, online: &[u128], eligible: bool, from: u32, to: u32) -> Vec<u32> {
    (from..=to).filter(|_| timer.tick(online, eligible)).collect()
}

#[test]
fn lone_ready_player_ends_on_the_second_call() {
    // Ready share 1 of 1 makes the countdown 0 ms, so the call after the start (elapsed 50 ms) ends it.
    let mut t = timer(10);
    t.set_ready(A, true);
    assert_eq!(run(&mut t, &[A], true, 1, 3), vec![2]);
}

#[test]
fn half_ready_countdown_ends_on_call_101() {
    // 10 s = 10000 ms, half ready gives 5000 ms. Call 101 has elapsed 5000 ms: remaining 0 ends it, call 100 does not.
    let mut t = timer(10);
    t.set_ready(A, true);
    assert_eq!(run(&mut t, &[A, B], true, 1, 150), vec![101]);
}

#[test]
fn ready_share_truncates_to_whole_milliseconds() {
    // 1 of 3 ready: (1 - 1/3) * 10000 = 6666.67, truncated to 6666 ms. Call 134 has elapsed 6650 ms, call 135 has 6700.
    let mut t = timer(10);
    t.set_ready(A, true);
    assert_eq!(run(&mut t, &[A, B, C], true, 1, 200), vec![135]);
}

#[test]
fn second_player_becoming_ready_ends_the_countdown_at_once() {
    // Call 50 has elapsed 2450 ms of a 5000 ms countdown. B ready makes the share 1, so call 51 (2500 ms) ends it.
    let mut t = timer(10);
    t.set_ready(A, true);
    assert!(run(&mut t, &[A, B], true, 1, 50).is_empty());
    t.set_ready(B, true);
    assert_eq!(run(&mut t, &[A, B], true, 51, 60), vec![51]);
}

#[test]
fn a_player_unreadying_lengthens_the_countdown() {
    // 2 of 4 ready gives 5000 ms. After call 50 (2450 ms), B unreadies: 1 of 4 gives 7500 ms, ended on call 151.
    let mut t = timer(10);
    let online = [A, B, C, D];
    t.set_ready(A, true);
    t.set_ready(B, true);
    assert!(run(&mut t, &online, true, 1, 50).is_empty());
    t.set_ready(B, false);
    assert_eq!(run(&mut t, &online, true, 51, 160), vec![151]);
}

#[test]
fn offline_players_are_not_counted_as_ready() {
    let mut t = timer(10);
    t.set_ready(A, true);
    t.set_ready(C, true);
    assert_eq!(t.ready_count(&[B]), 0);
    assert_eq!(t.ready_count(&[A, B]), 1);
    assert_eq!(t.ready_count(&[A, C, B]), 2);
    // A is ready but offline, so nothing starts.
    assert!(run(&mut t, &[B], true, 1, 5).is_empty());
}

#[test]
fn timer_needs_min_players_online() {
    let mut t = ReadyTimer::new(ReadyOptions { timer_secs: 10, min_players: 2 });
    t.set_ready(A, true);
    assert!(run(&mut t, &[A], true, 1, 10).is_empty());
    assert!(!t.update_packet(A, &[A], "lobby", GameStatus::Pregame).is_running);
    // Two online, one ready: half of 10000 ms, starts on call 11 and ends on call 111.
    assert_eq!(run(&mut t, &[A, B], true, 11, 150), vec![111]);
}

#[test]
fn ineligible_lobby_does_not_start() {
    let mut t = timer(10);
    t.set_ready(A, true);
    assert!(run(&mut t, &[A, B], false, 1, 20).is_empty());
    // Eligible again: call 21 starts, call 121 ends (half of 10000 ms).
    assert_eq!(run(&mut t, &[A, B], true, 21, 130), vec![121]);
}

#[test]
fn ineligible_lobby_resets_a_running_countdown() {
    let mut t = timer(10);
    let online = [A, B];
    t.set_ready(A, true);
    assert!(run(&mut t, &online, true, 1, 50).is_empty());
    assert!(run(&mut t, &online, false, 51, 51).is_empty());
    assert!(!t.is_ready(A));
    assert!(!t.update_packet(A, &online, "lobby", GameStatus::Pregame).is_running);
    // A starts over: call 52 starts from zero, so the end is call 152, not earlier.
    t.set_ready(A, true);
    assert_eq!(run(&mut t, &online, true, 52, 160), vec![152]);
}

#[test]
fn cancel_stops_the_timer_and_clears_readiness() {
    let mut t = timer(10);
    let online = [A, B];
    t.set_ready(A, true);
    assert!(run(&mut t, &online, true, 1, 10).is_empty());
    t.cancel();
    assert!(!t.is_ready(A));
    assert!(!t.update_packet(A, &online, "lobby", GameStatus::Pregame).is_running);
    // Nobody is ready, so no restart. Ready again: the cancel is lifted and call 31 starts, ending on call 131.
    assert!(run(&mut t, &online, true, 11, 30).is_empty());
    t.set_ready(A, true);
    assert_eq!(run(&mut t, &online, true, 31, 140), vec![131]);
}

#[test]
fn remove_forgets_only_that_player() {
    let mut t = timer(10);
    t.set_ready(A, true);
    t.set_ready(B, true);
    t.remove(A);
    assert!(!t.is_ready(A));
    assert!(t.is_ready(B));
    assert_eq!(t.ready_count(&[A, B]), 1);
}

#[test]
fn clear_resets_readiness_and_restarts_from_zero() {
    let mut t = timer(10);
    let online = [A, B];
    t.set_ready(A, true);
    assert!(run(&mut t, &online, true, 1, 10).is_empty());
    t.clear();
    assert!(!t.is_ready(A));
    let idle = t.update_packet(A, &online, "lobby", GameStatus::Pregame);
    assert!(!idle.is_running);
    assert_eq!((idle.remaining_millis, idle.total_millis, idle.ready_players), (0, 0, 0));
    // B alone is ready again: call 13 starts from zero, ends on call 113.
    assert!(run(&mut t, &online, true, 11, 12).is_empty());
    t.set_ready(B, true);
    assert_eq!(run(&mut t, &online, true, 13, 120), vec![113]);
}

#[test]
fn update_packet_reports_the_countdown_and_round_trips_v2() {
    let mut t = timer(10);
    let online = [A, B];
    t.set_ready(A, true);
    // Start on call 1, then calls 2..=40: elapsed 1950 ms of 5000 ms, so 3050 ms remain.
    assert!(run(&mut t, &online, true, 1, 40).is_empty());

    let packet = t.update_packet(A, &online, "lobby", GameStatus::Pregame);
    assert!(packet.is_running);
    assert!(packet.is_ready);
    assert_eq!(packet.remaining_millis, 3050);
    assert_eq!(packet.total_millis, 10_000);
    assert_eq!((packet.ready_players, packet.total_players), (1, 2));
    assert_eq!(packet.title, Some(Text::empty()));
    assert_eq!(packet.subtitle, None);
    assert!(packet.can_send_ready);
    assert!(packet.vote_options.is_empty());
    assert_eq!(packet.vote_pending, None);
    assert!(!t.update_packet(B, &online, "lobby", GameStatus::Pregame).is_ready);

    let (path, bytes) = encode_update_v2(&packet);
    assert_eq!(path, "ready_timer_update_v2");
    let decoded = decode(&bytes, ReadyUpdatePacket::read_v2).expect("v2 packet decodes");
    assert_eq!(decoded, packet);
    assert_eq!(decoded.state.identifier, "lobby");
    assert_eq!(decoded.state.status, GameStatus::Pregame);
}

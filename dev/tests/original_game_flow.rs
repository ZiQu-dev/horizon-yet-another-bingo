//! Game lifecycle and timers against the original (`GameService.kt`, `CountdownController.kt`, `CountdownService.kt`,
//! `TimerCheck.kt`, `TimerPacketController.kt`, `GameOverController.kt`). Expected values are hand-traced from those
//! files at 20 server ticks per second; the line numbers are in each test. Tests that expose a real difference are
//! `#[ignore]`d with the reason.
use horizon_bingo::game::GameStatus;
use horizon_bingo::server::game::{Event, Game, GameOptions};

fn options(countdown_secs: i32, time_limit_secs: Option<i32>) -> GameOptions {
    GameOptions { countdown_secs, time_limit_secs }
}

/// Runs ticks `from + 1` through `to` and returns the non-empty results, numbered by the absolute tick count.
fn ticks(game: &mut Game, from: u32, to: u32) -> Vec<(u32, Vec<Event>)> {
    (from + 1..=to)
        .filter_map(|tick| {
            let events = game.tick();
            (!events.is_empty()).then_some((tick, events))
        })
        .collect()
}

/// The seconds of the Timer event sent at `tick`, if any.
fn timer_at(log: &[(u32, Vec<Event>)], tick: u32) -> Option<i32> {
    log.iter()
        .find(|(t, _)| *t == tick)?
        .1
        .iter()
        .find_map(|event| match event {
            Event::Timer { seconds_remaining } => Some(*seconds_remaining),
            _ => None,
        })
}

fn ended_at(log: &[(u32, Vec<Event>)]) -> Option<u32> {
    log.iter()
        .find(|(_, events)| events.iter().any(|e| matches!(e, Event::Ended { .. })))
        .map(|(tick, _)| *tick)
}

#[test]
fn countdown_numbers_are_sent_once_a_second_from_start() {
    // CountdownController.kt:96-108 schedules one send per second, 3 at 0s, 2 at 1s, 1 at 2s, 0 at 3s (the 0 starts the game).
    // CountdownService.kt sends each number as a title. 20 ticks per second: 2 at tick 20, 1 at 40, 0 at 60.
    let mut game = Game::new(options(3, None));
    let start = game.start().unwrap();
    assert_eq!(start, vec![Event::StatusChanged(GameStatus::Starting), Event::Countdown { seconds: 3 }]);

    let log = ticks(&mut game, 0, 60);
    assert_eq!(
        log,
        vec![
            (20, vec![Event::Countdown { seconds: 2 }]),
            (40, vec![Event::Countdown { seconds: 1 }]),
            (60, vec![Event::Countdown { seconds: 0 }, Event::StatusChanged(GameStatus::Playing)]),
        ]
    );
    assert_eq!(game.status(), GameStatus::Playing);
}

#[test]
fn countdown_zero_with_no_delay_starts_at_once() {
    // CountdownController.kt:118-126: with no countdown and no delay, it sends 0 and starts the game immediately.
    let mut game = Game::new(options(0, None));
    let events = game.start().unwrap();
    assert_eq!(
        events,
        vec![
            Event::StatusChanged(GameStatus::Starting),
            Event::Countdown { seconds: 0 },
            Event::StatusChanged(GameStatus::Playing),
        ]
    );
}

#[test]
#[ignore = "differs from the original: the original waits countdownDelayTicks (default 100 = 5 s, BingoConfig.kt:74) before the first number"]
fn countdown_waits_100_ticks_before_its_first_number() {
    // CountdownController.kt:118-119 sets the delay to 100 on entering COUNTDOWN; each TickEvent.Start counts it down,
    // and the tick that takes it below zero (tick 101, :150-152) schedules the countdown. So nothing is sent on start.
    let mut game = Game::new(options(3, None));
    let start = game.start().unwrap();
    assert!(!start.iter().any(|e| matches!(e, Event::Countdown { .. })));
    let log = ticks(&mut game, 0, 101);
    assert_eq!(log.first().map(|(t, _)| *t), Some(101));
    assert_eq!(log[0].1, vec![Event::Countdown { seconds: 3 }]);
}

#[test]
fn countdown_time_does_not_count_toward_the_time_limit() {
    // TimerCheck.kt:19-20: startedAt is set when PLAYING is entered, not when the countdown starts.
    // With a 3 s countdown (60 ticks) and a 10 s limit (200 ticks), the game ends at tick 60 + 200 = 260.
    let mut game = Game::new(options(3, Some(10)));
    game.start().unwrap();
    let log = ticks(&mut game, 0, 300);
    assert_eq!(ended_at(&log), Some(260));
}

#[test]
fn time_limit_ends_the_game_with_no_winner_at_the_limit() {
    // TimerCheck.kt:37-43: when duration >= timeLimit, gameService.end(GameEndReason.Timer), which has no winner.
    let mut game = Game::new(options(0, Some(5)));
    game.start().unwrap();
    let log = ticks(&mut game, 0, 100);
    let end = log.iter().find_map(|(_, events)| {
        events.iter().find_map(|e| match e {
            Event::Ended { winner } => Some(winner.clone()),
            _ => None,
        })
    });
    assert_eq!(end, Some(None));
    assert_eq!(ended_at(&log), Some(100));
    assert_eq!(game.status(), GameStatus::Postgame);
}

#[test]
fn timer_shows_whole_seconds_left_at_each_20_tick_boundary() {
    // TimerPacketController.kt:24-29: the packet is the whole seconds left, sent when that changes.
    // With a 10 s limit, at each whole second j of play the remaining value is 10 - j.
    let mut game = Game::new(options(0, Some(10)));
    game.start().unwrap();
    let log = ticks(&mut game, 0, 200);
    for j in 1..10u32 {
        assert_eq!(timer_at(&log, 20 * j), Some(10 - j as i32), "at play second {j}");
    }
}

#[test]
#[ignore = "differs from the original: the original never sends the full limit; its first packet is limit-1 on the first tick"]
fn timer_first_packet_is_one_second_under_the_limit() {
    // TimerPacketController.kt:17,27: prev starts at -1, and the first game tick after PLAYING has a duration of a few
    // milliseconds, so the first packet is floor(10 - ~0.05) = 9 on play tick 1. The port sends 10 when play begins.
    let mut game = Game::new(options(0, Some(10)));
    let start = game.start().unwrap();
    assert!(!start.iter().any(|e| matches!(e, Event::Timer { .. })));
    let log = ticks(&mut game, 0, 1);
    assert_eq!(timer_at(&log, 1), Some(9));
}

#[test]
#[ignore = "differs from the original: the original's second change is on play tick 21 (ceil), the port's on tick 20 (floor)"]
fn timer_changes_one_tick_after_each_whole_second() {
    // TimerPacketController.kt:24-27 with duration = k * 50 ms: floor(10 - k/20) changes at k = 1, 21, 41, ... so the
    // packet showing 8 comes on play tick 21. The port only sends at multiples of 20 and shows 9 there.
    let mut game = Game::new(options(0, Some(10)));
    game.start().unwrap();
    let log = ticks(&mut game, 0, 21);
    assert_eq!(timer_at(&log, 21), Some(8));
}

#[test]
fn game_timer_stops_when_the_game_ends() {
    // BingoState.kt:194-199: the duration uses endedAt once it is set, so the timer is frozen after the end.
    let mut game = Game::new(options(0, Some(5)));
    game.start().unwrap();
    ticks(&mut game, 0, 100);
    assert_eq!(game.status(), GameStatus::Postgame);
    let frozen = game.elapsed_ms();
    assert!(ticks(&mut game, 100, 400).is_empty());
    assert_eq!(game.elapsed_ms(), frozen);
    assert_eq!(game.status(), GameStatus::Postgame);
}

#[test]
fn reset_during_the_countdown_drops_the_pending_numbers() {
    // CountdownController.kt:103-104: a scheduled number is skipped unless the state is still COUNTDOWN.
    // After a reset to Pregame nothing is sent, and a new start counts from 3 again.
    let mut game = Game::new(options(3, None));
    game.start().unwrap();
    assert!(ticks(&mut game, 0, 10).is_empty());
    game.reset();
    assert!(ticks(&mut game, 10, 100).is_empty());
    assert_eq!(game.status(), GameStatus::Pregame);

    let restart = game.start().unwrap();
    assert_eq!(restart, vec![Event::StatusChanged(GameStatus::Starting), Event::Countdown { seconds: 3 }]);
    let log = ticks(&mut game, 0, 20);
    assert_eq!(log, vec![(20, vec![Event::Countdown { seconds: 2 }])]);
}

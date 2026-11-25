//! Game state machine and timer. Expected ticks are counted by hand: 20 server ticks per second.
use horizon_bingo::game::GameStatus;
use horizon_bingo::server::game::{Event, Game, GameOptions};

fn options(countdown_secs: i32, time_limit_secs: Option<i32>) -> GameOptions {
    GameOptions { countdown_secs, time_limit_secs }
}

/// Runs ticks `from + 1` through `to` (so `from` ticks ran before) and returns the non-empty results,
/// numbered by the same absolute count.
fn ticks(game: &mut Game, from: u32, to: u32) -> Vec<(u32, Vec<Event>)> {
    (from + 1..=to)
        .filter_map(|tick| {
            let events = game.tick();
            (!events.is_empty()).then_some((tick, events))
        })
        .collect()
}

fn status_changed(status: GameStatus) -> Event {
    Event::StatusChanged(status)
}

#[test]
fn default_options_are_five_seconds_and_no_limit() {
    assert_eq!(GameOptions::default(), options(3, None));
}

#[test]
fn new_game_is_pregame_and_silent_until_started() {
    let mut game = Game::new(GameOptions::default());
    assert_eq!(game.status(), GameStatus::Pregame);
    assert!(ticks(&mut game, 0, 100).is_empty());
    assert_eq!(game.status(), GameStatus::Pregame);
    assert_eq!(game.elapsed_ms(), 0);
    assert_eq!(game.remaining_ms(), None);
}

#[test]
fn start_enters_starting_and_waits_out_the_delay_before_the_first_number() {
    let mut game = Game::new(options(3, None));
    let events = game.start().unwrap();
    assert_eq!(events, vec![status_changed(GameStatus::Starting)]);
    assert_eq!(game.status(), GameStatus::Starting);
}

#[test]
fn three_second_countdown_ticks_down_after_the_100_tick_delay() {
    let mut game = Game::new(options(3, None));
    game.start().unwrap();

    // The 100 tick delay is silent; the number 3 comes on tick 101, then one a second (20 ticks) later.
    assert!(ticks(&mut game, 0, 100).is_empty());
    assert_eq!(game.status(), GameStatus::Starting);

    assert_eq!(
        ticks(&mut game, 100, 161),
        vec![
            (101, vec![Event::Countdown { seconds: 3 }]),
            (121, vec![Event::Countdown { seconds: 2 }]),
            (141, vec![Event::Countdown { seconds: 1 }]),
            (161, vec![Event::Countdown { seconds: 0 }, status_changed(GameStatus::Playing)]),
        ]
    );
    assert_eq!(game.status(), GameStatus::Playing);
}

#[test]
fn countdown_stops_once_play_begins() {
    let mut game = Game::new(options(1, None));
    game.start().unwrap();
    assert_eq!(ticks(&mut game, 0, 121).len(), 2);
    assert_eq!(game.status(), GameStatus::Playing);
    assert!(ticks(&mut game, 121, 220).is_empty());
}

#[test]
fn zero_countdown_begins_play_on_start() {
    let mut game = Game::new(options(0, None));
    let events = game.start().unwrap();
    assert_eq!(
        events,
        vec![
            status_changed(GameStatus::Starting),
            Event::Countdown { seconds: 0 },
            status_changed(GameStatus::Playing),
        ]
    );
    assert_eq!(game.status(), GameStatus::Playing);
}

#[test]
fn negative_countdown_counts_as_zero() {
    let mut game = Game::new(options(-4, None));
    let events = game.start().unwrap();
    assert_eq!(game.status(), GameStatus::Playing);
    assert!(events.contains(&Event::Countdown { seconds: 0 }));
}

#[test]
fn start_outside_pregame_is_an_error_and_does_not_restart_the_countdown() {
    let mut game = Game::new(options(3, None));
    game.start().unwrap();
    assert!(game.start().is_err());
    assert_eq!(game.status(), GameStatus::Starting);
    // Play still begins at tick 161, not 161 plus the refused start.
    assert_eq!(ticks(&mut game, 0, 161).last().unwrap().0, 161);
    assert_eq!(game.status(), GameStatus::Playing);

    assert!(game.start().is_err());
    assert_eq!(game.status(), GameStatus::Playing);

    game.end(None).unwrap();
    assert!(game.start().is_err());
    assert_eq!(game.status(), GameStatus::Postgame);
}

#[test]
fn end_is_only_allowed_while_playing() {
    let mut game = Game::new(options(3, None));
    assert!(game.end(None).is_err());
    assert_eq!(game.status(), GameStatus::Pregame);

    game.start().unwrap();
    assert!(game.end(None).is_err());
    assert_eq!(game.status(), GameStatus::Starting);

    ticks(&mut game, 0, 161);
    game.end(Some("red".to_string())).unwrap();
    assert!(game.end(None).is_err());
    assert_eq!(game.status(), GameStatus::Postgame);
}

#[test]
fn end_while_playing_announces_postgame_and_winner() {
    let mut game = Game::new(options(0, None));
    game.start().unwrap();
    let events = game.end(Some("red".to_string())).unwrap();
    assert_eq!(
        events,
        vec![status_changed(GameStatus::Postgame), Event::Ended { winner: Some("red".to_string()) }]
    );
    assert_eq!(game.status(), GameStatus::Postgame);
}

#[test]
fn reset_from_every_state_returns_to_pregame_and_can_start_again() {
    let mut pregame = Game::new(options(0, None));
    let mut starting = Game::new(options(3, None));
    starting.start().unwrap();
    let mut playing = Game::new(options(0, None));
    playing.start().unwrap();
    let mut postgame = Game::new(options(0, None));
    postgame.start().unwrap();
    postgame.end(None).unwrap();

    for game in [&mut pregame, &mut starting, &mut playing, &mut postgame] {
        assert_eq!(game.reset(), vec![status_changed(GameStatus::Pregame), Event::Reset]);
        assert_eq!(game.status(), GameStatus::Pregame);
        assert!(ticks(game, 0, 25).is_empty());
        assert!(game.start().is_ok());
    }
}

#[test]
fn reset_clears_elapsed_time() {
    let mut game = Game::new(options(0, Some(10)));
    game.start().unwrap();
    ticks(&mut game, 0, 7);
    assert_eq!(game.elapsed_ms(), 350);
    game.reset();
    assert_eq!(game.elapsed_ms(), 0);
    assert_eq!(game.remaining_ms(), Some(10_000));
}

#[test]
fn two_second_limit_ends_after_exactly_40_playing_ticks() {
    let mut game = Game::new(options(0, Some(2)));
    let start = game.start().unwrap();
    assert_eq!(
        start,
        vec![
            status_changed(GameStatus::Starting),
            Event::Countdown { seconds: 0 },
            status_changed(GameStatus::Playing),
        ]
    );

    // Play tick 1 already has time on the clock, so the first packet is 1, not 2; it changes to 0 on tick 21 (1950 ms
    // left on tick 1, 950 ms on tick 21); tick 40 ends the game.
    assert_eq!(game.status(), GameStatus::Playing);
    assert_eq!(
        ticks(&mut game, 0, 40),
        vec![
            (1, vec![Event::Timer { seconds_remaining: 1 }]),
            (21, vec![Event::Timer { seconds_remaining: 0 }]),
            (
                40,
                vec![
                    status_changed(GameStatus::Postgame),
                    Event::Ended { winner: None },
                ],
            ),
        ]
    );
    assert_eq!(game.status(), GameStatus::Postgame);
    assert!(ticks(&mut game, 40, 90).is_empty());
}

#[test]
fn time_limit_counts_only_playing_ticks_not_countdown_ticks() {
    // A one-second countdown (after the 100 tick delay), then a one-second limit. Play begins at tick 121 and ends 20
    // ticks of play later, at tick 141; the timer shows 0 from play tick 1.
    let mut game = Game::new(options(1, Some(1)));
    assert_eq!(game.start().unwrap().len(), 1);
    assert_eq!(
        ticks(&mut game, 0, 121),
        vec![
            (101, vec![Event::Countdown { seconds: 1 }]),
            (121, vec![Event::Countdown { seconds: 0 }, status_changed(GameStatus::Playing)]),
        ]
    );
    assert_eq!(
        ticks(&mut game, 121, 200),
        vec![
            (122, vec![Event::Timer { seconds_remaining: 0 }]),
            (141, vec![status_changed(GameStatus::Postgame), Event::Ended { winner: None }]),
        ]
    );
}

#[test]
fn zero_limit_ends_on_the_first_playing_tick() {
    let mut game = Game::new(options(0, Some(0)));
    let start = game.start().unwrap();
    assert!(!start.iter().any(|e| matches!(e, Event::Timer { .. })));
    assert_eq!(game.status(), GameStatus::Playing);
    assert_eq!(
        game.tick(),
        vec![Event::Timer { seconds_remaining: 0 }, status_changed(GameStatus::Postgame), Event::Ended { winner: None }]
    );
    assert_eq!(game.elapsed_ms(), 50);
}

#[test]
fn elapsed_and_remaining_start_at_zero_and_follow_playing_ticks() {
    let mut game = Game::new(options(3, Some(2)));
    game.start().unwrap();
    assert_eq!(game.elapsed_ms(), 0);
    assert_eq!(game.remaining_ms(), Some(2_000));

    // The countdown does not count against the limit.
    ticks(&mut game, 0, 161);
    assert_eq!(game.elapsed_ms(), 0);
    assert_eq!(game.remaining_ms(), Some(2_000));

    ticks(&mut game, 161, 168);
    assert_eq!(game.elapsed_ms(), 350);
    assert_eq!(game.remaining_ms(), Some(1_650));
}

#[test]
fn elapsed_time_is_frozen_after_end() {
    let mut game = Game::new(options(0, Some(2)));
    game.start().unwrap();
    ticks(&mut game, 0, 40);
    assert_eq!(game.status(), GameStatus::Postgame);
    assert_eq!(game.elapsed_ms(), 2_000);
    assert_eq!(game.remaining_ms(), Some(0));

    ticks(&mut game, 40, 140);
    assert_eq!(game.elapsed_ms(), 2_000);
}

#[test]
fn manual_end_freezes_elapsed_time_at_the_tick_it_happened() {
    let mut game = Game::new(options(0, None));
    game.start().unwrap();
    ticks(&mut game, 0, 33);
    game.end(None).unwrap();
    ticks(&mut game, 33, 233);
    assert_eq!(game.elapsed_ms(), 1_650);
}

#[test]
fn no_limit_means_no_remaining_time_and_no_timer_events() {
    let mut game = Game::new(options(0, None));
    let start = game.start().unwrap();
    assert!(!start.iter().any(|e| matches!(e, Event::Timer { .. })));
    assert!(ticks(&mut game, 0, 100).is_empty());
    assert_eq!(game.status(), GameStatus::Playing);
    assert_eq!(game.elapsed_ms(), 5_000);
    assert_eq!(game.remaining_ms(), None);
}

#[test]
fn negative_time_limit_counts_as_zero() {
    let mut game = Game::new(options(0, Some(-3)));
    game.start().unwrap();
    assert_eq!(game.remaining_ms(), Some(0));
    assert_eq!(
        game.tick(),
        vec![Event::Timer { seconds_remaining: 0 }, status_changed(GameStatus::Postgame), Event::Ended { winner: None }]
    );
}

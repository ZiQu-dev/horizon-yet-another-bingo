//! The client state against the original client's rules (BingoHudController.kt, ScorePacketReceiver.kt) and against
//! payloads from the real server (tests/captures) and from the real mod's encoders (tests/oracle/*.tsv).
//! Every expected number is worked out by hand from the Kotlin source, not taken from the code under test.
use horizon_bingo::buf::{decode, encode, DecodeError};
use horizon_bingo::channels::id;
use horizon_bingo::game_message::GameMessagePacket;
use horizon_bingo::game_over::GameOverPacket;
use horizon_bingo::ready_update::ReadyUpdatePacket;
use horizon_bingo::score_update::{ScoredItemPacket, TeamScore};
use horizon_bingo::state::{Cards, ClientEvent, Handled, HudState, SoundKey, SHUFFLE_DURATION_MS, TILES_PER_CARD};
use horizon_bingo::text::Text;
use horizon_bingo::tooltip_v2::TooltipPacket;
use horizon_bingo::update_card_tiles::CardTilesPacket;

fn hex(s: &str) -> Vec<u8> {
    let s = s.trim();
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn capture(name: &str) -> Vec<u8> {
    let text = match name {
        "display_cards" => include_str!("captures/display_cards.hex"),
        "update_card_tiles" => include_str!("captures/update_card_tiles.hex"),
        "game_status" => include_str!("captures/game_status.hex"),
        "stats_check" => include_str!("captures/stats_check.hex"),
        other => panic!("no capture {other}"),
    };
    hex(text)
}

/// A row of any oracle file, by name.
fn row(name: &str) -> Vec<u8> {
    let all = [
        include_str!("oracle/game_message.tsv"),
        include_str!("oracle/game_over.tsv"),
        include_str!("oracle/ready_update.tsv"),
        include_str!("oracle/tooltip_v2.tsv"),
        include_str!("oracle/card_image.tsv"),
    ];
    for tsv in all {
        if let Some(line) = tsv.lines().find(|l| l.split('\t').next() == Some(name)) {
            return hex(line.split('\t').nth(2).unwrap());
        }
    }
    panic!("no oracle row {name}");
}

fn feed(state: &mut HudState, path: &str, data: &[u8]) -> Handled {
    state.on_packet(&id(path), data).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn the_real_servers_packets_build_a_card() {
    let mut state = HudState::new();
    state.set_now(5_000);
    assert!(matches!(feed(&mut state, "display_cards", &capture("display_cards")), Handled::Applied { unread: 0 }));
    assert!(matches!(feed(&mut state, "update_card_tiles", &capture("update_card_tiles")), Handled::Applied { unread: 0 }));
    feed(&mut state, "game_status", &capture("game_status"));
    feed(&mut state, "stats_check", &capture("stats_check"));

    let team = Some("bingo_red".to_string());
    let card = &state.cards[&team];
    // What the captures hold, as read by hand from the dump: team bingo_red, one player, 25 tiles.
    assert_eq!(card.display.players.len(), 1);
    assert_eq!(card.display.players[0].name, "RegTest_control");
    assert_eq!(card.tiles.len(), TILES_PER_CARD);
    assert_eq!(card.tiles[0].id.as_deref(), Some("minecraft:dark_oak_pressure_plate"));
    // `shouldNotify` is false in the capture, so no tile flashes.
    assert!(card.tile_updated_at.iter().all(Option::is_none));
    assert!(card.dirty);
    assert!(state.game_status.is_some());
    assert_eq!(state.stats_hash.as_ref().map(String::len), Some(128), "a SHA-512 in hex");
    assert_eq!(state.unread_bytes, 0);
}

#[test]
fn other_channels_and_unhandled_ones_are_ignored() {
    let mut state = HudState::new();
    assert_eq!(state.on_packet("minecraft:brand", &[1, 2, 3]).unwrap(), Handled::Ignored);
    assert_eq!(state.on_packet("voicechat:secret", &[]).unwrap(), Handled::Ignored);
    assert_eq!(state.on_packet(&id("game_reset"), &[]).unwrap(), Handled::Ignored, "the client has no handler for game_reset");
    assert_eq!(state.on_packet(&id("not_a_channel"), &[]).unwrap(), Handled::Ignored);
    assert!(state.cards.is_empty() && state.take_events().is_empty());
}

#[test]
fn a_payload_that_does_not_decode_changes_nothing() {
    let mut state = HudState::new();
    let mut bytes = capture("display_cards");
    bytes.pop();
    assert!(matches!(state.on_packet(&id("display_cards"), &bytes), Err(DecodeError::UnexpectedEnd { .. })));
    assert!(state.cards.is_empty(), "no half-built card");
}

#[test]
fn bytes_after_a_packet_are_ignored_as_the_kotlin_readers_do_but_counted() {
    let mut state = HudState::new();
    let payload = [0, 0, 0, 7, 0xaa, 0xbb, 0xcc]; // a timer of 7 seconds and 3 stray bytes
    assert_eq!(state.on_packet(&id("timer"), &payload).unwrap(), Handled::Applied { unread: 3 });
    assert_eq!(state.timer_seconds, Some(7));
    assert_eq!(state.unread_bytes, 3);
}

fn int(n: i32) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}

#[test]
fn the_timer_ticks_on_odd_seconds_from_1_to_20() {
    // ScorePacketReceiver: `secondsRemaining in 1..20 && secondsRemaining % 2 != 0`, volume = 1 - s/20,
    // played at volume * 0.8 and pitch volume * 0.5 + 0.5.
    let mut state = HudState::new();
    for s in -2..=25 {
        feed(&mut state, "timer", &int(s));
        let events = state.take_events();
        let ticks = (1..=20).contains(&s) && s % 2 != 0;
        assert_eq!(events.len(), usize::from(ticks), "second {s}");
    }
    feed(&mut state, "timer", &int(5));
    // s = 5: volume = 1 - 5/20 = 0.75; played at 0.75 * 0.8 = 0.6 and pitch 0.75 * 0.5 + 0.5 = 0.875.
    assert_eq!(state.take_events(), [ClientEvent::Sound { key: SoundKey::TimerTick, volume: 0.6, pitch: 0.875 }]);
    feed(&mut state, "timer", &int(19));
    // s = 19: volume = 0.05; 0.05 * 0.8 = 0.04 and 0.05 * 0.5 + 0.5 = 0.525.
    match &state.take_events()[..] {
        [ClientEvent::Sound { key: SoundKey::TimerTick, volume, pitch }] => {
            assert!((volume - 0.04).abs() < 1e-6 && (pitch - 0.525).abs() < 1e-6, "{volume} {pitch}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_countdown_plays_its_sounds_on_3_2_1_and_0() {
    let mut state = HudState::new();
    for (s, want) in [(4, None), (3, Some(SoundKey::StartCountdown)), (2, Some(SoundKey::StartCountdown)), (1, Some(SoundKey::StartCountdown)), (0, Some(SoundKey::StartRelease)), (-1, None)] {
        feed(&mut state, "countdown", &int(s));
        let keys: Vec<_> = state
            .take_events()
            .into_iter()
            .filter_map(|e| if let ClientEvent::Sound { key, .. } = e { Some(key) } else { None })
            .collect();
        assert_eq!(keys, want.into_iter().collect::<Vec<_>>(), "countdown {s}");
    }
    assert_eq!(state.countdown_seconds, Some(-1));
}

#[test]
fn scoring_plays_the_scorers_sound_with_pitch_by_items() {
    let mut state = HudState::new();
    let packet = |viewer: bool, items: i32| ScoredItemPacket { is_viewer_on_team: viewer, team: "bingo_red".into(), score: TeamScore { items, lines: 1, cards: 0 } };
    feed(&mut state, "score_update", &encode(|w| packet(true, 10).write(w)));
    // pitch = 0.5 + 10 / 25 = 0.9
    assert_eq!(state.take_events(), [ClientEvent::Sound { key: SoundKey::ItemScored, volume: 1.0, pitch: 0.9 }]);
    feed(&mut state, "score_update", &encode(|w| packet(false, 25).write(w)));
    // the opponent's sound, pitch 0.5 + 25 / 25 = 1.5
    assert_eq!(state.take_events(), [ClientEvent::Sound { key: SoundKey::OpponentScored, volume: 1.0, pitch: 1.5 }]);
    assert_eq!(state.scores["bingo_red"], TeamScore { items: 25, lines: 1, cards: 0 });
    feed(&mut state, "scored_item_lost", &[]);
    assert_eq!(state.take_events(), [ClientEvent::Sound { key: SoundKey::ItemLost, volume: 0.8, pitch: 1.5 }]);
}

#[test]
fn a_completed_card_plays_win_or_loss_and_fireworks_only_for_the_winner() {
    let mut state = HudState::new();
    feed(&mut state, "card_completed", &[1]);
    assert_eq!(
        state.take_events(),
        [ClientEvent::Fireworks, ClientEvent::Sound { key: SoundKey::GameWon, volume: 1.0, pitch: 1.0 }]
    );
    feed(&mut state, "card_completed", &[0]);
    assert_eq!(state.take_events(), [ClientEvent::Sound { key: SoundKey::GameLost, volume: 1.0, pitch: 1.0 }]);
}

fn message(id_low: u128, seconds: i64, is_update: bool) -> Vec<u8> {
    let mut p = decode(&row("plain_message"), GameMessagePacket::read).unwrap();
    p.id = id_low;
    p.time_elapsed = horizon_bingo::game_status::JavaDuration { seconds, nanos: 0 };
    p.is_update = is_update;
    encode(|w| p.write(w).unwrap())
}

#[test]
fn messages_go_to_the_list_and_to_the_history_and_an_update_only_replaces_in_the_history() {
    let mut state = HudState::new();
    state.set_now(100);
    feed(&mut state, "game_message", &message(1, 30, false));
    feed(&mut state, "game_message", &message(2, 10, false));
    assert_eq!(state.messages.len(), 2);
    // the history is sorted by time elapsed: 10 s before 30 s
    assert_eq!(state.past_messages.iter().map(|m| m.packet.id).collect::<Vec<_>>(), [2, 1]);
    // an update of message 1 (now at 5 s): not added to the list, replaces its history entry and re-sorts
    state.set_now(200);
    feed(&mut state, "game_message", &message(1, 5, true));
    assert_eq!(state.messages.len(), 2, "an update is not a new message");
    assert_eq!(state.past_messages.iter().map(|m| m.packet.id).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(state.past_messages[0].created_at, 200);
    feed(&mut state, "game_message_clear", &[]);
    assert!(state.messages.is_empty() && state.past_messages.is_empty());
}

fn display(teams: &[Option<&str>]) -> Vec<u8> {
    let mut p = decode(&capture("display_cards"), horizon_bingo::display_cards::CardDisplayPacket::read).unwrap();
    let one = p.display[0].1.clone();
    p.display = teams.iter().map(|t| (t.map(str::to_string), one.clone())).collect();
    encode(|w| p.write(w).unwrap())
}

#[test]
fn a_card_that_is_no_longer_displayed_is_dropped() {
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[Some("bingo_red"), Some("bingo_blue")]));
    assert_eq!(state.cards.len(), 2);
    feed(&mut state, "display_cards", &display(&[Some("bingo_red")]));
    assert_eq!(state.cards.keys().cloned().collect::<Vec<_>>(), [Some("bingo_red".to_string())]);
}

#[test]
fn an_empty_display_resets_the_hud_only_while_no_game_status_has_arrived() {
    // BingoHudController.onCardDisplay: `displays.isEmpty() && state.gameStatus.isDefaultInstance` (identity with
    // DEFAULT: a received game_status, even an empty one, is not the default).
    let mut state = HudState::new();
    state.selected_team = Some("bingo_red".into());
    state.ready = Some(decode(&row("v2_zero"), ReadyUpdatePacket::read_v2).unwrap());
    state.game_over = Some(decode(&row("read_v2_minimal"), GameOverPacket::read_v2).unwrap());
    feed(&mut state, "display_cards", &display(&[]));
    assert!(state.selected_team.is_none() && state.ready.is_none() && state.game_over.is_none());

    let mut state = HudState::new();
    state.selected_team = Some("bingo_red".into());
    state.ready = Some(decode(&row("v2_zero"), ReadyUpdatePacket::read_v2).unwrap());
    feed(&mut state, "game_status", &capture("game_status"));
    feed(&mut state, "display_cards", &display(&[]));
    assert!(state.selected_team.is_some() && state.ready.is_some(), "a game_status arrived: not the default");
}

fn tiles(should_notify: bool, edit: impl FnOnce(&mut CardTilesPacket)) -> Vec<u8> {
    let mut p = decode(&capture("update_card_tiles"), CardTilesPacket::read).unwrap();
    p.should_notify = should_notify;
    edit(&mut p);
    encode(|w| p.write(w).unwrap())
}

#[test]
fn tiles_flash_when_their_id_changes_and_keep_their_time_otherwise() {
    let mut state = HudState::new();
    state.set_now(1_000);
    feed(&mut state, "update_card_tiles", &tiles(true, |_| {}));
    // A card that did not exist: every tile is new (existing id is none, the new ids are Some) -> all flash at 1000.
    let team = Some("bingo_red".to_string());
    assert!(state.cards[&team].tile_updated_at.iter().all(|t| *t == Some(1_000)));

    // The same tiles again: ids equal -> the old time is kept.
    state.set_now(2_000);
    feed(&mut state, "update_card_tiles", &tiles(true, |_| {}));
    assert!(state.cards[&team].tile_updated_at.iter().all(|t| *t == Some(1_000)));

    // Tile 3 gets another id -> it alone flashes again, at 3000.
    state.set_now(3_000);
    feed(&mut state, "update_card_tiles", &tiles(true, |p| {
        let tile = &mut p.tiles.iter_mut().find(|(i, _)| *i == 3).unwrap().1;
        tile.id = Some("minecraft:stone".into());
    }));
    let at = &state.cards[&team].tile_updated_at;
    assert_eq!(at[3], Some(3_000));
    assert!(at.iter().enumerate().all(|(i, t)| i == 3 || *t == Some(1_000)));

    // `shouldNotify` false (almost the whole card re-rolled): nothing flashes.
    state.set_now(4_000);
    feed(&mut state, "update_card_tiles", &tiles(false, |_| {}));
    assert!(state.cards[&team].tile_updated_at.iter().all(Option::is_none));
}

#[test]
fn a_tile_missing_from_an_update_keeps_its_old_tile() {
    let mut state = HudState::new();
    feed(&mut state, "update_card_tiles", &tiles(false, |_| {}));
    let before = state.cards[&Some("bingo_red".to_string())].tiles[7].clone();
    feed(&mut state, "update_card_tiles", &tiles(false, |p| p.tiles.retain(|(i, _)| *i != 7)));
    assert_eq!(state.cards[&Some("bingo_red".to_string())].tiles[7], before);
}

#[test]
fn a_card_is_not_shuffled_again_within_280_ms() {
    assert_eq!(SHUFFLE_DURATION_MS, 280);
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[Some("bingo_red")]));
    let shuffled = encode(|w| horizon_bingo::card_shuffled::CardShuffledPacket { team_key: Some("bingo_red".into()) }.write(w));
    let at = |s: &HudState| s.cards[&Some("bingo_red".to_string())].shuffled_at;
    state.set_now(1_000);
    feed(&mut state, "card_shuffled", &shuffled);
    assert_eq!(at(&state), Some(1_000));
    state.set_now(1_280); // exactly 280 ms later: `(now - shuffledAt) > SHUFFLE_DURATION` is false
    feed(&mut state, "card_shuffled", &shuffled);
    assert_eq!(at(&state), Some(1_000));
    state.set_now(1_281);
    feed(&mut state, "card_shuffled", &shuffled);
    assert_eq!(at(&state), Some(1_281));
    // A shuffle for a card that is not shown does nothing.
    let other = encode(|w| horizon_bingo::card_shuffled::CardShuffledPacket { team_key: Some("bingo_blue".into()) }.write(w));
    feed(&mut state, "card_shuffled", &other);
    assert_eq!(state.cards.len(), 1);
}

#[test]
fn the_ready_screen_is_told_only_when_something_visible_changed() {
    let mut state = HudState::new();
    let first = row("v2_zero");
    feed(&mut state, "ready_timer_update_v2", &first);
    assert_eq!(state.take_events(), [ClientEvent::HudChanged], "the first packet");
    feed(&mut state, "ready_timer_update_v2", &first);
    assert!(state.take_events().is_empty(), "the same packet again");
    let mut p = decode(&first, ReadyUpdatePacket::read_v2).unwrap();
    p.remaining_millis += 1000; // the countdown moving is not a state change
    feed(&mut state, "ready_timer_update_v2", &encode(|w| p.write_v2(w).unwrap()));
    assert!(state.take_events().is_empty(), "only the remaining time changed");
    p.is_ready = !p.is_ready;
    feed(&mut state, "ready_timer_update_v2", &encode(|w| p.write_v2(w).unwrap()));
    assert_eq!(state.take_events(), [ClientEvent::HudChanged]);
    assert_eq!(state.ready.as_ref().unwrap().remaining_millis, p.remaining_millis, "the newest packet is kept either way");
}

#[test]
fn the_end_screen_opens_for_a_new_result_and_not_for_an_update() {
    let mut state = HudState::new();
    let mut p = decode(&row("read_v2_minimal"), GameOverPacket::read_v2).unwrap();
    p.is_update = false;
    feed(&mut state, "game_over_v2", &encode(|w| p.write_v2(w).unwrap()));
    assert_eq!(state.take_events(), [ClientEvent::OpenEndScreen]);
    assert!(state.game_over.is_some());
    p.is_update = true;
    feed(&mut state, "game_over_v2", &encode(|w| p.write_v2(w).unwrap()));
    assert!(state.take_events().is_empty());
    // V1 is handled the same way.
    let mut v1 = decode(&row("read_v1_minimal"), GameOverPacket::read_v1).unwrap();
    v1.is_update = false;
    let mut other = HudState::new();
    feed(&mut other, "game_over", &encode(|w| v1.write_v1(w).unwrap()));
    assert_eq!(other.take_events(), [ClientEvent::OpenEndScreen]);
}

#[test]
fn open_card_card_image_tooltip_and_settings_are_stored() {
    let mut state = HudState::new();
    feed(&mut state, "open_card", &encode(|w| horizon_bingo::open_card::CardOpenPacket { team_key: Some("bingo_red".into()) }.write(w)));
    assert_eq!(state.take_events(), [ClientEvent::OpenCard(Some("bingo_red".into()))]);

    let png = encode(|w| horizon_bingo::card_image::CardImagePacket { id: "img".into(), image: Some(vec![1, 2, 3]) }.write(w));
    feed(&mut state, "card_image", &png);
    assert_eq!(state.images["img"], [1, 2, 3]);
    let gone = encode(|w| horizon_bingo::card_image::CardImagePacket { id: "img".into(), image: None }.write(w));
    feed(&mut state, "card_image", &gone);
    assert!(state.images.is_empty(), "a packet with no image removes it");

    let tip = row("high_bit_uuid");
    feed(&mut state, "tooltip_v2", &tip);
    assert_eq!(state.tooltips_by_entity.len(), 1);
}

#[test]
fn reset_all_forgets_everything_the_game_showed() {
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &capture("display_cards"));
    feed(&mut state, "game_status", &capture("game_status"));
    feed(&mut state, "game_message", &message(1, 1, false));
    state.reset_all();
    assert!(state.cards.is_empty() && state.messages.is_empty() && state.past_messages.is_empty() && state.game_status.is_none());
}

#[test]
fn cards_keep_the_order_they_were_first_shown_and_the_first_one_is_the_default_card() {
    // The original keeps a LinkedHashMap, and `getSelectedCard` falls back to `cards.keys.firstOrNull()`:
    // "yellow" shown first stays first even though "blue" sorts before it.
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[Some("bingo_yellow"), Some("bingo_blue")]));
    assert_eq!(state.cards.keys().cloned().collect::<Vec<_>>(), [Some("bingo_yellow".to_string()), Some("bingo_blue".to_string())]);
    assert_eq!(state.selected_card().unwrap().0, &Some("bingo_yellow".to_string()));
    // An update that lists them the other way round does not reorder the cards already shown.
    feed(&mut state, "display_cards", &display(&[Some("bingo_blue"), Some("bingo_yellow")]));
    assert_eq!(state.cards.keys().next(), Some(&Some("bingo_yellow".to_string())));
    // A selected team that is shown wins over the first card; one that is not shown falls back to the first.
    state.selected_team = Some("bingo_blue".into());
    assert_eq!(state.selected_card().unwrap().0, &Some("bingo_blue".to_string()));
    state.selected_team = Some("bingo_red".into());
    assert_eq!(state.selected_card().unwrap().0, &Some("bingo_yellow".to_string()));
    // A new card goes last.
    feed(&mut state, "display_cards", &display(&[Some("bingo_blue"), Some("bingo_yellow"), Some("bingo_aqua")]));
    assert_eq!(state.cards.keys().last(), Some(&Some("bingo_aqua".to_string())));
    // No cards, no selection.
    state.cards.clear();
    assert!(state.selected_card().is_none());
}

#[test]
fn the_end_screens_cards_outlive_the_servers_card_reset_and_exclude_the_preview_card() {
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[None, Some("bingo_red"), Some("bingo_blue")]));
    let mut p = decode(&row("read_v2_minimal"), GameOverPacket::read_v2).unwrap();
    p.is_update = false;
    feed(&mut state, "game_over_v2", &encode(|w| p.write_v2(w).unwrap()));
    // the preview card (no team) is not on the end screen
    let keys = |c: &horizon_bingo::state::Cards| c.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
    assert_eq!(keys(state.screen_cards()), [Some("bingo_red".to_string()), Some("bingo_blue".to_string())]);
    // the next game's lobby: only a new team's card is displayed, the old ones are gone from the live cards...
    feed(&mut state, "display_cards", &display(&[Some("bingo_green")]));
    assert_eq!(keys(&state.cards), [Some("bingo_green".to_string())]);
    // ...but the end screen keeps them, the newest first
    assert_eq!(
        keys(state.screen_cards()),
        [Some("bingo_green".to_string()), Some("bingo_red".to_string()), Some("bingo_blue".to_string())]
    );
    // a reset of the state clears the end screen's cards with the result
    state.reset();
    assert!(state.game_over_cards.is_empty());
    assert_eq!(keys(state.screen_cards()), [Some("bingo_green".to_string())]);
}

/// Sends `card_shuffled` for `bingo_red` at `at` ms.
fn shuffle_red_at(state: &mut HudState, at: u64) {
    state.set_now(at);
    let shuffled = encode(|w| horizon_bingo::card_shuffled::CardShuffledPacket { team_key: Some("bingo_red".into()) }.write(w));
    feed(state, "card_shuffled", &shuffled);
}

fn red_slots(state: &HudState) -> Vec<usize> {
    state.cards[&Some("bingo_red".to_string())].shuffle_positions.clone()
}

/// The orders from seed 42 and the card's slots after each shuffle, computed outside this crate: SplitMix64 from 42,
/// a Fisher-Yates permutation of 25 for each shuffle, the second one applied to the slots the first one left.
const SEED_42_FIRST: [usize; 25] = [20, 3, 21, 1, 15, 18, 4, 8, 5, 0, 10, 22, 11, 16, 23, 17, 6, 14, 9, 2, 24, 12, 7, 19, 13];
const SEED_42_SECOND: [usize; 25] = [19, 2, 20, 6, 4, 11, 1, 5, 24, 14, 16, 9, 17, 8, 10, 23, 13, 15, 18, 22, 3, 12, 7, 0, 21];

#[test]
fn a_shuffle_reorders_the_slots_of_the_card_by_the_seeded_order() {
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[Some("bingo_red")]));
    assert_eq!(red_slots(&state), (0..25).collect::<Vec<_>>(), "a new card starts in its own slots");
    state.set_shuffle_seed(42);
    shuffle_red_at(&mut state, 1_000);
    assert_eq!(red_slots(&state), SEED_42_FIRST);
    // 281 ms later the next order is taken from the same generator
    shuffle_red_at(&mut state, 1_281);
    assert_eq!(red_slots(&state), SEED_42_SECOND);
}

#[test]
fn a_debounced_shuffle_changes_neither_the_slots_nor_the_order_source() {
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[Some("bingo_red")]));
    state.set_shuffle_seed(42);
    shuffle_red_at(&mut state, 1_000);
    shuffle_red_at(&mut state, 1_100);
    shuffle_red_at(&mut state, 1_280);
    assert_eq!(red_slots(&state), SEED_42_FIRST, "inside the 280 ms the packets are ignored");
    // had the ignored packets drawn from the generator, this order would not be the second one
    shuffle_red_at(&mut state, 1_281);
    assert_eq!(red_slots(&state), SEED_42_SECOND);
}

#[test]
fn the_same_seed_and_packets_give_the_same_slots_and_another_seed_others() {
    let run = |seed: u64| {
        let mut state = HudState::new();
        feed(&mut state, "display_cards", &display(&[Some("bingo_red")]));
        state.set_shuffle_seed(seed);
        shuffle_red_at(&mut state, 1_000);
        shuffle_red_at(&mut state, 2_000);
        red_slots(&state)
    };
    assert_eq!(run(42), run(42));
    assert_ne!(run(42), run(43));
    // seed 43, first and second order (computed outside this crate), so the second shuffle is the same kind of check
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&[Some("bingo_red")]));
    state.set_shuffle_seed(43);
    shuffle_red_at(&mut state, 1_000);
    assert_eq!(red_slots(&state), [21, 24, 14, 9, 17, 15, 8, 13, 16, 23, 4, 5, 1, 7, 12, 20, 11, 0, 2, 6, 18, 22, 3, 19, 10]);
}

// The caps on what a server can make the client hold. The numbers are the limits themselves, written out here.

fn picture(id: &str, len: usize) -> Vec<u8> {
    encode(|w| horizon_bingo::card_image::CardImagePacket { id: id.into(), image: Some(vec![7; len]) }.write(w))
}

#[test]
fn card_images_are_refused_past_64_ids_and_past_8_mebibytes_but_a_held_id_can_be_replaced() {
    const MIB: usize = 1 << 20;
    let mut state = HudState::new();
    for i in 0..64 {
        feed(&mut state, "card_image", &picture(&format!("img{i}"), 1));
    }
    assert_eq!(state.images.len(), 64);
    feed(&mut state, "card_image", &picture("extra", 1));
    assert!(!state.images.contains_key("extra"), "the 65th id is refused");
    feed(&mut state, "card_image", &picture("img0", 2));
    assert_eq!(state.images["img0"].len(), 2, "a held id is replaced even at the count limit");

    // Bytes: a, b and d are 3, 3 and 2 MiB, which is exactly 8 MiB; c (3 MiB) would make 9 and is refused.
    let mut state = HudState::new();
    feed(&mut state, "card_image", &picture("a", 3 * MIB));
    feed(&mut state, "card_image", &picture("b", 3 * MIB));
    feed(&mut state, "card_image", &picture("c", 3 * MIB));
    let mut held: Vec<String> = state.images.keys().cloned().collect();
    held.sort();
    assert_eq!(held, ["a", "b"]);
    feed(&mut state, "card_image", &picture("d", 2 * MIB));
    assert_eq!(state.images.len(), 3, "2 MiB brings the total to exactly 8 MiB, which is allowed");
    feed(&mut state, "card_image", &picture("e", 1));
    assert!(!state.images.contains_key("e"), "one byte more than 8 MiB is refused");
    // Replacing "a" (3 MiB) with 4 MiB would make 9 MiB: refused, the 3 MiB picture stays.
    feed(&mut state, "card_image", &picture("a", 4 * MIB));
    assert_eq!(state.images["a"].len(), 3 * MIB);
    // With 1 MiB, the total is 8 - 3 + 1 = 6 MiB: accepted.
    feed(&mut state, "card_image", &picture("a", MIB));
    assert_eq!(state.images["a"].len(), MIB);
}

#[test]
fn tooltips_are_refused_past_256_entities_and_a_known_entity_still_updates() {
    let tip = |entity_id: u128, lines: usize| {
        encode(|w| TooltipPacket { text: vec![Text::plain("line"); lines], entity_id }.write(w).unwrap())
    };
    let mut state = HudState::new();
    for entity in 1..=256u128 {
        feed(&mut state, "tooltip_v2", &tip(entity, 1));
    }
    assert_eq!(state.tooltips_by_entity.len(), 256);
    feed(&mut state, "tooltip_v2", &tip(257, 1));
    assert!(!state.tooltips_by_entity.contains_key(&257), "the 257th entity is refused");
    feed(&mut state, "tooltip_v2", &tip(1, 2));
    assert_eq!(state.tooltips_by_entity[&1].text.len(), 2, "an entity already held is updated");
}

#[test]
fn team_scores_are_kept_for_at_most_64_teams() {
    let score = |team: &str, items: i32| {
        encode(|w| {
            ScoredItemPacket { is_viewer_on_team: false, team: team.into(), score: TeamScore { items, lines: 0, cards: 0 } }
                .write(w)
        })
    };
    let mut state = HudState::new();
    for i in 0..64 {
        feed(&mut state, "score_update", &score(&format!("team_{i}"), 1));
    }
    feed(&mut state, "score_update", &score("team_64", 1));
    assert_eq!(state.scores.len(), 64);
    assert!(!state.scores.contains_key("team_64"), "the 65th team is refused");
    feed(&mut state, "score_update", &score("team_0", 9));
    assert_eq!(state.scores["team_0"].items, 9, "a held team still updates");
}

#[test]
fn cards_are_refused_past_64_teams_and_tiles_for_a_refused_team_make_no_card() {
    let names: Vec<String> = std::iter::once("bingo_red".to_string()).chain((0..64).map(|i| format!("team_{i}"))).collect();
    let teams: Vec<Option<&str>> = names.iter().map(|n| Some(n.as_str())).collect();
    let mut state = HudState::new();
    feed(&mut state, "display_cards", &display(&teams));
    assert_eq!(state.cards.len(), 64, "65 teams shown, 64 kept");
    assert!(state.cards.contains_key(&Some("bingo_red".into())));
    assert!(!state.cards.contains_key(&Some("team_63".into())), "the 65th team is refused");

    feed(&mut state, "update_card_tiles", &tiles(true, |p| p.team_key = Some("team_99".into())));
    assert_eq!(state.cards.len(), 64);
    assert!(!state.cards.contains_key(&Some("team_99".into())), "tiles for a refused team make no card");

    feed(&mut state, "update_card_tiles", &tiles(true, |_| {}));
    assert_eq!(state.cards[&Some("bingo_red".to_string())].tiles.len(), TILES_PER_CARD, "a held card still takes tiles");
}

#[test]
fn the_end_screens_cards_keep_at_most_64_teams_the_newest_first() {
    let keys = |c: &Cards| c.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
    let teams = |prefix: &str| -> Vec<String> { (0..64).map(|i| format!("{prefix}_{i}")).collect() };
    fn as_display(names: &[String]) -> Vec<Option<&str>> {
        names.iter().map(|n| Some(n.as_str())).collect()
    }
    let (a, b) = (teams("a"), teams("b"));

    let mut state = HudState::new();
    let mut p = decode(&row("read_v2_minimal"), GameOverPacket::read_v2).unwrap();
    p.is_update = false;
    feed(&mut state, "game_over_v2", &encode(|w| p.write_v2(w).unwrap()));
    feed(&mut state, "display_cards", &display(&as_display(&a)));
    feed(&mut state, "display_cards", &display(&as_display(&b)));
    // 128 teams have been shown; the end screen keeps the 64 newest, the b cards, in the order shown.
    assert_eq!(keys(state.screen_cards()), as_display(&b).into_iter().map(|k| k.map(str::to_string)).collect::<Vec<_>>());
}

#[test]
fn the_list_keeps_the_newest_64_messages_and_the_history_the_latest_500_by_game_time() {
    let mut state = HudState::new();
    for id in 1..=65u128 {
        feed(&mut state, "game_message", &message(id, id as i64, false));
    }
    assert_eq!(state.messages.iter().map(|m| m.packet.id).collect::<Vec<_>>(), (2..=65).collect::<Vec<u128>>());

    // History: 500 messages at game times 2..=501, then one at game time 1 arrives last. It sorts first, so it is the
    // oldest and is the one dropped.
    let mut state = HudState::new();
    for id in 2..=501u128 {
        feed(&mut state, "game_message", &message(id, id as i64, false));
    }
    assert_eq!(state.past_messages.len(), 500);
    feed(&mut state, "game_message", &message(1, 1, false));
    assert_eq!(state.past_messages.iter().map(|m| m.packet.id).collect::<Vec<_>>(), (2..=501).collect::<Vec<u128>>());
    feed(&mut state, "game_message", &message(502, 502, false));
    assert_eq!(state.past_messages.iter().map(|m| m.packet.id).collect::<Vec<_>>(), (3..=502).collect::<Vec<u128>>());
}

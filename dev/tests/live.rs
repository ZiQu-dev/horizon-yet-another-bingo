//! Payloads recorded from a live server running the real mod (tests/captures/live/<channel>.<n>.hex), taken by the
//! native mod itself inside Horizon (BINGO_H0_DUMP): decode each one, encode it back, and the bytes must be identical;
//! then feed all of them to the client and it must accept every one.
use horizon_bingo::buf::{decode, encode};
use horizon_bingo::channels::id;
use horizon_bingo::state::{Handled, HudState};
use horizon_bingo::update_settings::PlayerSettings;

fn files() -> Vec<(String, Vec<u8>)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/captures/live");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let channel = name.split('.').next().unwrap().to_string(); // `ready_timer_update_v2.2.hex`
        let text = std::fs::read_to_string(&path).unwrap();
        let text = text.trim();
        let bytes = (0..text.len() / 2).map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap()).collect();
        out.push((channel, bytes));
    }
    out.sort();
    out
}

#[test]
fn the_live_recording_is_not_empty_and_covers_text_bearing_packets() {
    let channels: Vec<String> = files().into_iter().map(|(c, _)| c).collect();
    for needed in ["ready_timer_update_v2", "update_card_tiles", "update_settings", "countdown", "display_cards"] {
        assert!(channels.iter().any(|c| c == needed), "no live {needed}");
    }
}

#[test]
fn every_live_payload_decodes_and_encodes_back_to_the_same_bytes() {
    let mut checked = 0;
    for (channel, bytes) in files() {
        let back = match channel.as_str() {
            "ready_timer_update_v2" => {
                let p = decode(&bytes, horizon_bingo::ready_update::ReadyUpdatePacket::read_v2).unwrap();
                encode(|w| p.write_v2(w).unwrap())
            }
            "update_card_tiles" => {
                let p = decode(&bytes, horizon_bingo::update_card_tiles::CardTilesPacket::read).unwrap();
                encode(|w| p.write(w).unwrap())
            }
            "display_cards" => {
                let p = decode(&bytes, horizon_bingo::display_cards::CardDisplayPacket::read).unwrap();
                encode(|w| p.write(w).unwrap())
            }
            "update_settings" => {
                let p = decode(&bytes, horizon_bingo::update_settings::PlayerSettings::read).unwrap();
                encode(|w| p.write(w))
            }
            "countdown" => {
                let p = decode(&bytes, horizon_bingo::timer::CountdownPacket::read).unwrap();
                encode(|w| p.write(w))
            }
            "game_status" => {
                let p = decode(&bytes, horizon_bingo::game_status::GameStatusPacket::read).unwrap();
                encode(|w| p.write(w))
            }
            "stats_check" => {
                let p = decode(&bytes, horizon_bingo::stats_check::StatsCheckPacket::read).unwrap();
                encode(|w| p.write(w))
            }
            _ => continue, // card_reset and game_message_clear carry no bytes
        };
        assert_eq!(back, bytes, "{channel}");
        checked += 1;
    }
    // 19 files: 3 countdown, 3 display_cards, 3 game_status, 3 ready_timer_update_v2, 3 update_card_tiles,
    // 1 update_settings, 1 stats_check; the 2 empty ones are skipped above.
    assert_eq!(checked, 17);
}

#[test]
fn the_client_accepts_every_live_payload_with_nothing_left_over() {
    let mut state = HudState::new();
    for (channel, bytes) in files() {
        match state.on_packet(&id(&channel), &bytes) {
            Ok(Handled::Applied { unread: 0 }) | Ok(Handled::Ignored) => {}
            other => panic!("{channel}: {other:?}"),
        }
    }
    assert_eq!(state.unread_bytes, 0);
    // The recording is one game of the yellow team: one card of 25 tiles, the player's settings as sent,
    // and the server's stats hash.
    let yellow = Some("bingo_yellow".to_string());
    assert_eq!(state.cards.keys().cloned().collect::<Vec<_>>(), vec![yellow.clone()]);
    assert_eq!(state.cards.get(&yellow).unwrap().tiles.len(), 25);
    assert_eq!(
        state.settings,
        Some(PlayerSettings {
            seen_tutorial: false,
            hide_lobby_prompt: false,
            bossbar: false,
            scoreboard: true,
            scoreboard_auto_hide: false,
            leading_messages: true,
            score_messages: true,
            item_messages: true,
            night_vision: true,
        })
    );
    assert_eq!(
        state.stats_hash.as_deref(),
        Some("cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e")
    );
}

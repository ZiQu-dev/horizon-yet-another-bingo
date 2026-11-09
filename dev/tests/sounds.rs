use horizon_bingo::sounds::*;
use horizon_bingo::state::SoundKey;

#[test]
fn every_sound_has_its_own_event_named_like_the_originals_files() {
    // resources/assets/yet-another-minecraft-bingo/sounds/*.ogg of the original mod
    let files = ["game_lost", "game_won", "item_lost", "item_scored", "opponent_scored", "start_countdown", "start_release", "timer_tick"];
    let mut names: Vec<&str> = ALL.iter().map(|k| name(*k)).collect();
    names.sort();
    assert_eq!(names, files);
    assert_eq!(event_id(SoundKey::ItemScored), "yet-another-minecraft-bingo:item_scored");
    assert_eq!(event_id(SoundKey::StartRelease), "yet-another-minecraft-bingo:start_release");
}

#[test]
fn the_volume_is_the_calls_volume_times_the_default_0_8() {
    assert!((actual_volume(1.0) - 0.8).abs() < 1e-6);
    assert!((actual_volume(0.8) - 0.64).abs() < 1e-6, "the item-lost sound plays at 0.8");
    assert_eq!(actual_volume(0.0), 0.0);
}

#[test]
fn the_event_names_match_the_originals_sounds_json_when_it_is_at_hand() {
    // With BINGO_ASSETS_DIR (a folder made by prepare_assets.py) the original's sounds.json is checked too.
    let Ok(dir) = std::env::var("BINGO_ASSETS_DIR") else { return };
    let text = std::fs::read_to_string(std::path::Path::new(&dir).join("sounds.json")).unwrap();
    for key in ALL {
        assert!(text.contains(&format!("\"{}\"", name(key))), "{} is not in sounds.json", name(key));
        assert!(text.contains(&event_id(key)), "{}", event_id(key));
    }
}

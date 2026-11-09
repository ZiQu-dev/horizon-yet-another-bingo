//! The mod's eight sounds (`ClientSounds.Key`) as Horizon plays them: `native_play_local_sound` with the sound event the
//! original's own `sounds.json` defines. The files come from a resource pack the player builds from their copy of the
//! original mod (`tools/prepare_assets.py --pack`), exactly as a Fabric player has them in the mod's jar.

use crate::state::SoundKey;

/// `config.client.getSoundVolume(name)`'s default.
pub const DEFAULT_VOLUME: f32 = 0.8;
/// The namespace of the mod's sound events (`MOD_ID_BINGO`).
pub const NAMESPACE: &str = "yet-another-minecraft-bingo";

pub const ALL: [SoundKey; 8] = [
    SoundKey::ItemScored,
    SoundKey::ItemLost,
    SoundKey::OpponentScored,
    SoundKey::GameWon,
    SoundKey::GameLost,
    SoundKey::TimerTick,
    SoundKey::StartCountdown,
    SoundKey::StartRelease,
];

/// The file/event name: `ClientSounds.Key.name.lowercase()`.
pub fn name(key: SoundKey) -> &'static str {
    match key {
        SoundKey::ItemScored => "item_scored",
        SoundKey::ItemLost => "item_lost",
        SoundKey::OpponentScored => "opponent_scored",
        SoundKey::GameWon => "game_won",
        SoundKey::GameLost => "game_lost",
        SoundKey::TimerTick => "timer_tick",
        SoundKey::StartCountdown => "start_countdown",
        SoundKey::StartRelease => "start_release",
    }
}

/// `ClientSounds.Key.id`: `yet-another-minecraft-bingo:item_scored`.
pub fn event_id(key: SoundKey) -> String {
    format!("{NAMESPACE}:{}", name(key))
}

/// `SoundService.play`: the call's volume times the configured volume of that sound.
pub fn actual_volume(volume: f32) -> f32 {
    volume * DEFAULT_VOLUME
}

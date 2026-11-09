//! The client config (`ClientConfig.kt`: the `client` object of the mod's `bingo/config.json`) and the card's place in
//! the window (`BingoHudController.getCardPos`, `BingoMessageRenderer`'s scale). Pure: the caller holds the config and
//! the window size.
//!
//! The file. Kotlin writes it with `json` (kotlinx, `prettyPrint`, `allowComments`): a `//` comment line above each
//! field, which `from_json` skips. Reading is tolerant, as the Kotlin's is not: a missing or invalid field takes its
//! default, a number outside its YACL range is clamped into it, an unknown key is ignored, and text that is not a JSON
//! object gives the defaults. Nothing panics. `to_json` writes every field, in the Kotlin's declaration order.
//!
//! Ranges are the YACL options' (`YetAnotherConfigLibIntegration.kt`): card and message scales 0.25 to 4, message
//! duration 1 to 30 seconds, sound volumes 0 to 1.5. The offsets have a floor of 0 only: YACL allows up to half the
//! window, which only the settings screen knows (`settings_screen`).
//!
//! The card's alignment and overlap are `screen_placement`'s types, so the placement screen and the HUD share them.
//! `card_pos` is the HUD's formula, which does not clamp the card into the window (`init_position` does, as the
//! placement screen's `initCardPosition` does). A test below checks the two agree when the card fits.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::card_draw::{CARD_HEIGHT, CARD_WIDTH};
pub use crate::screen_placement::{CardAlignment, CardOverlap};

/// `FloatSliderControllerBuilder.range(0.25f, 4.0f)`: the card's scale, and the message scale.
pub const SCALE_RANGE: (f32, f32) = (0.25, 4.0);
/// `IntegerSliderControllerBuilder.range(1, 30)`: how long a message stays, in seconds.
pub const MESSAGE_DURATION_RANGE: (i64, i64) = (1, 30);
/// `FloatSliderControllerBuilder.range(0f, 1.5f)`: a sound's volume.
pub const SOUND_VOLUME_RANGE: (f32, f32) = (0.0, 1.5);
/// `ClientConfig.getSoundVolume`: a sound with no entry plays at this volume.
pub const SOUND_VOLUME_DEFAULT: f32 = 0.8;
/// `IntegerSliderControllerBuilder.range(0, ...)`: the floor of the card's offsets. The top is the window's half.
pub const OFFSET_MIN: i32 = 0;

/// The sounds of `ClientSounds.Key`, in its order. Each has a volume in `ClientConfig.soundVolumes`, keyed by the
/// lower-case name.
pub const SOUNDS: [&str; 8] = [
    "item_scored",
    "item_lost",
    "opponent_scored",
    "game_won",
    "game_lost",
    "timer_tick",
    "start_countdown",
    "start_release",
];

/// `ClientConfig`, field for field, with the Kotlin defaults. `message_duration_seconds` is an `i64`, as
/// `MessageView::duration_seconds` is, so the two can be copied across.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientConfig {
    pub enable_hud: bool,
    pub show_quick_start_button: bool,
    pub card_pauses_game: bool,
    pub card_scale: f32,
    pub card_alignment: CardAlignment,
    pub card_offset_x: i32,
    pub card_offset_y: i32,
    pub card_overlap: CardOverlap,
    pub card_team_outlines: bool,
    pub show_multiple_cards: bool,
    pub hide_on_f3: bool,
    pub hide_on_chat: bool,
    pub message_from_other_teams: bool,
    pub message_duration_seconds: i64,
    pub message_scale: f32,
    /// `soundVolumes`: a sound's name (`item_scored`) to its volume. Absent sounds play at `SOUND_VOLUME_DEFAULT`.
    pub sound_volumes: BTreeMap<String, f32>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            enable_hud: true,
            show_quick_start_button: true,
            card_pauses_game: true,
            card_scale: 1.0,
            card_alignment: CardAlignment::TopLeft,
            card_offset_x: 0,
            card_offset_y: 0,
            card_overlap: CardOverlap::Above,
            card_team_outlines: true,
            show_multiple_cards: false,
            hide_on_f3: true,
            hide_on_chat: false,
            message_from_other_teams: true,
            message_duration_seconds: 5,
            message_scale: 1.0,
            sound_volumes: BTreeMap::new(),
        }
    }
}

impl ClientConfig {
    /// `getSoundVolume(sound)`: the sound's own volume, else 0.8.
    pub fn sound_volume(&self, sound: &str) -> f32 {
        self.sound_volumes.get(sound).copied().unwrap_or(SOUND_VOLUME_DEFAULT)
    }

    /// The config as the mod writes it: every field, in declaration order, one per line.
    pub fn to_json(&self) -> String {
        let volumes: Map<String, Value> =
            self.sound_volumes.iter().map(|(name, v)| (name.clone(), number(*v))).collect();
        let fields: [(&str, Value); 16] = [
            ("enableHud", Value::Bool(self.enable_hud)),
            ("showQuickStartButton", Value::Bool(self.show_quick_start_button)),
            ("cardPausesGame", Value::Bool(self.card_pauses_game)),
            ("cardScale", number(self.card_scale)),
            ("cardAlignment", Value::String(alignment_name(self.card_alignment).into())),
            ("cardOffsetX", Value::from(self.card_offset_x)),
            ("cardOffsetY", Value::from(self.card_offset_y)),
            ("cardOverlap", Value::String(overlap_name(self.card_overlap).into())),
            ("cardTeamOutlines", Value::Bool(self.card_team_outlines)),
            ("showMultipleCards", Value::Bool(self.show_multiple_cards)),
            ("hideOnF3", Value::Bool(self.hide_on_f3)),
            ("hideOnChat", Value::Bool(self.hide_on_chat)),
            ("messageFromOtherTeams", Value::Bool(self.message_from_other_teams)),
            ("messageDurationSeconds", Value::from(self.message_duration_seconds)),
            ("messageScale", number(self.message_scale)),
            ("soundVolumes", Value::Object(volumes)),
        ];
        let mut out = String::from("{\n");
        let last = fields.len() - 1;
        for (i, (name, value)) in fields.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&Value::String((*name).into()).to_string());
            out.push_str(": ");
            out.push_str(&value.to_string());
            out.push_str(if i < last { ",\n" } else { "\n" });
        }
        out.push('}');
        out
    }

    /// Reads the `client` object. Never fails: see the module notes for what it tolerates.
    pub fn from_json(text: &str) -> ClientConfig {
        let d = ClientConfig::default();
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&strip_comments(text)) else {
            return d;
        };
        let mut sound_volumes = BTreeMap::new();
        if let Some(volumes) = map.get("soundVolumes").and_then(Value::as_object) {
            for (name, value) in volumes {
                if let Some(v) = finite(value) {
                    sound_volumes.insert(name.clone(), clamp(v, SOUND_VOLUME_RANGE));
                }
            }
        }
        ClientConfig {
            enable_hud: flag(&map, "enableHud", d.enable_hud),
            show_quick_start_button: flag(&map, "showQuickStartButton", d.show_quick_start_button),
            card_pauses_game: flag(&map, "cardPausesGame", d.card_pauses_game),
            card_scale: float(&map, "cardScale", SCALE_RANGE, d.card_scale),
            card_alignment: map
                .get("cardAlignment")
                .and_then(Value::as_str)
                .and_then(alignment_from_name)
                .unwrap_or(d.card_alignment),
            card_offset_x: offset(&map, "cardOffsetX", d.card_offset_x),
            card_offset_y: offset(&map, "cardOffsetY", d.card_offset_y),
            card_overlap: map
                .get("cardOverlap")
                .and_then(Value::as_str)
                .and_then(overlap_from_name)
                .unwrap_or(d.card_overlap),
            card_team_outlines: flag(&map, "cardTeamOutlines", d.card_team_outlines),
            show_multiple_cards: flag(&map, "showMultipleCards", d.show_multiple_cards),
            hide_on_f3: flag(&map, "hideOnF3", d.hide_on_f3),
            hide_on_chat: flag(&map, "hideOnChat", d.hide_on_chat),
            message_from_other_teams: flag(&map, "messageFromOtherTeams", d.message_from_other_teams),
            message_duration_seconds: integer(
                &map,
                "messageDurationSeconds",
                MESSAGE_DURATION_RANGE,
                d.message_duration_seconds,
            ),
            message_scale: float(&map, "messageScale", SCALE_RANGE, d.message_scale),
            sound_volumes,
        }
    }
}

/// `ClientConfig.cardOverlap.z`: the z the card's draws use, so the HUD's other draws go under or over it.
pub fn overlap_z(overlap: CardOverlap) -> i32 {
    match overlap {
        CardOverlap::Above => 800,
        CardOverlap::Underneath => -800,
    }
}

/// `CardAlignment.x` and `.y`: 1 on the right and on the bottom, 0 on the left and at the top.
pub fn alignment_xy(alignment: CardAlignment) -> (i32, i32) {
    match alignment {
        CardAlignment::TopLeft => (0, 0),
        CardAlignment::TopRight => (1, 0),
        CardAlignment::BottomLeft => (0, 1),
        CardAlignment::BottomRight => (1, 1),
    }
}

/// `cardScale` as the HUD uses it: the stored value kept to the slider's range (a file is clamped on load, but a field
/// set by hand is not), and 1 when it is not a number.
pub fn card_scale(config: &ClientConfig) -> f32 {
    scale_in(config.card_scale)
}

/// `BingoMessageRenderer`'s scale: the message scale times the chat's text size (`options.chatScale`).
pub fn message_scale(config: &ClientConfig, chat_scale: f32) -> f32 {
    scale_in(config.message_scale) * chat_scale
}

/// `BingoHudController.getCardPos`: the card's top left corner in a window of `gui_w` by `gui_h` GUI pixels. The
/// alignment's edge is the window's edge less the card; a right or bottom anchor counts its offset inwards. Not
/// clamped, so a card bigger than the window hangs off its far side, as the Kotlin's does.
pub fn card_pos(config: &ClientConfig, gui_w: i32, gui_h: i32) -> (f32, f32) {
    let scale = card_scale(config);
    let (ax, ay) = alignment_xy(config.card_alignment);
    let offset_x = if ax > 0 { config.card_offset_x.saturating_neg() } else { config.card_offset_x };
    let offset_y = if ay > 0 { config.card_offset_y.saturating_neg() } else { config.card_offset_y };
    let x = ax as f32 * (gui_w as f32 - CARD_WIDTH as f32 * scale) + offset_x as f32;
    let y = ay as f32 * (gui_h as f32 - CARD_HEIGHT as f32 * scale) + offset_y as f32;
    (x, y)
}

fn scale_in(v: f32) -> f32 {
    if v.is_finite() { v.clamp(SCALE_RANGE.0, SCALE_RANGE.1) } else { 1.0 }
}

fn clamp(v: f32, (lo, hi): (f32, f32)) -> f32 {
    v.clamp(lo, hi)
}

/// A JSON number as the `f32` it was written from: `0.8` is written as `0.8`, not as the binary's longer digits.
fn number(v: f32) -> Value {
    format!("{v}")
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
        .map_or(Value::Null, Value::Number)
}

fn finite(value: &Value) -> Option<f32> {
    value.as_f64().filter(|v| v.is_finite()).map(|v| v as f32)
}

fn flag(map: &Map<String, Value>, key: &str, default: bool) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn float(map: &Map<String, Value>, key: &str, range: (f32, f32), default: f32) -> f32 {
    map.get(key).and_then(finite).map_or(default, |v| clamp(v, range))
}

fn integer(map: &Map<String, Value>, key: &str, (lo, hi): (i64, i64), default: i64) -> i64 {
    map.get(key).and_then(Value::as_i64).map_or(default, |v| v.clamp(lo, hi))
}

fn offset(map: &Map<String, Value>, key: &str, default: i32) -> i32 {
    let (lo, hi) = (OFFSET_MIN as i64, i32::MAX as i64);
    map.get(key).and_then(Value::as_i64).map_or(default, |v| v.clamp(lo, hi) as i32)
}

/// Kotlin's `json` allows `//` comments: the mod's file has one above each field. Only whole lines are dropped, since
/// a JSON string cannot run over a line break.
fn strip_comments(text: &str) -> String {
    text.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
}

fn alignment_name(alignment: CardAlignment) -> &'static str {
    match alignment {
        CardAlignment::TopLeft => "TOP_LEFT",
        CardAlignment::TopRight => "TOP_RIGHT",
        CardAlignment::BottomLeft => "BOTTOM_LEFT",
        CardAlignment::BottomRight => "BOTTOM_RIGHT",
    }
}

fn alignment_from_name(name: &str) -> Option<CardAlignment> {
    match name {
        "TOP_LEFT" => Some(CardAlignment::TopLeft),
        "TOP_RIGHT" => Some(CardAlignment::TopRight),
        "BOTTOM_LEFT" => Some(CardAlignment::BottomLeft),
        "BOTTOM_RIGHT" => Some(CardAlignment::BottomRight),
        _ => None,
    }
}

fn overlap_name(overlap: CardOverlap) -> &'static str {
    match overlap {
        CardOverlap::Above => "ABOVE",
        CardOverlap::Underneath => "UNDERNEATH",
    }
}

fn overlap_from_name(name: &str) -> Option<CardOverlap> {
    match name {
        "ABOVE" => Some(CardOverlap::Above),
        "UNDERNEATH" => Some(CardOverlap::Underneath),
        _ => None,
    }
}

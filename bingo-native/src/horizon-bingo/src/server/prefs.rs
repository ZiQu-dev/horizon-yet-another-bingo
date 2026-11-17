//! The player settings command, `bingo prefs <setting> [on|off]`: the original's `BingoPrefsCommand.kt`. Its settings are
//! `PlayerSettings.kt`, and a change is saved as `PlayerSettingsService.write` saves it (the session keeps the settings in
//! `player-settings.json`, see `store`). This file names the settings and reads and changes them; the session does the
//! rest, and [`click_command`] is the command the book's toggles run.
//!
//! The settings' English names are the original's `en_us` text, under the same keys (`bingo.player_settings.*`).
use crate::update_settings::PlayerSettings;

/// The settings the command changes, in the original's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Bossbar,
    Scoreboard,
    ScoreboardAutoHide,
    LeadingMessages,
    ScoreMessages,
    ItemMessages,
    NightVision,
    /// The lobby prompt's flag: only a value changes it (the original registers no toggle form for it).
    HideLobbyPrompt,
}

impl Setting {
    pub const ALL: [Setting; 8] = [
        Setting::Bossbar,
        Setting::Scoreboard,
        Setting::ScoreboardAutoHide,
        Setting::LeadingMessages,
        Setting::ScoreMessages,
        Setting::ItemMessages,
        Setting::NightVision,
        Setting::HideLobbyPrompt,
    ];

    /// The word that names the setting in the command: its field name in snake_case (`getFieldName`).
    pub fn word(self) -> &'static str {
        match self {
            Setting::Bossbar => "bossbar",
            Setting::Scoreboard => "scoreboard",
            Setting::ScoreboardAutoHide => "scoreboard_auto_hide",
            Setting::LeadingMessages => "leading_messages",
            Setting::ScoreMessages => "score_messages",
            Setting::ItemMessages => "item_messages",
            Setting::NightVision => "night_vision",
            Setting::HideLobbyPrompt => "hide_lobby_prompt",
        }
    }

    /// The setting a word names, ignoring case; `None` for an unknown word.
    pub fn from_word(word: &str) -> Option<Setting> {
        let word = word.to_lowercase();
        Self::ALL.into_iter().find(|setting| setting.word() == word)
    }

    /// The English name the chat and the book give the setting (`bingo.player_settings.*`).
    pub fn label(self) -> &'static str {
        match self {
            Setting::Bossbar => "Bossbar",
            Setting::Scoreboard => "Scoreboard",
            Setting::ScoreboardAutoHide => "Scoreboard Auto Hide",
            Setting::LeadingMessages => "Leading Team",
            Setting::ScoreMessages => "Line Scored",
            Setting::ItemMessages => "Item Scored",
            Setting::NightVision => "Night Vision",
            Setting::HideLobbyPrompt => "Never show this again",
        }
    }

    /// Whether the command needs a value (`on` or `off`) for this setting.
    pub fn needs_value(self) -> bool {
        self == Setting::HideLobbyPrompt
    }

    /// The setting's value in the player's settings.
    pub fn get(self, settings: &PlayerSettings) -> bool {
        match self {
            Setting::Bossbar => settings.bossbar,
            Setting::Scoreboard => settings.scoreboard,
            Setting::ScoreboardAutoHide => settings.scoreboard_auto_hide,
            Setting::LeadingMessages => settings.leading_messages,
            Setting::ScoreMessages => settings.score_messages,
            Setting::ItemMessages => settings.item_messages,
            Setting::NightVision => settings.night_vision,
            Setting::HideLobbyPrompt => settings.hide_lobby_prompt,
        }
    }

    /// Sets the setting's value in the player's settings.
    pub fn set(self, settings: &mut PlayerSettings, value: bool) {
        match self {
            Setting::Bossbar => settings.bossbar = value,
            Setting::Scoreboard => settings.scoreboard = value,
            Setting::ScoreboardAutoHide => settings.scoreboard_auto_hide = value,
            Setting::LeadingMessages => settings.leading_messages = value,
            Setting::ScoreMessages => settings.score_messages = value,
            Setting::ItemMessages => settings.item_messages = value,
            Setting::NightVision => settings.night_vision = value,
            Setting::HideLobbyPrompt => settings.hide_lobby_prompt = value,
        }
    }
}

/// The command a settings line runs when clicked: the setting set to `value` (`BingoPrefsCommand.getCommand`).
pub fn click_command(setting: Setting, value: bool) -> String {
    format!("/bingo prefs {} {value}", setting.word())
}

/// The chat line after a change: `Set 'Bossbar' = On` (`bingo.options.notify_changed`). The hidden lobby prompt says
/// what it does instead when it is turned on, and nothing when it is turned off, as the original does.
pub fn changed_text(setting: Setting, value: bool) -> String {
    match setting {
        Setting::HideLobbyPrompt if value => "Okay, you won't see this message again.".to_string(),
        Setting::HideLobbyPrompt => String::new(),
        _ => format!("Set '{}' = {}", setting.label(), if value { "On" } else { "Off" }),
    }
}

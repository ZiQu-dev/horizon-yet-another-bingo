//! Round options of the server: what `/bingo goal`, `mode`, `endwhen` and the toggles set.
//! Names and defaults follow options/BingoOptions.kt and options/BingoCardOptions.kt of the original
//! (goal `Lines(1)`, end `FirstWin`, pvp on, keep inventory off, elytra off, night vision off, spawn distance 25 chunks).
//! Deviations: only one card per round (win condition `Cards(1)`). The hidden-items and consume-items flags are in
//! `hidden_consume` (see `hidden_consume.rs`).

use super::hidden_consume::HiddenConsume;

/// What a team must reach on its card (`BingoGoal`: `Items(n)`, `Lines(n)`; `FullCard` is `Items(25)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Lines(u32),
    Items(u32),
    FullCard,
}

/// When the round ends (`EndWhen`): never (only `/bingo end` or the time limit), when the first team reaches the goal,
/// when `TeamsWin(n)` teams have, or when all teams have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndWhen {
    Never,
    FirstWin,
    TeamsWin(u32),
    AllWin,
}

/// Card mode: `Standard` (an item found stays found), `Inventory` (a square counts only while the item is held),
/// `Lockout` (an item found by one team is lost to the others).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standard,
    Inventory,
    Lockout,
}

/// A yes/no round setting with a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Pvp,
    KeepInventory,
    Elytra,
    NightVision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub goal: Goal,
    pub mode: Mode,
    pub end_when: EndWhen,
    pub pvp: bool,
    pub keep_inventory: bool,
    pub elytra: bool,
    pub night_vision: bool,
    /// Distance between team spawns in chunks (`spawnDistance`, default 25).
    pub spawn_distance: u32,
    /// Hidden-items and consume-items flags of the card, both off by default.
    pub hidden_consume: HiddenConsume,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            goal: Goal::Lines(1),
            mode: Mode::Standard,
            end_when: EndWhen::FirstWin,
            pvp: true,
            keep_inventory: false,
            elytra: false,
            night_vision: false,
            spawn_distance: 25,
            hidden_consume: HiddenConsume::default(),
        }
    }
}

impl Options {
    pub fn set_toggle(&mut self, toggle: Toggle, on: bool) {
        match toggle {
            Toggle::Pvp => self.pvp = on,
            Toggle::KeepInventory => self.keep_inventory = on,
            Toggle::Elytra => self.elytra = on,
            Toggle::NightVision => self.night_vision = on,
        }
    }
}

//! Round options of the server: what `/bingo goal`, `mode`, `endwhen` and the toggles set.
//! Names and defaults follow options/BingoOptions.kt and options/BingoCardOptions.kt of the original
//! (goal `Lines(1)`, end `FirstWin`, pvp on, keep inventory off, elytra off, night vision off, spawn distance 25 chunks).
//! The win condition (`play_to`, default `Cards(1)`) and the stalemate behaviour (default end game) are in `multicard`.
//! The hidden-items and consume-items flags are in `hidden_consume` (see `hidden_consume.rs`).

use super::card_gen::DEFAULT_DISTRIBUTION;
use super::hidden_consume::HiddenConsume;
use super::kit::Kit;
use super::multicard::{StalemateBehavior, WinCondition};

/// Teams the auto shuffle makes when none is given (`teamCount`, default 3).
pub const DEFAULT_TEAM_COUNT: usize = 3;

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
    /// `teamAutoShuffle`: the teams are shuffled when the round starts.
    AutoShuffle,
    /// `objectives`: the advanced squares of the original's tier lists go on the cards. Pregame only, like the others.
    Objectives,
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
    /// How many cards a team completes to win (`play_to`), default one card.
    pub win_condition: WinCondition,
    /// What a stalemate does (`stalemate`), default end the game.
    pub stalemate: StalemateBehavior,
    /// Squares of each tier, S first (`itemDistribution`), default easy. Sums to 25: see `difficulty`.
    pub tier_counts: [usize; 5],
    /// Whether the teams are shuffled when the round starts (`teamAutoShuffle`), default off.
    pub team_auto_shuffle: bool,
    /// How many teams that shuffle makes (`teamCount`), default 3.
    pub team_count: usize,
    /// Team chat (`chat.defaultToTeamChat`): while a game runs, chat goes to the team. On by default. `chat.rs`.
    pub team_chat: bool,
    /// The sidebar (`scoreboard`, the original's default): game info and the ranking on screen. On by default. `sidebar.rs`.
    pub sidebar: bool,
    /// The starting kit of team players (`bingo kit`), empty by default.
    pub kit: Kit,
    /// Advanced squares on the cards: the advancements and scorable challenges of the original's tier lists are drawn
    /// with the items (`objectives`, in `options.json` only when on). Off by default: the cards are then all items.
    pub objectives: bool,
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
            win_condition: WinCondition::default(),
            stalemate: StalemateBehavior::default(),
            tier_counts: DEFAULT_DISTRIBUTION,
            team_auto_shuffle: false,
            team_count: DEFAULT_TEAM_COUNT,
            team_chat: true,
            sidebar: true,
            kit: Kit::default(),
            objectives: false,
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
            Toggle::AutoShuffle => self.team_auto_shuffle = on,
            Toggle::Objectives => self.objectives = on,
        }
    }
}

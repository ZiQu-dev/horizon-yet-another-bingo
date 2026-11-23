//! Types shared by the server modules. Owned by the lead; modules may not change them without asking.
use crate::card_tile::TierLabel;

/// A team's id as it travels on the wire (`BingoTeamKey.id`), for example `bingo_red`.
pub type TeamKey = String;

/// A player: Minecraft UUID as a u128 (Horizon's `player: u128`) and the name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Player {
    pub uuid: u128,
    pub name: String,
}

/// One square of a card: an item to collect. `item` is the namespaced id, for example `minecraft:oak_log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Objective {
    pub item: String,
    pub tier: TierLabel,
}

/// The item id of a free space (`BingoObjective.FreeSpace.id`): a square every team has achieved from the start.
pub const FREE_SPACE: &str = "bingo:free_space";

/// A 5x5 card, row-major: index = row * 5 + column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub squares: Vec<Objective>,
}

pub const CARD_SIZE: usize = 5;
pub const CARD_SQUARES: usize = CARD_SIZE * CARD_SIZE;

/// What a team has: indices of squares collected (in order found) and the resulting score.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TeamProgress {
    pub found: Vec<usize>,
}

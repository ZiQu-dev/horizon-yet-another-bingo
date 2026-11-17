//! Hidden-items and consume-items card modes. Pure logic: no game types, no I/O.
//!
//! Sources in orig/common/src/main/java/me/jfenn/bingo/common/:
//! - the flags: `BingoCardOptions.isHiddenItemsMode` and `isConsumeItemsMode`, both `false` by default
//!   (options/BingoCardOptions.kt:27-28). The original's kotlinx serializer writes them under those names.
//! - hidden items: `CardViewService.getTileState` (map/CardViewService.kt:215-219 and 276). A square is hidden while
//!   the team has not seen its item (`BingoObjective.hasSeen`, card/objective/BingoObjective.kt:89: `teamsSeen` or
//!   `teamsAchieved`). A hidden square has no id, name, lore, action or image; its tier is shown only when
//!   `showHiddenTiers` is on (options/BingoOptions.kt:49, default off). Nothing is hidden in POSTGAME or for a
//!   finished team. A player on no team sees every square hidden (team == null).
//!   Free space is not hidden in the original; the native card has no free space, so nothing is exempt here.
//! - `teamsSeen` only grows (card/objective/ObjectiveService.kt:141), so a square stays seen after its item leaves.
//! - consume items: `ScoredItemCheck.updateScores` (scoring/ScoredItemCheck.kt:98-101) takes the scored item from the
//!   inventory of the team player who captured it (the latest capture among the team, `maxByOrNull`, line 93). It
//!   applies only when the mode is on, the card is not in inventory mode, and the server is in lobby mode
//!   (`isLobbyMode`, a server config flag, not a game state). Removal is `ItemObjectiveManager.confiscateScoredItem`
//!   (card/objective/ItemObjectiveManager.kt:141-165) over `visitInventories` (utils/InventoryVisitor.kt:27-73): each
//!   stack in slot order, its nested bundle or container contents right after it, and a stack must have the item and
//!   not carry `NBT_BINGO_IGNORE`. Item components are not modelled (the native objectives have no components).
//! - exclusion: consume items turns inventory mode off and the reverse (options/OptionsService.kt:178-184). The native
//!   `Mode` is a single value, so the two cannot be on together there; callers pass `inventory_mode` as
//!   `mode == Mode::Inventory`.
use serde_json::{Map, Value};

use crate::card_tile::TierLabel;
use crate::server::types::Card;

/// The key of the hidden-items flag in the card options object (the original's property name).
pub const HIDDEN_ITEMS_KEY: &str = "isHiddenItemsMode";
/// The key of the consume-items flag in the card options object (the original's property name).
pub const CONSUME_ITEMS_KEY: &str = "isConsumeItemsMode";

/// The two card modes that have a command of their own (`mode hidden_items on|off`, `mode consume_items on|off`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardMode {
    HiddenItems,
    ConsumeItems,
}

/// The hidden-items and consume-items flags of a card, both off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HiddenConsume {
    pub hidden_items: bool,
    pub consume_items: bool,
}

impl HiddenConsume {
    /// Sets one flag. Does not touch inventory mode; the caller applies the exclusion rule.
    pub fn set(&mut self, mode: CardMode, on: bool) {
        match mode {
            CardMode::HiddenItems => self.hidden_items = on,
            CardMode::ConsumeItems => self.consume_items = on,
        }
    }

    /// Writes both keys into the card options object `object`, as `true` or `false`.
    pub fn write_into(&self, object: &mut Map<String, Value>) {
        object.insert(HIDDEN_ITEMS_KEY.to_string(), Value::Bool(self.hidden_items));
        object.insert(CONSUME_ITEMS_KEY.to_string(), Value::Bool(self.consume_items));
    }

    /// The two keys as a JSON object.
    pub fn to_value(&self) -> Value {
        let mut object = Map::new();
        self.write_into(&mut object);
        Value::Object(object)
    }

    /// Reads both keys from the card options object `object`. A missing or non-boolean key is off (the original's
    /// default), and other keys are ignored.
    pub fn read_from(object: &Map<String, Value>) -> Self {
        Self {
            hidden_items: object.get(HIDDEN_ITEMS_KEY).and_then(Value::as_bool).unwrap_or(false),
            consume_items: object.get(CONSUME_ITEMS_KEY).and_then(Value::as_bool).unwrap_or(false),
        }
    }

    /// Reads the flags from a JSON value. A value that is not an object gives both flags off.
    pub fn from_value(value: &Value) -> Self {
        match value {
            Value::Object(object) => Self::read_from(object),
            _ => Self::default(),
        }
    }
}

/// What one team knows of the card. `seen` is the team's `teamsSeen` and `found` its `teamsAchieved`: a square counts
/// as seen when it is in either. `finished` is `TeamState.Finished`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sight<'a> {
    pub seen: &'a [usize],
    pub found: &'a [usize],
    pub finished: bool,
}

impl Sight<'_> {
    fn has_seen(&self, index: usize) -> bool {
        self.seen.contains(&index) || self.found.contains(&index)
    }
}

/// One square as a viewer sees it. A hidden square has no item, and no tier unless `show_hidden_tiers` is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileView {
    pub index: usize,
    pub hidden: bool,
    pub item: Option<String>,
    pub tier: Option<TierLabel>,
}

/// Whether square `index` is hidden for `viewer`. `None` is a player on no team (a spectator, or anyone off a team).
/// Hidden only when hidden items is on, the round is not in postgame, the viewer is not finished, and the square
/// has not been seen.
pub fn is_hidden(hidden_items: bool, index: usize, viewer: Option<Sight<'_>>, postgame: bool) -> bool {
    if !hidden_items || postgame {
        return false;
    }
    match viewer {
        None => true,
        Some(sight) => !sight.finished && !sight.has_seen(index),
    }
}

/// Every square of `card` as `viewer` sees it, in card order.
pub fn tile_views(
    card: &Card,
    modes: HiddenConsume,
    show_hidden_tiers: bool,
    viewer: Option<Sight<'_>>,
    postgame: bool,
) -> Vec<TileView> {
    card.squares
        .iter()
        .enumerate()
        .map(|(index, square)| {
            let hidden = is_hidden(modes.hidden_items, index, viewer, postgame);
            TileView {
                index,
                hidden,
                item: (!hidden).then(|| square.item.clone()),
                tier: (!modes.hidden_items || show_hidden_tiers || !hidden).then_some(square.tier),
            }
        })
        .collect()
}

/// Adds the squares whose items the team holds now (`held`) to `seen`. Keeps the order and skips squares already
/// there. Nothing is removed: a seen square stays seen after its item leaves the inventory.
pub fn record_seen(seen: &mut Vec<usize>, held: &[usize]) {
    for &index in held {
        if !seen.contains(&index) {
            seen.push(index);
        }
    }
}

/// A stack in a player's inventory. `nested` holds the stacks inside it (a bundle or a container); `bingo_ignore` is the
/// `NBT_BINGO_IGNORE` tag, which keeps a stack from counting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldStack {
    pub item: String,
    pub count: u32,
    pub bingo_ignore: bool,
    pub nested: Vec<HeldStack>,
}

/// Whether scoring a square takes items from the capturing player: consume items is on, the card is not in inventory
/// mode, and the server is in lobby mode.
pub fn consumes_on_score(modes: HiddenConsume, inventory_mode: bool, lobby_mode: bool) -> bool {
    modes.consume_items && !inventory_mode && lobby_mode
}

/// The player who captured a square: of the `members` of the team, the one with the latest capture in `captures`
/// (`(player, instant)` in the order the captures were made). On equal instants the first one listed wins, as Kotlin's
/// `maxByOrNull` does. `None` when no member captured it.
pub fn capturing_player(captures: &[(u128, u64)], members: &[u128]) -> Option<u128> {
    let mut best: Option<(u128, u64)> = None;
    for &(player, at) in captures {
        if !members.contains(&player) {
            continue;
        }
        match best {
            Some((_, best_at)) if best_at >= at => {}
            _ => best = Some((player, at)),
        }
    }
    best.map(|(player, _)| player)
}

/// Takes up to `count` items named `item` out of `stacks`, as `visitInventories` visits them: a stack, then its nested
/// stacks, then the next stack in slot order. A stack with `bingo_ignore` is skipped. Stops as soon as `count` is taken.
/// Returns `(item, taken)` for each stack that gave items, in the order they were taken; a stack that gave none is not
/// listed.
pub fn confiscate(stacks: &mut [HeldStack], item: &str, count: u32) -> Vec<(String, u32)> {
    let mut remaining = count;
    let mut taken = Vec::new();
    take_from(stacks, item, &mut remaining, &mut taken);
    taken
}

fn take_from(stacks: &mut [HeldStack], item: &str, remaining: &mut u32, taken: &mut Vec<(String, u32)>) {
    for stack in stacks.iter_mut() {
        if *remaining == 0 {
            return;
        }
        if stack.item == item && !stack.bingo_ignore {
            let n = stack.count.min(*remaining);
            if n > 0 {
                taken.push((stack.item.clone(), n));
            }
            stack.count -= n;
            *remaining -= n;
        }
        take_from(&mut stack.nested, item, remaining, taken);
    }
}

/// The items to remove from the capturing player's inventory when a square with `item` (`item_count` of it, 1 for a
/// square) is scored. Empty when consume items does not apply (see [`consumes_on_score`]). The stacks are changed in
/// place, as the original changes them.
pub fn consume_on_score(
    stacks: &mut [HeldStack],
    item: &str,
    item_count: u32,
    modes: HiddenConsume,
    inventory_mode: bool,
    lobby_mode: bool,
) -> Vec<(String, u32)> {
    if !consumes_on_score(modes, inventory_mode, lobby_mode) {
        return Vec::new();
    }
    confiscate(stacks, item, item_count)
}

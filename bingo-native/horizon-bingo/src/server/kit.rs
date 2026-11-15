//! The starting kit: the items a team player is given when a round starts (the original's player kit, `SpawnKit` for
//! `playerkit`, given by `SpawnService.giveSpawnEquipment`). Pure: the kit is part of the round options and is saved
//! with them. The session gives it with console `give` commands and keeps its items off the cards, as the original
//! does by default (`excludeSpawnKitItemsFromCards`).
//!
//! Not ported: the team kit (a chest per team), the editor's chest screen, the NBT of an item (only its id and count
//! are kept), and the gear given again on respawn.
use serde_json::{Map, Value};

use super::card_gen::Pool;
use super::texts;

/// Most entries in a kit: the 36 inventory slots, which the original's kit editor also fills.
pub const MAX_ITEMS: usize = 36;
/// Most of one item in a kit entry. `give` splits it into stacks, so this is a sanity limit, not a stack size.
pub const MAX_COUNT: u32 = 6400;

/// One kit entry: an item id (`minecraft:` prefix included) and how many of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitItem {
    pub item: String,
    pub count: u32,
}

/// The starting kit, in the order it was added. An empty kit gives nothing and changes no card.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Kit {
    pub items: Vec<KitItem>,
}

/// `bingo kit ...`: show the kit, add an item (or change its count), remove one, or clear the kit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitCommand {
    Show,
    Add { item: String, count: u32 },
    Remove(String),
    Clear,
}

impl KitCommand {
    /// Whether the command changes the kit (and so needs an operator, and a game not yet started).
    pub fn changes(&self) -> bool {
        !matches!(self, KitCommand::Show)
    }
}

/// `minecraft:` ids: a bare name gets the prefix; the namespace must be `minecraft` and the path lowercase letters,
/// digits and underscores.
pub fn normalise_item(name: &str) -> Result<String, String> {
    let lower = name.to_ascii_lowercase();
    let id = if lower.contains(':') { lower } else { format!("minecraft:{lower}") };
    let valid = match id.split_once(':') {
        Some(("minecraft", path)) => {
            !path.is_empty() && path.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }
        _ => false,
    };
    if valid {
        Ok(id)
    } else {
        Err(texts::unknown_item(name))
    }
}

/// Parses the words after `kit`: none, `add <item> [count]`, `remove <item>` or `clear`.
pub fn parse(args: &[&str]) -> Result<KitCommand, String> {
    match args {
        [] => Ok(KitCommand::Show),
        [kind] if kind.eq_ignore_ascii_case("clear") => Ok(KitCommand::Clear),
        [kind, item] if kind.eq_ignore_ascii_case("add") => {
            Ok(KitCommand::Add { item: normalise_item(item)?, count: 1 })
        }
        [kind, item, count] if kind.eq_ignore_ascii_case("add") => {
            let n = match count.parse::<u32>() {
                Ok(n) if (1..=MAX_COUNT).contains(&n) => n,
                _ => return Err(format!("kit add needs a number of items from 1 to {MAX_COUNT}, got '{count}'")),
            };
            Ok(KitCommand::Add { item: normalise_item(item)?, count: n })
        }
        [kind] if kind.eq_ignore_ascii_case("add") => Err(texts::KIT_ADD_USAGE.to_string()),
        [kind, item] if kind.eq_ignore_ascii_case("remove") => Ok(KitCommand::Remove(normalise_item(item)?)),
        _ => Err(texts::KIT_USAGE.to_string()),
    }
}

impl Kit {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Sets the count of `item` (already normalised), adding it at the end when the kit does not have it yet.
    pub fn add(&mut self, item: &str, count: u32) -> Result<(), String> {
        if let Some(entry) = self.items.iter_mut().find(|entry| entry.item == item) {
            entry.count = count;
            return Ok(());
        }
        if self.items.len() >= MAX_ITEMS {
            return Err(texts::kit_full(MAX_ITEMS));
        }
        self.items.push(KitItem { item: item.to_string(), count });
        Ok(())
    }

    /// Takes `item` out of the kit; false if it was not there.
    pub fn remove(&mut self, item: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|entry| entry.item != item);
        self.items.len() != before
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The item ids, for keeping them off the cards.
    pub fn names(&self) -> Vec<String> {
        self.items.iter().map(|entry| entry.item.clone()).collect()
    }

    /// The console commands that give the kit to one player, in kit order.
    pub fn give_commands(&self, player: &str) -> Vec<String> {
        self.items.iter().map(|entry| format!("give {player} {} {}", entry.item, entry.count)).collect()
    }

    /// The kit as one line for `bingo kit`.
    pub fn describe(&self) -> String {
        if self.is_empty() {
            return texts::KIT_EMPTY.to_string();
        }
        let items: Vec<String> = self.items.iter().map(|entry| format!("{} {}", entry.count, entry.item)).collect();
        texts::kit_list(&items.join(", "))
    }

    /// Writes `"kit": [{"item": .., "count": ..}]` into the options object, when the kit is not empty.
    pub fn write_into(&self, object: &mut Map<String, Value>) {
        if self.is_empty() {
            return;
        }
        let entries: Vec<Value> = self
            .items
            .iter()
            .map(|entry| serde_json::json!({ "item": entry.item, "count": entry.count }))
            .collect();
        object.insert("kit".to_string(), Value::Array(entries));
    }

    /// Reads the kit of an options object. Entries that are not an item id with a count from 1 to `MAX_COUNT` are
    /// skipped, a repeated item keeps its first entry, and at most `MAX_ITEMS` entries are read.
    pub fn read_from(object: &Map<String, Value>) -> Kit {
        let Some(entries) = object.get("kit").and_then(Value::as_array) else {
            return Kit::default();
        };
        let mut kit = Kit::default();
        for entry in entries {
            let Some(obj) = entry.as_object() else { continue };
            let Some(item) = obj.get("item").and_then(Value::as_str).and_then(|s| normalise_item(s).ok()) else {
                continue;
            };
            let count = obj.get("count").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
            let Some(count) = count.filter(|n| (1..=MAX_COUNT).contains(n)) else { continue };
            if kit.items.iter().any(|seen| seen.item == item) {
                continue;
            }
            if kit.add(&item, count).is_err() {
                break;
            }
        }
        kit
    }
}

/// The pool of the tiers without the items of the kit; the same pool when the kit is empty.
pub fn pool_without(pool: &Pool, items: &[String]) -> Pool {
    let mut out = pool.clone();
    for (tier, weights) in out.tiers.iter_mut().zip(out.weights.iter_mut()) {
        // A tier's weights are kept beside its items: the same positions go.
        let kept: Vec<usize> = (0..tier.len()).filter(|&i| !items.contains(&tier[i])).collect();
        *tier = kept.iter().map(|&i| tier[i].clone()).collect();
        *weights = kept.iter().map(|&i| weights[i]).collect();
    }
    out
}

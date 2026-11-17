//! The card pool, built the way the original builds it (`CardService`, `TierListConfig`, `TagExpansionService`, and the
//! default filter `EVERYTHING`, which is `-tedious -unobtainable`):
//! - the four tier lists are combined (`items`, `simplified`, `challenge`, `advancements`), each entry keeping the tier
//!   it has in each list;
//! - tags are expanded to items: the player's vanilla item tags (read from the assets folder, see
//!   [`pool_from_assets`]) first, then the mod's own tags, as the original looks them up;
//! - item groups are kept: an item's draw weight is 1 / (the sizes of the groups it is in), and drawing an item excludes
//!   its whole group (see `card_gen`);
//! - the entries of `tedious` and `unobtainable` are left out.
//!
//! Only plain item ids are kept (`minecraft:apple`): an id with a `/` is an advancement or a challenge, which the port
//! cannot score. Two original rules are not ported: the uncategorized items (the original adds every registry item that
//! no list names; the `unobtainable` tag of the original is computed from loot tables and recipes the port cannot read,
//! so only the mod's static `tags/unobtainable.json` applies), and the filter presets of `filters.json`.
use super::card_gen::Pool;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const TIER_NAMES: [&str; 5] = ["S", "A", "B", "C", "D"];

/// The mod's tier lists, in the order the original combines them. Copied from the original (see NOTICE.md).
pub const TIER_LISTS: [(&str, &str); 4] = [
    ("items", crate::data::items_tierlist_json::JSON),
    ("simplified", crate::data::simplified_tierlist_json::JSON),
    ("challenge", crate::data::challenge_tierlist_json::JSON),
    ("advancements", crate::data::advancements_tierlist_json::JSON),
];

/// The mod's own tags, by name. Copied from the original (see NOTICE.md).
pub const MOD_TAGS: [(&str, &str); 3] = [
    ("tedious", crate::data::tags_tedious_json::JSON),
    ("unobtainable", crate::data::tags_unobtainable_json::JSON),
    ("needs_silk_touch", crate::data::tags_needs_silk_touch_json::JSON),
];

/// The default filter of the original: nothing tedious and nothing unobtainable.
pub const DEFAULT_EXCLUDED_TAGS: [&str; 2] =
    ["#yet-another-minecraft-bingo:tedious", "#yet-another-minecraft-bingo:unobtainable"];

/// One tier list as the file has it: the entries of each tier (tags not yet expanded), the uncategorized entries, and
/// the item groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TierList {
    pub replace: bool,
    pub values: Vec<String>,
    pub tiers: [Vec<String>; 5],
    pub groups: Vec<Vec<String>>,
}

/// Entries of a tag or tier list: strings, or objects with an `id` (the tag format's form for optional entries).
fn ids(value: &Value, what: &str) -> Result<Vec<String>, String> {
    let array = value.as_array().ok_or_else(|| format!("{what} is not an array"))?;
    array
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .or_else(|| entry.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .ok_or_else(|| format!("{what} has a non-string entry"))
        })
        .collect()
}

/// Reads one tier list file. The keys S, A, B, C, D are required; `replace`, `values` and `groups` are optional, and a
/// `groups` value that is not an array of arrays is ignored.
pub fn parse_tier_list(json: &str) -> Result<TierList, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let object = root.as_object().ok_or("tier list is not a JSON object")?;
    let mut list = TierList {
        replace: object.get("replace").and_then(Value::as_bool).unwrap_or(false),
        values: match object.get("values") {
            Some(values) => ids(values, "values")?,
            None => Vec::new(),
        },
        ..TierList::default()
    };
    for (index, name) in TIER_NAMES.iter().enumerate() {
        let entries = object.get(*name).ok_or_else(|| format!("tier {name} is missing"))?;
        list.tiers[index] = ids(entries, &format!("tier {name}"))?;
    }
    if let Some(groups) = object.get("groups").and_then(Value::as_array) {
        list.groups = groups.iter().filter_map(|group| ids(group, "group").ok()).collect();
    }
    Ok(list)
}

/// Reads a tag file: its `values`, which hold ids and `#tag` references.
pub fn parse_tag(json: &str) -> Result<Vec<String>, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let object = root.as_object().ok_or("tag is not a JSON object")?;
    match object.get("values") {
        Some(values) => ids(values, "values"),
        None => Ok(Vec::new()),
    }
}

/// Tags by id. Vanilla item tags are keyed `namespace:path`; the mod's own tags by their path only, as the original
/// finds a `#namespace:name` it does not know from the vanilla tags by `name`.
#[derive(Debug, Clone, Default)]
pub struct TagTable {
    vanilla: HashMap<String, Vec<String>>,
    own: HashMap<String, Vec<String>>,
    /// How many vanilla tag files were read (for the log).
    pub vanilla_files: usize,
}

impl TagTable {
    pub fn add_vanilla(&mut self, id: &str, values: Vec<String>) {
        self.vanilla.insert(id.to_string(), values);
    }

    pub fn add_own(&mut self, name: &str, values: Vec<String>) {
        self.own.insert(name.to_string(), values);
    }

    fn lookup(&self, tag: &str) -> Option<&Vec<String>> {
        self.vanilla.get(tag).or_else(|| self.own.get(tag.split_once(':').map_or(tag, |(_, path)| path)))
    }

    fn expand_into(&self, entry: &str, seen: &mut HashSet<String>, out: &mut Vec<String>, missing: &mut Vec<String>) {
        let Some(tag) = entry.strip_prefix('#') else {
            out.push(entry.to_string());
            return;
        };
        if !seen.insert(entry.to_string()) {
            return;
        }
        match self.lookup(tag) {
            Some(values) => {
                for value in values {
                    self.expand_into(value, seen, out, missing);
                }
            }
            None => missing.push(entry.to_string()),
        }
    }

    /// The plain ids that `entries` stand for, without repeats, in order. A `#tag` this table does not know gives
    /// nothing and is added to `missing`.
    pub fn expand(&self, entries: &[String], missing: &mut Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for entry in entries {
            self.expand_into(entry, &mut seen, &mut out, missing);
        }
        unique(out)
    }
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.into_iter().filter(|value| seen.insert(value.clone())).collect()
}

/// Only plain item ids: a namespaced id with no `/` and no tag reference.
fn is_item(id: &str) -> bool {
    id.contains(':') && !id.contains('/') && !id.starts_with('#')
}

/// Builds the pool the original builds from `lists` (in order) with the tags in `tags`, leaving out the ids that
/// `excluded_tags` expands to (`#`-references, as in [`DEFAULT_EXCLUDED_TAGS`]).
pub fn build_pool(lists: &[TierList], tags: &TagTable, excluded_tags: &[&str]) -> Pool {
    let mut missing = Vec::new();
    // TierListConfig.combine: a list with `replace` takes the place of everything before it; otherwise the sets join.
    let mut tiers: [Vec<String>; 5] = Default::default();
    let mut values: Vec<String> = Vec::new();
    let mut groups: Vec<Vec<String>> = Vec::new();
    for list in lists {
        if list.replace {
            tiers = Default::default();
            values.clear();
            groups.clear();
        }
        for (index, entries) in list.tiers.iter().enumerate() {
            tiers[index].extend(tags.expand(entries, &mut missing));
        }
        values.extend(tags.expand(&list.values, &mut missing));
        groups.extend(list.groups.iter().map(|group| unique(tags.expand(group, &mut missing))));
    }
    let tiers: [Vec<String>; 5] = tiers.map(unique);

    // Uncategorized entries are only those no tier names.
    let categorized: HashSet<&String> = tiers.iter().flatten().collect();
    let values: Vec<String> = unique(values).into_iter().filter(|value| !categorized.contains(value)).collect();

    let excluded_entries: Vec<String> = excluded_tags.iter().map(|tag| tag.to_string()).collect();
    let excluded: HashSet<String> = tags.expand(&excluded_entries, &mut missing).into_iter().collect();

    // An item's weight is 1 / the sizes of the groups it is in (at least 1), as the original's getWeightedSet.
    let mut group_size: HashMap<&str, usize> = HashMap::new();
    for group in &groups {
        for member in group {
            *group_size.entry(member.as_str()).or_insert(0) += group.len();
        }
    }
    let weight = |id: &str| 1.0 / group_size.get(id).copied().unwrap_or(0).max(1) as f64;

    let mut pool = Pool {
        tiers: Default::default(),
        weights: Default::default(),
        groups: Vec::new(),
        skipped_tags: Vec::new(),
        tag_files: tags.vanilla_files,
    };
    for index in 0..5 {
        for id in tiers[index].iter().filter(|id| is_item(id) && !excluded.contains(*id)) {
            pool.tiers[index].push(id.clone());
            pool.weights[index].push(weight(id));
        }
        // Uncategorized entries join every tier, each with a fifth of its weight (the original's `weight / 5.0`).
        for id in values.iter().filter(|id| is_item(id) && !excluded.contains(*id)) {
            pool.tiers[index].push(id.clone());
            pool.weights[index].push(weight(id) / 5.0);
        }
    }
    pool.groups = groups;
    pool.skipped_tags = unique(missing);
    pool
}

/// The mod's own tier lists and tags, parsed.
fn builtin_lists() -> Vec<TierList> {
    TIER_LISTS
        .iter()
        .map(|(name, json)| parse_tier_list(json).unwrap_or_else(|e| panic!("built-in {name} tier list: {e}")))
        .collect()
}

fn builtin_tags() -> TagTable {
    let mut tags = TagTable::default();
    for (name, json) in MOD_TAGS {
        tags.add_own(name, parse_tag(json).unwrap_or_else(|e| panic!("built-in {name} tag: {e}")));
    }
    tags
}

/// The default pool: the mod's four tier lists and tags, with no vanilla item tags (those come from the player's
/// assets, see [`pool_from_assets`]). Tags that need them are listed in `skipped_tags`.
pub fn builtin_pool() -> Pool {
    build_pool(&builtin_lists(), &builtin_tags(), &DEFAULT_EXCLUDED_TAGS)
}

/// The pool with the item tags the assets folder holds: `<dir>/tags/item/<namespace>/<path>.json`, one file per vanilla
/// item tag, as `tools/prepare_assets.py` writes them. A missing folder leaves those tags out (listed in
/// `skipped_tags`), so the pool still works without them.
pub fn pool_from_assets(dir: &Path) -> Pool {
    let root = dir.join("tags").join("item");
    let mut tags = builtin_tags();
    let mut stack = vec![root.clone()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&root) else { continue };
            let parts: Vec<String> = relative
                .with_extension("")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            if parts.len() < 2 {
                continue;
            }
            let id = format!("{}:{}", parts[0], parts[1..].join("/"));
            if let Ok(values) = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| parse_tag(&t)) {
                tags.add_vanilla(&id, values);
                tags.vanilla_files += 1;
            }
        }
    }
    build_pool(&builtin_lists(), &tags, &DEFAULT_EXCLUDED_TAGS)
}

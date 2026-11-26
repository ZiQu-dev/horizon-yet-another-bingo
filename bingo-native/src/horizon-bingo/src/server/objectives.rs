//! The advanced objectives of the original (`card/objective/`): `one_of`, `all_of`, `inverse`, `stats` and `advancement`,
//! next to the plain item square that `scoring.rs` scores. A square is an [`Objective`] with its tier ([`Square`]); a
//! plain card converts with [`from_card`], so an all-item board scores exactly as `scoring.rs` does.
//!
//! Sources in orig/common/src/main/java/me/jfenn/bingo/common/card/:
//! - data defaults (data/ObjectiveData.kt): `OneOf` and `AllOf` are not permanent; `Inverse` is permanent; `Stats` is
//!   relative, with `min` 1 and `max` unlimited; `Advancement` is not permanent.
//! - one_of / all_of (objective/SomeOfObjectiveManager.kt, tick): every item of the list is a sub-objective. A player
//!   holds `one_of` with at least one of them (`valueMin` 1) and `all_of` with every one (`valueMin` = count). Tags are
//!   expanded to their items first, so `all_of` over a tag needs all of the tag's items. An empty list holds nothing.
//! - inverse (objective/InverseObjectiveManager.kt, tick): a team holds the tile while no member holds the referenced
//!   objective now, or (when permanent) has held it at any earlier point. Nothing is retained: the tile is lost again as
//!   soon as a member holds the referenced objective. With no players in the team it holds (the mod's `none` is true).
//! - stats (objective/StatsObjectiveManager.kt, tick): the counter minus the value when the player was first counted
//!   (`relative`), held when the result is within `min..=max`. Not retained: the tile is lost when the value leaves the
//!   range, unless permanent.
//! - advancement (objective/AdvancementObjectiveManager.kt, tick): a player holds it when the advancement is done.
//! - retention (objective/ObjectiveService.kt, `getPlayers`): captures are kept in standard mode and dropped in inventory
//!   mode, except for the kinds above that are not retained at all. Lockout is for the caller: the squares that
//!   [`update_team`] returns go through `scoring::claim_lockout`, as the item squares do.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use serde_json::{json, Value};

use super::card_gen::Pool;
use super::host::ServerHost;
use super::pool;
use super::types::{Card, CARD_SQUARES, FREE_SPACE};
use crate::buf::Writer;
use crate::card_tile::{CardTile, TierLabel};
use crate::card_tile_action::CardTileAction;
use crate::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use crate::item::ItemStack;
use crate::text::Text;
use crate::update_card_tiles::CardTilesPacket;
use super::objective_names::{contents_name, italic, stat_name as stat_name_text, translated};
use super::wire;

/// Item tags of the round: a tag key with its `#` (`#minecraft:banners`) to the item ids it holds. A tag missing here
/// has no items.
pub type Tags = HashMap<String, Vec<String>>;

/// One square's objective. Entries of `OneOf` and `AllOf` are item ids or `#tag` keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Objective {
    /// A plain item (the square `scoring.rs` scores).
    Item { item: String },
    /// Any one of the entries (`one_of`).
    OneOf { entries: Vec<String>, permanent: bool },
    /// Every entry (`all_of`).
    AllOf { entries: Vec<String>, permanent: bool },
    /// The team holds this square while no member holds `of` (`inverse`).
    Inverse { of: Box<Objective>, permanent: bool },
    /// The statistic `stat_type` (and `stat_name`, for example `minecraft:mined` and `minecraft:stone`), relative to its
    /// value when first counted if `relative`, within `min..=max` (`stats`).
    Stats {
        stat_type: String,
        stat_name: Option<String>,
        min: i64,
        max: i64,
        relative: bool,
        permanent: bool,
    },
    /// The advancement `id` is done (`advancement`), such as `minecraft:story/mine_stone`.
    Advancement { id: String, permanent: bool },
}

/// A square of the board: its objective and its tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Square {
    pub objective: Objective,
    pub tier: TierLabel,
}

/// The squares of a plain card, as item objectives.
pub fn from_card(card: &Card) -> Vec<Square> {
    card.squares
        .iter()
        .map(|square| Square { objective: Objective::Item { item: square.item.clone() }, tier: square.tier })
        .collect()
}

/// What the rules read about a player: what the player holds, their statistics and advancements. Players are the
/// Minecraft UUIDs as `u128`.
pub trait Facts {
    /// How many of `item` the player holds.
    fn held(&self, uuid: u128, item: &str) -> u32;
    /// The statistic's counter, or `None` when the host cannot read it.
    fn stat(&self, uuid: u128, stat_type: &str, stat_name: Option<&str>) -> Option<i64>;
    /// Whether the player has the advancement done.
    fn advancement_done(&self, uuid: u128, id: &str) -> bool;
}

/// [`Facts`] over a server host: the inventories the session has seen (from `on_inventory`) and the host's statistics
/// and advancements.
pub struct HostFacts<'a> {
    pub held: &'a HashMap<u128, HashMap<String, u32>>,
    pub host: &'a dyn ServerHost,
}

impl Facts for HostFacts<'_> {
    fn held(&self, uuid: u128, item: &str) -> u32 {
        self.held.get(&uuid).and_then(|items| items.get(item)).copied().unwrap_or(0)
    }

    fn stat(&self, uuid: u128, stat_type: &str, stat_name: Option<&str>) -> Option<i64> {
        self.host.stat_value(uuid, stat_type, stat_name)
    }

    fn advancement_done(&self, uuid: u128, id: &str) -> bool {
        self.host.has_advancement(uuid, id)
    }
}

/// Reads an advancement's done flag from vanilla's progress JSON (`players/advancements/<uuid>.json`, as Horizon's
/// `player_advancements` gives it): `{"minecraft:story/root": {"criteria": {..}, "done": true}, ..}`. False when the
/// advancement is absent or not done.
pub fn advancement_done_json(progress: &serde_json::Value, id: &str) -> bool {
    progress.get(id).and_then(|advancement| advancement.get("done")).and_then(|done| done.as_bool()).unwrap_or(false)
}

/// What the rules are evaluated with: the facts and the round's tags.
pub struct Ctx<'a> {
    pub facts: &'a dyn Facts,
    pub tags: &'a Tags,
}

/// State the rules keep between evaluations: the players a `permanent` square has been held by, and the statistic each
/// player had when first counted (for `relative` stats). One per board, kept for the round.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Runtime {
    once: HashMap<usize, HashSet<u128>>,
    baseline: HashMap<(usize, u128), i64>,
}

/// The entries of a `one_of` or `all_of`, with tags replaced by their items. Duplicates count once, in list order. A
/// tag missing from `tags` contributes nothing.
pub fn expand(entries: &[String], tags: &Tags) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for entry in entries {
        let held: Vec<String> = if entry.starts_with('#') {
            tags.get(entry).cloned().unwrap_or_default()
        } else {
            vec![entry.clone()]
        };
        for item in held {
            if !items.contains(&item) {
                items.push(item);
            }
        }
    }
    items
}

/// Whether the objective is permanent: once a player holds it, the player keeps it.
fn is_permanent(objective: &Objective) -> bool {
    match objective {
        Objective::Item { .. } => false,
        Objective::OneOf { permanent, .. }
        | Objective::AllOf { permanent, .. }
        | Objective::Inverse { permanent, .. }
        | Objective::Stats { permanent, .. }
        | Objective::Advancement { permanent, .. } => *permanent,
    }
}

/// Whether a team keeps the square once found, when the rules say it is not holding it now. Stats and inverse squares
/// are not retained; in inventory mode nothing is retained.
fn is_retained(objective: &Objective, inventory_mode: bool) -> bool {
    !inventory_mode && !matches!(objective, Objective::Inverse { .. } | Objective::Stats { .. })
}

/// Whether `uuid` holds `objective` now (a player's own state; inverse is not a player's, see [`team_achieves`]).
fn holds_now(objective: &Objective, index: usize, uuid: u128, ctx: &Ctx, runtime: &mut Runtime) -> bool {
    match objective {
        Objective::Item { item } => item == FREE_SPACE || ctx.facts.held(uuid, item) >= 1,
        Objective::OneOf { entries, .. } => {
            expand(entries, ctx.tags).iter().any(|item| ctx.facts.held(uuid, item) >= 1)
        }
        Objective::AllOf { entries, .. } => {
            let items = expand(entries, ctx.tags);
            !items.is_empty() && items.iter().all(|item| ctx.facts.held(uuid, item) >= 1)
        }
        Objective::Stats { stat_type, stat_name, min, max, relative, .. } => {
            let Some(value) = ctx.facts.stat(uuid, stat_type, stat_name.as_deref()) else { return false };
            let base = if *relative { *runtime.baseline.entry((index, uuid)).or_insert(value) } else { 0 };
            (*min..=*max).contains(&(value - base))
        }
        Objective::Advancement { id, .. } => ctx.facts.advancement_done(uuid, id),
        Objective::Inverse { .. } => false,
    }
}

/// Whether the team of `members` holds square `index` now. Updates the permanent players in `runtime`.
pub fn team_achieves(index: usize, objective: &Objective, members: &[u128], ctx: &Ctx, runtime: &mut Runtime) -> bool {
    match objective {
        Objective::Inverse { of, permanent } => {
            let holding: Vec<(u128, bool)> =
                members.iter().map(|&uuid| (uuid, holds_now(of, index, uuid, ctx, runtime))).collect();
            if *permanent {
                for (uuid, now) in &holding {
                    if *now {
                        runtime.once.entry(index).or_default().insert(*uuid);
                    }
                }
            }
            let once = runtime.once.get(&index);
            holding.iter().all(|(uuid, now)| !(*now || once.is_some_and(|set| set.contains(uuid))))
        }
        _ => {
            let permanent = is_permanent(objective);
            let mut achieved = false;
            for &uuid in members {
                let now = holds_now(objective, index, uuid, ctx, runtime);
                if now && permanent {
                    runtime.once.entry(index).or_default().insert(uuid);
                }
                let kept = permanent && runtime.once.get(&index).is_some_and(|set| set.contains(&uuid));
                achieved |= now || kept;
            }
            achieved
        }
    }
}

/// Updates one team's found squares for the whole board, in square order. A square becomes found when the team holds it.
/// A found square stays found while it is retained (see the module note); otherwise it is removed again when the team
/// no longer holds it. Returns the squares newly found, in square order, for the announcements and for lockout.
pub fn update_team(
    squares: &[Square],
    found: &mut Vec<usize>,
    members: &[u128],
    ctx: &Ctx,
    runtime: &mut Runtime,
    inventory_mode: bool,
) -> Vec<usize> {
    let mut newly = Vec::new();
    for (index, square) in squares.iter().enumerate() {
        let achieved = team_achieves(index, &square.objective, members, ctx, runtime);
        let was_found = found.contains(&index);
        if achieved && !was_found {
            found.push(index);
            newly.push(index);
        } else if !achieved && was_found && !is_retained(&square.objective, inventory_mode) {
            found.retain(|&found_index| found_index != index);
        }
    }
    newly
}

/// The name of an objective for a tile's id and a `not` tile's text.
pub fn describe(objective: &Objective) -> String {
    match objective {
        Objective::Item { item } => item.clone(),
        Objective::OneOf { entries, .. } | Objective::AllOf { entries, .. } => entries.join(", "),
        Objective::Inverse { of, .. } => describe(of),
        Objective::Stats { stat_type, stat_name, .. } => stat_name.clone().unwrap_or_else(|| stat_type.clone()),
        Objective::Advancement { id, .. } => id.clone(),
    }
}

fn stack(item: &str, item_id: &dyn Fn(&str) -> Option<i32>) -> Option<ItemStack> {
    item_id(item).map(|id| ItemStack { count: 1, item: id, added: vec![], removed: vec![] })
}

/// One square as the client's tile. `is_found` is the team's state for the square; `index` is its place on the card.
/// An item square is built as `wire::card_tiles` builds it, so a card of plain items reads the same either way.
fn to_tile(team: &str, square: &Square, index: usize, is_found: bool, item_id: &dyn Fn(&str) -> Option<i32>, tags: &Tags) -> CardTile {
    let mut image = CardTileImage { is_achieved: is_found, ..CardTileImage::default() };
    let mut image_list = Vec::new();
    let mut action = CardTileAction::None;
    let name = match &square.objective {
        Objective::Item { item } => {
            image.item = stack(item, item_id);
            if let Some(stack) = &image.item {
                action = CardTileAction::Item(stack.clone());
            }
            if item == FREE_SPACE {
                image.decoration = Decoration::FreeSpace;
            }
            wire::item_name(item)
        }
        Objective::OneOf { entries, .. } | Objective::AllOf { entries, .. } => {
            let items = expand(entries, tags);
            let is_one_of = matches!(square.objective, Objective::OneOf { .. });
            image.item = items.iter().find_map(|item| stack(item, item_id));
            image.item_style = if is_one_of { ItemStyle::OneOf } else { ItemStyle::AllOf };
            image.decoration = Decoration::MultiItem;
            image_list = items
                .iter()
                .map(|item| CardTileImage {
                    item: stack(item, item_id),
                    is_achieved: is_found,
                    ..CardTileImage::default()
                })
                .collect();
            let key = if is_one_of { "bingo.objective.one_of" } else { "bingo.objective.all_of" };
            let first = items.first().map(|item| wire::item_name(item));
            italic(translated(key, vec![contents_name(first)]))
        }
        Objective::Inverse { of, .. } => {
            if let Objective::Item { item } = of.as_ref() {
                image.item = stack(item, item_id);
            }
            image.decoration = Decoration::Forbidden;
            let inner = match of.as_ref() {
                Objective::Item { item } => wire::item_name(item),
                other => Text::plain(&describe(other)),
            };
            italic(translated("bingo.objective.inverse", vec![inner]))
        }
        Objective::Stats { stat_type, stat_name, min, .. } => {
            image.decoration = Decoration::Advancement;
            italic(stat_name_text(stat_type, stat_name.as_deref(), *min))
        }
        Objective::Advancement { id, .. } => {
            image.decoration = Decoration::Advancement;
            action = CardTileAction::Advancement(id.clone());
            Text::plain(id)
        }
    };
    CardTile {
        id: Some(describe(&square.objective)),
        image,
        image_list,
        action,
        item_tier: Some(square.tier),
        name: Some(name),
        lore: vec![wire::tile_lore(square.tier, index)],
        is_hidden: false,
        is_locked: false,
        is_flashing: false,
        progress: if is_found { 1.0 } else { 0.0 },
        team_keys: if is_found { vec![team.to_owned()] } else { vec![] },
    }
}

/// The `update_card_tiles` packet for a board of any objectives: `(channel path, payload)`, as `wire::card_tiles` gives
/// for a plain card. `found` is the team's found squares. Errors when the board is not 25 squares.
pub fn card_from_objectives(
    team: &str,
    squares: &[Square],
    found: &[usize],
    item_id: &dyn Fn(&str) -> Option<i32>,
    tags: &Tags,
    should_notify: bool,
) -> Result<(&'static str, Vec<u8>), String> {
    if squares.len() != CARD_SQUARES {
        return Err(format!("a card has {CARD_SQUARES} squares, not {}", squares.len()));
    }
    let tiles = squares
        .iter()
        .enumerate()
        .map(|(index, square)| (index as i32, to_tile(team, square, index, found.contains(&index), item_id, tags)))
        .collect();
    let packet = CardTilesPacket { team_key: Some(team.to_owned()), tiles, should_notify };
    let mut writer = Writer::default();
    packet.write(&mut writer).map_err(|e| format!("update_card_tiles: {e:?}"))?;
    Ok(("update_card_tiles", writer.into_bytes()))
}

// ---- Cards with objective squares ----
//
// A card of plain items is a card of [`Objective::Item`] squares, and the session scores it as it always did. A card
// with advanced squares holds, in place of each such square's item, the key `objective:<id>`: an advancement id
// (`minecraft:adventure/arbalistic`) or a challenge of the original (`yet-another-minecraft-bingo:challenge/...`, read
// from `data/objectives.json`). The key is the square's only record, so the objective is read back from it when a
// game is restored and nothing can drift apart.

/// The prefix of the key an objective square holds in its card's item.
pub const KEY_PREFIX: &str = "objective:";
/// The original's challenge objectives the port can score, as `data/objectives.json` gives them (generated from the
/// original's `objectives/challenge/*.json`; NBT and component checks and the item counts are left out).
const CATALOG_JSON: &str = crate::data::objectives_json::JSON;

/// The challenge objectives by id, read once.
fn catalog() -> &'static HashMap<String, Objective> {
    static CATALOG: OnceLock<HashMap<String, Objective>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let root: Value = serde_json::from_str(CATALOG_JSON).expect("data/objectives.json is JSON");
        let object = root.as_object().expect("data/objectives.json is an object");
        object
            .iter()
            .filter_map(|(id, value)| parse_objective(value).map(|objective| (id.clone(), objective)))
            .collect()
    })
}

/// One objective of the catalog's form: `{"type": "item", "item"}`, `{"type": "one_of" | "all_of", "entries"}`,
/// `{"type": "inverse", "of"}` (permanent, as the original's default), or `{"type": "stats", "statType", "statName"?}`
/// (relative, `min` 1, `max` unbounded, as the original's defaults). `None` for anything else.
fn parse_objective(value: &Value) -> Option<Objective> {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let entries = || -> Option<Vec<String>> {
        value.get("entries")?.as_array()?.iter().map(|entry| entry.as_str().map(str::to_string)).collect()
    };
    match value.get("type")?.as_str()? {
        "item" => Some(Objective::Item { item: text("item")? }),
        "one_of" => Some(Objective::OneOf { entries: entries()?, permanent: false }),
        "all_of" => Some(Objective::AllOf { entries: entries()?, permanent: false }),
        "inverse" => {
            let of = value.get("of")?;
            // `of` may be a bare item id, as `never_get_flint_and_steel` writes it.
            let of = match of.as_str() {
                Some(item) => Objective::Item { item: item.to_string() },
                None => parse_objective(of)?,
            };
            Some(Objective::Inverse { of: Box::new(of), permanent: true })
        }
        "stats" => Some(Objective::Stats {
            stat_type: text("statType")?,
            stat_name: text("statName"),
            min: 1,
            max: i64::MAX,
            relative: true,
            permanent: false,
        }),
        _ => None,
    }
}

/// The objective a square key stands for, or `None` for a key that is neither a catalog id nor a vanilla advancement.
pub fn objective_of_key(key: &str) -> Option<Objective> {
    let id = key.strip_prefix(KEY_PREFIX)?;
    if let Some(objective) = catalog().get(id) {
        return Some(objective.clone());
    }
    id.starts_with("minecraft:").then(|| Objective::Advancement { id: id.to_string(), permanent: false })
}

/// The key a pool entry of the original's tier lists becomes, when it can be scored: every advancement, and every
/// challenge the catalog has. `None` for plain items and for challenges the port cannot score.
fn key_for_entry(entry: &str) -> Option<String> {
    let key = format!("{KEY_PREFIX}{entry}");
    objective_of_key(&key).map(|_| key)
}

/// The pool with the advanced squares added: each advancement and each scorable challenge of the original's tier lists
/// joins the tier it has in its list, with weight 1. The item entries are left as they are.
pub fn with_objectives(pool: &Pool) -> Pool {
    let mut pool = pool.clone();
    for (name, json) in pool::TIER_LISTS {
        if name != "advancements" && name != "challenge" {
            continue;
        }
        let Ok(list) = pool::parse_tier_list(json) else { continue };
        for (tier, entries) in list.tiers.iter().enumerate() {
            for entry in entries {
                let Some(key) = key_for_entry(entry) else { continue };
                if !pool.tiers[tier].contains(&key) {
                    pool.tiers[tier].push(key);
                    pool.weights[tier].push(1.0);
                }
            }
        }
    }
    pool
}

/// The squares of a card with objective squares, or `None` for a card of plain items (the session scores those as
/// it always has). Each square is its key's objective, or a plain item for an item.
pub fn card_squares(card: &Card) -> Option<Vec<Square>> {
    if !card.squares.iter().any(|square| square.item.starts_with(KEY_PREFIX)) {
        return None;
    }
    Some(
        card.squares
            .iter()
            .map(|square| Square {
                objective: objective_of_key(&square.item).unwrap_or_else(|| Objective::Item { item: square.item.clone() }),
                tier: square.tier,
            })
            .collect(),
    )
}

impl Runtime {
    /// Forgets the squares `squares` (a square replaced by another objective starts afresh: its permanent holders and
    /// its statistic baseline go).
    pub fn forget(&mut self, squares: &[usize]) {
        self.once.retain(|square, _| !squares.contains(square));
        self.baseline.retain(|(square, _), _| !squares.contains(square));
    }

    /// The runtime as JSON, for `game-state.json`: the players each permanent square has been held by, and each
    /// relative stat's baseline. Sorted, so the same state gives the same text.
    pub fn to_json(&self) -> Value {
        let once: BTreeMap<usize, Vec<String>> = self
            .once
            .iter()
            .map(|(&square, players)| {
                let mut hex: Vec<String> = players.iter().map(|&player| format!("{player:032x}")).collect();
                hex.sort();
                (square, hex)
            })
            .collect();
        let baseline: BTreeMap<(usize, String), i64> = self
            .baseline
            .iter()
            .map(|(&(square, player), &value)| ((square, format!("{player:032x}")), value))
            .collect();
        let baseline: Vec<Value> = baseline
            .into_iter()
            .map(|((square, player), value)| json!({ "square": square, "player": player, "value": value }))
            .collect();
        json!({ "once": once, "baseline": baseline })
    }

    /// Reads [`Runtime::to_json`]. Entries that do not parse are left out.
    pub fn from_json(value: &Value) -> Self {
        let mut runtime = Runtime::default();
        let hex = |text: &str| u128::from_str_radix(text, 16).ok();
        if let Some(object) = value.get("once").and_then(Value::as_object) {
            for (key, list) in object {
                let (Ok(square), Some(list)) = (key.parse::<usize>(), list.as_array()) else { continue };
                let set = runtime.once.entry(square).or_default();
                set.extend(list.iter().filter_map(Value::as_str).filter_map(hex));
            }
        }
        if let Some(list) = value.get("baseline").and_then(Value::as_array) {
            for entry in list {
                let square = entry.get("square").and_then(Value::as_u64);
                let player = entry.get("player").and_then(Value::as_str).and_then(hex);
                let stat = entry.get("value").and_then(Value::as_i64);
                if let (Some(square), Some(player), Some(stat)) = (square, player, stat) {
                    runtime.baseline.insert((square as usize, player), stat);
                }
            }
        }
        runtime
    }
}

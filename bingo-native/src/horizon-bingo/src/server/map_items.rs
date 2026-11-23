//! The bingo card on the players' maps: which map each player holds and when they get it, the map ids kept for each team
//! (and for the lobby preview), and the repaint of each map. Port of `map/MapItemController.kt` and `map/MapItemService.kt`
//! for players without the client's HUD, and of the throttled update of `map/BingoMapController.kt`.
//!
//! Pure logic: everything the server does goes through [`ServerHost`] (`new_map`, `paint_map`, `send_map`, `give_map`,
//! and the `clear` command). The session decides who is a player without the HUD and passes each player's [`Want`] in.
//!
//! Rules kept from the original: a team player gets the team's map while the game is set up or played; a player on no
//! team gets the preview map in the lobby; after the game a team player gets a memento (the team's own map, with the
//! result in its name); every map a player held is removed before the next one is given. A map cannot be deleted, so
//! each key keeps its map id for the whole server (`map-ids.json`).
//!
//! Known deviations: a picture is repainted whole, since the host has no partial paint (the dirty rectangle is passed to
//! the host, which may use it); repaints are throttled to one per [`PAINT_INTERVAL`] ticks; spectators get no maps.
use std::collections::BTreeMap;

use serde_json::Value;

use super::host::ServerHost;
use super::hidden_consume::TileView;
use super::map_render::{
    render_card, CardView, DirtyRect, IconSource, MapBuffer, MapTile, MemoryIcons, Overlays, Style, MAP_BYTES,
};
use super::teams;
use crate::game::GameStatus;

/// The map ids of the server, by key: a team's key (`bingo_red`) or [`PREVIEW_KEY`]. Kept beside the game, but not
/// emptied when a game ends, since a map cannot be deleted and each team keeps its map for good.
pub const MAP_IDS_FILE: &str = "map-ids.json";
/// The key of the lobby's preview map.
pub const PREVIEW_KEY: &str = "preview";
/// Ticks between two repaints of a map. A scored tile flashes on a 10-tick beat (`render_card`), so this is the
/// rate at which the picture can change.
pub const PAINT_INTERVAL: u64 = 10;
/// The lobby preview is shown to players on no team (`showPreviewCard`, on by default in the original).
pub const SHOW_PREVIEW_CARD: bool = true;
/// The item the server gives: a filled map.
pub const FILLED_MAP: &str = "minecraft:filled_map";
/// Removes every bingo map from a player. The predicate matches the marker every bingo map carries.
const CARD_ITEMS: &str = "minecraft:filled_map[minecraft:custom_data~{bingo_card:1b}]";

/// What a player should hold, for the map of one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Want {
    /// The team's card, while the game is being set up or is played.
    Team { key: String },
    /// The team's card after the game: the memento, named for its result.
    Memento { key: String, won: bool },
    /// The lobby's preview card, for a player on no team.
    Preview,
}

impl Want {
    /// The key of the map this item shows: the team's key, or [`PREVIEW_KEY`].
    pub fn key(&self) -> &str {
        match self {
            Want::Team { key } | Want::Memento { key, .. } => key,
            Want::Preview => PREVIEW_KEY,
        }
    }
}

/// The item a player should hold, from the game phase and the player's team. A spectator and a player with the client's
/// HUD get none. Before the game (`Pregame`, and `Starting` for a team) a team player gets the team's card and a player
/// on no team gets the preview; after the game a team player gets the memento.
pub fn want_for(status: GameStatus, team: Option<&str>, spectator: bool, hud: bool, won: bool) -> Option<Want> {
    want_for_with(status, team, spectator, hud, won, SHOW_PREVIEW_CARD)
}

/// [`want_for`] with the `showPreviewCard` option given: off, nobody gets a map before the game.
pub fn want_for_with(status: GameStatus, team: Option<&str>, spectator: bool, hud: bool, won: bool, preview: bool) -> Option<Want> {
    if hud || spectator {
        return None;
    }
    match (status, team) {
        (GameStatus::Pregame, Some(key)) if preview => Some(Want::Team { key: key.to_string() }),
        (GameStatus::Pregame, None) if preview => Some(Want::Preview),
        (GameStatus::Starting | GameStatus::Playing, Some(key)) => Some(Want::Team { key: key.to_string() }),
        (GameStatus::Postgame, Some(key)) => Some(Want::Memento { key: key.to_string(), won }),
        _ => None,
    }
}

/// The vanilla colour of a `Formatting` name, as `TextColor` gives it (the `map_color` of a team's item).
pub fn formatting_rgb(name: &str) -> Option<u32> {
    Some(match name {
        "BLACK" => 0x000000,
        "DARK_BLUE" => 0x0000AA,
        "DARK_GREEN" => 0x00AA00,
        "DARK_AQUA" => 0x00AAAA,
        "DARK_RED" => 0xAA0000,
        "DARK_PURPLE" => 0xAA00AA,
        "GOLD" => 0xFFAA00,
        "GRAY" => 0xAAAAAA,
        "DARK_GRAY" => 0x555555,
        "BLUE" => 0x5555FF,
        "GREEN" => 0x55FF55,
        "AQUA" => 0x55FFFF,
        "RED" => 0xFF5555,
        "LIGHT_PURPLE" => 0xFF55FF,
        "YELLOW" => 0xFFFF55,
        "WHITE" => 0xFFFFFF,
        _ => return None,
    })
}

/// A double-quoted SNBT string: a backslash and a quote are escaped.
fn snbt_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A text component in SNBT: the text, an optional colour (a `Formatting` name, lower case) and italics.
fn name_snbt(text: &str, color: Option<&str>) -> String {
    let color = color.map_or(String::new(), |c| format!("color:\"{}\",", c.to_ascii_lowercase()));
    format!("{{text:{},{color}italic:1b}}", snbt_string(text))
}

/// The SNBT of the item a player holds for `want`, on map `map_id`: a filled map with the bingo marker, named as the
/// original names it. Read by the host's `item_from_snbt`.
pub fn item_snbt(map_id: i32, want: &Want) -> String {
    let mut components = vec![format!("\"minecraft:map_id\":{map_id}")];
    match want {
        Want::Preview => {
            components.push(format!("\"minecraft:custom_name\":{}", name_snbt("Card preview", None)));
        }
        Want::Team { key } => {
            let (name, color) = team_text(key);
            components.push(format!("\"minecraft:custom_name\":{}", name_snbt(&format!("Team card ({name})"), color)));
            push_map_color(&mut components, color);
        }
        Want::Memento { key, won } => {
            let (name, color) = team_text(key);
            let result = if *won { "won" } else { "lost" };
            let text = format!("Team card ({name}, {result})");
            components.push(format!("\"minecraft:custom_name\":{}", name_snbt(&text, color)));
            push_map_color(&mut components, color);
        }
    }
    components.push("\"minecraft:custom_data\":{bingo_card:1b}".to_string());
    components.push("\"minecraft:tooltip_display\":{hide_additional_tooltip:1b}".to_string());
    format!("{{id:\"{FILLED_MAP}\",count:1,components:{{{}}}}}", components.join(","))
}

/// A team's display name and its colour name (lower case, for the text), from its preset. An unknown key is its own name
/// and no colour.
fn team_text(key: &str) -> (&str, Option<&'static str>) {
    match teams::preset(key) {
        Some(preset) => (preset.name, Some(preset.color)),
        None => (key, None),
    }
}

fn push_map_color(components: &mut Vec<String>, color: Option<&str>) {
    if let Some(rgb) = color.and_then(formatting_rgb) {
        components.push(format!("\"minecraft:map_color\":{rgb}"));
    }
}

/// The squares a team has just scored: they flash on the team's map for 80 ticks from the tick they were scored at. Kept
/// apart from [`MapItems`], since a map's card is read while the maps are being ticked.
#[derive(Debug, Clone, Default)]
pub struct Flashes {
    by_team: BTreeMap<String, BTreeMap<usize, u64>>,
}

impl Flashes {
    /// Squares a team has just scored, at `tick`.
    pub fn note_scored(&mut self, team: &str, squares: &[usize], tick: u64) {
        let flashes = self.by_team.entry(team.to_string()).or_default();
        for &square in squares {
            flashes.insert(square, tick);
        }
    }

    /// The tick a square of a team was scored at, while it can still flash.
    pub fn flash_since(&self, team: &str, square: usize) -> Option<u64> {
        self.by_team.get(team)?.get(&square).copied()
    }

    /// Clears every flash, at the start of a round.
    pub fn clear(&mut self) {
        self.by_team.clear();
    }
}

/// What one square of a team's card shows on its map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SquareState {
    /// The viewer's team has the square.
    pub found: bool,
    /// Other teams with the square, by their colour name (`Formatting`, as `teams::preset` gives it).
    pub achieved_by: Vec<String>,
    /// The tick the square was scored at, while it flashes.
    pub flash_since: Option<u64>,
}

/// A card as a map shows it: a tile per square from the viewer's view (hidden squares show the hidden picture), with
/// the team's state of each square. `lockout` locks the squares another team has and this one does not.
pub fn card_view(team: Option<&str>, views: &[TileView], squares: &[SquareState], lockout: bool) -> CardView {
    let tiles = views
        .iter()
        .enumerate()
        .map(|(index, view)| {
            let mut tile = MapTile::from_view(view);
            if view.hidden {
                return tile;
            }
            let square = squares.get(index).cloned().unwrap_or_default();
            tile.achieved = square.found;
            tile.locked = lockout && !square.found && !square.achieved_by.is_empty();
            tile.achieved_by = square.achieved_by;
            tile.flash_since = square.flash_since;
            tile
        })
        .collect();
    CardView { team_color: team.map(str::to_string), tiles }
}

/// One map the server keeps: its id, what it shows, and when it was last painted.
struct Slot {
    id: i32,
    buffer: MapBuffer,
    last_paint: u64,
}

/// A player's item by the map's key: `None` is no item, and an absent entry is a player the server has not checked yet.
type Held = Option<Want>;

/// The map items of the server. The session owns one for the whole server, and calls [`MapItems::tick`] every tick.
pub struct MapItems {
    /// Map id by key. Kept across games; written to `map-ids.json` when it changes.
    ids: BTreeMap<String, i32>,
    ids_changed: bool,
    /// The ids came from a file a previous server wrote: a player may still hold a map from it, so each player is
    /// checked once (their bingo maps are taken) before the first one is given.
    stale_maps: bool,
    /// The maps painted this server, by key.
    slots: BTreeMap<String, Slot>,
    /// What each player holds (`None` inside: nothing), by player.
    held: BTreeMap<u128, Held>,
    icons: Box<dyn IconSource + Send>,
    overlays: Overlays,
}

impl Default for MapItems {
    fn default() -> Self {
        Self {
            ids: BTreeMap::new(),
            ids_changed: false,
            stale_maps: false,
            slots: BTreeMap::new(),
            held: BTreeMap::new(),
            icons: Box::new(MemoryIcons::default()),
            overlays: Overlays::default(),
        }
    }
}

fn draw(icons: &dyn IconSource, overlays: &Overlays, buffer: &mut MapBuffer, card: &CardView, tick: u64) -> DirtyRect {
    render_card(card, &Style { icons, overlays }, tick, buffer)
}

impl MapItems {
    pub fn new() -> Self {
        Self::default()
    }

    /// The item pictures and the map overlays the maps are drawn with.
    pub fn set_assets(&mut self, icons: Box<dyn IconSource + Send>, overlays: Overlays) {
        self.icons = icons;
        self.overlays = overlays;
    }

    /// Reads the map ids from `map-ids.json`. A text that is not an object of numbers gives no ids.
    pub fn load_ids(&mut self, text: &str) {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else { return };
        self.ids = map
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), i32::try_from(value.as_i64()?).ok()?)))
            .collect();
        self.stale_maps = !self.ids.is_empty();
    }

    /// The map ids as `map-ids.json` holds them.
    pub fn ids_json(&self) -> String {
        serde_json::to_string_pretty(&self.ids).expect("a map of numbers always serializes")
    }

    /// The map ids' text, once after each change: `None` when nothing changed since the last call.
    pub fn take_ids_to_save(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.ids_changed) {
            return None;
        }
        Some(self.ids_json())
    }

    /// The map id of a key, if the server has one.
    pub fn id_of(&self, key: &str) -> Option<i32> {
        self.ids.get(key).copied()
    }

    /// A player's inventory has (or has not) a filled map. A player who no longer holds the map the server gave is
    /// given one again on the next tick.
    pub fn note_inventory(&mut self, player: u128, holds_a_filled_map: bool) {
        if holds_a_filled_map {
            return;
        }
        if let Some(held) = self.held.get_mut(&player) {
            if held.is_some() {
                *held = None;
            }
        }
    }

    /// One server tick. Each player's item is brought to their [`Want`] (one step per tick: a bingo map held is removed
    /// first, and the new one is given on the next tick, since the removal is a queued command). Then every map is
    /// repainted if its picture changed, at most once per [`PAINT_INTERVAL`] ticks. `view` gives a key's card as it is now.
    pub fn tick(
        &mut self,
        host: &mut dyn ServerHost,
        tick: u64,
        wants: &[(u128, String, Option<Want>)],
        view: &dyn Fn(&str) -> Option<CardView>,
    ) {
        for (uuid, name, want) in wants {
            let known: Option<Held> = match self.held.get(uuid) {
                Some(held) => Some(held.clone()),
                // Nobody holds a map from an earlier server unless the ids came from its file.
                None if !self.stale_maps => Some(None),
                None => None,
            };
            match (known, want) {
                (Some(held), _) if held == *want => {}
                (Some(None), Some(wanted)) => self.give(host, *uuid, wanted, tick, view),
                _ => {
                    host.command(&format!("clear {name} {CARD_ITEMS}"));
                    self.held.insert(*uuid, None);
                }
            }
        }

        let mut lost = Vec::new();
        for (key, slot) in self.slots.iter_mut() {
            if tick.saturating_sub(slot.last_paint) < PAINT_INTERVAL {
                continue;
            }
            slot.last_paint = tick;
            let Some(card) = view(key.as_str()) else { continue };
            let dirty = draw(&*self.icons, &self.overlays, &mut slot.buffer, &card, tick);
            if !dirty.is_empty() && !host.paint_map(slot.id, slot.buffer.pixels(), dirty) {
                lost.push(key.clone());
            }
        }
        // A map the host no longer has is made again the next time it is needed.
        for key in lost {
            self.slots.remove(&key);
        }
    }

    fn give(
        &mut self,
        host: &mut dyn ServerHost,
        player: u128,
        want: &Want,
        tick: u64,
        view: &dyn Fn(&str) -> Option<CardView>,
    ) {
        let Some(id) = self.slot_for(host, want.key(), tick, view) else { return };
        if host.give_map(player, &item_snbt(id, want)) {
            if let Some(slot) = self.slots.get(want.key()) {
                host.send_map(player, id, slot.buffer.pixels());
            }
            self.held.insert(player, Some(want.clone()));
        }
    }

    /// The map of a key, made or found on the first use. A map the server has an id for is painted again (the id is
    /// reused); a map the host no longer has gets a new one.
    fn slot_for(
        &mut self,
        host: &mut dyn ServerHost,
        key: &str,
        tick: u64,
        view: &dyn Fn(&str) -> Option<CardView>,
    ) -> Option<i32> {
        if let Some(slot) = self.slots.get(key) {
            return Some(slot.id);
        }
        let card = view(key)?;
        let mut buffer = MapBuffer::new();
        draw(&*self.icons, &self.overlays, &mut buffer, &card, tick);
        let pixels = buffer.pixels().to_vec();
        debug_assert_eq!(pixels.len(), MAP_BYTES);
        let reused = self.ids.get(key).copied().filter(|&id| host.paint_map(id, &pixels, DirtyRect::FULL));
        let id = match reused {
            Some(id) => id,
            None => {
                let id = host.new_map(&pixels)?;
                self.ids.insert(key.to_string(), id);
                self.ids_changed = true;
                id
            }
        };
        self.slots.insert(key.to_string(), Slot { id, buffer, last_paint: tick });
        Some(id)
    }
}

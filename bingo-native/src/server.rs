//! The server half of "Yet Another Minecraft BINGO": one game session, driven by Horizon's server hooks. The game is
//! `horizon-bingo`'s `server` modules; this file adapts the mod API's `ServerHost` to their `ServerHost` and keeps the session.
//! Each hook runs under `guarded`, and a poisoned lock is taken as it is, so one panic does not end the server's mod.
use super::{MOD_ID, guarded, log};
use horizon_bingo::lobby_world;
use horizon_bingo::server::chat;
use horizon_bingo::server::commands::{Command, chat_shortcut, parse, requires_op};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::lobby_scene;
use horizon_bingo::server::items::{ItemIds, NO_ICONS_NOTE, missing_names_warning};
use horizon_bingo::server::map_render::{DirtyRect, FileIcons, IconSource, Overlays};
use horizon_bingo::server::menu::{self, Menus, Outcome, Page, View};
use horizon_bingo::server::options::Options;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::sidebar::Shown;
use horizon_bingo::server::texts::{self, Line};
use horizon_bingo::server::types::Player;
use horizon_bingo::server::world_reset::{self, Mode, ResetRefusal, WorldPort};
use mod_api::{
    ConfigFileError, ConfigPath, Flow, GameMode, Identifier, ItemKey, ItemStack, LevelResetError, MenuReplace, PayloadBytes,
    PayloadId, RuntimeGenerator, RuntimeLevel, RuntimeLevelError, ServerTickPhase, StatType, Tag, Text,
};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock, PoisonError,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// The namespace of the mod's plugin channels; a channel's path is what follows it.
const CHANNEL_PREFIX: &str = "yet-another-minecraft-bingo:";
/// The command's first word, without its slash.
const COMMAND: &str = "bingo";
/// The operator level of the commands that change a game (the original asks for `CONFIGURE_GAME`).
const OP_LEVEL: u8 = 2;
/// The shortcuts a player or the console may type without `bingo`.
const SHORTCUTS: [&str; 3] = ["join", "ready", "spectator"];
/// The usages the mod registers, each with a line that parses to what it runs. The operator level comes from
/// `commands::requires_op`, so the rule lives in one place.
const USAGES: [(&str, &str); 53] = [
    ("bingo status", "status"),
    ("bingo book", "book"),
    ("bingo help", "help"),
    ("bingo card", "card"),
    ("bingo spectator", "spectator"),
    ("bingo ready", "ready"),
    ("bingo ready cancel", "ready cancel"),
    ("bingo start", "start"),
    ("bingo start ignore_warnings", "start ignore_warnings"),
    ("bingo end", "end"),
    ("bingo reset", "reset"),
    ("bingo shuffleteams <teams:integer>", "shuffleteams 2"),
    ("bingo timelimit <minutes:word>", "timelimit 5"),
    ("bingo join <team:word>", "join Red"),
    ("bingo goal lines <n:integer>", "goal lines 1"),
    ("bingo goal items <n:integer>", "goal items 5"),
    ("bingo goal full_card", "goal full_card"),
    ("bingo mode <mode:word>", "mode lockout"),
    (
        "bingo mode hidden_items <state:word>",
        "mode hidden_items on",
    ),
    (
        "bingo mode consume_items <state:word>",
        "mode consume_items on",
    ),
    ("bingo endwhen <when:word>", "endwhen never"),
    ("bingo endwhen teams_win <n:integer>", "endwhen teams_win 2"),
    ("bingo pvp <state:word>", "pvp on"),
    ("bingo keepinventory <state:word>", "keepinventory on"),
    ("bingo elytra <state:word>", "elytra on"),
    ("bingo nightvision <state:word>", "nightvision on"),
    ("bingo spawndistance <chunks:integer>", "spawndistance 25"),
    ("bingo playto cards <n:integer>", "playto cards 2"),
    ("bingo playto infinite", "playto infinite"),
    ("bingo playto replace_goals", "playto replace_goals"),
    ("bingo stalemate <behavior:word>", "stalemate end_game"),
    ("bingo options", "options"),
    ("bingo vote <choice:word>", "vote next"),
    ("bingo resume", "resume"),
    ("bingo stats", "stats"),
    ("bingo difficulty <preset:word>", "difficulty medium"),
    (
        "bingo difficulty <s:integer> <a:integer> <b:integer> <c:integer> <d:integer>",
        "difficulty 0 0 3 10 12",
    ),
    ("bingo reroll", "reroll"),
    ("bingo teams <n:integer>", "teams 3"),
    ("bingo autoshuffle <state:word>", "autoshuffle on"),
    ("bingo objectives <state:word>", "objectives on"),
    ("bingo stats <player:word>", "stats Alice"),
    ("bingo menu", "menu"),
    ("bingo kit", "kit"),
    ("bingo kit add <item:word>", "kit add diamond"),
    (
        "bingo kit add <item:word> <count:integer>",
        "kit add diamond 2",
    ),
    ("bingo kit remove <item:word>", "kit remove diamond"),
    ("bingo kit clear", "kit clear"),
    ("bingo prefs <setting:word>", "prefs bossbar"),
    (
        "bingo prefs <setting:word> <value:word>",
        "prefs bossbar on",
    ),
    ("join <team:word>", "join Red"),
    ("ready", "ready"),
    ("spectator", "spectator"),
];
/// The most console commands queued for the next server tick; more are dropped and logged.
const COMMAND_QUEUE_MAX: usize = 256;

/// The one session of this server. Made on the first server hook.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// The lobby world's runtime level: the mod opens it under this id, and its saved folder is `dimensions/bingo/lobby`.
const LOBBY_LEVEL: &str = "bingo:lobby";
/// Where the lobby's spawn is, and how the players face there (the original's lobby).
const LOBBY_SPAWN: [f64; 3] = [0.5, 17.0, 1.5];
const LOBBY_ROTATION: [f32; 2] = [-180.0, 0.0];
/// Whether the lobby level is still to be opened: set when a server starts, cleared once the engine has the request.
static LOBBY_TO_OPEN: AtomicBool = AtomicBool::new(false);
/// Whether the lobby's files were looked at for this server's world (`prepare_lobby_files`). Cleared when a server starts.
static LOBBY_FILES_DONE: AtomicBool = AtomicBool::new(false);
/// The overworld the reset makes, and the staging place the players wait in there until its spawn is known: high above
/// the terrain, so the wait is spent in the air, not on the ground.
const OVERWORLD: &str = "minecraft:overworld";
const STAGING_POS: [f64; 3] = [0.5, 150.0, 0.5];
/// Votes typed in chat (`vote lobby` and the rest) during Postgame, with the player who typed each. The chat hook has no
/// host and a vote needs one, so they run on the next server tick, in order.
static CHAT_VOTES: Mutex<Vec<(u128, Command)>> = Mutex::new(Vec::new());
/// Console commands the session asked for (`ServerHost::command`), run after the session's tick.
static COMMANDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The lobby menus open now, by player and window (`bingo menu`). Only the mod's menu is in here.
static LOBBY_MENUS: Mutex<Menus> = Mutex::new(Menus::new());
/// The item registry's names, read once. `None` when the assets are missing (logged once).
static ITEM_IDS: OnceLock<Option<ItemIds>> = OnceLock::new();
/// The item names in the names file's order, read with `ITEM_IDS`; the API knows items only by key, so these map to keys.
static ITEM_NAMES: OnceLock<Vec<String>> = OnceLock::new();
/// The item keys by name, built once from `ITEM_NAMES` (see `item_key_names`).
static ITEM_KEY_NAMES: OnceLock<HashMap<ItemKey, String>> = OnceLock::new();
/// The advancements the mod has seen a player finish, by player and id, from `advancement_done`.
static ADVANCEMENTS_DONE: Mutex<BTreeSet<(u128, String)>> = Mutex::new(BTreeSet::new());
/// Chat lines during team chat, `(player, line)`, queued by the chat hook and sent on the next tick (the hook has no host).
static TEAM_CHAT: Mutex<Vec<(u128, String)>> = Mutex::new(Vec::new());
/// The sidebar as last shown on the server (`sidebar::Shown`).
static SIDEBAR: Mutex<Shown> = Mutex::new(Shown::new());

/// A session that starts from the clock's time, with the default game options.
fn new_session() -> Session {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut session = Session::new(SessionOptions {
        seed,
        game: GameOptions::default(),
        ready: ReadyOptions::default(),
        // Teams start `rules.spawn_distance` chunks apart (the original's default is 25) within 1000 blocks of the origin.
        spread: Some((0, 1000)),
        ..SessionOptions::default()
    });
    if let Some(pool) = CARD_POOL.get() {
        session.set_pool(pool.clone());
    }
    if let Some((dir, overlays)) = MAP_ART.get() {
        let icons: Box<dyn IconSource + Send> = Box::new(FileIcons {
            dir: dir.join("item_icons"),
        });
        session.set_map_assets(icons, overlays.clone());
    }
    session
}

/// The card pool with the item tags of the assets folder, read once on the first tick (see [`load_card_pool`]).
static CARD_POOL: OnceLock<horizon_bingo::server::card_gen::Pool> = OnceLock::new();

/// Reads the item tags beside the item names and installs the pool in the session, once per process. Tags missing from
/// the folder are logged: the pool then leaves those tags out.
fn load_card_pool(host: &dyn mod_api::Host) {
    CARD_POOL.get_or_init(|| {
        let dir = assets_dir(host);
        let pool = horizon_bingo::server::pool::pool_from_assets(&dir);
        if pool.skipped_tags.is_empty() {
            log(&format!("bingo: card pool: {} vanilla item tag files read from {}", pool.tag_files, dir.display()));
        } else {
            log(&format!(
                "bingo: card pool: {} vanilla item tag files read from {}; tags not found, left out: {}. \
                 tools/prepare_assets.py writes them from the game's data.",
                pool.tag_files,
                dir.display(),
                pool.skipped_tags.join(", ")
            ));
        }
        pool
    });
    if let Some(pool) = CARD_POOL.get() {
        with_session(|s| s.set_pool(pool.clone()));
    }
}

/// Runs `f` with the session, made on first use.
fn with_session<T>(f: impl FnOnce(&mut Session) -> T) -> T {
    let mut guard = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(new_session))
}

/// A new server (a world opens): the old game and its players belong to the previous world.
pub fn server_about_to_start() {
    *SESSION.lock().unwrap_or_else(PoisonError::into_inner) = None;
    // The new world may carry the old sidebar objective in its save: forget what was shown, so the first update sweeps it.
    *SIDEBAR.lock().unwrap_or_else(PoisonError::into_inner) = Shown::new();
    super::lobby_scene::server_about_to_start();
    LOBBY_TO_OPEN.store(true, Ordering::Relaxed);
    LOBBY_FILES_DONE.store(false, Ordering::Relaxed);
    CHAT_VOTES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    LOBBY_MENUS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    TEAM_CHAT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    ADVANCEMENTS_DONE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    *SIDEBAR.lock().unwrap_or_else(PoisonError::into_inner) = Shown::new();
}

/// A player finished an advancement (the mod's `player_advancement_done` hook calls this). Kept for `has_advancement`.
pub fn advancement_done(player: u128, id: &str) {
    ADVANCEMENTS_DONE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((player, id.to_owned()));
}

/// Sends a chat line to one online player. An offline or unknown uuid gets nothing.
fn tell_player(host: &mut dyn mod_api::ServerHost, uuid: u128, text: Text) {
    if let Some(player) = host.player(uuid) {
        let _ = player.tell(host, text);
    }
}

/// The name of an online player, or `None` when no such player is online.
fn online_name(host: &dyn mod_api::ServerHost, uuid: u128) -> Option<String> {
    host.players()
        .server_players()
        .into_iter()
        .find(|p| p.uuid() == uuid)
        .map(|p| p.name(host).unwrap_or_default())
}

/// Whether a player has the operator level the mod's operator commands need.
// PORT-GAP: the API has no permission nodes, so the node `bingo.op` is read as operator level 2 (a permissions plugin
// can no longer grant it on its own).
fn is_op(host: &dyn mod_api::Host, uuid: u128) -> bool {
    host.player(uuid)
        .and_then(|p| p.permission_level(host))
        .is_some_and(|level| level >= OP_LEVEL)
}

/// Horizon's `ServerHost` (the mod API) as the crate's `ServerHost`.
struct ServerAdapter<'a> {
    host: &'a mut dyn mod_api::ServerHost,
}

impl ServerHost for ServerAdapter<'_> {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        let channel = format!("{CHANNEL_PREFIX}{path}");
        // The channel is declared once at registration (`Hooks::register`, `payload_handlers`), so the id must match it.
        let sent = match self.host.player(player) {
            Some(handle) => handle
                .send_payload(
                    &mut *self.host,
                    &PayloadId(channel.clone()),
                    PayloadBytes(payload),
                )
                .is_ok(),
            None => false,
        };
        if !sent {
            log(&format!("bingo: {channel} to {player:032x} not sent"));
        }
    }

    fn tell(&mut self, player: u128, line: &str) {
        tell_player(&mut *self.host, player, Text::literal(line));
    }

    // PORT-GAP: the API's `Text` has no colour or click, so a styled line goes out as a queued `tellraw` command.
    fn tell_spans(&mut self, player: u128, spans: &[texts::Span]) {
        match self.players().into_iter().find(|p| p.uuid == player) {
            Some(found) => self.command(&format!("tellraw {} {}", found.name, texts::spans_json(spans))),
            None => {}
        }
    }

    fn broadcast(&mut self, line: &str) {
        let _ = self.host.messages().broadcast(Text::literal(line));
    }

    fn tell_line(&mut self, player: u128, line: &Line) {
        tell_player(&mut *self.host, player, translated(line));
    }

    fn broadcast_line(&mut self, line: &Line) {
        let _ = self.host.messages().broadcast(translated(line));
    }

    fn players(&self) -> Vec<Player> {
        let host: &dyn mod_api::ServerHost = &*self.host;
        host.players()
            .server_players()
            .into_iter()
            .map(|p| Player {
                uuid: p.uuid(),
                name: p.name(host).unwrap_or_default(),
            })
            .collect()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        // The number is the line of the item in the names file (the registry order), as before; the API has no numeric
        // item id to read.
        item_ids(&*self.host).as_ref()?.id(item)
    }

    fn command(&mut self, line: &str) {
        let mut queue = COMMANDS.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.len() < COMMAND_QUEUE_MAX {
            queue.push(line.to_owned());
        } else {
            log(&format!(
                "bingo: console command `{line}` dropped, the queue is full"
            ));
        }
    }

    fn set_game_rule(&mut self, rule: &str, value: bool) {
        // The overworld's own setter (`GameRules.set`). The `gamerule` command refuses a rule that already has the
        // value, so it failed for the defaults (`pvp true`, `keep_inventory false`); the setter does not. Without an
        // open overworld, or for a rule it refuses, the command is queued as before.
        let overworld = (&*self.host).level(OVERWORLD);
        let set = overworld.is_some_and(|level| {
            level.set_game_rule(&mut *self.host, rule, mod_api::GameRuleValue::Bool(value))
        });
        if !set {
            self.command(&format!("gamerule {rule} {value}"));
        }
    }

    fn world_port(&mut self) -> Option<&mut dyn WorldPort> {
        Some(self)
    }

    fn stat_value(&self, player: u128, stat_type: &str, stat_name: Option<&str>) -> Option<i64> {
        // A player's counter of one statistic (`minecraft:mined`, `minecraft:stone`); without a name there is none.
        let key = stat_name?;
        let kind = stat_kind(stat_type)?;
        self.host
            .player(player)?
            .stat(&*self.host, kind, key)
            .map(i64::from)
    }

    fn has_advancement(&self, player: u128, id: &str) -> bool {
        // PORT-GAP: the API has no per-player progress read. Only advancements finished while the mod runs are known
        // (`advancement_done`, fed by the `player_advancement_done` hook); earlier ones read as not done.
        ADVANCEMENTS_DONE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&(player, id.to_owned()))
    }

    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn read_file(&mut self, name: &str) -> Option<String> {
        let path = ConfigPath::new(name).ok()?;
        let bytes = self.host.world_data_files().read(&path).ok()?;
        String::from_utf8(bytes).ok()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        let result = ConfigPath::new(name).and_then(|path| {
            self.host
                .world_data_files()
                .write(&path, content.as_bytes())
        });
        if let Err(e) = result {
            log(&format!("bingo: {name} not written: {e:?}"));
        }
    }

    fn warn(&mut self, line: &str) {
        warn_line(line);
    }

    fn append_line(&mut self, name: &str, line: &str) {
        // PORT-GAP: the API has no append. The file is read, the line added and the whole file written back, so each
        // line costs the file's size (fine for the mod's small logs).
        let result = ConfigPath::new(name).and_then(|path| {
            let files = self.host.world_data_files();
            let mut bytes = match files.read(&path) {
                Ok(bytes) => bytes,
                Err(ConfigFileError::NotFound) => Vec::new(),
                Err(e) => return Err(e),
            };
            bytes.extend_from_slice(line.as_bytes());
            bytes.push(b'\n');
            files.write(&path, &bytes)
        });
        if let Err(e) = result {
            log(&format!("bingo: {name} not appended: {e:?}"));
        }
    }

    fn boss_bar(&mut self, player: u128, id: &str, title: &str, progress: f32, visible: bool) {
        // Bars are saved custom boss events keyed by id, each with its own viewers, so the player is added to or removed
        // from the bar. The bar's maximum is 1000, so the progress keeps a tenth of a percent.
        let Some(member) = self.host.player(player) else {
            log(&format!("bingo: boss bar {id} for {player:032x} not shown"));
            return;
        };
        if !visible {
            if let Some(bar) = self.host.boss_bar(id) {
                let _ = bar.remove_player(&mut *self.host, member);
            }
            return;
        }
        let bar = match self.host.boss_bar(id) {
            Some(bar) => bar,
            None => match self.host.create_boss_bar(id, Text::literal(title)) {
                Ok(Some(bar)) => {
                    let _ = bar.set_max(&mut *self.host, 1000);
                    bar
                }
                _ => {
                    log(&format!("bingo: boss bar {id} not made"));
                    return;
                }
            },
        };
        let _ = bar.set_name(&mut *self.host, Text::literal(title));
        let _ = bar.set_value(
            &mut *self.host,
            (progress.clamp(0.0, 1.0) * 1000.0).round() as i32,
        );
        let _ = bar.add_player(&mut *self.host, member);
    }

    fn title(&mut self, player: u128, title: &Line, subtitle: &Line) {
        // PORT-GAP: the API has no title or subtitle call. The title goes to the action bar (over the hotbar) and the
        // subtitle to chat, so a title moment shows as those lines.
        if let Some(handle) = self.host.player(player) {
            let _ = handle.action_bar(&mut *self.host, translated(title));
        } else {
            log(&format!("bingo: title for {player:032x} not shown"));
            return;
        }
        if !subtitle.render().is_empty() {
            tell_player(&mut *self.host, player, translated(subtitle));
        }
    }

    fn show_title(&mut self, player: u128, title: &Line, subtitle: &Line, color: Option<&str>) {
        // The title is the `/title` command (the API has no title call), run with the tick's other console commands.
        let Some(name) = online_name(&*self.host, player) else {
            log(&format!("bingo: title for {player:032x} not shown"));
            return;
        };
        for line in lobby_scene::title_commands(&name, title, subtitle, color) {
            self.command(&line);
        }
    }

    fn new_map(&mut self, _pixels: &[u8]) -> Option<i32> {
        // PORT-GAP: the API has no call that makes a map id (`new_map` is gone). Without one, the mod's map pictures
        // (the card and score maps) are not made; cards and the rest work without them.
        None
    }

    fn paint_map(&mut self, _map_id: i32, _pixels: &[u8], _dirty: DirtyRect) -> bool {
        // PORT-GAP: no map id is ever made (see `new_map`), so there is no map to repaint. `level.set_map_data` only
        // paints saved map data that vanilla made.
        false
    }

    fn send_map(&mut self, player: u128, map_id: i32, pixels: &[u8]) -> bool {
        // The server keeps no map picture: the API's `send_map` is sent again after each join by the session.
        match self.host.player(player) {
            Some(handle) => handle.send_map(&mut *self.host, map_id, pixels),
            None => false,
        }
    }

    fn give_map(&mut self, player: u128, snbt: &str) -> bool {
        let Some(stack) = self.host.item_from_snbt(snbt) else {
            log("bingo: a map item's SNBT was not read");
            return false;
        };
        // An item that does not fit is thrown from the player's eyes, so the give itself succeeds whenever the player is
        // here (before, the leftover was returned and had to be checked).
        match self.host.player(player) {
            Some(handle) => handle.give_stack(&mut *self.host, stack).is_ok(),
            None => false,
        }
    }

    fn is_operator(&self, player: u128) -> bool {
        is_op(&*self.host, player)
    }

    fn give_item(&mut self, player: u128, item: &str, components: &str) -> bool {
        // Best effort: the item goes out as a `give` command, run at the next tick, so the pages are the command's
        // components. Only a player who is online can be given it.
        let Some(name) = online_name(&*self.host, player) else {
            return false;
        };
        self.command(&format!("give {name} {item}[{components}]"));
        true
    }

    fn held_tagged(&self, player: u128, tag: &str) -> Option<usize> {
        // Read from the inventory itself (the API has no command feedback to count with).
        let slots = self.host.player(player)?.inventory_slots(&*self.host)?;
        Some(slots.iter().filter(|(_, stack)| carries_tag(stack, tag)).count())
    }

    fn replace_held_tagged(&mut self, player: u128, tag: &str, snbt: &str) -> bool {
        let Some(handle) = self.host.player(player) else {
            return false;
        };
        let Some(slots) = handle.inventory_slots(&*self.host) else {
            return false;
        };
        let held: Vec<u32> = slots
            .iter()
            .filter(|(_, stack)| carries_tag(stack, tag))
            .map(|(slot, _)| *slot)
            .collect();
        let Some((&first, others)) = held.split_first() else {
            return false;
        };
        let Some(stack) = self.host.item_from_snbt(snbt) else {
            log("bingo: a tutorial book's SNBT was not read");
            return false;
        };
        // The first held stack takes the new book; the other copies are emptied, so one is left.
        let mut written = handle.set_inventory_slot(&mut *self.host, first, Some(stack)).is_ok();
        for slot in others {
            written &= handle.set_inventory_slot(&mut *self.host, *slot, None).is_ok();
        }
        written
    }

    fn item_name_key(&self, item: &str) -> String {
        // A block's item names the block (`block.minecraft.copper_torch`), as vanilla's BlockItem does.
        let is_block = self.host.block_key(item).is_some();
        let kind = if is_block { "block" } else { "item" };
        format!("{kind}.{}", item.replacen(':', ".", 1))
    }
}

/// The lobby flow's reads and moves through the mod API: the lobby level, the vanilla reset and the players.
impl WorldPort for ServerAdapter<'_> {
    fn lobby_is_open(&mut self) -> bool {
        self.host.runtime_levels().is_open(LOBBY_LEVEL)
    }

    fn vanilla_players(&self) -> Vec<u128> {
        let host: &dyn mod_api::ServerHost = &*self.host;
        host.players()
            .server_players()
            .into_iter()
            .filter(|player| {
                player
                    .dimension(host)
                    .is_some_and(|dimension| world_reset::is_vanilla_dimension(&dimension.to_string()))
            })
            .map(|player| player.uuid())
            .collect()
    }

    fn reset_vanilla(&mut self, seed: u64) -> Result<(), ResetRefusal> {
        self.host
            .runtime_levels()
            .reset_vanilla(seed as i64)
            .map_err(|error| match error {
                LevelResetError::NotPermitted => ResetRefusal::NotPermitted,
                LevelResetError::PlayersInside => ResetRefusal::PlayersInside,
                LevelResetError::NoHoldingLevel => ResetRefusal::NoHoldingLevel,
                LevelResetError::Busy => ResetRefusal::Busy,
                // Unavailable, and any refusal a later engine adds: the start gives up.
                _ => ResetRefusal::Unavailable,
            })
    }

    fn teleport_all_to_lobby(&mut self) -> bool {
        teleport_all(&mut *self.host, LOBBY_LEVEL, LOBBY_SPAWN, Some(LOBBY_ROTATION))
    }

    fn teleport_to_lobby(&mut self, player: u128) -> bool {
        teleport_one(&mut *self.host, player, LOBBY_LEVEL, LOBBY_SPAWN, Some(LOBBY_ROTATION))
    }

    fn teleport_all_to_staging(&mut self) -> bool {
        teleport_all(&mut *self.host, OVERWORLD, STAGING_POS, None)
    }

    fn overworld_spawn_known(&self) -> bool {
        overworld_spawn(&*self.host).is_some()
    }

    fn teleport_all_to_overworld_spawn(&mut self) -> bool {
        let Some((pos, rotation)) = overworld_spawn(&*self.host) else {
            return false;
        };
        teleport_all(&mut *self.host, OVERWORLD, pos, Some(rotation))
    }

    fn online_players(&self) -> Vec<u128> {
        let host: &dyn mod_api::ServerHost = &*self.host;
        host.players().server_players().into_iter().map(|player| player.uuid()).collect()
    }

    fn player_mode(&self, player: u128) -> Option<Mode> {
        let host: &dyn mod_api::ServerHost = &*self.host;
        let player = host.player(player)?;
        player.game_mode(host).map(mode_from_game_mode)
    }

    fn set_player_mode(&mut self, player: u128, mode: Mode) -> bool {
        let Some(player) = self.host.player(player) else {
            return false;
        };
        player.set_game_mode(&mut *self.host, game_mode_of(mode))
    }

    fn player_away_from_overworld_spawn(&self, blocks: f64) -> bool {
        let host: &dyn mod_api::ServerHost = &*self.host;
        let Some((spawn, _)) = overworld_spawn(host) else {
            return false;
        };
        host.players().server_players().into_iter().any(|player| {
            let in_overworld = player.dimension(host).is_some_and(|dimension| dimension.to_string() == OVERWORLD);
            let far = player.entity().position(host).is_some_and(|pos| distance(pos, spawn) > blocks);
            !in_overworld || far
        })
    }
}

/// The distance between two points, in blocks.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The flow's mode as the engine's game mode.
fn game_mode_of(mode: Mode) -> GameMode {
    match mode {
        Mode::Survival => GameMode::Survival,
        Mode::Creative => GameMode::Creative,
        Mode::Adventure => GameMode::Adventure,
        Mode::Spectator => GameMode::Spectator,
    }
}

/// The engine's game mode as the flow's mode.
fn mode_from_game_mode(mode: GameMode) -> Mode {
    match mode {
        GameMode::Survival => Mode::Survival,
        GameMode::Creative => Mode::Creative,
        GameMode::Adventure => Mode::Adventure,
        GameMode::Spectator => Mode::Spectator,
    }
}

/// The new overworld's spawn as the players are sent to it: the centre of the spawn block, and the way they face. None
/// while the engine has not opened the overworld or has not chosen its spawn. The engine opens the level when a player
/// first enters it, and computes the spawn then, so the staging place is what gets it going.
fn overworld_spawn(host: &dyn mod_api::ServerHost) -> Option<([f64; 3], [f32; 2])> {
    let spawn = host.level(OVERWORLD)?.respawn_data(host)?;
    let pos = [f64::from(spawn.pos[0]) + 0.5, f64::from(spawn.pos[1]), f64::from(spawn.pos[2]) + 0.5];
    Some((pos, [spawn.yaw, spawn.pitch]))
}

/// Sends every online player to `dimension` at `pos`, facing `rotation` when given. A move the engine refuses is logged.
/// True only when every player was moved: a refused move is tried again by the caller.
fn teleport_all(host: &mut dyn mod_api::ServerHost, dimension: &str, pos: [f64; 3], rotation: Option<[f32; 2]>) -> bool {
    let players = host.players().server_players();
    let mut all_moved = true;
    for player in players {
        if !move_player(host, player, dimension, pos, rotation) {
            log(&format!("bingo: a player was not sent to {dimension}"));
            all_moved = false;
        }
    }
    all_moved
}

/// Sends one online player to `dimension` at `pos`, facing `rotation` when given. False when they are not online or the
/// engine refuses the move.
fn teleport_one(host: &mut dyn mod_api::ServerHost, uuid: u128, dimension: &str, pos: [f64; 3], rotation: Option<[f32; 2]>) -> bool {
    let Some(player) = host.player(uuid) else {
        return false;
    };
    move_player(host, player, dimension, pos, rotation)
}

/// Moves one player to `dimension`. A dead player (on the death screen) cannot be moved, so they are respawned first, as
/// their Respawn button does; the respawn is refused while a player's own hook runs, and then the move is not made here
/// and the caller tries again on a later tick. False when the respawn or the move is refused.
fn move_player(
    host: &mut dyn mod_api::ServerHost,
    player: mod_api::PlayerRef,
    dimension: &str,
    pos: [f64; 3],
    rotation: Option<[f32; 2]>,
) -> bool {
    let alive = mod_api::LivingRef::new(player.entity()).alive(&*host).unwrap_or(true);
    if !alive && player.respawn(&mut *host).is_err() {
        return false;
    }
    player.entity().teleport_to_level(&mut *host, dimension, pos, rotation)
}

/// A line for the operator: the `log` crate's and the debug file's (`BINGO_H0_LOG`, see `log`).
fn warn_line(line: &str) {
    ::log::warn!("{line}");
    log(line);
}

/// Makes the lobby's files ready in the world, once per server start and before the lobby opens: the lobby's dimension
/// (`dimensions/bingo/lobby`) and the datapack (`datapacks/bingo`) are copied from the assets when the world does not
/// have them (`lobby_world::install_into_world`). Nothing that exists is overwritten. Returns false while the world is
/// not open yet, so the next tick asks again.
fn prepare_lobby_files(host: &dyn mod_api::ServerHost) -> bool {
    let Some(world) = host.world_dir() else {
        return false;
    };
    let template = assets_dir(host).join("lobby_world");
    if !template.is_dir() {
        warn_line(&format!(
            "bingo: no lobby world in {}: the lobby will be empty",
            template.display()
        ));
        return true;
    }
    match lobby_world::install_into_world(&template, &world) {
        Ok(parts) => {
            log(&format!(
                "bingo: lobby world in {}: dimension {:?}, datapack {:?}",
                world.display(),
                parts.dimension,
                parts.datapack
            ));
            if parts.datapack == lobby_world::Part::Copied {
                log("bingo: the datapack bingo was copied into the world; it applies after the next server restart");
            }
        }
        Err(error) => warn_line(&format!("bingo: the lobby world was not copied into {}: {error}", world.display())),
    }
    true
}

/// Asks the engine for the lobby level once per server start. A request the engine refuses for now (no world yet) is
/// asked again on the next tick; `AlreadyOpen` means the level is there. The lobby's files come first.
fn open_lobby(host: &mut dyn mod_api::ServerHost) {
    if !LOBBY_TO_OPEN.load(Ordering::Relaxed) {
        return;
    }
    if !LOBBY_FILES_DONE.load(Ordering::Relaxed) && prepare_lobby_files(&*host) {
        LOBBY_FILES_DONE.store(true, Ordering::Relaxed);
    }
    let level = RuntimeLevel::new("minecraft:overworld", RuntimeGenerator::Void, 0);
    match host.runtime_levels().open(LOBBY_LEVEL, level) {
        Err(RuntimeLevelError::Unavailable) => {}
        Ok(_) | Err(RuntimeLevelError::AlreadyOpen) => LOBBY_TO_OPEN.store(false, Ordering::Relaxed),
        Err(error) => {
            LOBBY_TO_OPEN.store(false, Ordering::Relaxed);
            log(&format!("bingo: the lobby level was not opened: {error:?}"));
        }
    }
}

/// The engine's `levels_reset` hook: the vanilla worlds are new, so the lobby flow's start can finish.
pub fn levels_reset(host: &mut dyn mod_api::ServerHost) {
    guarded("levels reset", || {
        with_session(|s| s.levels_reset(&mut ServerAdapter { host: &mut *host }))
    });
}

/// Whether a stack carries custom data with the key `tag` (the mod's own item tags, such as the tutorial book's).
fn carries_tag(stack: &ItemStack, tag: &str) -> bool {
    let Some(components) = stack.components() else {
        return false;
    };
    components.as_nbt().iter().any(|(key, value)| {
        key.to_string_lossy() == "minecraft:custom_data"
            && matches!(value, Tag::Compound(data) if data.keys().any(|key| key.to_string_lossy() == tag))
    })
}

/// The statistic kind of a vanilla statistic name (`minecraft:mined`), or `None` for a name the API has no kind for.
fn stat_kind(name: &str) -> Option<StatType> {
    let path = name.strip_prefix("minecraft:")?;
    [
        StatType::Mined,
        StatType::Crafted,
        StatType::Used,
        StatType::Broken,
        StatType::PickedUp,
        StatType::Dropped,
        StatType::Killed,
        StatType::KilledBy,
        StatType::Custom,
    ]
    .into_iter()
    .find(|kind| kind.path() == path)
}

/// The pictures the maps are drawn with, read once: the item pictures folder and the map overlays. The assets folder is
/// BINGO_ASSETS, else the mod's data folder, as the item names are read.
static MAP_ART: OnceLock<(PathBuf, Overlays)> = OnceLock::new();

/// The folder of the mod's assets: `BINGO_ASSETS`, else the mod's folder in the game (`horizon-mod-data/<id>/assets`).
// PORT-GAP: the API gives the mod's data folders as `ConfigFiles` (bytes, no path), and bingo's loaders read folders of
// files, so the folder is the game's instance folder plus the documented layout. Moving the assets elsewhere needs
// BINGO_ASSETS.
fn assets_dir(host: &dyn mod_api::Host) -> PathBuf {
    match std::env::var_os("BINGO_ASSETS") {
        Some(dir) => PathBuf::from(dir),
        None => host
            .instance_dir()
            .join("horizon-mod-data")
            .join(MOD_ID)
            .join("assets"),
    }
}

/// Reads the map pictures once, and installs them in the session. A missing overlay is drawn as the original's fallback
/// would not (see `map_render`); a damaged one is logged and left out.
fn load_map_art(host: &dyn mod_api::Host) {
    if MAP_ART.get().is_some() {
        return;
    }
    let dir = assets_dir(host);
    let overlays = Overlays::load(&dir.join("map")).unwrap_or_else(|e| {
        log(&format!("bingo: map overlays not loaded: {e}"));
        Overlays::default()
    });
    let _ = MAP_ART.set((dir, overlays));
    if let Some((dir, overlays)) = MAP_ART.get() {
        let icons: Box<dyn IconSource + Send> = Box::new(FileIcons {
            dir: dir.join("item_icons"),
        });
        with_session(|s| s.set_map_assets(icons, overlays.clone()));
    }
}

/// The line as a text the client translates: its key, with the arguments as plain text.
// PORT-GAP: `Text::translate_with_fallback` has no equivalent, so the English template is lost; a client without the
// key shows the raw key. The lang files must cover every language the mod ships.
fn translated(line: &Line) -> Text {
    let args: Vec<Text> = line
        .args
        .iter()
        .zip(line.arg_keys.iter())
        .map(|(arg, key)| match key {
            // An item's name: the client's translation of its key, in brackets as the original shows it.
            Some(key) => Text::literal("[")
                .append(Text::translate(key.as_str(), Vec::new()))
                .append(Text::literal("]")),
            None => Text::literal(arg.as_str()),
        })
        .collect();
    Text::translate(line.key, args)
}

/// The item names, loaded on the first use: BINGO_ASSETS, else the mod's data folder, as the client reads them.
fn item_ids(host: &dyn mod_api::Host) -> &'static Option<ItemIds> {
    ITEM_IDS.get_or_init(|| load_item_ids(host))
}

fn load_item_ids(host: &dyn mod_api::Host) -> Option<ItemIds> {
    let path = assets_dir(host).join("item_names.txt");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let names = text
                .lines()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect();
            let _ = ITEM_NAMES.set(names);
            Some(ItemIds::from_names(&text))
        }
        Err(e) => {
            log(&format!(
                "bingo: item ids not loaded from {}: {e}",
                path.display()
            ));
            // Always on, not only with BINGO_H0_LOG: an operator has to see that the cards have no icons.
            ::log::warn!("{}", missing_names_warning(&path, &e.to_string()));
            None
        }
    }
}

/// The item keys by name. A stack gives its item only as a key, so the names file's items are looked up once; an item
/// the game does not have is left out.
fn item_key_names(host: &dyn mod_api::Host) -> &'static HashMap<ItemKey, String> {
    let _ = item_ids(host);
    ITEM_KEY_NAMES.get_or_init(|| {
        let mut keys = HashMap::new();
        for name in ITEM_NAMES.get().map(Vec::as_slice).unwrap_or_default() {
            let Some(id) = Identifier::parse_namespaced(name) else {
                continue;
            };
            if let Some(key) = host.item_key(&id) {
                keys.insert(key, name.clone());
            }
        }
        keys
    })
}

/// The line for an operator's `status` or `start` while the item names are missing; `None` otherwise.
fn names_note(host: &dyn mod_api::Host, line: &str) -> Option<&'static str> {
    let word = line.split_whitespace().next()?.to_lowercase();
    (matches!(word.as_str(), "status" | "start") && item_ids(host).is_none())
        .then_some(NO_ICONS_NOTE)
}

/// The authoritative tick: the session advances once per server tick that starts.
pub fn server_tick(host: &mut dyn mod_api::ServerHost, phase: ServerTickPhase) {
    if !matches!(phase, ServerTickPhase::ServerTickStart { .. }) {
        return;
    }
    open_lobby(&mut *host);
    // The item names are read on the first tick, so a missing file is reported when the server starts.
    let _ = item_ids(&*host);
    // Once per process: the first tick installs the pool, later sessions take it in `new_session`.
    if CARD_POOL.get().is_none() {
        load_card_pool(&*host);
    }
    // The map pictures, once per process, like the card pool (a session made later takes them in `new_session`).
    load_map_art(&*host);
    guarded("server tick", || {
        with_session(|s| s.tick(&mut ServerAdapter { host: &mut *host }))
    });
    guarded("bingo chat votes", || run_chat_votes(&mut *host));
    guarded("bingo team chat", || run_team_chat(&mut *host));
    guarded("bingo sidebar", || update_sidebar(&mut *host));
    guarded("bingo lobby clicks", || super::lobby_scene::tick(&mut *host));
    // Outside the session's lock: a console command may run hooks that reach the session again.
    guarded("bingo console commands", || run_queued_commands(&mut *host));
}

/// Runs the chat votes queued since the last tick, in the order they were typed; each voter is told the answer, as a
/// typed `bingo vote` is.
fn run_chat_votes(host: &mut dyn mod_api::ServerHost) {
    let votes = std::mem::take(&mut *CHAT_VOTES.lock().unwrap_or_else(PoisonError::into_inner));
    for (uuid, command) in votes {
        let reply = guarded("bingo chat vote", || {
            with_session(|s| s.run(&mut ServerAdapter { host: &mut *host }, Some(uuid), command))
        });
        if let Some(reply) = reply {
            tell_player(host, uuid, Text::literal(reply));
        }
    }
}

/// A player's chat line, before anyone sees it. In Postgame a plain `vote lobby`, `vote next` or `vote keep` is that
/// player's vote: it is cancelled here, so nobody sees it, and runs on the next tick. While a game is Starting or
/// Playing (and team chat is on) every line is cancelled and sent to the player's team on the next tick
/// (`run_team_chat`). Any other line goes on unchanged.
pub fn player_chat(player: u128, message: &str) -> Flow {
    if let Some(command) = chat_shortcut(message) {
        if guarded("player chat", || with_session(|s| s.is_postgame())).unwrap_or(false) {
            guarded("player chat", || {
                let mut votes = CHAT_VOTES.lock().unwrap_or_else(PoisonError::into_inner);
                if votes.len() < COMMAND_QUEUE_MAX {
                    votes.push((player, command));
                } else {
                    log(&format!(
                        "bingo: chat vote from {player:032x} dropped, the queue is full"
                    ));
                }
            });
            return Flow::Cancel;
        }
    }
    if guarded("player chat", || with_session(|s| s.team_chat_on())).unwrap_or(false) {
        guarded("player chat", || {
            let mut lines = TEAM_CHAT.lock().unwrap_or_else(PoisonError::into_inner);
            if lines.len() < COMMAND_QUEUE_MAX {
                lines.push((player, message.to_owned()));
            } else {
                log(&format!(
                    "bingo: team chat from {player:032x} dropped, the queue is full"
                ));
            }
        });
        return Flow::Cancel;
    }
    Flow::Continue
}

/// Sends the team chat lines queued since the last tick: each to the recipients `chat.rs` picks. If the game has stopped
/// taking team chat since the line was typed, it goes to everyone, so no line is lost.
fn run_team_chat(host: &mut dyn mod_api::ServerHost) {
    let lines = std::mem::take(&mut *TEAM_CHAT.lock().unwrap_or_else(PoisonError::into_inner));
    for (uuid, message) in lines {
        let mut adapter = ServerAdapter { host: &mut *host };
        let players = adapter.players();
        if with_session(|s| s.team_chat_on()) {
            let routed = with_session(|s| s.team_chat_route(&players, uuid, &message));
            for recipient in routed.recipients {
                adapter.tell(recipient, &routed.line);
            }
        } else {
            let name = players
                .iter()
                .find(|p| p.uuid == uuid)
                .map_or("", |p| p.name.as_str());
            adapter.broadcast(&chat::global_line(name, &message));
        }
    }
}

/// Shows the sidebar as the session says it should be (`sidebar::Shown` sends only the changes).
fn update_sidebar(host: &mut dyn mod_api::ServerHost) {
    let players = ServerAdapter { host: &mut *host }.players();
    let rows = with_session(|s| s.sidebar_rows(&players));
    let mut shown = SIDEBAR.lock().unwrap_or_else(PoisonError::into_inner);
    shown.update(&mut ServerAdapter { host: &mut *host }, rows);
}

/// Runs the console commands queued since the last tick, in order; a failed one is logged.
fn run_queued_commands(host: &mut dyn mod_api::ServerHost) {
    let lines = std::mem::take(&mut *COMMANDS.lock().unwrap_or_else(PoisonError::into_inner));
    for line in lines {
        if !host.commands().run_command(&line) {
            log(&format!("bingo: console command `{line}` did not run"));
        }
    }
}

pub fn player_logged_in(host: &mut dyn mod_api::ServerHost, uuid: u128, name: &str) {
    let player = Player {
        uuid,
        name: name.to_owned(),
    };
    guarded("player logged in", || {
        with_session(|s| s.on_join(&mut ServerAdapter { host: &mut *host }, &player))
    });
}

pub fn player_logged_out(host: &mut dyn mod_api::ServerHost, uuid: u128) {
    guarded("player logged out", || {
        with_session(|s| s.on_leave(&mut ServerAdapter { host: &mut *host }, uuid))
    });
}

/// The stacks of a player's inventory, counted by item name (empty slots and stacks of nothing add nothing).
pub fn player_inventory_changed(
    host: &mut dyn mod_api::ServerHost,
    uuid: u128,
    stacks: &[Option<ItemStack>],
) {
    guarded("inventory changed", || {
        let names = item_key_names(&*host);
        let mut held: HashMap<String, u32> = HashMap::new();
        for stack in stacks.iter().flatten() {
            let Some(name) = names.get(&stack.item()) else {
                continue;
            };
            let count = held.entry(name.clone()).or_insert(0);
            *count = count.saturating_add(stack.count());
        }
        with_session(|s| s.on_inventory(&mut ServerAdapter { host: &mut *host }, uuid, &held));
    });
}

/// A plugin channel message: the mod's own channels go to the session, with the part after the namespace as the path.
pub fn custom_payload_from_client(
    host: &mut dyn mod_api::ServerHost,
    uuid: u128,
    channel: &str,
    data: &[u8],
) {
    let Some(path) = channel.strip_prefix(CHANNEL_PREFIX) else {
        return;
    };
    guarded("custom payload", || {
        with_session(|s| s.on_packet(&mut ServerAdapter { host: &mut *host }, uuid, path, data));
    });
}

/// The usages the commands tree gets: `bingo ...`, and the shortcuts. Operators get the ones `requires_op` names.
pub fn register_commands(commands: &mut Vec<mod_api::Command>) {
    // `g`, `global` and `all` send a line to everyone during a game (the original's global command aliases).
    for alias in chat::GLOBAL_COMMANDS {
        commands.push(mod_api::Command::new(&format!("{alias} <message:text>")));
    }
    for (usage, sample) in USAGES {
        let Ok(command) = parse(sample) else {
            log(&format!(
                "bingo: usage `{usage}` has no parsable sample `{sample}`"
            ));
            continue;
        };
        if requires_op(&command) {
            commands.push(mod_api::Command::op(usage, OP_LEVEL));
        } else {
            commands.push(mod_api::Command::new(usage));
        }
    }
}

/// The session's line for a typed command: the rest after `bingo`, or the whole line for a shortcut. `None` for any
/// other command, which is left to the game.
fn session_line(command: &str) -> Option<String> {
    if let Some(rest) = command
        .strip_prefix(COMMAND)
        .filter(|rest| rest.is_empty() || rest.starts_with(' '))
    {
        return Some(rest.trim().to_owned());
    }
    let word = command.split_whitespace().next()?;
    SHORTCUTS.contains(&word).then(|| command.trim().to_owned())
}

/// A `bingo ...` command, or a shortcut, typed on the dedicated server's console; the answer goes to the console.
pub fn perform_console_command(host: &mut dyn mod_api::ServerHost, command: &str) -> Flow {
    let Some(line) = session_line(command) else {
        return Flow::Continue;
    };
    let reply = guarded("bingo console command", || {
        with_session(|s| s.command(&mut ServerAdapter { host: &mut *host }, None, &line))
    });
    // An empty reply says nothing (a setting turned off that has no message, `prefs::changed_text`).
    if let Some(reply) = reply.filter(|reply| !reply.is_empty()) {
        let _ = host.commands().feedback(Text::literal(reply));
    }
    if let Some(note) = names_note(&*host, &line) {
        let _ = host.commands().feedback(Text::literal(note));
    }
    Flow::Cancel
}

/// A typed `bingo ...` command, or a shortcut, run by the session; its answer goes to the player who typed it.
pub fn perform_command(host: &mut dyn mod_api::ServerHost, uuid: u128, command: &str) -> Flow {
    if let Some(text) = chat::global_text(command) {
        let name = online_name(&*host, uuid).unwrap_or_default();
        let _ = host
            .messages()
            .broadcast(Text::literal(chat::global_line(&name, text)));
        return Flow::Cancel;
    }
    let Some(line) = session_line(command) else {
        return Flow::Continue;
    };
    // `bingo menu` opens a chest, which only the host can do: any player may open it.
    if line.eq_ignore_ascii_case("menu") {
        open_lobby_menu(host, uuid);
        return Flow::Cancel;
    }
    // Horizon checks a command's operator level on its first word only, and `bingo` also has commands for everyone,
    // so the commands that need an operator are checked here.
    if parse(&line).is_ok_and(|command| requires_op(&command)) && !is_op(&*host, uuid) {
        tell_player(
            host,
            uuid,
            Text::literal("You must be an operator to use this command.".to_string()),
        );
        return Flow::Cancel;
    }
    let reply = guarded("bingo command", || {
        with_session(|s| {
            let text = s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line);
            (text, s.take_reply())
        })
    });
    // An empty reply says nothing (a setting turned off that has no message, `prefs::changed_text`). A reply with a
    // translation key is sent as one, so the player's language applies; the English is the fallback.
    if let Some((reply, translatable)) = reply.filter(|(reply, _)| !reply.is_empty()) {
        match translatable {
            Some(line) => tell_player(host, uuid, translated(&line)),
            None => tell_player(host, uuid, Text::literal(reply)),
        }
    }
    if let Some(note) = names_note(&*host, &line) {
        tell_player(host, uuid, Text::literal(note));
    }
    Flow::Cancel
}

/// The lobby menu's rows and title, as the chest shows them.
const MENU_TITLE: &str = "Bingo";

/// The rules, whether the round is still pregame, and whether the player may run operator commands: what the menu shows.
fn menu_view(host: &dyn mod_api::Host, uuid: u128) -> (Options, bool, bool) {
    let (rules, pregame) = with_session(|s| (s.rules().clone(), s.is_pregame()));
    (rules, pregame, is_op(host, uuid))
}

/// The items of a menu page, as the chest holds them: the slots' items read from their SNBT.
fn menu_items(
    host: &dyn mod_api::Host,
    page: &Page,
    rules: &Options,
    pregame: bool,
    operator: bool,
) -> Vec<Option<ItemStack>> {
    let view = View {
        rules,
        pregame,
        operator,
    };
    menu::contents(page, &view)
        .into_iter()
        .map(|slot| slot.and_then(|slot| host.item_from_snbt(&menu::item_snbt(&slot))))
        .collect()
}

/// `bingo menu`: opens the lobby menu on its root page for the player.
pub(crate) fn open_lobby_menu(host: &mut dyn mod_api::ServerHost, uuid: u128) {
    open_lobby_page(host, uuid, Page::Root);
}

/// Opens the lobby menu on `page` for the player.
fn open_lobby_page(host: &mut dyn mod_api::ServerHost, uuid: u128, page: Page) {
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let items = menu_items(&*host, &page, &rules, pregame, operator);
    // `CloseFirst` is vanilla's default: a new menu closes the old screen first.
    match host
        .open_menu(
            uuid,
            menu::ROWS,
            Text::literal(MENU_TITLE),
            items,
            MenuReplace::CloseFirst,
        )
        .ok()
        .flatten()
    {
        Some(window) => LOBBY_MENUS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .open(uuid, window, page),
        None => tell_player(
            host,
            uuid,
            Text::literal("The menu could not be opened.".to_string()),
        ),
    }
}

/// A click on the lobby panel's start or reroll tile: what a click on that slot of the menu's root page does, with no menu
/// open. A refused start opens the menu on its warnings page, which offers the start anyway.
/// What the lobby panel's live tiles show now: the session's auto shuffle rule and team count.
pub(crate) fn panel_state() -> lobby_scene::PanelState {
    with_session(|s| lobby_scene::PanelState { auto_shuffle: s.rules().team_auto_shuffle, teams: s.rules().team_count })
}

pub(crate) fn panel_slot(host: &mut dyn mod_api::ServerHost, uuid: u128, slot: usize) {
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let view = View {
        rules: &rules,
        pregame,
        operator,
    };
    match menu::click(&Page::Root, slot, &view) {
        Outcome::Tell(text) => tell_player(host, uuid, Text::literal(text)),
        Outcome::Run(line) => {
            ServerAdapter { host: &mut *host }.play_sound(uuid, lobby_scene::SOUND_BUTTON);
            let reply = guarded("bingo panel command", || {
                with_session(|s| {
                    s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line)
                })
            })
            .unwrap_or_default();
            if menu::start_refused(&line, &reply) {
                open_lobby_page(host, uuid, Page::Warnings(reply));
            } else if !reply.is_empty() {
                tell_player(host, uuid, Text::literal(reply));
            }
        }
        _ => {}
    }
}

/// Shows `page` in the player's open lobby menu, refreshing its items. False if that window is not the lobby menu.
fn show_lobby_page(
    host: &mut dyn mod_api::ServerHost,
    uuid: u128,
    window: i32,
    page: Page,
) -> bool {
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let items = menu_items(&*host, &page, &rules, pregame, operator);
    if !LOBBY_MENUS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .turn(uuid, window, page)
    {
        return false;
    }
    let _ = host.menu_set_items(uuid, window, items);
    true
}

/// A click in a menu the mod opened (`native_open_menu`): the lobby menu's slots run their outcome. Only a click that
/// picks up or shift-clicks a slot of the chest counts; the player's inventory and drags do nothing.
pub fn menu_clicked(
    host: &mut dyn mod_api::ServerHost,
    uuid: u128,
    window: i32,
    slot: i32,
    mode: i32,
) {
    if !matches!(mode, 0 | 1) {
        return;
    }
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    if slot >= menu::SLOTS {
        return;
    }
    let Some(page) = LOBBY_MENUS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .page(uuid, window)
    else {
        return;
    };
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let view = View {
        rules: &rules,
        pregame,
        operator,
    };
    let outcome = menu::click(&page, slot, &view);
    if !matches!(outcome, Outcome::Nothing) {
        ServerAdapter { host: &mut *host }.play_sound(uuid, lobby_scene::SOUND_BUTTON);
    }
    match outcome {
        Outcome::Nothing => {}
        Outcome::Show(next) => {
            show_lobby_page(host, uuid, window, next);
        }
        Outcome::Close => {
            LOBBY_MENUS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .close(uuid, window);
            host.close_menu(uuid);
        }
        Outcome::Tell(text) => {
            tell_player(host, uuid, Text::literal(text));
        }
        Outcome::Run(line) => {
            let reply = guarded("bingo menu command", || {
                with_session(|s| {
                    s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line)
                })
            })
            .unwrap_or_default();
            if menu::start_refused(&line, &reply) {
                // The start was refused: its reason is the warnings page, which offers the start anyway.
                show_lobby_page(host, uuid, window, Page::Warnings(reply));
            } else {
                if !reply.is_empty() {
                    tell_player(host, uuid, Text::literal(reply));
                }
                let page = LOBBY_MENUS
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .page(uuid, window);
                if let Some(page) = page {
                    show_lobby_page(host, uuid, window, page);
                }
            }
        }
    }
}

/// The player's menu window closed: a lobby menu it was is forgotten.
pub fn menu_closed(uuid: u128, window: i32) {
    LOBBY_MENUS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .close(uuid, window);
}

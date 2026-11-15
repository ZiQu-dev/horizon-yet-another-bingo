//! The server half of "Yet Another Minecraft BINGO": one game session, driven by Horizon's server hooks. The game is
//! `horizon-bingo`'s `server` modules; this file adapts Horizon's `Host` to their `ServerHost` and keeps the session.
//! Each hook runs under `guarded`, and a poisoned lock is taken as it is, so one panic does not end the server's mod.
use super::{MOD_ID, guarded, log};
use horizon_bingo::server::chat;
use horizon_bingo::server::commands::{Command, chat_shortcut, parse, requires_op};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::items::{ItemIds, NO_ICONS_NOTE, missing_names_warning};
use horizon_bingo::server::map_render::{DirtyRect, FileIcons, IconSource, MAP_BYTES, Overlays};
use horizon_bingo::server::menu::{self, Menus, Outcome, Page, View};
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::options::Options;
use horizon_bingo::server::sidebar::Shown;
use horizon_bingo::server::texts::Line;
use horizon_bingo::server::types::Player;
use horizon_client::mod_api::{Flow, Host, ItemStack, NativeBossBar, NativeCommand, ServerTickPhase, Text};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock, PoisonError},
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
const USAGES: [(&str, &str); 49] = [
    ("bingo status", "status"),
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
    ("bingo mode hidden_items <state:word>", "mode hidden_items on"),
    ("bingo mode consume_items <state:word>", "mode consume_items on"),
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
    ("bingo difficulty <s:integer> <a:integer> <b:integer> <c:integer> <d:integer>", "difficulty 0 0 3 10 12"),
    ("bingo reroll", "reroll"),
    ("bingo teams <n:integer>", "teams 3"),
    ("bingo autoshuffle <state:word>", "autoshuffle on"),
    ("bingo stats <player:word>", "stats Alice"),
    ("bingo menu", "menu"),
    ("bingo kit", "kit"),
    ("bingo kit add <item:word>", "kit add diamond"),
    ("bingo kit add <item:word> <count:integer>", "kit add diamond 2"),
    ("bingo kit remove <item:word>", "kit remove diamond"),
    ("bingo kit clear", "kit clear"),
    ("join <team:word>", "join Red"),
    ("ready", "ready"),
    ("spectator", "spectator"),
];
/// The most console commands queued for the next server tick; more are dropped and logged.
const COMMAND_QUEUE_MAX: usize = 256;

/// The one session of this server. Made on the first server hook.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// Votes typed in chat (`vote lobby` and the rest) during Postgame, with the player who typed each. The chat hook has no
/// host and a vote needs one, so they run on the next server tick, in order.
static CHAT_VOTES: Mutex<Vec<(u128, Command)>> = Mutex::new(Vec::new());
/// Console commands the session asked for (`ServerHost::command`), run after the session's tick.
static COMMANDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The lobby menus open now, by player and window (`bingo menu`). Only the mod's menu is in here.
static LOBBY_MENUS: Mutex<Menus> = Mutex::new(Menus::new());
/// The item registry's names, read once. `None` when the assets are missing (logged once).
static ITEM_IDS: OnceLock<Option<ItemIds>> = OnceLock::new();
/// Chat lines during team chat, `(player, line)`, queued by the chat hook and sent on the next tick (the hook has no host).
static TEAM_CHAT: Mutex<Vec<(u128, String)>> = Mutex::new(Vec::new());
/// The sidebar as last shown on the server (`sidebar::Shown`).
static SIDEBAR: Mutex<Shown> = Mutex::new(Shown::new());

/// A session that starts from the clock's time, with the default game options.
fn new_session() -> Session {
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
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
        let icons: Box<dyn IconSource + Send> = Box::new(FileIcons { dir: dir.join("item_icons") });
        session.set_map_assets(icons, overlays.clone());
    }
    session
}

/// The card pool with the item tags of the assets folder, read once on the first tick (see [`load_card_pool`]).
static CARD_POOL: OnceLock<horizon_bingo::server::card_gen::Pool> = OnceLock::new();

/// Reads the item tags beside the item names and installs the pool in the session, once per process. Tags missing from
/// the folder are logged: the pool then leaves those tags out.
fn load_card_pool(host: &dyn Host) {
    CARD_POOL.get_or_init(|| {
        let dir = match std::env::var_os("BINGO_ASSETS") {
            Some(dir) => PathBuf::from(dir),
            None => host.data_dir(MOD_ID).join("assets"),
        };
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
    CHAT_VOTES.lock().unwrap_or_else(PoisonError::into_inner).clear();
    LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).clear();
    TEAM_CHAT.lock().unwrap_or_else(PoisonError::into_inner).clear();
    *SIDEBAR.lock().unwrap_or_else(PoisonError::into_inner) = Shown::new();
}

/// Horizon's `Host` as the crate's `ServerHost`.
struct ServerAdapter<'a> {
    host: &'a mut dyn Host,
}

impl ServerHost for ServerAdapter<'_> {
    fn send(&mut self, player: u128, path: &str, payload: Vec<u8>) {
        let channel = format!("{CHANNEL_PREFIX}{path}");
        if let Err(e) = self.host.send_custom_payload_to_player(player, &channel, &payload) {
            log(&format!("bingo: {channel} to {player:032x} not sent: {e}"));
        }
    }

    fn tell(&mut self, player: u128, line: &str) {
        let _ = self.host.tell(player, Text::literal(line));
    }

    fn broadcast(&mut self, line: &str) {
        self.host.broadcast(Text::literal(line));
    }

    fn tell_line(&mut self, player: u128, line: &Line) {
        let _ = self.host.tell(player, translated(line));
    }

    fn broadcast_line(&mut self, line: &Line) {
        self.host.broadcast(translated(line));
    }

    fn players(&self) -> Vec<Player> {
        self.host
            .server_players()
            .into_iter()
            .map(|p| Player { uuid: p.uuid, name: p.name })
            .collect()
    }

    fn item_numeric_id(&self, item: &str) -> Option<i32> {
        item_ids(&*self.host).as_ref()?.id(item)
    }

    fn command(&mut self, line: &str) {
        let mut queue = COMMANDS.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.len() < COMMAND_QUEUE_MAX {
            queue.push(line.to_owned());
        } else {
            log(&format!("bingo: console command `{line}` dropped, the queue is full"));
        }
    }

    fn stat_value(&self, player: u128, stat_type: &str, stat_name: Option<&str>) -> Option<i64> {
        // Horizon's `player_stat` reads one counter (`minecraft:mined`, `minecraft:stone`); without a name there is none.
        let key = stat_name?;
        self.host.player_stat(player, stat_type, key).map(i64::from)
    }

    fn has_advancement(&self, player: u128, id: &str) -> bool {
        // Horizon's `player_advancements` gives vanilla's progress JSON (`objectives::advancement_done_json` reads it).
        self.host
            .player_advancements(player)
            .is_some_and(|progress| horizon_bingo::server::objectives::advancement_done_json(&progress, id))
    }

    fn now_ms(&self) -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
    }

    fn read_file(&mut self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.host.data_dir(MOD_ID).join(name)).ok()
    }

    fn write_file(&mut self, name: &str, content: &str) {
        let dir = self.host.data_dir(MOD_ID);
        let result = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(name), content));
        if let Err(e) = result {
            log(&format!("bingo: {name} not written: {e}"));
        }
    }

    fn append_line(&mut self, name: &str, line: &str) {
        use std::io::Write;
        let dir = self.host.data_dir(MOD_ID);
        let result = std::fs::create_dir_all(&dir).and_then(|()| {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(name))?;
            writeln!(file, "{line}")
        });
        if let Err(e) = result {
            log(&format!("bingo: {name} not appended: {e}"));
        }
    }

    fn boss_bar(&mut self, player: u128, id: &str, title: &str, progress: f32, visible: bool) {
        let bar = visible.then(|| NativeBossBar {
            name: Text::literal(title),
            progress: progress.clamp(0.0, 1.0),
            ..NativeBossBar::default()
        });
        if !self.host.boss_bar(player, bar_uuid(id), bar.as_ref()) {
            log(&format!("bingo: boss bar {id} for {player:032x} not shown"));
        }
    }

    fn new_map(&mut self, pixels: &[u8]) -> Option<i32> {
        let colors: &[u8; MAP_BYTES] = pixels.try_into().ok()?;
        self.host.new_map(MAP_DIMENSION, colors)
    }

    fn paint_map(&mut self, map_id: i32, pixels: &[u8], _dirty: DirtyRect) -> bool {
        // Horizon's `paint_map` takes the whole picture and sends it to each carrier, so the dirty part is not used here.
        let Ok(colors) = <&[u8; MAP_BYTES]>::try_from(pixels) else { return false };
        self.host.paint_map(map_id, colors)
    }

    fn send_map(&mut self, player: u128, map_id: i32, pixels: &[u8]) -> bool {
        self.host.send_map(player, map_id, pixels)
    }

    fn give_map(&mut self, player: u128, snbt: &str) -> bool {
        let Some(stack) = self.host.item_from_snbt(snbt) else {
            log("bingo: a map item's SNBT was not read");
            return false;
        };
        // `give_stack` returns what did not fit, so none means the whole stack went to the player.
        self.host.give_stack(player, stack).is_none()
    }
}

/// The overworld: the dimension the maps are made in, as the original makes them.
const MAP_DIMENSION: &str = "minecraft:overworld";

/// The pictures the maps are drawn with, read once: the item pictures folder and the map overlays. The assets folder is
/// BINGO_ASSETS, else the mod's data folder, as the item names are read.
static MAP_ART: OnceLock<(PathBuf, Overlays)> = OnceLock::new();

fn assets_dir(host: &dyn Host) -> PathBuf {
    match std::env::var_os("BINGO_ASSETS") {
        Some(dir) => PathBuf::from(dir),
        None => host.data_dir(MOD_ID).join("assets"),
    }
}

/// Reads the map pictures once, and installs them in the session. A missing overlay is drawn as the original's fallback
/// would not (see `map_render`); a damaged one is logged and left out.
fn load_map_art(host: &dyn Host) {
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
        let icons: Box<dyn IconSource + Send> = Box::new(FileIcons { dir: dir.join("item_icons") });
        with_session(|s| s.set_map_assets(icons, overlays.clone()));
    }
}

/// The line as a translatable text: its key, with the English as the fallback, so a client without the key shows the
/// English. The arguments are plain text.
fn translated(line: &Line) -> Text {
    let args: Vec<Text> = line.args.iter().map(|arg| Text::literal(arg.as_str())).collect();
    Text::translate_with_fallback(line.key, line.template, &args)
}

/// The UUID of the boss bar called `id`: FNV-1a over its bytes, so each name always gives the same bar and no two
/// names of the mod's collide in practice.
fn bar_uuid(id: &str) -> u128 {
    const OFFSET_BASIS: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = (1 << 88) + 0x13b;
    id.bytes().fold(OFFSET_BASIS, |hash, byte| (hash ^ u128::from(byte)).wrapping_mul(PRIME))
}

/// The item names, loaded on the first use: BINGO_ASSETS, else the mod's data folder, as the client reads them.
fn item_ids(host: &dyn Host) -> &'static Option<ItemIds> {
    ITEM_IDS.get_or_init(|| load_item_ids(host))
}

fn load_item_ids(host: &dyn Host) -> Option<ItemIds> {
    let dir = match std::env::var_os("BINGO_ASSETS") {
        Some(dir) => PathBuf::from(dir),
        None => host.data_dir(MOD_ID).join("assets"),
    };
    let path = dir.join("item_names.txt");
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(ItemIds::from_names(&text)),
        Err(e) => {
            log(&format!("bingo: item ids not loaded from {}: {e}", path.display()));
            // Always on, not only with BINGO_H0_LOG: an operator has to see that the cards have no icons.
            horizon_client::logging::write_as(MOD_ID, ::log::Level::Warn, &missing_names_warning(&path, &e.to_string()));
            None
        }
    }
}

/// The line for an operator's `status` or `start` while the item names are missing; `None` otherwise.
fn names_note(host: &dyn Host, line: &str) -> Option<&'static str> {
    let word = line.split_whitespace().next()?.to_lowercase();
    (matches!(word.as_str(), "status" | "start") && item_ids(host).is_none()).then_some(NO_ICONS_NOTE)
}

/// The authoritative tick: the session advances once per server tick that starts.
pub fn server_tick(host: &mut dyn Host, phase: ServerTickPhase) {
    if !matches!(phase, ServerTickPhase::ServerTickStart { .. }) {
        return;
    }
    // The item names are read on the first tick, so a missing file is reported when the server starts.
    let _ = item_ids(&*host);
    // Once per process: the first tick installs the pool, later sessions take it in `new_session`.
    if CARD_POOL.get().is_none() {
        load_card_pool(&*host);
    }
    // The map pictures, once per process, like the card pool (a session made later takes them in `new_session`).
    load_map_art(&*host);
    guarded("server tick",|| with_session(|s| s.tick(&mut ServerAdapter { host: &mut *host })));
    guarded("bingo chat votes", || run_chat_votes(&mut *host));
    guarded("bingo team chat", || run_team_chat(&mut *host));
    guarded("bingo sidebar", || update_sidebar(&mut *host));
    // Outside the session's lock: a console command may run hooks that reach the session again.
    guarded("bingo console commands", || run_queued_commands(&mut *host));
}

/// Runs the chat votes queued since the last tick, in the order they were typed; each voter is told the answer, as a
/// typed `bingo vote` is.
fn run_chat_votes(host: &mut dyn Host) {
    let votes = std::mem::take(&mut *CHAT_VOTES.lock().unwrap_or_else(PoisonError::into_inner));
    for (uuid, command) in votes {
        let reply = guarded("bingo chat vote", || {
            with_session(|s| s.run(&mut ServerAdapter { host: &mut *host }, Some(uuid), command))
        });
        if let Some(reply) = reply {
            let _ = host.tell(uuid, Text::literal(reply));
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
                    log(&format!("bingo: chat vote from {player:032x} dropped, the queue is full"));
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
                log(&format!("bingo: team chat from {player:032x} dropped, the queue is full"));
            }
        });
        return Flow::Cancel;
    }
    Flow::Continue
}

/// Sends the team chat lines queued since the last tick: each to the recipients `chat.rs` picks. If the game has stopped
/// taking team chat since the line was typed, it goes to everyone, so no line is lost.
fn run_team_chat(host: &mut dyn Host) {
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
            let name = players.iter().find(|p| p.uuid == uuid).map_or("", |p| p.name.as_str());
            adapter.broadcast(&chat::global_line(name, &message));
        }
    }
}

/// Shows the sidebar as the session says it should be (`sidebar::Shown` sends only the changes).
fn update_sidebar(host: &mut dyn Host) {
    let rows = with_session(|s| s.sidebar_rows());
    let mut shown = SIDEBAR.lock().unwrap_or_else(PoisonError::into_inner);
    shown.update(&mut ServerAdapter { host: &mut *host }, rows);
}

/// Runs the console commands queued since the last tick, in order; a failed one is logged.
fn run_queued_commands(host: &mut dyn Host) {
    let lines = std::mem::take(&mut *COMMANDS.lock().unwrap_or_else(PoisonError::into_inner));
    for line in lines {
        if !host.run_command(&line) {
            log(&format!("bingo: console command `{line}` did not run"));
        }
    }
}

pub fn player_logged_in(host: &mut dyn Host, uuid: u128, name: &str) {
    let player = Player { uuid, name: name.to_owned() };
    guarded("player logged in", || {
        with_session(|s| s.on_join(&mut ServerAdapter { host: &mut *host }, &player))
    });
}

pub fn player_logged_out(host: &mut dyn Host, uuid: u128) {
    guarded("player logged out", || with_session(|s| s.on_leave(&mut ServerAdapter { host: &mut *host }, uuid)));
}

/// The stacks of a player's inventory, counted by item id (empty slots and stacks of nothing add nothing).
pub fn player_inventory_changed(host: &mut dyn Host, uuid: u128, stacks: &[Option<ItemStack>]) {
    guarded("inventory changed", || {
        let mut held: HashMap<String, u32> = HashMap::new();
        for stack in stacks.iter().flatten() {
            let count = held.entry(stack.item.to_string()).or_insert(0);
            *count = count.saturating_add(stack.count);
        }
        with_session(|s| s.on_inventory(&mut ServerAdapter { host: &mut *host }, uuid, &held));
    });
}

/// A plugin channel message: the mod's own channels go to the session, with the part after the namespace as the path.
pub fn custom_payload_from_client(host: &mut dyn Host, uuid: u128, channel: &str, data: &[u8]) {
    let Some(path) = channel.strip_prefix(CHANNEL_PREFIX) else { return };
    guarded("custom payload", || {
        with_session(|s| s.on_packet(&mut ServerAdapter { host: &mut *host }, uuid, path, data));
    });
}

/// The usages the commands tree gets: `bingo ...`, and the shortcuts. Operators get the ones `requires_op` names.
pub fn register_commands(commands: &mut Vec<NativeCommand>) {
    // `g`, `global` and `all` send a line to everyone during a game (the original's global command aliases).
    for alias in chat::GLOBAL_COMMANDS {
        commands.push(NativeCommand::new(&format!("{alias} <message:text>")));
    }
    for (usage, sample) in USAGES {
        let Ok(command) = parse(sample) else {
            log(&format!("bingo: usage `{usage}` has no parsable sample `{sample}`"));
            continue;
        };
        if requires_op(&command) {
            commands.push(NativeCommand::op(usage, OP_LEVEL));
        } else {
            commands.push(NativeCommand::new(usage));
        }
    }
}

/// The session's line for a typed command: the rest after `bingo`, or the whole line for a shortcut. `None` for any
/// other command, which is left to the game.
fn session_line(command: &str) -> Option<String> {
    if let Some(rest) = command.strip_prefix(COMMAND).filter(|rest| rest.is_empty() || rest.starts_with(' ')) {
        return Some(rest.trim().to_owned());
    }
    let word = command.split_whitespace().next()?;
    SHORTCUTS.contains(&word).then(|| command.trim().to_owned())
}

/// A `bingo ...` command, or a shortcut, typed on the dedicated server's console; the answer goes to the console.
pub fn perform_console_command(host: &mut dyn Host, command: &str) -> Flow {
    let Some(line) = session_line(command) else {
        return Flow::Continue;
    };
    let reply = guarded("bingo console command", || {
        with_session(|s| s.command(&mut ServerAdapter { host: &mut *host }, None, &line))
    });
    if let Some(reply) = reply {
        host.feedback(Text::literal(reply));
    }
    if let Some(note) = names_note(&*host, &line) {
        host.feedback(Text::literal(note));
    }
    Flow::Cancel
}

/// A typed `bingo ...` command, or a shortcut, run by the session; its answer goes to the player who typed it.
pub fn perform_command(host: &mut dyn Host, uuid: u128, command: &str) -> Flow {
    if let Some(text) = chat::global_text(command) {
        let name = host.server_players().into_iter().find(|p| p.uuid == uuid).map(|p| p.name).unwrap_or_default();
        host.broadcast(Text::literal(chat::global_line(&name, text)));
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
    if parse(&line).is_ok_and(|command| requires_op(&command)) && !host.has_permission(uuid, "bingo.op", OP_LEVEL) {
        let _ = host.tell(uuid, Text::literal("You must be an operator to use this command.".to_string()));
        return Flow::Cancel;
    }
    let reply = guarded("bingo command", || {
        with_session(|s| s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line))
    });
    if let Some(reply) = reply {
        let _ = host.tell(uuid, Text::literal(reply));
    }
    if let Some(note) = names_note(&*host, &line) {
        let _ = host.tell(uuid, Text::literal(note));
    }
    Flow::Cancel
}

/// The lobby menu's rows and title, as the chest shows them.
const MENU_TITLE: &str = "Bingo";

/// The rules, whether the round is still pregame, and whether the player may run operator commands: what the menu shows.
fn menu_view(host: &dyn Host, uuid: u128) -> (Options, bool, bool) {
    let (rules, pregame) = with_session(|s| (s.rules().clone(), s.is_pregame()));
    (rules, pregame, host.has_permission(uuid, "bingo.op", OP_LEVEL))
}

/// The items of a menu page, as the chest holds them: the slots' items read from their SNBT.
fn menu_items(host: &dyn Host, page: &Page, rules: &Options, pregame: bool, operator: bool) -> Vec<Option<ItemStack>> {
    let view = View { rules, pregame, operator };
    menu::contents(page, &view)
        .into_iter()
        .map(|slot| slot.and_then(|slot| host.item_from_snbt(&menu::item_snbt(&slot))))
        .collect()
}

/// `bingo menu`: opens the lobby menu on its root page for the player.
fn open_lobby_menu(host: &mut dyn Host, uuid: u128) {
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let items = menu_items(&*host, &Page::Root, &rules, pregame, operator);
    match host.open_menu(uuid, menu::ROWS, Text::literal(MENU_TITLE), items) {
        Some(window) => LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).open(uuid, window, Page::Root),
        None => {
            let _ = host.tell(uuid, Text::literal("The menu could not be opened.".to_string()));
        }
    }
}

/// Shows `page` in the player's open lobby menu, refreshing its items. False if that window is not the lobby menu.
fn show_lobby_page(host: &mut dyn Host, uuid: u128, window: i32, page: Page) -> bool {
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let items = menu_items(&*host, &page, &rules, pregame, operator);
    if !LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).turn(uuid, window, page) {
        return false;
    }
    host.set_menu_items(uuid, items);
    true
}

/// A click in a menu the mod opened (`native_open_menu`): the lobby menu's slots run their outcome. Only a click that
/// picks up or shift-clicks a slot of the chest counts; the player's inventory and drags do nothing.
pub fn menu_clicked(host: &mut dyn Host, uuid: u128, window: i32, slot: i32, mode: i32) {
    if !matches!(mode, 0 | 1) {
        return;
    }
    let Ok(slot) = usize::try_from(slot) else { return };
    if slot >= menu::SLOTS {
        return;
    }
    let Some(page) = LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).page(uuid, window) else { return };
    let (rules, pregame, operator) = menu_view(&*host, uuid);
    let view = View { rules: &rules, pregame, operator };
    match menu::click(&page, slot, &view) {
        Outcome::Nothing => {}
        Outcome::Show(next) => {
            show_lobby_page(host, uuid, window, next);
        }
        Outcome::Close => {
            LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).close(uuid, window);
            host.close_menu(uuid);
        }
        Outcome::Tell(text) => {
            let _ = host.tell(uuid, Text::literal(text));
        }
        Outcome::Run(line) => {
            let reply = guarded("bingo menu command", || {
                with_session(|s| s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line))
            })
            .unwrap_or_default();
            if menu::start_refused(&line, &reply) {
                // The start was refused: its reason is the warnings page, which offers the start anyway.
                show_lobby_page(host, uuid, window, Page::Warnings(reply));
            } else {
                if !reply.is_empty() {
                    let _ = host.tell(uuid, Text::literal(reply));
                }
                let page = LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).page(uuid, window);
                if let Some(page) = page {
                    show_lobby_page(host, uuid, window, page);
                }
            }
        }
    }
}

/// The player's menu window closed: a lobby menu it was is forgotten.
pub fn menu_closed(uuid: u128, window: i32) {
    LOBBY_MENUS.lock().unwrap_or_else(PoisonError::into_inner).close(uuid, window);
}

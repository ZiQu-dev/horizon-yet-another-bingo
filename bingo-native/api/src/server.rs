//! The server half of "Yet Another Minecraft BINGO": one game session, driven by Horizon's server hooks. The game is
//! `horizon-bingo`'s `server` modules; this file adapts Horizon's `Host` to their `ServerHost` and keeps the session.
//! Each hook runs under `guarded`, and a poisoned lock is taken as it is, so one panic does not end the server's mod.
use super::{MOD_ID, guarded, log};
use horizon_bingo::server::commands::{parse, requires_op};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::items::ItemIds;
use horizon_bingo::server::ready::ReadyOptions;
use horizon_bingo::server::session::{Session, SessionOptions};
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
const USAGES: [(&str, &str); 16] = [
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
    ("bingo shuffleteams word?", "shuffleteams 2"),
    ("bingo timelimit word?", "timelimit 5"),
    ("bingo join word?", "join Red"),
    ("join word?", "join Red"),
    ("ready", "ready"),
    ("spectator", "spectator"),
];
/// The most console commands queued for the next server tick; more are dropped and logged.
const COMMAND_QUEUE_MAX: usize = 256;

/// The one session of this server. Made on the first server hook.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// Console commands the session asked for (`ServerHost::command`), run after the session's tick.
static COMMANDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The item registry's names, read once. `None` when the assets are missing (logged once).
static ITEM_IDS: OnceLock<Option<ItemIds>> = OnceLock::new();

/// A session that starts from the clock's time, with the default game options.
fn new_session() -> Session {
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Session::new(SessionOptions {
        seed,
        game: GameOptions::default(),
        ready: ReadyOptions::default(),
        // Teams start 200 blocks apart within 1000 of the world's origin (the original's default is 25 chunks).
        spread: Some((200, 1000)),
    })
}

/// Runs `f` with the session, made on first use.
fn with_session<T>(f: impl FnOnce(&mut Session) -> T) -> T {
    let mut guard = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(new_session))
}

/// A new server (a world opens): the old game and its players belong to the previous world.
pub fn server_about_to_start() {
    *SESSION.lock().unwrap_or_else(PoisonError::into_inner) = None;
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
            None
        }
    }
}

/// The authoritative tick: the session advances once per server tick that starts.
pub fn server_tick(host: &mut dyn Host, phase: ServerTickPhase) {
    if !matches!(phase, ServerTickPhase::ServerTickStart { .. }) {
        return;
    }
    guarded("server tick", || with_session(|s| s.tick(&mut ServerAdapter { host: &mut *host })));
    // Outside the session's lock: a console command may run hooks that reach the session again.
    guarded("bingo console commands", || run_queued_commands(&mut *host));
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
    Flow::Cancel
}

/// A typed `bingo ...` command, or a shortcut, run by the session; its answer goes to the player who typed it.
pub fn perform_command(host: &mut dyn Host, uuid: u128, command: &str) -> Flow {
    let Some(line) = session_line(command) else {
        return Flow::Continue;
    };
    let reply = guarded("bingo command", || {
        with_session(|s| s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line))
    });
    if let Some(reply) = reply {
        let _ = host.tell(uuid, Text::literal(reply));
    }
    Flow::Cancel
}

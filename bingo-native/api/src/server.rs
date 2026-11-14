//! The server half of "Yet Another Minecraft BINGO": one game session, driven by Horizon's server hooks. The game is
//! `horizon-bingo`'s `server` modules; this file adapts Horizon's `Host` to their `ServerHost` and keeps the session.
//! Each hook runs under `guarded`, and a poisoned lock is taken as it is, so one panic does not end the server's mod.
use super::{MOD_ID, guarded, log};
use horizon_bingo::server::game::GameOptions;
use horizon_bingo::server::host::ServerHost;
use horizon_bingo::server::items::ItemIds;
use horizon_bingo::server::session::{Session, SessionOptions};
use horizon_bingo::server::types::Player;
use horizon_client::mod_api::{Flow, Host, ItemStack, NativeCommand, ServerTickPhase, Text};
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
/// The operator level that starts, ends and resets a game (the original asks for `CONFIGURE_GAME`).
const OP_LEVEL: u8 = 2;

/// The one session of this server. Made on the first server hook.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// The item registry's names, read once. `None` when the assets are missing (logged once).
static ITEM_IDS: OnceLock<Option<ItemIds>> = OnceLock::new();

/// A session that starts from the clock's time, with the default team and game options.
fn new_session() -> Session {
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Session::new(SessionOptions {
        seed,
        game: GameOptions::default(),
        team: "bingo_red".to_owned(),
        team_color: "RED".to_owned(),
        team_name: "Red".to_owned(),
    })
}

/// Runs `f` with the session, made on first use.
fn with_session<T>(f: impl FnOnce(&mut Session) -> T) -> T {
    let mut guard = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(new_session))
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

/// The commands a player may type: `bingo status`, and `start`, `end` and `reset` for operators.
pub fn register_commands(commands: &mut Vec<NativeCommand>) {
    commands.push(NativeCommand::new("bingo status"));
    commands.push(NativeCommand::op("bingo start", OP_LEVEL));
    commands.push(NativeCommand::op("bingo end", OP_LEVEL));
    commands.push(NativeCommand::op("bingo reset", OP_LEVEL));
}

/// A typed `bingo ...` command, run by the session; its answer goes to the player who typed it.
pub fn perform_command(host: &mut dyn Host, uuid: u128, command: &str) -> Flow {
    let Some(rest) = command.strip_prefix(COMMAND).filter(|rest| rest.is_empty() || rest.starts_with(' ')) else {
        return Flow::Continue;
    };
    let line = rest.trim().to_owned();
    let reply = guarded("bingo command", || {
        with_session(|s| s.command(&mut ServerAdapter { host: &mut *host }, Some(uuid), &line))
    });
    if let Some(reply) = reply {
        let _ = host.tell(uuid, Text::literal(reply));
    }
    Flow::Cancel
}

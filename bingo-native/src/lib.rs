//! The wiring between Horizon's native hooks and `horizon-bingo`'s [`BingoClient`]. All behaviour is in that crate and
//! tested there against a fake host; this file only adapts Horizon's hosts and draw context to the crate's `Host` trait.
//! Logs a status line to the file named by BINGO_H0_LOG, and only when that variable is set.
use horizon_bingo::assets::Assets;
use horizon_bingo::client::BingoClient;
use horizon_bingo::config::ClientConfig;
use horizon_bingo::host::{Argb, Host as BingoHost, scale_alpha};
use horizon_bingo::state::SoundKey;
use mod_api::{
    ClientHost, ClientPacketEvent, ClientPlayerLoggingInEvent, ClientPlayerLoggingOutEvent, ClientTickPostEvent,
    CommandArgumentValue, CommandInvocation, CommandSource, ConfigPath, DrawContext, Flow, GuiLayerPostEvent,
    GuiLayerPreEvent, Hooks, IPayloadContext, IPayloadHandler, KeyRegistration, MenuClickedEvent, MenuClosedEvent,
    PayloadBytes, PayloadHandleError, PayloadId, PlayerInventoryChangedEvent, PlayerLoggedInEvent, PlayerLoggedOutEvent,
    RegisterCommandsEvent, Registrar, RegistrationContext, RunModCommandEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent,
    ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, ServerAboutToStartEvent, ServerChatEvent, ServerHost, ServerTickEvent,
    EntityRef, HostSide, RightClickBlockEvent, HudMode, Identifier, ItemKey, LevelsResetEvent, SoundPlay, Text,
};
use std::{
    collections::HashMap,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

mod lobby_scene;
mod server;
mod title_button;

/// How many text widths are kept before the cache starts again.
const WIDTH_CACHE_MAX: usize = 2048;
/// Text widths already measured, so a frame's repeated measurements do not cross the mod boundary each time. Cleared
/// when a connection starts or ends (the language and its font may have changed).
static WIDTHS: Mutex<Option<HashMap<String, i32>>> = Mutex::new(None);

/// The HUD layer the overlay is drawn after (it is drawn whenever the HUD is).
const OVERLAY_LAYER: &str = "minecraft:hotbar";

/// The HUD atlas slot Horizon gives a mod's own picture.
const MOD_IMAGE_SPRITE: &str = "horizon/modimage";
/// The mod's id: its manifest's `id`, for its own screen, key bindings, settings and config folder.
const MOD_ID: &str = "bingo";
/// The client config file, in the game's `config/` folder (`config/bingo/client.json`), read and written through the
/// mod's config files.
const CONFIG_FILE: &str = "bingo/client.json";
/// The default key bindings, as vanilla names. The keys are registered in Controls under these defaults.
const OPEN_CARD_KEY: &str = "key.keyboard.y";
const UNBOUND_KEY: &str = "key.keyboard.unknown";
/// The Options > Controls heading the mod's keys are listed under (the original's `key.category.yet-another-minecraft-bingo.bingo`).
const KEY_CATEGORY: &str = "key.category.yet-another-minecraft-bingo.bingo";
/// The client config is read again every this many ticks, so an edit of the file is taken as it was on disk.
const CONFIG_POLL_TICKS: u64 = 20;
/// The last config text read or written, to tell an edit of the file from the mod's own write.
static CONFIG_TEXT: Mutex<Option<String>> = Mutex::new(None);
/// The GUI size of the last frame drawn, in GUI pixels: the presses and the scroll read it, as the GUI size is not
/// given to a press. Zero before the first frame.
static LAST_GUI: Mutex<[f32; 2]> = Mutex::new([0.0; 2]);

fn last_gui() -> Option<(i32, i32)> {
    let [w, h] = *LAST_GUI.lock().unwrap_or_else(PoisonError::into_inner);
    (w > 0.0 && h > 0.0).then_some((w as i32, h as i32))
}

fn remember_gui(size: [f32; 2]) {
    *LAST_GUI.lock().unwrap_or_else(PoisonError::into_inner) = size;
}

fn mouse_of(host: &dyn ClientHost) -> (i32, i32) {
    let c = host.cursor();
    (c[0] as i32, c[1] as i32)
}

/// Where the mod's assets are: BINGO_ASSETS, else the mod's data folder in the game folder (`horizon-mod-data/bingo/
/// assets`), which `tools/prepare_assets.py` fills from the player's copy of the mod.
/// The folder `tools/prepare_assets.py` writes: `BINGO_ASSETS`, else the mod's data folder of the instance.
fn assets_dir(host: &dyn ClientHost) -> std::path::PathBuf {
    match std::env::var("BINGO_ASSETS") {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => host.instance_dir().join("horizon-mod-data").join(MOD_ID).join("assets"),
    }
}

fn load_assets(host: &dyn ClientHost) -> (Result<Assets, String>, String) {
    let game = host.instance_dir();
    let dir = assets_dir(host);
    let mut assets = Assets::load(&dir);
    if let Ok(a) = assets.as_mut() {
        // options.txt sits in the game folder.
        let language = std::fs::read_to_string(game.join("options.txt"))
            .ok()
            .and_then(|t| t.lines().find_map(|l| l.strip_prefix("lang:").map(str::to_string)))
            .unwrap_or_else(|| "en_us".into());
        if let Err(e) = a.set_language(&dir, language.trim()) {
            log(&format!("language {language}: {e}"));
        }
    }
    (assets, dir.display().to_string())
}

/// Whether the debug log is on: only when BINGO_H0_LOG names a file. Expensive lines are built only when it is.
fn logging() -> bool {
    std::env::var_os("BINGO_H0_LOG").is_some()
}

fn log(line: &str) {
    let Some(path) = std::env::var_os("BINGO_H0_LOG") else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

/// A channel's name as part of a file name: letters, digits, `_`, `-` and `.` only, no `..`, at most 80 characters, so
/// a server's channel cannot name a folder or a file outside BINGO_H0_DUMP.
fn file_name_part(channel: &str) -> String {
    let safe: String = channel
        .chars()
        .take(80)
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') { c } else { '_' })
        .collect();
    if safe.is_empty() { "channel".to_string() } else { safe.replace("..", "_") }
}

/// With BINGO_H0_DUMP=<dir>, writes the first three payloads of each channel as hex (fixtures for the tests).
fn dump(channel: &str, data: &[u8]) {
    static SEEN: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());
    let Ok(dir) = std::env::var("BINGO_H0_DUMP") else { return };
    let mut seen = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    let n = match seen.iter_mut().find(|(c, _)| c == channel) {
        Some((_, n)) => {
            *n += 1;
            *n
        }
        None => {
            seen.push((channel.to_string(), 1));
            1
        }
    };
    if n <= 3 {
        let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
        let _ = std::fs::write(format!("{dir}/{}.{n}.hex", file_name_part(channel)), hex);
    }
}

/// One drawing call, kept until the frame takes the run. A run is kept so that `end_alpha` can fade it before it is
/// drawn, as the old vertex-level fade did.
enum Op {
    Fill { rect: [f32; 4], argb: u32 },
    Quad { points: [[f32; 2]; 4], argb: u32 },
    Text { text: String, at: [f32; 2], argb: u32, shadow: bool },
    Sprite { src: [f32; 4], dst: [f32; 4], tint: u32 },
    Item { key: ItemKey, at: [f32; 2], size: f32 },
    NineSlice { sprite: String, rect: [f32; 4], argb: u32 },
    // PORT-GAP: a tooltip is drawn at full alpha; the new draw context gives no way to fade it.
    Tooltip { lines: Vec<String>, at: [f32; 2] },
}

impl Op {
    fn fade(&mut self, alpha: f32) {
        match self {
            Op::Fill { argb, .. } | Op::Quad { argb, .. } | Op::Text { argb, .. } | Op::NineSlice { argb, .. } => {
                *argb = scale_alpha(*argb, alpha)
            }
            Op::Sprite { tint, .. } => *tint = scale_alpha(*tint, alpha),
            // PORT-GAP: items are drawn at full alpha; the item call takes no colour.
            Op::Item { .. } | Op::Tooltip { .. } => {}
        }
    }
}

/// The drawing of one screen or HUD layer, recorded in order and handed to the frame's draw context at the end.
#[derive(Default)]
struct Draw {
    ops: Vec<Op>,
}

impl Draw {
    fn flush(self, out: &mut dyn DrawContext) {
        for op in self.ops {
            match op {
                Op::Fill { rect, argb } => out.fill(rect, argb),
                Op::Quad { points, argb } => out.quad(points, argb),
                Op::Text { text, at, argb, shadow } => out.text(&text, at, argb, shadow),
                Op::Sprite { src, dst, tint } => {
                    let _ = out.sprite(MOD_IMAGE_SPRITE, src, dst, Some(tint));
                }
                Op::Item { key, at, size } => {
                    let _ = out.item(key, at, size);
                }
                Op::NineSlice { sprite, rect, argb } => out.gui_nine_slice(&sprite, rect, argb),
                Op::Tooltip { lines, at } => {
                    let texts: Vec<Text> = lines.into_iter().map(|l| Text::literal(l)).collect();
                    let _ = out.gui_tooltip(&texts, at);
                }
            }
        }
    }
}

fn gui_pixels(size: [f32; 2]) -> (i32, i32) {
    (size[0] as i32, size[1] as i32)
}

/// The config text as the file holds it, None when it cannot be read.
fn read_client_config(host: &dyn ClientHost) -> Option<String> {
    let path = ConfigPath::new(CONFIG_FILE).ok()?;
    host.config_files().read(&path).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The config file is written with the crate's defaults when it is missing.
fn write_client_config(host: &dyn ClientHost, text: &str) {
    let Ok(path) = ConfigPath::new(CONFIG_FILE) else { return };
    if let Err(e) = host.config_files().write(&path, text.as_bytes()) {
        log(&format!("client config not saved: {e:?}"));
    }
}

struct Mod {
    client: Mutex<BingoClient>,
    key_bound: std::sync::atomic::AtomicBool,
}

impl Default for Mod {
    fn default() -> Self {
        Mod {
            client: Mutex::new(BingoClient::new()),
            key_bound: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl Mod {
    /// The crate's state. A lock poisoned by an earlier panic is taken as it is, so one panic does not end the mod.
    fn lock_client(&self) -> MutexGuard<'_, BingoClient> {
        self.client.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Runs a call into the crate so that a panic there cannot reach the game: `None` if it panicked.
fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => Some(value),
        Err(_) => {
            log(&format!("{what}: the crate panicked, the call was skipped"));
            None
        }
    }
}

/// The width of `text` in the game's font, rounded up, from the cache when it was measured before.
fn cached_width(host: &dyn ClientHost, text: &str) -> i32 {
    let mut cache = WIDTHS.lock().unwrap_or_else(PoisonError::into_inner);
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(&width) = map.get(text) {
        return width;
    }
    let width = host.text_width(&Text::literal(text.to_owned())).unwrap_or(0);
    if map.len() >= WIDTH_CACHE_MAX {
        map.clear();
    }
    map.insert(text.to_owned(), width);
    width
}

/// Empties the width cache (a new connection, or the first tick, when the language is read).
fn forget_widths() {
    *WIDTHS.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// Horizon's client host as the crate's `Host`, for the client tick: it writes, plays sounds and reads the config. It
/// draws nothing: the tick has no draw context, and the crate draws in the screen and HUD hooks.
struct Adapter<'a> {
    host: &'a mut dyn ClientHost,
}

// PORT-GAP: `looked_at_player` is not answered (the default, None): the new API has no client entity list, and the
// crosshair's entity has no UUID the client can read (`EntityRef::uuid` is server-side), so the player tooltip is lost.

impl BingoHost for Adapter<'_> {
    fn send_to_server(&mut self, channel: &str, data: &[u8]) -> Result<(), String> {
        // PORT-GAP: `minecraft:register` is vanilla's channel and cannot be declared here, so the client's list of
        // channels is not sent; raw channels reach the server without it.
        if channel == horizon_bingo::client::REGISTER_CHANNEL {
            return Ok(());
        }
        self.host
            .send_payload_to_server(&PayloadId(channel.to_owned()), PayloadBytes(data.to_vec()))
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }

    fn fill_rect(&mut self, _: i32, _: i32, _: i32, _: i32, _: Argb) {}

    fn draw_text(&mut self, _: i32, _: i32, _: &str, _: Argb, _: bool) {}

    fn draw_sprite(&mut self, _: [f32; 4], _: [f32; 4]) {}

    fn draw_item(&mut self, _: &str, _: f32, _: f32, _: f32) {}

    fn draw_vanilla_nine_slice(&mut self, _: &str, _: f32, _: f32, _: f32, _: f32, _: Argb) {}

    fn draw_tooltip(&mut self, _: &[String], _: i32, _: i32) {}

    // PORT-GAP: `player_face` is not overridden (the default, None): the new client API has no skin pixels, so the
    // card and score faces are dropped.
    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> bool {
        self.host.mod_image(width, height, rgba)
    }

    fn text_width(&self, text: &str) -> i32 {
        cached_width(&*self.host, text)
    }

    fn gui_width(&self) -> Option<i32> {
        // No frame is being drawn on the tick, so the width is not known here.
        None
    }

    fn gui_size(&self) -> Option<(i32, i32)> {
        last_gui()
    }

    fn cursor(&self) -> (i32, i32) {
        mouse_of(&*self.host)
    }

    fn is_playing_screen(&self) -> bool {
        self.host.playing()
    }

    fn key_down(&self, key: &str) -> bool {
        self.host.is_key_down(key)
    }

    fn sneaking(&self) -> bool {
        sneaking(&*self.host)
    }

    fn chat_open(&self) -> bool {
        chat_open(&*self.host)
    }

    fn hud_hidden(&self) -> bool {
        hud_hidden(&*self.host)
    }

    fn debug_screen(&self) -> bool {
        // PORT-GAP: F3's debug screen is not exposed by the new API, so the crate treats it as closed.
        false
    }

    fn tab_list_down(&self) -> bool {
        self.host.is_key_down("key.keyboard.tab")
    }

    fn setting(&self, key: &str) -> Option<String> {
        self.host.setting(MOD_ID, key)
    }

    fn set_setting(&mut self, key: &str, value: &str) {
        self.host.set_setting(MOD_ID, key, value);
    }

    fn open_screen(&mut self) -> bool {
        self.host.mod_screen_open(MOD_ID)
    }

    fn close_screen(&mut self) {
        self.host.mod_screen_close(MOD_ID);
    }

    fn screen_is_open(&self) -> bool {
        mod_screen_up(&*self.host)
    }

    fn config_changed(&mut self, json: &str) {
        write_client_config(&*self.host, json);
        *CONFIG_TEXT.lock().unwrap_or_else(PoisonError::into_inner) = Some(json.to_owned());
    }

    fn play_sound(&mut self, key: SoundKey, volume: f32, pitch: f32) {
        // The client has put the configured volume into `volume` (over the default one, which `actual_volume` scales by).
        self.play_local(key, horizon_bingo::sounds::actual_volume(volume), pitch);
    }

    fn play_test_sound(&mut self, id: &str, volume: f32) {
        // The settings screen's test button: the configured volume already, so it is played as it is.
        if let Some(key) = horizon_bingo::sounds::ALL.iter().copied().find(|k| horizon_bingo::sounds::name(*k) == id) {
            self.play_local(key, volume, 1.0);
        }
    }
}

impl Adapter<'_> {
    /// Plays one of the mod's sounds for this player at their head, at `volume` and `pitch` as Horizon takes them.
    // PORT-GAP: the sound plays on the UI source at the listener, as the new API's `SoundPlay::ui` does; the old call
    // named no source.
    fn play_local(&mut self, key: SoundKey, volume: f32, pitch: f32) {
        // The original's own sound event (from the sounds the player's copy of the mod provides as a content mod),
        // played for this player only, as the original plays them.
        let event = horizon_bingo::sounds::event_id(key);
        let sound = SoundPlay::ui(&event).with_volume(volume).with_pitch(pitch);
        let ok = self.host.play_local_sound(&sound, 0).is_some();
        log(&format!("sound {event} volume {volume} pitch {pitch}: {}", if ok { "played" } else { "no such sound event" }));
    }
}

/// Whether the player is sneaking: the local player's own shift key.
fn sneaking(host: &dyn ClientHost) -> bool {
    host.local_player()
        .and_then(|player| EntityRef::from(player).is_shift_key_down(host))
        .unwrap_or(false)
}

/// Whether the chat screen is open.
fn chat_open(host: &dyn ClientHost) -> bool {
    host.observed_screen_id().is_some_and(|screen| screen.id == "chat")
}

/// Whether the HUD is hidden (F1, or the mod's own hide).
fn hud_hidden(host: &dyn ClientHost) -> bool {
    host.hud_mode() != HudMode::AllVisible
}

/// Horizon's client host as a read-only crate `Host`, with the frame's drawing recorded. The screen and HUD hooks get
/// a `&dyn ClientHost`, so nothing here can send, open or play.
struct ReadOnly<'a> {
    host: &'a dyn ClientHost,
    draw: Draw,
    gui: [f32; 2],
}

impl<'a> ReadOnly<'a> {
    fn new(host: &'a dyn ClientHost, gui: [f32; 2]) -> Self {
        remember_gui(gui);
        ReadOnly { host, draw: Draw::default(), gui }
    }
}

// PORT-GAP: `draw_text_xf` is not overridden, so the crate's default places text by its anchor and offset but draws it
// upright and unscaled: the text's rotation and scale are lost (the new draw context has no transform).
impl BingoHost for ReadOnly<'_> {
    fn send_to_server(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Err("read-only host".into())
    }
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Argb) {
        self.draw.ops.push(Op::Fill { rect: [x as f32, y as f32, w as f32, h as f32], argb: color });
    }
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: Argb, shadow: bool) {
        self.draw.ops.push(Op::Text {
            text: text.to_owned(),
            at: [x as f32, y as f32],
            argb: 0xFF00_0000 | (color & 0x00FF_FFFF),
            shadow,
        });
    }
    fn text_width(&self, text: &str) -> i32 {
        cached_width(self.host, text)
    }
    fn fill_square_xf(&mut self, cx: f32, cy: f32, half: f32, rotation: f32, color: Argb) {
        let (s, c) = rotation.sin_cos();
        let corner = |x: f32, y: f32| [cx + c * x - s * y, cy + s * x + c * y];
        let points = [corner(-half, -half), corner(half, -half), corner(half, half), corner(-half, half)];
        self.draw.ops.push(Op::Quad { points, argb: color });
    }
    fn upload_image(&mut self, _: u32, _: u32, _: &[u8]) -> bool {
        false
    }
    fn draw_sprite(&mut self, src: [f32; 4], dst: [f32; 4]) {
        self.draw.ops.push(Op::Sprite { src, dst, tint: 0xFFFF_FFFF });
    }
    fn draw_vanilla_nine_slice(&mut self, sprite: &str, x: f32, y: f32, w: f32, h: f32, argb: Argb) {
        // The GUI atlas and its `gui.scaling` metadata are keyed by the full path (`gui/sprites/widget/tab`), and
        // the crate names vanilla widgets `widget/...`.
        let full = match sprite.starts_with("gui/sprites/") {
            true => sprite.to_string(),
            false => format!("gui/sprites/{sprite}"),
        };
        self.draw.ops.push(Op::NineSlice { sprite: full, rect: [x, y, w, h], argb });
    }
    fn draw_tooltip(&mut self, lines: &[String], x: i32, y: i32) {
        self.draw.ops.push(Op::Tooltip { lines: lines.to_vec(), at: [x as f32, y as f32] });
    }
    fn draw_item(&mut self, name: &str, x: f32, y: f32, size: f32) {
        let Some(id) = Identifier::parse_namespaced(name) else { return };
        let Some(key) = self.host.item_key(&id) else { return };
        self.draw.ops.push(Op::Item { key, at: [x, y], size });
    }
    fn begin_alpha(&mut self) -> usize {
        self.draw.ops.len()
    }
    fn end_alpha(&mut self, start: usize, alpha: f32) {
        for op in self.draw.ops.iter_mut().skip(start) {
            op.fade(alpha);
        }
    }
    fn gui_width(&self) -> Option<i32> {
        Some(self.gui[0] as i32)
    }
    fn gui_size(&self) -> Option<(i32, i32)> {
        Some(gui_pixels(self.gui))
    }
    fn cursor(&self) -> (i32, i32) {
        mouse_of(self.host)
    }
    fn is_playing_screen(&self) -> bool {
        self.host.playing()
    }
    fn key_down(&self, key: &str) -> bool {
        self.host.is_key_down(key)
    }
    fn sneaking(&self) -> bool {
        sneaking(self.host)
    }
    fn chat_open(&self) -> bool {
        chat_open(self.host)
    }
    fn hud_hidden(&self) -> bool {
        hud_hidden(self.host)
    }
    fn debug_screen(&self) -> bool {
        // PORT-GAP: F3's debug screen is not exposed by the new API, so the crate treats it as closed.
        false
    }
    fn tab_list_down(&self) -> bool {
        self.host.is_key_down("key.keyboard.tab")
    }
    fn setting(&self, key: &str) -> Option<String> {
        self.host.setting(MOD_ID, key)
    }
    /// Read-only: the settings are kept by the tick's adapter, which is the only one that flips the HUD.
    fn set_setting(&mut self, _: &str, _: &str) {}
    fn open_screen(&mut self) -> bool {
        false
    }
    fn close_screen(&mut self) {}
    fn screen_is_open(&self) -> bool {
        mod_screen_up(self.host)
    }
    fn play_sound(&mut self, _: SoundKey, _: f32, _: f32) {}
}

fn mod_screen_up(host: &dyn ClientHost) -> bool {
    host.mod_screen() == Some(MOD_ID)
}

fn width_fn(host: &dyn ClientHost) -> impl Fn(&str) -> i32 + '_ {
    move |s: &str| cached_width(host, s)
}

/// A raw plugin channel's server side: the mod's own channels reach the session with the channel's full name, from
/// the player who sent them.
struct ServerChannel {
    channel: String,
}

impl IPayloadHandler for ServerChannel {
    fn handle(&self, payload: PayloadBytes, context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
        let Ok(player) = context.player() else { return Ok(()) };
        let uuid = player.uuid();
        let Ok(host) = context.server_host() else { return Ok(()) };
        guarded("payload", || server::custom_payload_from_client(host, uuid, &self.channel, &payload.0));
        Ok(())
    }
}

/// The typed command as the line the command parser reads: its literals, then its arguments in order. None for an
/// argument kind no Bingo usage has.
fn command_line(invocation: &CommandInvocation) -> Option<String> {
    let mut words: Vec<String> = invocation.literals.clone();
    for argument in &invocation.arguments {
        words.push(match &argument.value {
            CommandArgumentValue::String(text) => text.clone(),
            CommandArgumentValue::Integer(n) => n.to_string(),
            CommandArgumentValue::Number(n) => n.to_string(),
            CommandArgumentValue::Boolean(b) => b.to_string(),
            _ => return None,
        });
    }
    Some(words.join(" "))
}

/// The key the player bound a mod key to in Options > Controls, or its default when none is bound.
fn mod_key(host: &dyn ClientHost, name: &str, default: &str) -> String {
    host.mod_key(MOD_ID, name).map(|(key, _)| key).unwrap_or_else(|| default.into())
}

/// Hands the client the keys of the card (on Y by default), the HUD toggle and the settings screen (both unbound by default).
fn read_keys(host: &dyn ClientHost, client: &mut BingoClient) {
    client.set_open_key(&mod_key(host, "open_card", OPEN_CARD_KEY));
    client.set_toggle_key(&mod_key(host, "toggle_hud", UNBOUND_KEY));
    client.set_settings_key(&mod_key(host, "settings", UNBOUND_KEY));
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// The raw channels the crate uses, declared once: the server hears what the clients send on them.
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        // The labels and the category are the original's translation keys (`bingo.keybind.*`, `key.category.*`).
        r.register_key_mapping(KeyRegistration::new("open_card", OPEN_CARD_KEY).label("bingo.keybind.open_card").category(KEY_CATEGORY));
        r.register_key_mapping(KeyRegistration::new("toggle_hud", UNBOUND_KEY).label("bingo.keybind.toggle_hud").category(KEY_CATEGORY));
        r.register_key_mapping(KeyRegistration::new("settings", UNBOUND_KEY).label("bingo.keybind.settings").category(KEY_CATEGORY));
        let mut payloads = r.payload_handlers();
        let mut channels = payloads.registrar("1");
        for channel in horizon_bingo::channels::CHANNELS.iter() {
            let name = horizon_bingo::channels::id(channel.path);
            let handler: Arc<dyn IPayloadHandler> = Arc::new(ServerChannel { channel: name.clone() });
            channels.play_raw(PayloadId(name), Some(handler));
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        guarded("title button", || title_button::tick(host));
        guarded("client tick", || {
            let mut client = self.lock_client();
            if !self.key_bound.swap(true, std::sync::atomic::Ordering::Relaxed) {
                let (assets, dir) = load_assets(&*host);
                match &assets {
                    Ok(a) => log(&format!("assets from {dir}: {} sprites in a {}x{} atlas", a.atlas.len(), a.atlas.width, a.atlas.height)),
                    Err(e) => log(&format!("assets NOT loaded from {dir}: {e}")),
                }
                client.set_assets(assets);
                log(&format!("card key: {}", mod_key(&*host, "open_card", OPEN_CARD_KEY)));
                // The client config: the file the player has, or the crate's defaults written in its place.
                let text = match read_client_config(&*host) {
                    Some(text) => text,
                    None => {
                        let default = ClientConfig::default().to_json();
                        write_client_config(&*host, &default);
                        default
                    }
                };
                client.set_config(ClientConfig::from_json(&text));
                *CONFIG_TEXT.lock().unwrap_or_else(PoisonError::into_inner) = Some(text);
                forget_widths();
            } else if client.ticks() % CONFIG_POLL_TICKS == 0 {
                // An edit of the file on disk (the config's reload, which the new API does not give for this file).
                if let Some(text) = read_client_config(&*host) {
                    let mut last = CONFIG_TEXT.lock().unwrap_or_else(PoisonError::into_inner);
                    if last.as_deref() != Some(text.as_str()) {
                        client.set_config(ClientConfig::from_json(&text));
                        *last = Some(text);
                    }
                }
            }
            // The keys are read each tick, so a key bound in Options > Controls applies without a relaunch.
            read_keys(&*host, &mut client);
            client.on_tick(&mut Adapter { host });
            // The crate keeps its screen and firework events until the host takes them; this mod has no use for them yet.
            let _ = client.take_ui_events();
            if client.ticks() % 100 == 0 && logging() {
                log(&format!("tiles: {}", client.tile_ids().join(",")));
                log(&format!(
                    "tick {}: applied {:?} errors {:?} | {}",
                    client.ticks(),
                    client.applied,
                    client.errors,
                    client.overlay_lines().join(" / ")
                ));
            }
        });
    }

    fn client_packet(&self, _host: &mut dyn ClientHost, event: &mut ClientPacketEvent<'_>) {
        if let Some(p) = &event.packet.custom_payload {
            dump(&p.channel, &p.data);
            guarded("payload", || {
                let _ = self.lock_client().on_packet(&p.channel, &p.data);
            });
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        guarded("title button draw", || title_button::render(self, host, event));
        if mod_screen_up(host) {
            // The original's screens sit over vanilla's blurred menu background.
            host.apply_screen_blur();
            let gui = event.draw.size();
            guarded("screen draw", || {
                let mut view = ReadOnly::new(host, gui);
                self.lock_client().render_screen(&mut view);
                view.draw.flush(&mut *event.draw);
            });
        }
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        if let Some(flow) = title_button::press(event) {
            return flow;
        }
        if !mod_screen_up(host) {
            return Flow::Continue;
        }
        if event.button == 0 {
            // PORT-GAP: the GUI size is the last frame's (a press does not carry it); zero before the first frame.
            if let Some(gui) = last_gui() {
                let width = width_fn(host);
                if guarded("screen press", || self.lock_client().screen_press(gui, mouse_of(host), &width)).is_none() {
                    return Flow::Continue;
                }
            }
        }
        Flow::Cancel
    }

    fn screen_mouse_button_released_pre(&self, host: &dyn ClientHost, _: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
        if !mod_screen_up(host) {
            return Flow::Continue;
        }
        if guarded("screen release", || {
            self.lock_client().screen_release();
        })
        .is_none()
        {
            return Flow::Continue;
        }
        Flow::Cancel
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        if !mod_screen_up(host) {
            return Flow::Continue;
        }
        if guarded("screen scroll", || {
            self.lock_client().screen_wheel(mouse_of(host), event.scroll[1]);
        })
        .is_none()
        {
            return Flow::Continue;
        }
        Flow::Cancel
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn ClientHost, _: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        if !mod_screen_up(host) {
            return Flow::Continue;
        }
        if guarded("screen drag", || {
            self.lock_client().screen_drag(mouse_of(host));
        })
        .is_none()
        {
            return Flow::Continue;
        }
        Flow::Cancel
    }

    fn screen_key_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        if !mod_screen_up(host) {
            return Flow::Continue;
        }
        // Escape: the screen closes with its own animation (the end screen cannot be escaped: the game's Escape is kept).
        let handled = guarded("screen key", || {
            let mut client = self.lock_client();
            let escaped = event.key == "key.keyboard.escape" && client.screen_escape();
            escaped || client.screen_key(event.key)
        });
        match handled {
            Some(true) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    fn gui_layer_pre(&self, _host: &dyn ClientHost, event: &mut GuiLayerPreEvent<'_>) -> Flow {
        // The sidebar is left out for a player who turned the scoreboard off (`BingoClient::hides_layer`).
        if self.lock_client().hides_layer(event.layer) { Flow::Cancel } else { Flow::Continue }
    }

    fn gui_layer_post(&self, host: &dyn ClientHost, event: &mut GuiLayerPostEvent<'_>) {
        if event.layer == OVERLAY_LAYER {
            let gui = event.draw.size();
            guarded("overlay draw", || {
                let mut view = ReadOnly::new(host, gui);
                self.lock_client().draw(&mut view);
                view.draw.flush(&mut *event.draw);
            });
        }
    }

    /// The window leaves its connection: the server's state (cards, scores, messages) belongs to that server only.
    fn client_player_logging_out(&self, _host: &mut dyn ClientHost, _: &mut ClientPlayerLoggingOutEvent) {
        guarded("disconnect", || {
            self.lock_client().state.reset_all();
        });
        forget_widths();
    }

    /// The window logs in to a server: a new server starts from an empty state, as after a disconnect.
    fn client_player_logging_in(&self, _host: &mut dyn ClientHost, _: &mut ClientPlayerLoggingInEvent) {
        guarded("login", || {
            self.lock_client().state.reset_all();
        });
        forget_widths();
    }

    /// The server side: the session advances on the server's tick, and takes its players, inventories and packets.
    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        server::server_tick(host, event.phase.clone());
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        server::player_logged_in(host, event.player, event.name);
    }

    /// Plain chat votes in Postgame (`vote lobby`, `vote next`, `vote keep`); every other line is left alone.
    fn server_chat(&self, _host: &mut dyn ServerHost, event: &mut ServerChatEvent<'_>) -> Flow {
        server::player_chat(event.player, event.raw_text)
    }

    fn player_logged_out(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        server::player_logged_out(host, event.player);
    }

    fn player_inventory_changed(&self, host: &mut dyn ServerHost, event: &mut PlayerInventoryChangedEvent<'_>) {
        server::player_inventory_changed(host, event.player, event.stacks);
    }

    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        server::register_commands(event.commands);
    }

    /// A typed command of the mod's: a player's, or the console's and the command blocks' and functions' (their answers
    /// go to their source, as the server's feedback does).
    // PORT-GAP: the server's own parser reads a raw line, so the typed command is joined back into one (`command_line`);
    // runs of spaces in a greedy `text` argument collapse. Bare `join`, `ready` and `spectator` (the shortcuts) only
    // reach here if server.rs registers them as commands; otherwise they are not heard.
    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        let Some(line) = command_line(&event.command) else { return Flow::Continue };
        match &event.source {
            CommandSource::Player { player, .. } => server::perform_command(host, player.uuid(), &line),
            _ => server::perform_console_command(host, &line),
        }
    }

    fn server_about_to_start(&self, _host: &mut dyn ServerHost, _event: &mut ServerAboutToStartEvent) {
        server::server_about_to_start();
    }

    /// The vanilla worlds were reset for a game: the lobby flow puts the players into the new overworld and begins.
    fn levels_reset(&self, host: &mut dyn ServerHost, _event: &mut LevelsResetEvent) {
        server::levels_reset(host);
    }

    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent) {
        server::menu_clicked(host, event.uuid, event.window, event.slot, event.mode);
    }

    /// A right click on the lobby's menu signs or a team pad (server side, main hand): opens the menu or joins the team.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        if !matches!(host, HostSide::Server(_)) {
            return Flow::Continue;
        }
        let mut result = *event.cancellation_result;
        let flow = lobby_scene::right_click_block(event.click, &mut result);
        *event.cancellation_result = result;
        flow
    }

    fn menu_closed(&self, _host: &mut dyn ServerHost, event: &mut MenuClosedEvent) {
        server::menu_closed(event.uuid, event.window);
    }
}

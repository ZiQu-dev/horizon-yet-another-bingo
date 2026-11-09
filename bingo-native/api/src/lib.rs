//! The wiring between Horizon's native hooks and `horizon-bingo`'s [`BingoClient`]. All behaviour is in that crate and
//! tested there against a fake host; this file only adapts Horizon's `Host` and HUD batch to the crate's `Host` trait.
//! Logs a status line to the file named by BINGO_H0_LOG, and only when that variable is set.
use horizon_bingo::assets::Assets;
use horizon_bingo::buf::encode;
use horizon_bingo::client::BingoClient;
use horizon_bingo::config::ClientConfig;
use horizon_bingo::host::{Argb, Host as BingoHost, scale_alpha};
use horizon_bingo::state::SoundKey;
use horizon_client::mod_api::{
    ClientDisconnectingEvent, ClientPacketEvent, ClientPlayerNetworkLoggingInEvent, ClientTickPostEvent,
    ClientboundPlayPacket, ConfigLoadingEvent, ConfigReloadingEvent, Flow, GuiLayerPostEvent, Hooks, Host, HudBatch,
    KeyCode, NativeConfig, NativeConfigFile, NativeConfigKind, NativeScreenKind, RegisterConfigsEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent, ServerboundPlayPacket, Text,
};
use std::{
    collections::HashMap,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, MutexGuard, PoisonError},
};

/// How many text widths are kept before the cache starts again.
const WIDTH_CACHE_MAX: usize = 2048;
/// Text widths already measured, so a frame's repeated measurements do not cross the mod boundary each time. Cleared
/// when a connection starts or ends (the language and its font may have changed).
static WIDTHS: Mutex<Option<HashMap<String, i32>>> = Mutex::new(None);

/// `ids::play::serverbound::CUSTOM_PAYLOAD` of protocol 777.
const PLAY_SERVERBOUND_CUSTOM_PAYLOAD: i32 = 22;
/// The HUD layer the overlay is drawn after (it is drawn whenever the HUD is).
const OVERLAY_LAYER: &str = "minecraft:hotbar";

/// The HUD atlas slot Horizon gives a mod's own picture.
const MOD_IMAGE_SPRITE: &str = "horizon/modimage";
/// The mod's id, for its own screen and key binding.
const MOD_ID: &str = "bingo_native";
/// The client config file, in Horizon's config folder (`config/bingo/client.json`).
const CONFIG_FILE: &str = "bingo/client.json";
/// How far the player tooltip's look reaches, in blocks. The mod API does not say the creative reach (5 blocks in
/// vanilla), so the survival reach is used for both.
const TOOLTIP_REACH: f64 = 3.0;
/// Where the client config was read from, kept for its writes. Set when the game reads the file.
static CONFIG_PATH: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// A vanilla key name (`key.keyboard.y`) as a key code: the letters and digits, which is what a card key can be.
fn key_code(name: &str) -> Option<KeyCode> {
    let key = name.strip_prefix("key.keyboard.")?;
    Some(match key {
        "a" => KeyCode::KeyA, "b" => KeyCode::KeyB, "c" => KeyCode::KeyC, "d" => KeyCode::KeyD, "e" => KeyCode::KeyE,
        "f" => KeyCode::KeyF, "g" => KeyCode::KeyG, "h" => KeyCode::KeyH, "i" => KeyCode::KeyI, "j" => KeyCode::KeyJ,
        "k" => KeyCode::KeyK, "l" => KeyCode::KeyL, "m" => KeyCode::KeyM, "n" => KeyCode::KeyN, "o" => KeyCode::KeyO,
        "p" => KeyCode::KeyP, "q" => KeyCode::KeyQ, "r" => KeyCode::KeyR, "s" => KeyCode::KeyS, "t" => KeyCode::KeyT,
        "u" => KeyCode::KeyU, "v" => KeyCode::KeyV, "w" => KeyCode::KeyW, "x" => KeyCode::KeyX, "y" => KeyCode::KeyY,
        "z" => KeyCode::KeyZ,
        "0" => KeyCode::Digit0, "1" => KeyCode::Digit1, "2" => KeyCode::Digit2, "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4, "5" => KeyCode::Digit5, "6" => KeyCode::Digit6, "7" => KeyCode::Digit7,
        "8" => KeyCode::Digit8, "9" => KeyCode::Digit9,
        _ => return None,
    })
}

fn gui_of(host: &dyn Host) -> Option<(i32, i32)> {
    host.client_state().map(|s| (s.gui_size[0] as i32, s.gui_size[1] as i32))
}

fn mouse_of(host: &dyn Host) -> (i32, i32) {
    let c = host.cursor();
    (c[0] as i32, c[1] as i32)
}

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod {
        client: Mutex::new(BingoClient::new()),
        key_bound: std::sync::atomic::AtomicBool::new(false),
    })
}

/// Where the original mod's files are: BINGO_ASSETS, else the mod's own data folder (`horizon-mods/.horizon-mod-data/
/// bingo_native/assets`), which `tools/prepare_assets.py` fills from the player's copy of the mod.
fn load_assets(host: &dyn Host) -> (Result<Assets, String>, String) {
    let dir = match std::env::var("BINGO_ASSETS") {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => host.data_dir(MOD_ID).join("assets"),
    };
    let mut assets = Assets::load(&dir);
    if let Ok(a) = assets.as_mut() {
        // options.txt sits in the game folder, three levels above the mod's data folder.
        let game = host.data_dir(MOD_ID).ancestors().nth(3).map(|p| p.to_path_buf());
        let language = game
            .and_then(|g| std::fs::read_to_string(g.join("options.txt")).ok())
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

/// Moves the vertices drawn since `start` as `translate(anchor); rotate(rotation); scale(scale)` does to what was drawn
/// at the origin: scaled about it, turned counter-clockwise by `rotation` radians, then moved to `anchor`.
fn transform_since(batch: &mut HudBatch, start: usize, anchor: (f32, f32), scale: f32, rotation: f32) {
    let (s, c) = rotation.sin_cos();
    for v in batch.vertices.iter_mut().skip(start) {
        let (x, y) = (v.pos[0] * scale, v.pos[1] * scale);
        v.pos[0] = anchor.0 + c * x - s * y;
        v.pos[1] = anchor.1 + s * x + c * y;
    }
}

/// Multiplies the alpha of the vertices drawn since `start` by `alpha`. A vertex's `color` packs its alpha in the top
/// byte, as `rgba` does for the GUI colours, so `scale_alpha` fades it without touching the colour.
fn fade_since(batch: &mut HudBatch, start: usize, alpha: f32) {
    for v in batch.vertices.iter_mut().skip(start) {
        v.color = scale_alpha(v.color, alpha);
    }
}

struct Mod {
    client: Mutex<BingoClient>,
    key_bound: std::sync::atomic::AtomicBool,
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
fn cached_width(host: &dyn Host, text: &str) -> i32 {
    let mut cache = WIDTHS.lock().unwrap_or_else(PoisonError::into_inner);
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(&width) = map.get(text) {
        return width;
    }
    let width = host.text_width(text).ceil() as i32;
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

/// Horizon's `Host` (and, inside a HUD layer, its batch) as the crate's `Host`.
struct Adapter<'a> {
    host: &'a mut dyn Host,
    batch: Option<&'a mut HudBatch>,
}

impl BingoHost for Adapter<'_> {
    fn send_to_server(&mut self, channel: &str, data: &[u8]) -> Result<(), String> {
        // ServerboundCustomPayloadPacket: the channel as a VarInt-length string, then the data. Built through
        // `decode` so the mod needs no crate besides horizon-client.
        let body = encode(|w| {
            w.write_string(channel);
            w.write_raw(data);
        });
        let packet = ServerboundPlayPacket::decode(PLAY_SERVERBOUND_CUSTOM_PAYLOAD, &body).map_err(|e| e.to_string())?;
        self.host.send_packet(packet)
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Argb) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.fill(batch, [x as f32, y as f32, w as f32, h as f32], color);
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: Argb, shadow: bool) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.text(batch, text, [x as f32, y as f32], color & 0x00FF_FFFF, shadow);
        }
    }

    fn draw_text_xf(&mut self, text: &str, anchor: (f32, f32), offset: (f32, f32), scale: f32, rotation: f32, color: Argb, shadow: bool) {
        if let Some(batch) = self.batch.as_mut() {
            let start = batch.vertices.len();
            self.host.text(batch, text, [offset.0, offset.1], color & 0x00FF_FFFF, shadow);
            transform_since(batch, start, anchor, scale, rotation);
        }
    }

    fn fill_square_xf(&mut self, cx: f32, cy: f32, half: f32, rotation: f32, color: Argb) {
        if let Some(batch) = self.batch.as_mut() {
            let start = batch.vertices.len();
            self.host.fill(batch, [-half, -half, 2.0 * half, 2.0 * half], color);
            transform_since(batch, start, (cx, cy), 1.0, rotation);
        }
    }

    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> bool {
        self.host.mod_image(width, height, rgba)
    }

    fn player_face(&mut self, uuid: u128) -> Option<Vec<u8>> {
        let profiles = self.host.client_profiles();
        let Some(profile) = profiles.into_iter().find(|p| p.uuid == uuid) else {
            log(&format!("face {uuid:032x}: no profile"));
            return None;
        };
        let skin = profile.skin;
        let Some(tex) = self.host.client_texture(&skin) else {
            log(&format!("face {uuid:032x}: no texture for {skin:?}"));
            return None;
        };
        let (w, h) = (tex.width as usize, tex.height as usize);
        if w < 64 || h < 16 || tex.rgba.len() < w * h * 4 {
            return None;
        }
        // The face is the skin's 8 x 8 at (8, 8); the hat layer at (40, 8) goes over it where it is not clear.
        let px = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * w + x) * 4;
            [tex.rgba[i], tex.rgba[i + 1], tex.rgba[i + 2], tex.rgba[i + 3]]
        };
        let mut out = Vec::with_capacity(8 * 8 * 4);
        for y in 0..8 {
            for x in 0..8 {
                let (base, hat) = (px(8 + x, 8 + y), px(40 + x, 8 + y));
                let a = hat[3] as u32;
                for c in 0..3 {
                    out.push(((hat[c] as u32 * a + base[c] as u32 * (255 - a)) / 255) as u8);
                }
                out.push(255);
            }
        }
        Some(out)
    }

    fn draw_sprite(&mut self, src: [f32; 4], dst: [f32; 4]) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.sprite(batch, MOD_IMAGE_SPRITE, src, dst);
        }
    }

    fn draw_item(&mut self, name: &str, x: f32, y: f32, size: f32) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.item(batch, name, [x, y], size);
        }
    }

    fn begin_alpha(&mut self) -> usize {
        self.batch.as_ref().map_or(0, |batch| batch.vertices.len())
    }

    fn end_alpha(&mut self, start: usize, alpha: f32) {
        if let Some(batch) = self.batch.as_mut() {
            fade_since(batch, start, alpha);
        }
    }

    fn text_width(&self, text: &str) -> i32 {
        cached_width(&*self.host, text)
    }

    fn gui_size(&self) -> Option<(i32, i32)> {
        gui_of(&*self.host)
    }

    fn cursor(&self) -> (i32, i32) {
        mouse_of(&*self.host)
    }

    fn is_playing_screen(&self) -> bool {
        self.host.client_state().is_some_and(|s| matches!(s.screen, NativeScreenKind::Playing))
    }

    fn key_down(&self, key: &str) -> bool {
        key_code(key).is_some_and(|code| self.host.is_key_down(code))
    }

    fn sneaking(&self) -> bool {
        self.host.sneaking()
    }

    fn chat_open(&self) -> bool {
        self.host.client_state().is_some_and(|s| matches!(s.screen, NativeScreenKind::Chat))
    }

    fn hud_hidden(&self) -> bool {
        self.host.client_state().is_some_and(|s| s.gui_hidden)
    }

    fn debug_screen(&self) -> bool {
        self.host.client_state().is_some_and(|s| s.debug_screen)
    }

    fn tab_list_down(&self) -> bool {
        self.host.is_key_down(KeyCode::Tab)
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
        self.host.mod_screen() == Some(MOD_ID)
    }

    fn draw_vanilla_nine_slice(&mut self, sprite: &str, x: f32, y: f32, w: f32, h: f32, argb: Argb) {
        if let Some(batch) = self.batch.as_mut() {
            // The GUI atlas and its `gui.scaling` metadata are keyed by the full path (`gui/sprites/widget/tab`), and
            // `native_gui_nine_slice` looks the name up as given; the crate names vanilla widgets `widget/...`.
            let full = match sprite.starts_with("gui/sprites/") {
                true => sprite.to_string(),
                false => format!("gui/sprites/{sprite}"),
            };
            self.host.gui_nine_slice(batch, &full, [x, y, w, h], argb);
        }
    }

    fn draw_tooltip(&mut self, lines: &[String], x: i32, y: i32) {
        if let Some(batch) = self.batch.as_mut() {
            let texts: Vec<_> = lines.iter().map(|l| Text::literal(l.clone())).collect();
            self.host.gui_tooltip(batch, &texts, [x as f32, y as f32]);
        }
    }

    fn gui_width(&self) -> Option<i32> {
        // The HUD batch is in GUI pixels and carries the GUI size (docs: `batch.width` and `batch.height`).
        self.batch.as_ref().map(|batch| batch.width as i32)
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

    fn config_changed(&mut self, json: &str) {
        // The file the game read the config from, else the config folder's copy (the game's own `config/`).
        let remembered: Option<std::path::PathBuf> = CONFIG_PATH.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let path = match remembered {
            Some(path) => path,
            None => match self.host.config_dir() {
                Ok(dir) => dir.join(CONFIG_FILE),
                Err(e) => {
                    log(&format!("client config not saved: {e}"));
                    return;
                }
            },
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, json) {
            log(&format!("client config not saved to {}: {e}", path.display()));
        }
    }

    fn looked_at_player(&self) -> Option<u128> {
        // The view ray is the player's eye and rotation (`client_state`); the players are the living, visible ones
        // (vanilla's entity picking skips spectators). A player's box is its bounds, with its feet at the bottom middle.
        let state = self.host.client_state()?;
        let eye = state.eye_position?;
        let players: Vec<(u128, [f64; 3], f32, f32)> = self
            .host
            .client_entities()
            .into_iter()
            .filter(|e| e.alive && !e.spectator)
            .filter_map(|e| {
                let uuid = e.player_uuid?;
                let [lo, hi] = e.bounds;
                let feet = [(lo[0] + hi[0]) / 2.0, lo[1], (lo[2] + hi[2]) / 2.0];
                Some((uuid, feet, (hi[0] - lo[0]) as f32, e.height))
            })
            .collect();
        horizon_bingo::hud_tooltip::looked_at_player(eye, state.yaw, state.pitch, &players, TOOLTIP_REACH)
    }
}

impl Adapter<'_> {
    /// Plays one of the mod's sounds for this player at their head, at `volume` and `pitch` as Horizon takes them.
    fn play_local(&mut self, key: SoundKey, volume: f32, pitch: f32) {
        // The original's own sound event (from the sounds the player's copy of the mod provides as a content mod),
        // played for this player only, at their head, as the original plays them.
        let at = self.host.client_state().and_then(|s| s.eye_position).unwrap_or([0.0; 3]);
        let event = horizon_bingo::sounds::event_id(key);
        let ok = self.host.play_local_sound(&event, at, volume, pitch, 0);
        log(&format!("sound {event} volume {volume} pitch {pitch}: {}", if ok { "played" } else { "no such sound event" }));
    }
}

/// The client config file as the game read it or read it again: the crate takes its text, and its path is kept for the
/// writes. Other mods' configs, and this mod's server configs, are not this file.
fn read_client_config(hooks: &Mod, file: &NativeConfigFile) {
    if file.mod_id != MOD_ID || file.kind != NativeConfigKind::Client {
        return;
    }
    *CONFIG_PATH.lock().unwrap_or_else(PoisonError::into_inner) = Some(file.path.clone());
    let config = ClientConfig::from_json(&file.text);
    guarded("client config", || hooks.lock_client().set_config(config));
}

/// Horizon's `Host` as a read-only crate `Host`: the screen hooks get a `&dyn Host`, so nothing here can send or open.
struct ReadOnly<'a> {
    host: &'a dyn Host,
    batch: Option<&'a mut HudBatch>,
}

impl BingoHost for ReadOnly<'_> {
    fn send_to_server(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Err("read-only host".into())
    }
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Argb) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.fill(batch, [x as f32, y as f32, w as f32, h as f32], color);
        }
    }
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: Argb, shadow: bool) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.text(batch, text, [x as f32, y as f32], color & 0x00FF_FFFF, shadow);
        }
    }
    fn text_width(&self, text: &str) -> i32 {
        cached_width(self.host, text)
    }
    fn draw_text_xf(&mut self, text: &str, anchor: (f32, f32), offset: (f32, f32), scale: f32, rotation: f32, color: Argb, shadow: bool) {
        if let Some(batch) = self.batch.as_mut() {
            let start = batch.vertices.len();
            self.host.text(batch, text, [offset.0, offset.1], color & 0x00FF_FFFF, shadow);
            transform_since(batch, start, anchor, scale, rotation);
        }
    }

    fn fill_square_xf(&mut self, cx: f32, cy: f32, half: f32, rotation: f32, color: Argb) {
        if let Some(batch) = self.batch.as_mut() {
            let start = batch.vertices.len();
            self.host.fill(batch, [-half, -half, 2.0 * half, 2.0 * half], color);
            transform_since(batch, start, (cx, cy), 1.0, rotation);
        }
    }
    fn upload_image(&mut self, _: u32, _: u32, _: &[u8]) -> bool {
        false
    }
    fn draw_sprite(&mut self, src: [f32; 4], dst: [f32; 4]) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.sprite(batch, MOD_IMAGE_SPRITE, src, dst);
        }
    }
    fn draw_vanilla_nine_slice(&mut self, sprite: &str, x: f32, y: f32, w: f32, h: f32, argb: Argb) {
        if let Some(batch) = self.batch.as_mut() {
            // The GUI atlas and its `gui.scaling` metadata are keyed by the full path (`gui/sprites/widget/tab`), and
            // `native_gui_nine_slice` looks the name up as given; the crate names vanilla widgets `widget/...`.
            let full = match sprite.starts_with("gui/sprites/") {
                true => sprite.to_string(),
                false => format!("gui/sprites/{sprite}"),
            };
            self.host.gui_nine_slice(batch, &full, [x, y, w, h], argb);
        }
    }
    fn draw_tooltip(&mut self, lines: &[String], x: i32, y: i32) {
        if let Some(batch) = self.batch.as_mut() {
            let texts: Vec<_> = lines.iter().map(|l| Text::literal(l.clone())).collect();
            self.host.gui_tooltip(batch, &texts, [x as f32, y as f32]);
        }
    }
    fn draw_item(&mut self, name: &str, x: f32, y: f32, size: f32) {
        if let Some(batch) = self.batch.as_mut() {
            self.host.item(batch, name, [x, y], size);
        }
    }
    fn begin_alpha(&mut self) -> usize {
        self.batch.as_ref().map_or(0, |batch| batch.vertices.len())
    }
    fn end_alpha(&mut self, start: usize, alpha: f32) {
        if let Some(batch) = self.batch.as_mut() {
            fade_since(batch, start, alpha);
        }
    }
    fn gui_width(&self) -> Option<i32> {
        gui_of(self.host).map(|g| g.0)
    }
    fn gui_size(&self) -> Option<(i32, i32)> {
        gui_of(self.host)
    }
    fn cursor(&self) -> (i32, i32) {
        mouse_of(self.host)
    }
    fn is_playing_screen(&self) -> bool {
        self.host.client_state().is_some_and(|s| matches!(s.screen, NativeScreenKind::Playing))
    }
    fn key_down(&self, key: &str) -> bool {
        key_code(key).is_some_and(|code| self.host.is_key_down(code))
    }
    fn sneaking(&self) -> bool {
        self.host.sneaking()
    }
    fn chat_open(&self) -> bool {
        self.host.client_state().is_some_and(|s| matches!(s.screen, NativeScreenKind::Chat))
    }
    fn hud_hidden(&self) -> bool {
        self.host.client_state().is_some_and(|s| s.gui_hidden)
    }
    fn debug_screen(&self) -> bool {
        self.host.client_state().is_some_and(|s| s.debug_screen)
    }
    fn tab_list_down(&self) -> bool {
        self.host.is_key_down(KeyCode::Tab)
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
        self.host.mod_screen() == Some(MOD_ID)
    }
    fn play_sound(&mut self, _: SoundKey, _: f32, _: f32) {}
}

fn mod_screen_up(host: &dyn Host) -> bool {
    host.mod_screen() == Some(MOD_ID)
}

fn width_fn(host: &dyn Host) -> impl Fn(&str) -> i32 + '_ {
    move |s: &str| cached_width(host, s)
}

impl Hooks for Mod {
    fn client_tick_post(&self, host: &mut dyn Host, _: &mut ClientTickPostEvent) {
        guarded("client tick", || {
            let mut client = self.lock_client();
            if !self.key_bound.swap(true, std::sync::atomic::Ordering::Relaxed) {
                let (assets, dir) = load_assets(&*host);
                match &assets {
                    Ok(a) => log(&format!("assets from {dir}: {} sprites in a {}x{} atlas", a.atlas.len(), a.atlas.width, a.atlas.height)),
                    Err(e) => log(&format!("assets NOT loaded from {dir}: {e}")),
                }
                client.set_assets(assets);
                // Registers the key in the player's controls (default Y) and reads what it is bound to.
                let key = host.mod_key(MOD_ID, "open_card", "key.keyboard.y");
                client.set_open_key(&key);
                log(&format!("card key: {key}"));
                // The HUD toggle: unbound by default, bound in the player's controls like the card key.
                let toggle = host.mod_key(MOD_ID, "toggle_hud", "key.keyboard.unknown");
                client.set_toggle_key(&toggle);
                // The settings screen's key, unbound by default like the HUD toggle.
                let settings = host.mod_key(MOD_ID, "settings", "key.keyboard.unknown");
                client.set_settings_key(&settings);
                forget_widths();
            }
            client.on_tick(&mut Adapter { host, batch: None });
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

    fn client_packet(&self, _host: &mut dyn Host, event: &mut ClientPacketEvent<'_>) {
        if let ClientboundPlayPacket::CustomPayload(p) = event.packet {
            dump(&p.channel, &p.data);
            guarded("payload", || {
                let _ = self.lock_client().on_packet(&p.channel, &p.data);
            });
        }
    }

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        if mod_screen_up(host) {
            // The original's screens sit over vanilla's blurred menu background.
            host.apply_screen_blur();
            guarded("screen draw", || {
                self.lock_client().render_screen(&mut ReadOnly { host, batch: Some(&mut *event.batch) });
            });
        }
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        if !mod_screen_up(host) {
            return Flow::Continue;
        }
        if event.button == 0 {
            if let Some(gui) = gui_of(host) {
                let width = width_fn(host);
                if guarded("screen press", || self.lock_client().screen_press(gui, mouse_of(host), &width)).is_none() {
                    return Flow::Continue;
                }
            }
        }
        Flow::Cancel
    }

    fn screen_mouse_button_released_pre(&self, host: &dyn Host, _: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
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

    fn screen_mouse_scrolled_pre(&self, host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
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

    fn screen_mouse_dragged_pre(&self, host: &dyn Host, _: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
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

    fn screen_key_pressed_pre(&self, host: &dyn Host, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
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

    fn gui_layer_post(&self, host: &mut dyn Host, event: &mut GuiLayerPostEvent<'_>) {
        if event.layer == OVERLAY_LAYER {
            guarded("overlay draw", || {
                self.lock_client().draw(&mut Adapter { host, batch: Some(&mut *event.batch) });
            });
        }
    }

    /// The client config file: registered when the game starts, with the crate's defaults written when it is missing.
    fn register_configs(&self, _host: &mut dyn Host, event: &mut RegisterConfigsEvent<'_>) {
        let default = ClientConfig::default().to_json();
        event.configs.push(NativeConfig::new(MOD_ID, NativeConfigKind::Client, &default).file(CONFIG_FILE));
    }

    fn config_loading(&self, _host: &mut dyn Host, event: &mut ConfigLoadingEvent<'_>) {
        read_client_config(self, event.config);
    }

    fn config_reloading(&self, _host: &mut dyn Host, event: &mut ConfigReloadingEvent<'_>) {
        read_client_config(self, event.config);
    }

    /// The window leaves its connection: the server's state (cards, scores, messages) belongs to that server only.
    fn client_disconnecting(&self, _host: &dyn Host, _: &mut ClientDisconnectingEvent) {
        guarded("disconnect", || {
            self.lock_client().state.reset_all();
        });
        forget_widths();
    }

    /// The window logs in to a server: a new server starts from an empty state, as after a disconnect.
    fn client_player_network_logging_in(&self, _host: &mut dyn Host, _: &mut ClientPlayerNetworkLoggingInEvent) {
        guarded("login", || {
            self.lock_client().state.reset_all();
        });
        forget_widths();
    }
}

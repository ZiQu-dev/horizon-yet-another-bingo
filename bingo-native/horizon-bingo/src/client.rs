//! The client half as one object the host drives: it registers the mod's channels with the server, feeds packets to
//! the [`HudState`], forwards sounds, and draws. All of it runs against the [`Host`] trait, so the whole loop is
//! tested with a fake (tests/client.rs); the mod crate only wires Horizon's hooks to these four methods.

use std::collections::BTreeMap;

use crate::assets::Assets;
use crate::atlas::Atlas;
use crate::card_images::ImageStore;
use crate::buf::{encode, DecodeError};
use crate::card_draw::{draw_card, translate, truncate_to_width, CardView, Draw, CARD_HEIGHT, CARD_WIDTH};
use crate::config::{alignment_xy, card_pos, card_scale, message_scale, ClientConfig};
use crate::fireworks::Fireworks;
use crate::message_draw::{draw_messages, MessageView};
use crate::ready_draw::draw_ready;
use crate::channels::{id, Dir, CHANNELS};
use crate::host::Host;
use crate::hud_tooltip::LookTooltip;
use crate::hud_keys::{hud_visibility, HudKeys, HudView, KeyAction, KeyInput};
use crate::game_over::EndScreenTab;
use crate::ready::SetReadyPacket;
use crate::screen_controller::ScreenController;
use crate::screen_placement::{
    draw_placement, Action, ClientConfig as PlacementConfig, Outcome, Placement, PlacementDraw, PlacementView,
};
use crate::settings_screen::{SettingsScreen, SettingsView};
use crate::screen_main::Effect;
use crate::state::{ClientEvent, Handled, HudState};
use crate::text_plain::flatten;
use crate::ui_draw::{run as run_ui, UiDraw};
use crate::update_settings::PlayerSettings;

/// The client's tick is 50 ms (20 ticks a second); the state's clock is `ticks * 50`.
pub const TICK_MS: u64 = 50;

/// Which of the mod's screens is open. Both are the mod's own screen (`mod_screen_open`), so at most one is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    /// The card screen (`ScreenController`).
    Card,
    /// The settings screen of the client config (`SettingsScreen`).
    Settings,
    /// The card placement screen (`BingoCardPlacementScreen`), opened from the settings screen's card position row. It
    /// replaces the settings screen on the same mod screen; the settings screen comes back when it is done.
    Placement,
}

/// The card placement screen while it is open, with the window size its card was placed for.
#[derive(Debug)]
struct PlacementScreen {
    state: Placement,
    window: (i32, i32),
}

/// The plugin channel on which a client tells a server which channels it understands (Bukkit-style, which Fabric's
/// server also accepts in the play phase: tested against the real server, see DESIGN.md C1).
pub const REGISTER_CHANNEL: &str = "minecraft:register";
/// A server's own announcement of its channels arrives on the same channel at every join.
const ANNOUNCE_GRACE_TICKS: u64 = 40;
/// The grace period, for the tests.
pub const ANNOUNCE_FOR_TESTS: u64 = ANNOUNCE_GRACE_TICKS;
/// How many decode failures are kept for the log.
const MAX_ERRORS: usize = 32;

/// The channel names a client registers: every channel the server sends, NUL-separated (`minecraft:register`).
pub fn register_payload() -> Vec<u8> {
    let names: Vec<String> = CHANNELS.iter().filter(|c| matches!(c.dir, Dir::S2c | Dir::Both)).map(|c| id(c.path)).collect();
    names.join("\0").into_bytes()
}

#[derive(Debug, Default)]
pub struct BingoClient {
    pub state: HudState,
    ticks: u64,
    need_register: bool,
    registered_at: Option<u64>,
    settings_sent: bool,
    /// The player's stored settings, sent on registration; the defaults until the host sets them.
    settings: Option<PlayerSettings>,
    /// The open-card and end-screen events of this tick, for `tick_screen`. Sounds, fireworks and redraws are not
    /// kept here: nothing needs them after the tick.
    ui_events: Vec<ClientEvent>,
    /// The win fireworks over the HUD card (`BingoHudController.fireworkRenderer`), until every star is gone.
    hud_fireworks: Option<Fireworks>,
    /// How many payloads of each channel were applied.
    pub applied: BTreeMap<String, u32>,
    /// Payloads that did not decode: channel and the reason (the newest 32).
    pub errors: Vec<(String, String)>,
    assets: Option<Assets>,
    /// Why the original mod's files could not be loaded, shown instead of the card.
    pub asset_error: Option<String>,
    atlas_uploaded: bool,
    /// The ready bar's smoothed progress, kept between frames as the original does.
    ready_progress: f32,
    pub screen: ScreenController,
    image_store: ImageStore,
    /// The mod's sprites with the card pictures packed in, once any picture has arrived.
    dynamic_atlas: Option<Atlas>,
    pending_effects: Vec<Effect>,
    open_key: String,
    /// The card key, the HUD toggle and sneak-ready, as the handler reads them each tick.
    hud_keys: HudKeys,
    /// `ClientConfig`: the host reads it from the config file (`set_config`); the HUD and the sounds follow it.
    config: ClientConfig,
    /// The settings screen changed the config: the host is told on the next tick (`Host::config_changed`).
    config_dirty: bool,
    /// The settings screen, open while `Some`. It is the mod's own screen, as the card screen is.
    settings_screen: Option<SettingsScreen>,
    /// The card placement screen, open over the settings screen while `Some`: the settings screen waits under it.
    placement_screen: Option<PlacementScreen>,
    /// The key that opens the settings screen (unbound by default), and whether it was held at the last tick.
    settings_key: String,
    settings_key_was_down: bool,
    /// The player tooltip: the player under the crosshair for 20 ticks (`TooltipHudRenderer`).
    look_tooltip: LookTooltip,
    /// The key that toggles the HUD (unbound by default). It flips `config.enable_hud`, the one HUD flag.
    toggle_hud_key: String,
    /// Heads asked for: the tick of the last try and how many tries there were. A skin may not be downloaded yet, so a
    /// miss is tried again later, up to [`FACE_MAX_TRIES`]. Entries of heads no longer shown are dropped.
    face_tried: std::collections::HashMap<u128, (u64, u32)>,
    /// What the last look at the heads counted (messages, then the game over and its teams), and whether a head was
    /// still waiting for its skin.
    faces_seen: usize,
    faces_pending: bool,
}

/// Ticks between two looks for new player heads, and between two tries for the same head.
const FACE_SCAN_TICKS: u64 = 10;
const FACE_RETRY_TICKS: u64 = 60;
/// Tries per head before the client gives up on its skin (until the head leaves the messages).
const FACE_MAX_TRIES: u32 = 5;
/// The HUD's win fireworks fade out at `100 * cardScale` pixels (`BingoHudController`): this is the range at scale 1.
const HUD_FIREWORK_RANGE: f64 = 100.0;
/// The seed of the HUD's fireworks, mixed with their start time so they differ from the end screen's.
const HUD_FIREWORK_SEED: u64 = 0x2545_F491_4F6C_DD1D;

impl BingoClient {
    pub fn new() -> Self {
        let mut client = BingoClient {
            need_register: true,
            open_key: "key.keyboard.y".into(),
            toggle_hud_key: "key.keyboard.unknown".into(),
            settings_key: "key.keyboard.unknown".into(),
            ..Self::default()
        };
        // Shuffles differ between sessions: the order source starts from the clock.
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        client.state.set_shuffle_seed(nanos);
        client
    }

    /// Binds the key that opens the card screen (vanilla's key name; the host reads it from the player's controls).
    pub fn set_open_key(&mut self, key: &str) {
        self.open_key = key.to_string();
    }

    pub fn open_key(&self) -> &str {
        &self.open_key
    }

    /// Binds the key that toggles the HUD (vanilla's key name; `key.keyboard.unknown` when unbound).
    pub fn set_toggle_key(&mut self, key: &str) {
        self.toggle_hud_key = key.to_string();
    }

    /// Binds the key that opens the settings screen (vanilla's key name; `key.keyboard.unknown` when unbound).
    pub fn set_settings_key(&mut self, key: &str) {
        self.settings_key = key.to_string();
    }

    /// The client config as the HUD and the sounds use it (the defaults until `set_config`).
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Replaces the client config, as read from the config file. The host is not told: it is the file's own copy.
    ///
    /// Not while a change is still unsaved: the file the host reads back then holds the text from before the change
    /// (the file is written on the next tick), and taking it would undo the change, which the tick would then save.
    /// The save's own reload brings the same config, so nothing is lost.
    pub fn set_config(&mut self, config: ClientConfig) {
        if !self.config_dirty {
            self.config = config;
        }
    }

    /// Which of the mod's screens is up, if any.
    pub fn screen_kind(&self) -> Option<ScreenKind> {
        if self.placement_screen.is_some() {
            Some(ScreenKind::Placement)
        } else if self.settings_screen.is_some() {
            Some(ScreenKind::Settings)
        } else if self.screen.is_open() {
            Some(ScreenKind::Card)
        } else {
            None
        }
    }

    /// The mod's own screen is up (either kind): the HUD is not drawn under it, and the game gets no keys for it.
    fn mod_screen_open(&self) -> bool {
        self.screen_kind().is_some()
    }

    /// Opens the settings screen through the host's mod screen. False when a screen of the mod's is up already, when
    /// the original's files are not loaded (the screen's text comes from them, and an empty screen would block the
    /// game's input) or when the host refuses.
    pub fn open_settings(&mut self, host: &mut dyn Host) -> bool {
        if self.mod_screen_open() || self.assets.is_none() || !host.open_screen() {
            return false;
        }
        self.settings_screen = Some(SettingsScreen::new());
        true
    }

    /// The player's stored settings, sent to the server as `update_settings` once the channels are registered (the
    /// defaults until this is called). Called after that, they go out again on the next tick.
    pub fn set_settings(&mut self, settings: PlayerSettings) {
        self.settings = Some(settings);
        self.settings_sent = false;
    }

    /// The original mod's files, loaded by the host from the prepared folder (or why that failed).
    pub fn set_assets(&mut self, assets: Result<Assets, String>) {
        match assets {
            Ok(a) => {
                self.assets = Some(a);
                self.asset_error = None;
            }
            Err(e) => {
                self.assets = None;
                self.asset_error = Some(e);
            }
        }
        self.dynamic_atlas = None;
        self.image_store = ImageStore::default();
        self.face_tried.clear();
        self.atlas_uploaded = false;
    }

    /// The sprites to draw from: the mod's own, plus the card pictures when there are any.
    fn atlas(&self) -> Option<&Atlas> {
        self.dynamic_atlas.as_ref().or(self.assets.as_ref().map(|a| &a.atlas))
    }

    /// The item identifiers of the selected card's tiles (for logs and tests).
    pub fn tile_ids(&self) -> Vec<String> {
        match self.state.selected_card() {
            Some((_, card)) => card.tiles.iter().filter_map(|t| t.id.clone()).collect(),
            None => Vec::new(),
        }
    }

    pub fn has_assets(&self) -> bool {
        self.assets.is_some()
    }

    /// True while the next look at the heads comes back for one: a skin is missing and has tries left.
    pub fn faces_waiting(&self) -> bool {
        self.faces_pending
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Call once a client tick. Registers the channels as soon as the player has joined, then plays queued sounds.
    pub fn on_tick(&mut self, host: &mut dyn Host) {
        self.ticks += 1;
        self.state.set_now(self.ticks * TICK_MS);
        // The player under the crosshair, for the tooltip (`TooltipHudRenderer.tick`).
        self.look_tooltip.update(host.looked_at_player(), &self.state, self.ticks, self.state.now());
        if self.need_register && host.send_to_server(REGISTER_CHANNEL, &register_payload()).is_ok() {
            self.need_register = false;
            self.registered_at = Some(self.ticks);
            self.settings_sent = false;
        }
        // The original client sends its settings as soon as the channels are registered (ClientSettingsController).
        if !self.need_register && !self.settings_sent {
            let settings = self.settings.clone().unwrap_or_default();
            let payload = encode(|w| settings.write(w));
            if host.send_to_server(&id("update_settings"), &payload).is_ok() {
                self.settings_sent = true;
            }
        }
        // The heads of players that messages show: the host gives their faces, which go into the atlas too.
        // A game over counts as one, with each of its teams: its names carry heads too, and it may arrive alone.
        let game_over_count = self.state.game_over.as_ref().map_or(0, |g| 1 + g.scores.len());
        let seen = self.state.messages.len() + self.state.past_messages.len() + game_over_count;
        let look = self.ticks % FACE_SCAN_TICKS == 0 && (seen != self.faces_seen || self.faces_pending);
        let faces_changed = self.assets.is_some() && look && self.fetch_faces(host, seen);
        // Pictures that arrived in `card_image` packets go into the atlas with the mod's sprites.
        let pictures_changed = self.assets.is_some() && self.image_store.sync(&self.state.images);
        if pictures_changed || faces_changed {
            if let Some(base) = self.assets.as_ref().map(|a| &a.atlas) {
                if let Ok(atlas) = self.image_store.build_atlas(base) {
                    self.dynamic_atlas = Some(atlas);
                    self.atlas_uploaded = false;
                }
            }
        }
        // The picture goes up once the HUD slot exists (Horizon refuses it before the window is ready).
        if !self.atlas_uploaded {
            if let Some(atlas) = self.atlas() {
                let ok = host.upload_image(atlas.width, atlas.height, &atlas.rgba);
                self.atlas_uploaded = ok;
            }
        }
        // The settings changed the config since the last tick: the host saves it.
        if std::mem::take(&mut self.config_dirty) {
            host.config_changed(&self.config.to_json());
        }
        self.flush_events(host);
        self.tick_screen(host);
    }

    /// Asks the host for the face of each player head the messages and the game over's team names show, and the store
    /// lacks. True when one was added. `seen` is what the look counted, kept for the next look.
    ///
    /// A head is pending (the next look comes back for it) only while it may still arrive: its skin is missing and it
    /// has fewer than [`FACE_MAX_TRIES`] tries. A face the store refuses (it is full) is not pending.
    fn fetch_faces(&mut self, host: &mut dyn Host, seen: usize) -> bool {
        self.faces_seen = seen;
        self.faces_pending = false;
        let mut wanted: std::collections::HashSet<u128> = std::collections::HashSet::new();
        let messages = self.state.messages.iter().chain(&self.state.past_messages).map(|m| &m.packet.message);
        let names = self.state.game_over.iter().flat_map(|g| g.scores.iter().map(|s| &s.name));
        for text in messages.chain(names) {
            wanted.extend(crate::text_plain::heads(text));
        }
        // The heads no longer shown are forgotten, so the record is as large as the heads on screen.
        self.face_tried.retain(|uuid, _| wanted.contains(uuid));
        let mut changed = false;
        for &uuid in &wanted {
            if self.image_store.has_face(uuid) {
                continue;
            }
            let tries = match self.face_tried.get(&uuid) {
                // Given up on: the skin did not come in time, and asking again would only repeat that.
                Some(&(_, tries)) if tries >= FACE_MAX_TRIES => continue,
                Some(&(at, _)) if self.ticks - at < FACE_RETRY_TICKS => {
                    self.faces_pending = true;
                    continue;
                }
                Some(&(_, tries)) => tries,
                None => 0,
            };
            self.face_tried.insert(uuid, (self.ticks, tries + 1));
            match host.player_face(uuid) {
                // A refused face (the store is full) is not pending: the scan does not come back for it alone.
                Some(rgba) => changed |= self.image_store.set_face(uuid, rgba),
                // The skin may still be on its way: look again later, unless this was the last try.
                None => {
                    if tries + 1 < FACE_MAX_TRIES {
                        self.faces_pending = true;
                    }
                }
            }
        }
        changed
    }

    fn tick_screen(&mut self, host: &mut dyn Host) {
        // The screen was closed by the game (its own Escape handling, a disconnect): forget it.
        if self.screen.is_open() && !host.screen_is_open() {
            self.screen.closed_by_host();
        }
        if self.settings_screen.is_some() && !host.screen_is_open() {
            self.settings_screen = None;
            self.placement_screen = None;
        }
        let now = self.state.now();
        let mut open: Option<(EndScreenTab, bool)> = None;
        for event in std::mem::take(&mut self.ui_events) {
            match event {
                ClientEvent::OpenCard(team) => {
                    self.state.selected_team = team;
                    open = Some((EndScreenTab::Cards, true));
                }
                ClientEvent::OpenEndScreen => {
                    let tab = self.state.game_over.as_ref().map_or(EndScreenTab::Cards, |g| g.default_tab);
                    open = Some((tab, false));
                }
                // `flush_events` keeps only the open events.
                ClientEvent::Sound { .. } | ClientEvent::Fireworks | ClientEvent::HudChanged => {}
            }
        }
        // The settings key opens the settings screen on its press, while the player is playing.
        let settings_down = host.key_down(&self.settings_key);
        let settings_pressed = settings_down && !self.settings_key_was_down;
        self.settings_key_was_down = settings_down;
        if settings_pressed && host.is_playing_screen() {
            self.open_settings(host);
        }
        // The card key, the HUD toggle and sneak-ready, as `BingoHudController` reads them each tick.
        let input = KeyInput {
            open_card_down: host.key_down(&self.open_key),
            toggle_hud_down: host.key_down(&self.toggle_hud_key),
            sneak_down: host.sneaking(),
            chat_screen_open: host.chat_open(),
            ..KeyInput::default()
        };
        for action in self.hud_keys.tick(input, self.state.ready.as_ref()) {
            match action {
                // `openCardScreen` when there is something to show, and no other screen is up.
                KeyAction::OpenCard => {
                    if host.is_playing_screen() && !self.mod_screen_open() && (!self.state.cards.is_empty() || self.state.game_over.is_some()) {
                        open = Some((EndScreenTab::Cards, true));
                    }
                }
                // `SetReadyPacket(is_ready)`, as V2 (the option is not chosen from the HUD).
                KeyAction::SendReady(is_ready) => {
                    let payload = encode(|w| SetReadyPacket { is_ready, option_id: None }.write_v2(w));
                    let _ = host.send_to_server(&id("set_ready_v2"), &payload);
                }
                // The config's flag, as the settings screen flips it: the host is told on the next tick.
                KeyAction::ToggleHud => {
                    self.config.enable_hud = !self.config.enable_hud;
                    self.config_dirty = true;
                }
            }
        }
        if let Some((tab, can_escape)) = open {
            if !self.mod_screen_open() && host.open_screen() {
                self.screen.open(now, tab, can_escape);
                // Opened from the HUD (the key, not the end of a game): the selected card slides in from its HUD place.
                if can_escape {
                    if let Some((key, _)) = self.state.selected_card() {
                        let key = key.clone();
                        self.screen.set_entrance(key);
                    }
                }
                // The end screen of a game the player's team won opens with fireworks (only the first time).
                if !can_escape && self.state.game_over.as_ref().is_some_and(|g| g.is_winner) {
                    self.screen.start_fireworks(now, now ^ 0x5DEE_CE66_D1CE_4E5B);
                }
            }
        }
        if self.screen.is_open() {
            self.screen.tick();
        }
        // The settings screen's test button plays its sound at the configured volume; Done (or Escape) closes it.
        if let Some(settings) = self.settings_screen.as_mut() {
            if let Some(id) = settings.take_test_sound() {
                host.play_test_sound(id, self.config.sound_volume(id));
            }
        }
        if self.settings_screen.as_ref().is_some_and(|s| s.is_done()) {
            self.settings_screen = None;
            host.close_screen();
        }
        if self.screen.take_close_request() {
            host.close_screen();
            self.screen.closed_by_host();
        }
        for effect in std::mem::take(&mut self.pending_effects) {
            if let Effect::SendReady(vote) = effect {
                let payload = encode(|w| SetReadyPacket { is_ready: true, option_id: vote }.write_v2(w));
                let _ = host.send_to_server(&id("set_ready_v2"), &payload);
            }
        }
    }

    /// Draws the open mod screen (inside `screen_render_post`): the card screen, the settings screen, or the placement
    /// screen over the settings screen.
    pub fn render_screen(&mut self, host: &mut dyn Host) {
        let atlas = self.dynamic_atlas.as_ref().or(self.assets.as_ref().map(|a| &a.atlas));
        let (Some(assets), Some(atlas), Some(gui)) = (&self.assets, atlas, host.gui_size()) else { return };
        let mouse = host.cursor();
        match self.screen_kind() {
            Some(ScreenKind::Placement) => {
                let lang = |k: &str| assets.translate(k);
                let width_of = |s: &str| host.text_width(s);
                let colors = assets.team_colors(None, None);
                let now_ms = self.state.now();
                let Some(placement) = self.placement_screen.as_mut() else { return };
                if placement.window != gui {
                    placement.window = gui;
                    placement.state.resize(gui);
                }
                let view = PlacementView { now_ms, chat_scale: 1.0, colors: &colors, items: assets, lang: &lang, text_width: &width_of };
                let frame = draw_placement(&placement.state, &view, (mouse.0 as f64, mouse.1 as f64));
                let cmds: Vec<UiDraw> = frame.draws.iter().map(placement_ui).collect();
                run_ui(&cmds, atlas, host);
            }
            Some(ScreenKind::Settings) => {
                let lang = |k: &str| assets.translate(k);
                let width_of = |s: &str| host.text_width(s);
                let view = SettingsView { lang: &lang, text_width: &width_of };
                let cmds = self.settings_screen.as_mut().map(|s| s.frame(&self.config, (mouse.0 as f64, mouse.1 as f64), gui, &view));
                if let Some(cmds) = cmds {
                    run_ui(&cmds, atlas, host);
                }
            }
            _ => self.screen.render(&self.state, assets, atlas, host, gui, mouse),
        }
    }

    pub fn screen_press(&mut self, gui: (i32, i32), mouse: (i32, i32), width_of: &dyn Fn(&str) -> i32) {
        match self.screen_kind() {
            Some(ScreenKind::Placement) => self.placement_press(gui, mouse, width_of),
            Some(ScreenKind::Settings) => {
                let at = (mouse.0 as f64, mouse.1 as f64);
                if let Some(settings) = self.settings_screen.as_mut() {
                    if settings.click(&mut self.config, at) {
                        self.config_dirty = true;
                    }
                }
                self.start_placement_if_requested(gui);
            }
            _ => {
                let Some(assets) = &self.assets else { return };
                let effects = self.screen.mouse_press(&mut self.state, assets, width_of, gui, mouse);
                self.pending_effects.extend(effects);
            }
        }
    }

    /// The wheel. The placement screen has nothing to scroll, and it takes the wheel all the same.
    pub fn screen_wheel(&mut self, mouse: (i32, i32), amount: f64) -> bool {
        if self.placement_screen.is_some() {
            return true;
        }
        if let Some(settings) = self.settings_screen.as_mut() {
            settings.scroll(amount);
            return true;
        }
        self.screen.mouse_wheel(mouse, amount)
    }

    pub fn screen_drag(&mut self, mouse: (i32, i32)) -> bool {
        if let Some(placement) = self.placement_screen.as_mut() {
            placement.state.mouse_dragged((mouse.0 as f64, mouse.1 as f64));
            return true;
        }
        if let Some(settings) = self.settings_screen.as_mut() {
            let at = (mouse.0 as f64, mouse.1 as f64);
            if settings.drag(&mut self.config, at) {
                self.config_dirty = true;
            }
            return true;
        }
        self.screen.mouse_drag(mouse)
    }

    pub fn screen_release(&mut self) -> bool {
        if let Some(placement) = self.placement_screen.as_mut() {
            placement.state.mouse_released();
            return true;
        }
        if let Some(settings) = self.settings_screen.as_mut() {
            settings.release();
            return true;
        }
        self.screen.mouse_release()
    }

    /// An arrow key on the open screen; true when the screen used it (the game must not). The placement screen and the
    /// settings screen take no keys.
    pub fn screen_key(&mut self, name: &str) -> bool {
        if self.placement_screen.is_some() || self.settings_screen.is_some() {
            return false;
        }
        self.screen.key(name)
    }

    /// Escape on the open screen: true when the screen took it. The card closes itself with an animation; the settings
    /// screen closes on the next tick, as Done does. The placement screen cancels: it returns to the settings screen
    /// without writing its card position.
    pub fn screen_escape(&mut self) -> bool {
        if let Some(placement) = self.placement_screen.as_mut() {
            let outcome = placement.state.press(Action::Cancel);
            if let Some(outcome) = outcome {
                self.finish_placement(outcome);
            }
            return true;
        }
        if let Some(settings) = self.settings_screen.as_mut() {
            settings.escape();
            return true;
        }
        let now = self.state.now();
        self.screen.escape(now)
    }

    /// The settings screen's card position row was clicked: the placement screen takes over the mod's screen, edits a
    /// copy of the card's placement, and the settings screen waits under it. Nothing is closed or opened.
    fn start_placement_if_requested(&mut self, gui: (i32, i32)) {
        let requested = self.settings_screen.as_mut().is_some_and(|s| s.take_placement_request());
        if requested && self.placement_screen.is_none() {
            let edit = PlacementConfig::from_config(&self.config);
            self.placement_screen = Some(PlacementScreen { state: Placement::open(&edit, gui), window: gui });
        }
    }

    /// The placement screen ended. Saved (`Ok`) writes its four placement fields into the config and marks it changed,
    /// so the host is told once on the next tick. Cancelled changes nothing. Either way the settings screen is back.
    fn finish_placement(&mut self, outcome: Outcome) {
        if self.placement_screen.take().is_none() {
            return;
        }
        if let Outcome::Ok(edit) = outcome {
            edit.write_placement(&mut self.config);
            self.config_dirty = true;
        }
    }

    /// A click on the placement screen: one of its two buttons ends it, a click on the card does nothing (a drag moves
    /// or resizes the card, see `screen_drag`).
    fn placement_press(&mut self, gui: (i32, i32), mouse: (i32, i32), width_of: &dyn Fn(&str) -> i32) {
        let Some(assets) = &self.assets else { return };
        let at = (mouse.0 as f64, mouse.1 as f64);
        let lang = |k: &str| assets.translate(k);
        let colors = assets.team_colors(None, None);
        let now_ms = self.state.now();
        let Some(placement) = self.placement_screen.as_mut() else { return };
        if placement.window != gui {
            placement.window = gui;
            placement.state.resize(gui);
        }
        let view = PlacementView { now_ms, chat_scale: 1.0, colors: &colors, items: assets, lang: &lang, text_width: width_of };
        let frame = draw_placement(&placement.state, &view, at);
        let button = frame.hits.iter().find(|h| h.contains(at)).map(|h| h.action);
        if let Some(action @ (Action::Cancel | Action::Save)) = button {
            let outcome = placement.state.press(action);
            if let Some(outcome) = outcome {
                self.finish_placement(outcome);
            }
        }
    }

    /// A custom payload from the server, in the order it came.
    pub fn on_packet(&mut self, channel: &str, data: &[u8]) {
        if channel == REGISTER_CHANNEL {
            // The server announced its channels: a new join. Register again, unless we just did.
            let fresh = self.registered_at.is_some_and(|at| self.ticks.saturating_sub(at) <= ANNOUNCE_GRACE_TICKS);
            if !fresh {
                self.need_register = true;
            }
            return;
        }
        match self.state.on_packet(channel, data) {
            Ok(Handled::Applied { .. }) => *self.applied.entry(channel.to_string()).or_default() += 1,
            Ok(Handled::Ignored) => {}
            Err(e) => self.record_error(channel, &e),
        }
    }

    fn record_error(&mut self, channel: &str, error: &DecodeError) {
        if self.errors.len() == MAX_ERRORS {
            self.errors.remove(0);
        }
        self.errors.push((channel.to_string(), error.to_string()));
    }

    /// Plays the sounds, starts the HUD's fireworks, and keeps the open events for `tick_screen`. The rest is dropped:
    /// a redraw needs no event, since every screen draws each frame.
    fn flush_events(&mut self, host: &mut dyn Host) {
        let now = self.state.now();
        for event in self.state.take_events() {
            match event {
                ClientEvent::Sound { key, volume, pitch } => {
                    // The host scales the call's volume by the default (`sounds::actual_volume`). Multiplying by the
                    // configured volume over that default gives the configured volume, and exactly the call's volume
                    // at the default.
                    let factor = self.config.sound_volume(crate::sounds::name(key)) / crate::sounds::DEFAULT_VOLUME;
                    host.play_sound(key, volume * factor, pitch);
                }
                // `BingoHudController`: `cardCompletedV1` with `isWinner` starts the fireworks over the card.
                ClientEvent::Fireworks => {
                    let range = HUD_FIREWORK_RANGE * f64::from(card_scale(&self.config));
                    self.hud_fireworks = Some(Fireworks::with_range(now, now ^ HUD_FIREWORK_SEED, range));
                }
                ClientEvent::OpenCard(_) | ClientEvent::OpenEndScreen => self.ui_events.push(event),
                ClientEvent::HudChanged => {}
            }
        }
    }

    /// The open-card and end-screen events not yet handled. A tick handles them all, so this is empty between ticks.
    pub fn take_ui_events(&mut self) -> Vec<ClientEvent> {
        std::mem::take(&mut self.ui_events)
    }

    /// The lines of the status overlay: what the client currently knows.
    pub fn overlay_lines(&self) -> Vec<String> {
        let mut lines = vec!["Bingo".to_string()];
        match self.state.selected_card() {
            Some((team, card)) => {
                let named = card.tiles.iter().filter(|t| t.id.is_some()).count();
                lines.push(format!("card of {}: {} tiles", team.as_deref().unwrap_or("(no team)"), named));
            }
            None => lines.push("no card".to_string()),
        }
        if let Some(s) = self.state.timer_seconds {
            lines.push(format!("timer: {}:{:02}", s.max(0) / 60, s.max(0) % 60));
        }
        if let Some(status) = &self.state.game_status {
            if let Some(d) = status.ingame_duration {
                lines.push(format!("in game: {} s", d.seconds));
            }
        }
        if let Some((team, score)) = self.state.scores.iter().next() {
            lines.push(format!("{team}: {} items, {} lines", score.items, score.lines));
        }
        lines
    }

    /// Draws inside a HUD layer: the selected card at the top left (the mod's default placement) with the score
    /// messages under it, and the ready line, when the original mod's files are loaded; else the text overlay with
    /// the reason they are not.
    pub fn draw(&mut self, host: &mut dyn Host) {
        // `BingoHudController`: "Do not draw the HUD if the card screen is open" (the settings screen is the same).
        if self.mod_screen_open() {
            return;
        }
        if self.assets.is_none() || !self.atlas_uploaded {
            self.draw_text_overlay(host);
            return;
        }
        self.draw_hud(host);
    }

    fn run(host: &mut dyn Host, atlas: &Atlas, commands: Vec<Draw>, shadow: bool) {
        for command in commands {
            match command {
                Draw::Sprite { name, x, y, w, h } => {
                    if let Some(src) = atlas.rect(&name) {
                        host.draw_sprite(src, [x, y, w, h]);
                    }
                }
                Draw::Fill { x, y, w, h, argb } => host.fill_rect(x, y, w, h, argb),
                Draw::Text { x, y, text, argb } => host.draw_text(x, y, &text, argb, shadow),
                Draw::Item { name, x, y, size } => host.draw_item(&name, x, y, size),
                Draw::Count { text, right, y } => {
                    let x = right - host.text_width(&text);
                    host.draw_text(x, y, &text, 0xffff_ffff, true);
                }
            }
        }
    }

    fn draw_hud(&mut self, host: &mut dyn Host) {
        let Some(assets) = self.assets.take() else { return };
        let dynamic = self.dynamic_atlas.take();
        let atlas: &Atlas = dynamic.as_ref().unwrap_or(&assets.atlas);
        let known = |name: &str| atlas.contains(name);
        let lang = |k: &str| assets.translate(k);
        let now = self.state.now();
        // The card's place in the window (`BingoHudController.getCardPos`), and its scale. Without a window size the
        // place is the top left's, which is all a size-free corner needs.
        let gui = host.gui_size().unwrap_or((0, 0));
        let (card_x, card_y) = card_pos(&self.config, gui.0, gui.1);
        let scale = card_scale(&self.config);
        // `HudRenderEvent`'s gate: the F1 hide, a mod screen, F3, the chat and the Tab list (see `hud_keys`).
        let shown = hud_visibility(HudView {
            enable_hud: self.config.enable_hud,
            hide_on_f3: self.config.hide_on_f3,
            hide_on_chat: self.config.hide_on_chat,
            hud_hidden: host.hud_hidden(),
            own_screen_open: self.mod_screen_open(),
            debug_screen: host.debug_screen(),
            chat_open: host.chat_open(),
            tab_list_down: host.tab_list_down(),
        });
        // `drawHudCards` draws the win fireworks before the card, and only while a card is shown.
        if shown.card && self.state.selected_card().is_some() {
            // The fireworks are centred on the card's middle, wherever the card is drawn.
            let centre = (card_x + CARD_WIDTH as f32 * scale / 2.0, card_y + CARD_HEIGHT as f32 * scale / 2.0);
            self.draw_hud_fireworks(host, now, (centre.0.round() as i32, centre.1.round() as i32));
        }
        let mut team_name_draws = None;
        if let Some((key, card)) = self.state.selected_card().filter(|_| shown.card) {
            let formatting = card.display.team_color.as_deref();
            let colors = assets.team_colors(key.as_deref(), formatting);
            let team_name = card
                .display
                .team_name
                .as_ref()
                .map(|t| flatten(t, &lang))
                .map(|name| truncate_to_width(&name, 70, &|s| host.text_width(s)));
            // Every team's outline is looked up with the card's own colour, as the original does.
            let outline = |team: &str| assets.team_colors(Some(team), formatting).outline;
            let view = CardView { card, colors: &colors, now_ms: now, show_outlines: self.config.card_team_outlines, team_outline: &outline, items: &assets, images: Some(&known), team_name, is_gui: false };
            team_name_draws = Some(place_card(draw_card(&view), card_x, card_y, scale));
        }
        if let Some(commands) = team_name_draws {
            // The team name has no shadow; nothing else on the card is text.
            Self::run(host, atlas, commands, false);
            // `drawMessages(x = card x, y = card y + CARD_HEIGHT * scale * (1 - alignment.y))`: the messages are drawn
            // from the card's corner, and sit under the card (over it when the card is at the bottom).
            let alignment = alignment_xy(self.config.card_alignment);
            let width = |s: &str| host.text_width(s);
            let view = MessageView {
                now_ms: now,
                duration_seconds: self.config.message_duration_seconds,
                message_scale: message_scale(&self.config, 1.0),
                chat_scale: 1.0,
                card_alignment: alignment,
                card_scale: scale,
                items: &assets,
                lang: &lang,
                text_width: &width,
            };
            let commands = draw_messages(&mut self.state.messages, &view);
            let below = card_y + CARD_HEIGHT as f32 * scale * (1.0 - alignment.1 as f32);
            Self::run(host, atlas, translate(commands, card_x.round() as i32, below.round() as i32), true);
        }
        if let Some(screen_width) = host.gui_width().filter(|_| shown.ready) {
            let width = |s: &str| host.text_width(s);
            let commands = draw_ready(self.state.ready.as_ref(), now, screen_width, &lang, &width, &mut self.ready_progress);
            Self::run(host, atlas, commands, true);
        }
        // The player tooltip, drawn last (`drawHudInfo`), faded in and out as `TooltipHudRenderer` fades it.
        if shown.tooltip {
            if let Some(gui) = host.gui_size() {
                let width = |s: &str| host.text_width(s);
                let commands = self.look_tooltip.draw(&self.state, now, gui, &lang, &width);
                if !commands.is_empty() {
                    let alpha = self.look_tooltip.alpha(now);
                    let mark = host.begin_alpha();
                    run_ui(&commands, atlas, host);
                    host.end_alpha(mark, alpha);
                }
            }
        }
        self.dynamic_atlas = dynamic;
        self.assets = Some(assets);
    }

    /// The HUD's win fireworks, centred on the card's middle (`fireworkRenderer.render(centerX, centerY)` with the card's
    /// centre). Dropped once every star has faded, as the Kotlin does.
    fn draw_hud_fireworks(&mut self, host: &mut dyn Host, now: u64, centre: (i32, i32)) {
        let Some(fw) = self.hud_fireworks.as_mut() else { return };
        for command in fw.draw(now, centre.0, centre.1) {
            if let UiDraw::TextXf { text, anchor, offset, scale, rotation, argb, shadow } = command {
                host.draw_text_xf(&text, anchor, offset, scale, rotation, argb, shadow);
            }
        }
        if fw.is_done() {
            self.hud_fireworks = None;
        }
    }

    fn draw_text_overlay(&self, host: &mut dyn Host) {
        let mut lines = self.overlay_lines();
        if let Some(e) = &self.asset_error {
            lines.push(format!("assets: {e}"));
        }
        let width = lines.iter().map(|l| host.text_width(l)).max().unwrap_or(0);
        let height = lines.len() as i32 * 10 + 2;
        host.fill_rect(2, 2, width + 4, height, 0x78000000);
        for (i, line) in lines.iter().enumerate() {
            host.draw_text(4, 4 + i as i32 * 10, line, 0xFFFFFFFF, true);
        }
    }
}

/// One draw of the placement screen as the mod's `UiDraw`. Its text is drawn with a shadow, as vanilla draws the labels
/// of its buttons and the card's messages.
fn placement_ui(draw: &PlacementDraw) -> UiDraw {
    match draw {
        PlacementDraw::Draw(Draw::Text { x, y, text, argb }) => UiDraw::Text {
            x: *x as f32,
            y: *y as f32,
            text: text.clone(),
            argb: *argb,
            shadow: true,
            obfuscated: false,
        },
        PlacementDraw::Draw(other) => UiDraw::from(other),
        PlacementDraw::ScaledFill { x, y, w, h, argb } => UiDraw::Fill { x: *x, y: *y, w: *w, h: *h, argb: *argb },
    }
}
/// Moves the commands of a card drawn at (0, 0) to (x, y) at `scale`, as `screen_cards::place` does for the cards of the
/// card screen (private to that module). Glyphs keep their size, as the original's text does under `scale`.
fn place_card(draws: Vec<Draw>, x: f32, y: f32, scale: f32) -> Vec<Draw> {
    draws
        .into_iter()
        .map(|d| match d {
            Draw::Sprite { name, x: dx, y: dy, w, h } => {
                Draw::Sprite { name, x: x + dx * scale, y: y + dy * scale, w: w * scale, h: h * scale }
            }
            Draw::Fill { x: dx, y: dy, w, h, argb } => {
                let (x1, y1) = ((x + dx as f32 * scale).round() as i32, (y + dy as f32 * scale).round() as i32);
                let (x2, y2) = ((x + (dx + w) as f32 * scale).round() as i32, (y + (dy + h) as f32 * scale).round() as i32);
                Draw::Fill { x: x1, y: y1, w: x2 - x1, h: y2 - y1, argb }
            }
            Draw::Text { x: dx, y: dy, text, argb } => Draw::Text {
                x: (x + dx as f32 * scale).round() as i32,
                y: (y + dy as f32 * scale).round() as i32,
                text,
                argb,
            },
            Draw::Item { name, x: dx, y: dy, size } => {
                Draw::Item { name, x: x + dx * scale, y: y + dy * scale, size: size * scale }
            }
            Draw::Count { text, right, y: dy } => Draw::Count {
                text,
                right: (x + right as f32 * scale).round() as i32,
                y: (y + dy as f32 * scale).round() as i32,
            },
        })
        .collect()
}

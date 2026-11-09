//! The only seam to Horizon. In the game it is implemented over the native mod API
//! (`send_packet` 409, `fill` and `text` of the HUD batch inside `gui_layer_post` 139, `text_width`);
//! in tests, by a recording fake. Anything the native API cannot answer is not in this trait: the GUI size and the
//! game's clock are not given to a mod, so the client keeps its own tick count and draws from fixed offsets.

use crate::state::SoundKey;

/// A colour as 0xAARRGGBB, the form Horizon's GUI batch takes.
pub type Argb = u32;

/// `argb` with its alpha multiplied by `alpha` (clamped to 0..=1, rounded half up to a whole byte). The colour is kept.
/// A HUD batch vertex packs its colour the same way, its alpha in the top byte (`rgba` in `horizon-client-render`),
/// so this fades drawn vertices as well as colours passed to the host.
pub fn scale_alpha(argb: Argb, alpha: f32) -> Argb {
    let a = (((argb >> 24) & 0xff) as f32 * alpha.clamp(0.0, 1.0)).round() as u32;
    (argb & 0x00ff_ffff) | (a << 24)
}

pub trait Host {
    /// Send a plugin-channel message to the server. An error means the player is not in a server's play state yet.
    fn send_to_server(&mut self, channel: &str, data: &[u8]) -> Result<(), String>;

    // Drawing in GUI pixels, inside a HUD layer or a mod screen. They do nothing outside one.
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Argb);
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: Argb, shadow: bool);
    fn text_width(&self, text: &str) -> i32;
    /// Uploads the mod's one picture (the packed atlas) into Horizon's HUD slot; false when it does not fit.
    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> bool;
    /// Text in the frame of `translate(anchor); rotate(rotation); scale(scale)`, its top left at `offset` in that frame.
    /// The default draws it upright and unscaled where the frame puts its corner.
    fn draw_text_xf(
        &mut self,
        text: &str,
        anchor: (f32, f32),
        offset: (f32, f32),
        scale: f32,
        rotation: f32,
        color: Argb,
        shadow: bool,
    ) {
        let (sx, sy) = (offset.0 * scale, offset.1 * scale);
        let (s, c) = rotation.sin_cos();
        let (x, y) = (anchor.0 + c * sx - s * sy, anchor.1 + s * sx + c * sy);
        self.draw_text((x + 0.5).floor() as i32, (y + 0.5).floor() as i32, text, color, shadow);
    }
    /// A square of half side `half` centred on (`cx`, `cy`), turned by `rotation` radians. The default is axis-aligned.
    fn fill_square_xf(&mut self, cx: f32, cy: f32, half: f32, _rotation: f32, color: Argb) {
        let (x0, y0) = ((cx - half + 0.5).floor() as i32, (cy - half + 0.5).floor() as i32);
        let side = (2.0 * half + 0.5).floor() as i32;
        self.fill_rect(x0, y0, side, side, color);
    }
    /// A player's face as 8 x 8 RGBA (the skin's face with its hat layer over it), when the client has the skin.
    fn player_face(&mut self, _uuid: u128) -> Option<Vec<u8>> {
        None
    }
    /// Draws part `src` (`[x, y, w, h]` of the uploaded picture) stretched into `dst` (`[x, y, w, h]` in GUI pixels).
    fn draw_sprite(&mut self, src: [f32; 4], dst: [f32; 4]);
    /// A vanilla nine-slice widget sprite by its name (`widget/button`, `widget/tab_selected`...), stretched into the
    /// rectangle and tinted by `argb` (`0xffffffff` for none). Only for sprites that are NOT in the mod's own atlas.
    fn draw_vanilla_nine_slice(&mut self, sprite: &str, x: f32, y: f32, w: f32, h: f32, argb: Argb);
    /// A vanilla tooltip (a dark box with these lines of text) with its corner at (x, y).
    fn draw_tooltip(&mut self, lines: &[String], x: i32, y: i32);
    /// Draws an item icon by name (`minecraft:stone`), `size` pixels square, top left at (x, y).
    fn draw_item(&mut self, name: &str, x: f32, y: f32, size: f32);
    /// Marks where a run of drawing starts, for `end_alpha` to fade. The default has nothing to fade.
    fn begin_alpha(&mut self) -> usize {
        0
    }
    /// Multiplies the alpha of everything drawn since `begin_alpha` returned `start` by `alpha` (0 to 1), as
    /// `GuiGraphicsExtractor`'s shader colour does. The default fades nothing.
    fn end_alpha(&mut self, _start: usize, _alpha: f32) {}

    /// The GUI's width in pixels, or None when the native API does not tell a mod (it does not yet: Horizon would need a
    /// `gui_size` hook). Things centred on the screen are only drawn when it is known.
    fn gui_width(&self) -> Option<i32>;

    /// The GUI size in pixels (`client_state().gui_size`), None before a window exists.
    fn gui_size(&self) -> Option<(i32, i32)>;
    /// The mouse in GUI pixels.
    fn cursor(&self) -> (i32, i32);
    /// True when no screen covers the world: the player is playing (not in chat, a menu, the death screen).
    fn is_playing_screen(&self) -> bool;
    /// Whether a key is held; `key` is vanilla's name (`key.keyboard.y`).
    fn key_down(&self, key: &str) -> bool;
    /// Sneak is held (`options.keyShift.isDown`).
    fn sneaking(&self) -> bool {
        false
    }
    /// The chat screen is open (`client.screen is ChatScreen`).
    fn chat_open(&self) -> bool {
        false
    }
    /// The F1 hide: the GUI is hidden (`gui_hidden` of the client state).
    fn hud_hidden(&self) -> bool {
        false
    }
    /// F3's debug screen is up (`debug_screen` of the client state).
    fn debug_screen(&self) -> bool {
        false
    }
    /// The player list key is held (Tab by default, `keyPlayerList.isDown`).
    fn tab_list_down(&self) -> bool {
        false
    }
    /// A value the mod keeps for the player under `key`, None when it was never set.
    fn setting(&self, _key: &str) -> Option<String> {
        None
    }
    /// Keeps `value` under `key` for the player, so it is there the next time the game starts.
    fn set_setting(&mut self, _key: &str, _value: &str) {}
    /// Opens / closes the mod's own screen (`mod_screen_open`: the host draws and routes input to it) and tells whether it is up.
    fn open_screen(&mut self) -> bool;
    fn close_screen(&mut self);
    fn screen_is_open(&self) -> bool;

    /// The client config changed (a click or a drag on the settings screen): `json` is the whole config as its file
    /// holds it. The host writes it to the config file; the default does nothing.
    fn config_changed(&mut self, _json: &str) {}
    /// The settings screen's test button for the sound `id` (`sounds::name`), at the volume the config gives it.
    fn play_test_sound(&mut self, _id: &str, _volume: f32) {}

    /// The player the crosshair is on, within reach, by UUID; None for a block, nothing, or a non-player. Read once a tick
    /// for the player tooltip. The default looks at nothing.
    fn looked_at_player(&self) -> Option<u128> {
        None
    }

    /// One of the mod's own sounds (`ClientSounds`). Horizon has no sound events of the mod's, so the first host
    /// only records it; mapping them to Horizon's sounds is task R1.
    fn play_sound(&mut self, key: SoundKey, volume: f32, pitch: f32);
}

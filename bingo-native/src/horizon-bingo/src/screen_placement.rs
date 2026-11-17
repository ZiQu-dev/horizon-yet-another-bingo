//! The card placement screen (`BingoCardPlacementScreen.kt`): the player drags the card around the window and drags its
//! bottom right corner to resize it, then saves or cancels. Ported as pure functions and a small state machine: mouse
//! positions go in, the new `ClientConfig` comes out, and the draw commands and hit regions are plain data.
//!
//! Coordinates are GUI pixels of the scaled window. The card is `card_draw::CARD_WIDTH x CARD_HEIGHT` times its scale,
//! its top left corner at `(left, top)`.
//!
//! Divergences, all listed in the TODO comments below: the Kotlin `coerceIn` throws when its bounds are reversed (a card
//! wider than the window); here the lower bound wins. Hit regions are inclusive on all sides, as the Kotlin ranges are,
//! so a button also answers on its far edge (vanilla's `Button.isMouseOver` does not).

use crate::card_draw::{translate, Draw, ItemNames, TeamColors, CARD_HEIGHT, CARD_WIDTH};
use crate::card_tile_image::CardTileImage;
use crate::game_message::{GameMessagePacket, MessageType};
use crate::game_status::JavaDuration;
use crate::message_draw::{draw_messages, MessageView};
use crate::state::ScoreMessage;
use crate::text::Text;
use crate::text_plain::{format_template, Lang};

/// `resizeHandleSize`: the grab zone is this many pixels either side of the card's right and bottom edges.
pub const RESIZE_HANDLE: f32 = 10.0;
/// `cardScaleSnapToValues`, and the 0.05 window of `snap`.
const SCALE_SNAP_VALUES: [f32; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
const SCALE_SNAP_WINDOW: f32 = 0.05;
/// `coerceIn(.25f, ...)` of the resize.
const SCALE_MIN: f32 = 0.25;
/// `Button.DEFAULT_WIDTH` and `DEFAULT_HEIGHT` (vanilla), which `Button.builder(...).build()` uses.
const BUTTON_W: f32 = 150.0;
const BUTTON_H: f32 = 20.0;
/// `Font.lineHeight`, as `message_draw` has it.
const TEXT_HEIGHT: i32 = 9;
/// The help text colour `0xFF_A3F5A3`.
const HELP_COLOR: u32 = 0xffa3f5a3;
/// `overlayFill(..., 0x80ffffff)` while the card is moved or resized.
const DRAG_OVERLAY: u32 = 0x80ff_ffff;
/// The outline drawn under the mouse. `BingoCardColors.DEFAULTS.cardTextureOutline`.
// TODO(integration): a team's own `cardTextureOutline` is not in `TeamColors`; only the default is used.
const OUTLINE_TEXTURE: &str = "bingo/outline";

/// `CardAlignment`: which corner the card is anchored to. `x` and `y` are 1 on the right and bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAlignment {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl CardAlignment {
    fn x(self) -> i32 {
        matches!(self, CardAlignment::TopRight | CardAlignment::BottomRight) as i32
    }

    fn y(self) -> i32 {
        matches!(self, CardAlignment::BottomLeft | CardAlignment::BottomRight) as i32
    }

    fn from_edges(right: bool, bottom: bool) -> Self {
        match (right, bottom) {
            (true, true) => CardAlignment::BottomRight,
            (true, false) => CardAlignment::TopRight,
            (false, true) => CardAlignment::BottomLeft,
            (false, false) => CardAlignment::TopLeft,
        }
    }
}

/// `CardOverlap`. Not drawn by this screen; the HUD reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOverlap {
    Above,
    Underneath,
}

/// The `config.client` values this screen edits, with the Kotlin defaults of `ClientConfig`. The two message values
/// are the ones the card's messages preview reads.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientConfig {
    pub enable_hud: bool,
    pub card_scale: f32,
    pub card_alignment: CardAlignment,
    pub card_offset_x: i32,
    pub card_offset_y: i32,
    pub card_overlap: CardOverlap,
    pub message_duration_seconds: i64,
    pub message_scale: f32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            enable_hud: true,
            card_scale: 1.0,
            card_alignment: CardAlignment::TopLeft,
            card_offset_x: 0,
            card_offset_y: 0,
            card_overlap: CardOverlap::Above,
            message_duration_seconds: 5,
            message_scale: 1.0,
        }
    }
}

impl ClientConfig {
    /// The fields of the config file's client config that this screen edits and previews.
    pub fn from_config(config: &crate::config::ClientConfig) -> Self {
        ClientConfig {
            enable_hud: config.enable_hud,
            card_scale: config.card_scale,
            card_alignment: config.card_alignment,
            card_offset_x: config.card_offset_x,
            card_offset_y: config.card_offset_y,
            card_overlap: config.card_overlap,
            message_duration_seconds: config.message_duration_seconds,
            message_scale: config.message_scale,
        }
    }

    /// Writes the four placement fields back into the config file's client config. Every other field is left as it is.
    pub fn write_placement(&self, config: &mut crate::config::ClientConfig) {
        config.card_scale = self.card_scale;
        config.card_alignment = self.card_alignment;
        config.card_offset_x = self.card_offset_x;
        config.card_offset_y = self.card_offset_y;
    }
}

/// What a click or a key means. `Resize` is either edge, `Move` the card's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Cancel,
    Save,
    Resize,
    Move,
}

/// A clickable or grabbable rectangle, inclusive on all sides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub action: Action,
}

impl Hit {
    pub fn contains(&self, (mx, my): (f64, f64)) -> bool {
        mx >= self.x as f64 && mx <= (self.x + self.w) as f64 && my >= self.y as f64 && my <= (self.y + self.h) as f64
    }
}

/// `CursorType`, as the card sets it under the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    ResizeHorizontal,
    ResizeVertical,
    ResizeAll,
}

/// The card's rectangle on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardRect {
    pub left: f32,
    pub top: f32,
    pub scale: f32,
}

impl CardRect {
    pub fn right(&self) -> f32 {
        self.left + CARD_WIDTH as f32 * self.scale
    }

    pub fn bottom(&self) -> f32 {
        self.top + CARD_HEIGHT as f32 * self.scale
    }

    fn body(&self) -> Hit {
        Hit { x: self.left, y: self.top, w: self.right() - self.left, h: self.bottom() - self.top, action: Action::Move }
    }

    /// `isIntersectingRightBorder`: `x in right-10..right+10`, `y in top..bottom`.
    fn right_handle(&self) -> Hit {
        Hit {
            x: self.right() - RESIZE_HANDLE,
            y: self.top,
            w: 2.0 * RESIZE_HANDLE,
            h: self.bottom() - self.top,
            action: Action::Resize,
        }
    }

    /// `isIntersectingBottomBorder`: `x in left..right`, `y in bottom-10..bottom+10`.
    fn bottom_handle(&self) -> Hit {
        Hit {
            x: self.left,
            y: self.bottom() - RESIZE_HANDLE,
            w: self.right() - self.left,
            h: 2.0 * RESIZE_HANDLE,
            action: Action::Resize,
        }
    }
}

/// `Math.round` as Kotlin's `roundToInt`: halves go up, so `-0.5` is `0`.
pub fn round_half_up(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// `coerceIn` with the lower bound winning when the bounds are reversed (Kotlin throws there).
// TODO(integration): Kotlin throws for a card wider than the window; the Rust port keeps the lower bound instead.
fn coerce_in(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

fn card_size(scale: f32) -> (f32, f32) {
    (CARD_WIDTH as f32 * scale, CARD_HEIGHT as f32 * scale)
}

/// `initCardPosition`: the card's top left corner from its alignment, offsets and scale, kept inside the window.
pub fn init_position(alignment: CardAlignment, offset_x: i32, offset_y: i32, scale: f32, window: (i32, i32)) -> (f32, f32) {
    let (w, h) = (window.0 as f32, window.1 as f32);
    let (cw, ch) = card_size(scale);
    // A right or bottom anchor counts its offset inwards.
    let ox = offset_x * if alignment.x() > 0 { -1 } else { 1 };
    let oy = offset_y * if alignment.y() > 0 { -1 } else { 1 };
    let left = coerce_in(alignment.x() as f32 * (w - cw) + ox as f32, 0.0, w - cw);
    let top = coerce_in(alignment.y() as f32 * (h - ch) + oy as f32, 0.0, h - ch);
    (left, top)
}

/// `applyCardPosition`: the alignment and offsets that put the card where it is now, from its nearest edges.
pub fn apply_position(left: f32, top: f32, scale: f32, window: (i32, i32)) -> (CardAlignment, i32, i32) {
    let (w, h) = (window.0 as f32, window.1 as f32);
    let (cw, ch) = card_size(scale);
    let (right, bottom) = (left + cw, top + ch);
    let is_right_edge = (w - right) < left;
    let is_bottom_edge = (h - bottom) < top;
    let offset_x = if is_right_edge { w - right } else { left };
    let offset_y = if is_bottom_edge { h - bottom } else { top };
    (
        CardAlignment::from_edges(is_right_edge, is_bottom_edge),
        round_half_up(offset_x),
        round_half_up(offset_y),
    )
}

/// `mouseDragged` while moving: the card follows the mouse by the distance it has travelled. Not clamped.
pub fn drag_position(from: (f32, f32), start: (f64, f64), mouse: (f64, f64)) -> (f32, f32) {
    (from.0 + (mouse.0 - start.0) as f32, from.1 + (mouse.1 - start.1) as f32)
}

/// The snap to 0.5, 0.75, 1, 1.5 and 2: any scale within 0.05 of one becomes it.
pub fn snap_scale(scale: f32) -> f32 {
    let mut s = scale;
    for value in SCALE_SNAP_VALUES {
        if (value - s).abs() < SCALE_SNAP_WINDOW {
            s = value;
        }
    }
    s
}

/// `mouseDragged` while resizing: the scale grows with the distance from the card's top left corner to the mouse,
/// relative to where the drag started. Clamped to `0.25` up to the window height, then snapped.
pub fn resize_scale(from_scale: f32, left: f32, top: f32, start: (f64, f64), mouse: (f64, f64), window_height: i32) -> f32 {
    let anchor = (left as f64, top as f64);
    let dist = |p: (f64, f64)| ((p.0 - anchor.0).powi(2) + (p.1 - anchor.1).powi(2)).sqrt() as f32;
    let raw = (from_scale / dist(start)) * dist(mouse);
    let max = window_height as f32 / CARD_HEIGHT as f32;
    snap_scale(coerce_in(raw, SCALE_MIN, max))
}

/// A press-free state machine for the screen, with Kotlin's `dragStart` and `resizeStart` as optional fields that
/// start on the first drag event.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    window: (i32, i32),
    /// The config being edited: `cardAlignment`, `cardOffsetX/Y`, `cardScale` are the fields the screen writes.
    edit: ClientConfig,
    left: f32,
    top: f32,
    drag: Option<Drag>,
    resize: Option<Resize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    from: (f32, f32),
    start: (f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Resize {
    from_scale: f32,
    start: (f64, f64),
}

/// What the screen hands back to its caller (`BingoCardPlacementScreen.Result`).
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Save: the config with the four placement fields updated. Escape and Cancel give `Cancel`.
    Ok(ClientConfig),
    Cancel,
}

impl Placement {
    /// `init()`: the card where the config puts it.
    pub fn open(config: &ClientConfig, window: (i32, i32)) -> Self {
        let mut placement = Placement { window, edit: config.clone(), left: 0.0, top: 0.0, drag: None, resize: None };
        placement.init();
        placement
    }

    /// `resize(width, height)`: the card is put back from the values it has now, in the new window.
    pub fn resize(&mut self, window: (i32, i32)) {
        self.window = window;
        self.init();
    }

    pub fn card(&self) -> CardRect {
        CardRect { left: self.left, top: self.top, scale: self.edit.card_scale }
    }

    pub fn config(&self) -> &ClientConfig {
        &self.edit
    }

    fn init(&mut self) {
        let (l, t) = init_position(
            self.edit.card_alignment,
            self.edit.card_offset_x,
            self.edit.card_offset_y,
            self.edit.card_scale,
            self.window,
        );
        self.left = l;
        self.top = t;
    }

    fn apply(&mut self) {
        let (alignment, ox, oy) = apply_position(self.left, self.top, self.edit.card_scale, self.window);
        self.edit.card_alignment = alignment;
        self.edit.card_offset_x = ox;
        self.edit.card_offset_y = oy;
    }

    /// `mouseDragged`: true when the event moves or resizes the card. A drag that starts on a handle resizes, one that
    /// starts on the body moves; a drag in progress keeps its kind.
    pub fn mouse_dragged(&mut self, mouse: (f64, f64)) -> bool {
        let card = self.card();
        let on_border = card.right_handle().contains(mouse) || card.bottom_handle().contains(mouse);
        if self.resize.is_some() || (on_border && self.drag.is_none()) {
            let resize = match self.resize {
                Some(r) => r,
                None => {
                    let r = Resize { from_scale: self.edit.card_scale, start: mouse };
                    self.resize = Some(r);
                    r
                }
            };
            self.edit.card_scale = resize_scale(resize.from_scale, self.left, self.top, resize.start, mouse, self.window.1);
            return true;
        }

        if self.drag.is_some() || card.body().contains(mouse) {
            let drag = match self.drag {
                Some(d) => d,
                None => {
                    let d = Drag { from: (self.left, self.top), start: mouse };
                    self.drag = Some(d);
                    d
                }
            };
            (self.left, self.top) = drag_position(drag.from, drag.start, mouse);
            return true;
        }

        false
    }

    /// `mouseReleased`: ends the move or resize, and settles the card to its nearest edges.
    pub fn mouse_released(&mut self) -> bool {
        if self.resize.take().is_some() || self.drag.take().is_some() {
            self.apply();
            self.init();
            return true;
        }
        false
    }

    /// `saveButton.onClick`: settle the card, then return the config it leaves.
    pub fn save(&mut self) -> ClientConfig {
        self.init();
        self.apply();
        self.edit.clone()
    }

    /// A click on one of the screen's buttons, or Escape (`Action::Cancel`). Other actions give nothing.
    pub fn press(&mut self, action: Action) -> Option<Outcome> {
        match action {
            Action::Cancel => Some(Outcome::Cancel),
            Action::Save => Some(Outcome::Ok(self.save())),
            Action::Resize | Action::Move => None,
        }
    }
}

/// `CardDrawable.render`'s cursor choice: the resize cursor on an edge (not while resizing), the move cursor over the
/// card or while it is being moved or resized.
pub fn cursor_at(state: &Placement, mouse: (f64, f64)) -> Option<Cursor> {
    let card = state.card();
    let over_right = card.right_handle().contains(mouse);
    let over_border = over_right || card.bottom_handle().contains(mouse);
    let resizing = state.resize.is_some();
    if over_border && !resizing {
        Some(if over_right { Cursor::ResizeHorizontal } else { Cursor::ResizeVertical })
    } else if card.body().contains(mouse) || state.drag.is_some() || resizing {
        Some(Cursor::ResizeAll)
    } else {
        None
    }
}

/// What the screen reads from the rest of the client for one frame.
pub struct PlacementView<'a> {
    /// `state.now`, in milliseconds.
    pub now_ms: u64,
    /// `options.chatScale`; the message renderer multiplies it by `message_scale`.
    pub chat_scale: f32,
    pub colors: &'a TeamColors,
    pub items: &'a dyn ItemNames,
    pub lang: Lang<'a>,
    /// `Font.getTextWidth`.
    pub text_width: &'a dyn Fn(&str) -> i32,
}

/// A draw command of this screen. `Draw` has no fractional fill, and the drag overlay covers the scaled card.
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementDraw {
    Draw(Draw),
    // TODO(integration): `Draw::Fill` takes whole pixels; the overlay is over the card's scaled rectangle.
    ScaledFill { x: f32, y: f32, w: f32, h: f32, argb: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacementFrame {
    pub draws: Vec<PlacementDraw>,
    /// The buttons, then the card's right and bottom handles, then its body.
    pub hits: Vec<Hit>,
    pub cursor: Option<Cursor>,
}

fn tr(lang: Lang<'_>, key: &str, fallback: &str) -> String {
    lang(key).unwrap_or_else(|| fallback.to_string())
}

/// The one message the screen previews: `ObjectiveExampleYourNose` captured by a player, 500 ms old.
fn dummy_messages(now_ms: u64, lang: Lang<'_>) -> Vec<ScoreMessage> {
    let player = "\u{1F33A} fennifith";
    let nose = format!("[{}]", tr(lang, "bingo.objective.example.your_nose", "Your Nose"));
    let template = tr(lang, "bingo.game.message.captured_item", "%s got %s");
    vec![ScoreMessage {
        created_at: now_ms.saturating_sub(500),
        packet: GameMessagePacket {
            id: 0,
            time_elapsed: JavaDuration::of_millis(0),
            team: String::new(),
            image: CardTileImage::default(),
            image_list: Vec::new(),
            message_type: MessageType::ItemScored,
            message: Text::plain(&format_template(&template, &[player.to_string(), nose])),
            is_update: false,
        },
    }]
}

/// `Button` drawn at (x, y) with a label centred on it: the highlighted sprite under the mouse.
fn button(
    draws: &mut Vec<PlacementDraw>,
    hits: &mut Vec<Hit>,
    view: &PlacementView<'_>,
    mouse: (f64, f64),
    (x, y): (f32, f32),
    label: &str,
    action: Action,
) {
    let hit = Hit { x, y, w: BUTTON_W, h: BUTTON_H, action };
    let sprite = if hit.contains(mouse) { "widget/button_highlighted" } else { "widget/button" };
    draws.push(PlacementDraw::Draw(Draw::Sprite { name: sprite.into(), x, y, w: BUTTON_W, h: BUTTON_H }));
    // `drawCenteredString(x + w/2, y + (h - 8)/2)`: the text's width halved, rounded down.
    let width = (view.text_width)(label);
    draws.push(PlacementDraw::Draw(Draw::Text {
        x: x as i32 + BUTTON_W as i32 / 2 - width / 2,
        y: y as i32 + (BUTTON_H as i32 - 8) / 2,
        text: label.to_string(),
        argb: 0xffff_ffff,
    }));
    hits.push(hit);
}

/// The whole screen for one frame: the card with its preview messages, the two buttons, and the help text.
pub fn draw_placement(state: &Placement, view: &PlacementView<'_>, mouse: (f64, f64)) -> PlacementFrame {
    let (window_w, _) = state.window;
    let card = state.card();
    let (cw, ch) = card_size(card.scale);
    let mut draws = Vec::new();
    let mut hits = Vec::new();

    // The card's own background, then the overlay or the outline. Its hits come last.
    draws.push(PlacementDraw::Draw(Draw::Sprite {
        name: view.colors.card_texture.clone(),
        x: card.left,
        y: card.top,
        w: cw,
        h: ch,
    }));
    let resizing = state.resize.is_some();
    let dragging = state.drag.is_some();
    let over_border = card.right_handle().contains(mouse) || card.bottom_handle().contains(mouse);
    let over_card = card.body().contains(mouse);
    if dragging || resizing {
        draws.push(PlacementDraw::ScaledFill { x: card.left, y: card.top, w: cw, h: ch, argb: DRAG_OVERLAY });
    } else if over_border || over_card {
        draws.push(PlacementDraw::Draw(Draw::Sprite {
            name: OUTLINE_TEXTURE.into(),
            x: card.left,
            y: card.top,
            w: cw,
            h: ch,
        }));
    }

    // The messages sit at `(cardLeft.toInt(), cardTop.toInt() + CARD_HEIGHT * scale * (1 - alignment.y))`.
    let config = state.config();
    let anchor_x = card.left as i32;
    let anchor_y = card.top as i32 + (CARD_HEIGHT as f32 * card.scale * (1 - config.card_alignment.y()) as f32) as i32;
    let message_view = MessageView {
        now_ms: view.now_ms,
        duration_seconds: config.message_duration_seconds,
        message_scale: config.message_scale,
        chat_scale: view.chat_scale,
        card_alignment: (config.card_alignment.x(), config.card_alignment.y()),
        card_scale: card.scale,
        items: view.items,
        lang: view.lang,
        text_width: view.text_width,
    };
    let mut messages = dummy_messages(view.now_ms, view.lang);
    for draw in translate(draw_messages(&mut messages, &message_view), anchor_x, anchor_y) {
        draws.push(PlacementDraw::Draw(draw));
    }

    // initButtons: Cancel and Save side by side, 4 px either side of the window's centre, 8 px above the bottom.
    let half = window_w / 2;
    let y = (state.window.1 - BUTTON_H as i32 - 8) as f32;
    button(&mut draws, &mut hits, view, mouse, ((half - BUTTON_W as i32 - 4) as f32, y), &tr(view.lang, "gui.cancel", "Cancel"), Action::Cancel);
    button(&mut draws, &mut hits, view, mouse, ((half + 4) as f32, y), &tr(view.lang, "gui.ok", "Ok"), Action::Save);

    // The help text, two lines at the top, centred.
    let help = [
        tr(view.lang, "bingo.config.card_position.help_move", "Left click and drag the center of the card to move it around the screen."),
        tr(view.lang, "bingo.config.card_position.help_resize", "Click and drag near the bottom right of the card to resize it."),
    ];
    for (line, text) in help.into_iter().enumerate() {
        // Line 1 at y = 8, line 2 one text height and a 16 px gap below: 16 + 9 = 25.
        let y = if line == 0 { 8 } else { 16 + TEXT_HEIGHT };
        let x = half - (view.text_width)(&text) / 2;
        draws.push(PlacementDraw::Draw(Draw::Text { x, y, text, argb: HELP_COLOR }));
    }

    hits.push(card.right_handle());
    hits.push(card.bottom_handle());
    hits.push(card.body());

    PlacementFrame { draws, hits, cursor: cursor_at(state, mouse) }
}

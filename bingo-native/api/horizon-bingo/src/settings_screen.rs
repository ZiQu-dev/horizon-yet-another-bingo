//! The settings screen of the client config: the options of `YetAnotherConfigLibIntegration.kt` that edit `ClientConfig`
//! (the GUI options, the card position button, the messages, the manual card position and the sounds), as one list
//! the player scrolls. Pure, as `screen_placement` is: the caller gives the config, the mouse and the GUI size and gets
//! draw commands back. Clicks and drags change the config in place and say whether they did.
//!
//! Each change is what YACL writes at once (`configService.writeConfig`): the caller saves the config whenever a
//! method says it changed. Not here, on purpose: YACL's category tabs (one list instead), its tooltips, the "Player
//! Settings" and "Stats" categories (`PlayerSettings` is the server's, `syncStats` is the server config's), and the
//! keys of the host screen. The card position row asks the host to open the placement screen
//! (`take_placement_request`); a sound's test button asks it to play that sound (`take_test_sound`), at the volume
//! the config holds.
//!
//! Layout is in GUI pixels, the frame of `screen_placement`: a title at the top, the list from `LIST_TOP` down to the
//! footer, and Reset and Done at the bottom, 4 px either side of the middle. A row is drawn only when it fits in the
//! list, since `UiDraw` has no clip: a row half off the list waits until the scroll brings it in.
//!
//! Clicks use the last `frame`'s hit regions, so call `frame` before the input of a frame.

use crate::config::{
    CardAlignment, CardOverlap, ClientConfig, MESSAGE_DURATION_RANGE, OFFSET_MIN, SCALE_RANGE, SOUNDS,
    SOUND_VOLUME_RANGE,
};
use crate::text_plain::Lang;
use crate::ui_draw::UiDraw;

/// One option row, and each of a list's sections.
const ROW_H: i32 = 24;
const HEADER_H: i32 = 16;
/// The list's width, as the placement screen's buttons: 308 px, less where the window is narrower.
const LIST_W: i32 = 308;
/// `Button.DEFAULT_WIDTH` and `DEFAULT_HEIGHT`, the widget of a row.
const WIDGET_W: i32 = 150;
const WIDGET_H: i32 = 20;
/// The gap between a row's widget and the list's right edge; the label's gap from its left edge.
const INSET: i32 = 4;
/// `Font.lineHeight` and the 8 px of a centred button label (`(h - 8) / 2`).
const TEXT_H: i32 = 9;
const BUTTON_TEXT_H: i32 = 8;
const TITLE_Y: i32 = 8;
const LIST_TOP: i32 = 28;
/// The footer's buttons sit this far above the bottom, the list ends `FOOTER_GAP` above them.
const FOOTER_Y: i32 = 28;
const FOOTER_GAP: i32 = 4;
/// The slider handle's width: `SliderButton`'s 8 px, which moves over the track less its own width.
const HANDLE_W: i32 = 8;
/// The vanilla sprites: a 200 x 20 button (nine-slice, border 3), a 200 x 20 slider track (border 1) and an 8 x 20
/// handle (`widget/slider_handle`'s mcmeta borders 2, 2, 2, 3; the slice here is 2).
const BUTTON: &str = "widget/button";
const BUTTON_HIGHLIGHTED: &str = "widget/button_highlighted";
const SLIDER: &str = "widget/slider";
const HANDLE: &str = "widget/slider_handle";
const HANDLE_HIGHLIGHTED: &str = "widget/slider_handle_highlighted";
const WIDGET_TEX: (i32, i32) = (200, 20);
const HANDLE_TEX: (i32, i32) = (8, 20);
/// `Screen.MENU_BACKGROUND`: black at 25% alpha, as `screen_main` draws it.
const MENU_BACKGROUND: u32 = 0x4000_0000;
const WHITE: u32 = 0xffff_ffff;

/// An option a row edits: one `ClientConfig` field. `SoundVolume` is one entry of `soundVolumes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    QuickStart,
    EnableHud,
    CardOverlap,
    ShowMultipleCards,
    CardTeamOutlines,
    HideOnF3,
    HideOnChat,
    CardPausesGame,
    MessageDuration,
    MessageScale,
    MessageFromOtherTeams,
    CardScale,
    CardAlignment,
    CardOffsetX,
    CardOffsetY,
    SoundVolume(&'static str),
}

/// A button row: it does not change the config itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// `bingo.config.card_position`: asks the host to open the card placement screen.
    CardPosition,
    /// A sound's test button: asks the host to play the sound, by its name.
    TestSound(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    /// A group's or category's title, by its language key.
    Header(String),
    Setting(Setting),
    Button(Button),
}

fn row_height(row: &Row) -> i32 {
    match row {
        Row::Header(_) => HEADER_H,
        Row::Setting(_) | Row::Button(_) => ROW_H,
    }
}

/// The rows in YACL's order: the GUI category (quick start, then the HUD group, the messages group and the manual card
/// position group), then the sounds, a group per sound.
fn rows() -> Vec<Row> {
    let mut rows = vec![
        Row::Header("bingo.config.gui".into()),
        Row::Setting(Setting::QuickStart),
        Row::Header("bingo.config.gui.hud_settings".into()),
        Row::Setting(Setting::EnableHud),
        Row::Button(Button::CardPosition),
        Row::Setting(Setting::CardOverlap),
        Row::Setting(Setting::ShowMultipleCards),
        Row::Setting(Setting::CardTeamOutlines),
        Row::Setting(Setting::HideOnF3),
        Row::Setting(Setting::HideOnChat),
        Row::Setting(Setting::CardPausesGame),
        Row::Header("bingo.player_settings.messages".into()),
        Row::Setting(Setting::MessageDuration),
        Row::Setting(Setting::MessageScale),
        Row::Setting(Setting::MessageFromOtherTeams),
        Row::Header("bingo.config.card_position_manual".into()),
        Row::Setting(Setting::CardScale),
        Row::Setting(Setting::CardAlignment),
        Row::Setting(Setting::CardOffsetX),
        Row::Setting(Setting::CardOffsetY),
        Row::Header("bingo.sound_settings".into()),
    ];
    for id in SOUNDS {
        rows.push(Row::Header(format!("bingo.sound.{id}")));
        rows.push(Row::Setting(Setting::SoundVolume(id)));
        rows.push(Row::Button(Button::TestSound(id)));
    }
    rows
}

/// What the screen reads from the host for one frame: the language lookup and the text width, as
/// `screen_placement::PlacementView` has them.
pub struct SettingsView<'a> {
    pub lang: Lang<'a>,
    /// `Font.getTextWidth`.
    pub text_width: &'a dyn Fn(&str) -> i32,
}

/// A rectangle in GUI pixels, inclusive of its top left edge and exclusive of its far edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, (mx, my): (f64, f64)) -> bool {
        mx >= self.x as f64
            && mx < (self.x + self.w) as f64
            && my >= self.y as f64
            && my < (self.y + self.h) as f64
    }
}

/// What a clickable region does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Done,
    Reset,
    /// The row at this index in the list.
    Row(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hit {
    rect: Rect,
    target: Target,
}

/// The screen's state between frames: the scroll, the slider being dragged and the requests the host has not taken.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsScreen {
    rows: Vec<Row>,
    /// How far the list is scrolled down, in pixels.
    scroll: i32,
    /// The most the list can scroll, from the last frame.
    max_scroll: i32,
    /// The GUI size of the last frame: the offsets' slider tops out at half of it.
    gui: (i32, i32),
    hits: Vec<Hit>,
    /// The row of the slider under a drag.
    drag: Option<usize>,
    placement: bool,
    test_sound: Option<&'static str>,
    done: bool,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        SettingsScreen::new()
    }
}

impl SettingsScreen {
    pub fn new() -> Self {
        SettingsScreen {
            rows: rows(),
            scroll: 0,
            max_scroll: 0,
            gui: (0, 0),
            hits: Vec::new(),
            drag: None,
            placement: false,
            test_sound: None,
            done: false,
        }
    }

    /// The whole screen for one frame, and its hit regions for the next input.
    pub fn frame(
        &mut self,
        config: &ClientConfig,
        mouse: (f64, f64),
        gui: (i32, i32),
        view: &SettingsView<'_>,
    ) -> Vec<UiDraw> {
        let (gw, gh) = gui;
        self.gui = gui;
        let list_w = LIST_W.min(gw - 8).max(0);
        let list_x = (gw - list_w) / 2;
        let top = LIST_TOP;
        let bottom = gh - FOOTER_Y - FOOTER_GAP;
        let content: i32 = self.rows.iter().map(row_height).sum();
        self.max_scroll = (content - (bottom - top)).max(0);
        self.scroll = self.scroll.clamp(0, self.max_scroll);

        let mut out = vec![UiDraw::Fill { x: 0.0, y: 0.0, w: gw as f32, h: gh as f32, argb: MENU_BACKGROUND }];
        out.push(text(view, gw / 2, TITLE_Y, tr(view, "bingo.full_name"), WHITE, true));
        let mut hits = Vec::new();
        let dragging = self.drag;

        let mut y = top - self.scroll;
        for (index, row) in self.rows.iter().enumerate() {
            let h = row_height(row);
            if y >= top && y + h <= bottom {
                match row {
                    Row::Header(key) => {
                        out.push(text(view, gw / 2, y + (HEADER_H - TEXT_H) / 2, tr(view, key), WHITE, true));
                    }
                    Row::Setting(setting) => {
                        let rect = Rect {
                            x: list_x + list_w - INSET - WIDGET_W,
                            y: y + (ROW_H - WIDGET_H) / 2,
                            w: WIDGET_W,
                            h: WIDGET_H,
                        };
                        out.push(text(view, list_x + INSET, y + (ROW_H - TEXT_H) / 2, tr(view, name_key(*setting)), WHITE, false));
                        let hovered = rect.contains(mouse);
                        draw_setting(&mut out, view, config, *setting, rect, hovered, dragging == Some(index), self.gui);
                        hits.push(Hit { rect, target: Target::Row(index) });
                    }
                    Row::Button(button) => {
                        let rect = Rect {
                            x: list_x + list_w - INSET - WIDGET_W,
                            y: y + (ROW_H - WIDGET_H) / 2,
                            w: WIDGET_W,
                            h: WIDGET_H,
                        };
                        if let Some(key) = button_label(*button) {
                            out.push(text(view, list_x + INSET, y + (ROW_H - TEXT_H) / 2, tr(view, key), WHITE, false));
                        }
                        let hovered = rect.contains(mouse);
                        let label = tr(view, button_text(*button));
                        button_box(&mut out, view, rect, hovered, label);
                        hits.push(Hit { rect, target: Target::Row(index) });
                    }
                }
            }
            y += h;
        }

        // Reset and Done, as the placement screen's Cancel and Ok.
        let footer_y = gh - FOOTER_Y;
        let reset = Rect { x: gw / 2 - INSET - WIDGET_W, y: footer_y, w: WIDGET_W, h: WIDGET_H };
        let done = Rect { x: gw / 2 + INSET, y: footer_y, w: WIDGET_W, h: WIDGET_H };
        button_box(&mut out, view, reset, reset.contains(mouse), tr(view, "controls.reset"));
        button_box(&mut out, view, done, done.contains(mouse), tr(view, "gui.done"));
        hits.push(Hit { rect: reset, target: Target::Reset });
        hits.push(Hit { rect: done, target: Target::Done });

        self.hits = hits;
        out
    }

    /// A click. Returns true when the config changed. A button row or Done changes nothing here; the host reads the
    /// requests with `take_placement_request` and `take_test_sound`, and Done sets `is_done`.
    pub fn click(&mut self, config: &mut ClientConfig, mouse: (f64, f64)) -> bool {
        let Some(hit) = self.hits.iter().copied().find(|h| h.rect.contains(mouse)) else {
            return false;
        };
        match hit.target {
            Target::Done => {
                self.done = true;
                self.drag = None;
                false
            }
            Target::Reset => {
                let changed = *config != ClientConfig::default();
                *config = ClientConfig::default();
                changed
            }
            Target::Row(index) => self.press_row(index, hit.rect, config, mouse),
        }
    }

    /// The mouse moved with the button held: a slider under a drag follows it. Returns true when the config changed.
    pub fn drag(&mut self, config: &mut ClientConfig, mouse: (f64, f64)) -> bool {
        let Some(index) = self.drag else {
            return false;
        };
        let Some(hit) = self.hits.iter().copied().find(|h| h.target == Target::Row(index)) else {
            return false;
        };
        let Some(Row::Setting(setting)) = self.rows.get(index).cloned() else {
            return false;
        };
        match range(setting, self.gui) {
            Some(r) => set_from_mouse(config, setting, hit.rect, mouse.0, r),
            None => false,
        }
    }

    /// The button came up: a drag ends.
    pub fn release(&mut self) {
        self.drag = None;
    }

    /// The wheel. `amount` is positive for a turn up, which scrolls the list towards its top. It cannot change the
    /// config, so it returns nothing.
    pub fn scroll(&mut self, amount: f64) {
        let step = (amount * ROW_H as f64) as i32;
        self.scroll = (self.scroll - step).clamp(0, self.max_scroll);
    }

    /// Escape: the screen is done, as Done is.
    pub fn escape(&mut self) {
        self.done = true;
        self.drag = None;
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// True once after the card position row was clicked: the host opens the placement screen.
    pub fn take_placement_request(&mut self) -> bool {
        std::mem::take(&mut self.placement)
    }

    /// The sound whose test button was clicked, once: the host plays it at `config.sound_volume(name)`.
    pub fn take_test_sound(&mut self) -> Option<&'static str> {
        self.test_sound.take()
    }

    fn press_row(&mut self, index: usize, rect: Rect, config: &mut ClientConfig, mouse: (f64, f64)) -> bool {
        match self.rows.get(index).cloned() {
            Some(Row::Setting(setting)) => {
                if let Some(r) = range(setting, self.gui) {
                    self.drag = Some(index);
                    return set_from_mouse(config, setting, rect, mouse.0, r);
                }
                if is_toggle(setting) {
                    flip(config, setting);
                    return true;
                }
                if is_choice(setting) {
                    cycle(config, setting);
                    return true;
                }
                false
            }
            Some(Row::Button(Button::CardPosition)) => {
                self.placement = true;
                false
            }
            Some(Row::Button(Button::TestSound(id))) => {
                self.test_sound = Some(id);
                false
            }
            _ => false,
        }
    }
}

fn tr(view: &SettingsView<'_>, key: &str) -> String {
    (view.lang)(key).unwrap_or_else(|| key.to_string())
}

/// The language key of a row's name.
fn name_key(setting: Setting) -> &'static str {
    match setting {
        Setting::QuickStart => "bingo.config.quick_start_button",
        Setting::EnableHud => "bingo.config.enable_client_hud",
        Setting::CardOverlap => "bingo.config.card_overlap",
        Setting::ShowMultipleCards => "bingo.config.show_multiple_cards",
        Setting::CardTeamOutlines => "bingo.config.show_team_outlines",
        Setting::HideOnF3 => "bingo.config.hide_on_f3",
        Setting::HideOnChat => "bingo.config.hide_on_chat",
        Setting::CardPausesGame => "bingo.config.card_pauses_game",
        Setting::MessageDuration => "bingo.player_settings.messages.duration",
        Setting::MessageScale => "bingo.player_settings.messages.scale",
        Setting::MessageFromOtherTeams => "bingo.player_settings.messages.show_other_teams",
        Setting::CardScale => "bingo.config.card_scale",
        Setting::CardAlignment => "bingo.config.card_alignment",
        Setting::CardOffsetX => "bingo.config.card_offset_x",
        Setting::CardOffsetY => "bingo.config.card_offset_y",
        Setting::SoundVolume(_) => "bingo.sound_settings.volume",
    }
}

/// A button row's name, if it has one (the sounds' test buttons are named by their widget).
fn button_label(button: Button) -> Option<&'static str> {
    match button {
        Button::CardPosition => Some("bingo.config.card_position"),
        Button::TestSound(_) => None,
    }
}

/// A button row's text on its widget.
fn button_text(button: Button) -> &'static str {
    match button {
        Button::CardPosition => "bingo.config.card_position.edit",
        Button::TestSound(_) => "bingo.sound_settings.test_sound",
    }
}

fn is_toggle(setting: Setting) -> bool {
    matches!(
        setting,
        Setting::QuickStart
            | Setting::EnableHud
            | Setting::ShowMultipleCards
            | Setting::CardTeamOutlines
            | Setting::HideOnF3
            | Setting::HideOnChat
            | Setting::CardPausesGame
            | Setting::MessageFromOtherTeams
    )
}

fn is_choice(setting: Setting) -> bool {
    matches!(setting, Setting::CardOverlap | Setting::CardAlignment)
}

fn flag(config: &ClientConfig, setting: Setting) -> bool {
    match setting {
        Setting::QuickStart => config.show_quick_start_button,
        Setting::EnableHud => config.enable_hud,
        Setting::ShowMultipleCards => config.show_multiple_cards,
        Setting::CardTeamOutlines => config.card_team_outlines,
        Setting::HideOnF3 => config.hide_on_f3,
        Setting::HideOnChat => config.hide_on_chat,
        Setting::CardPausesGame => config.card_pauses_game,
        Setting::MessageFromOtherTeams => config.message_from_other_teams,
        _ => false,
    }
}

fn flip(config: &mut ClientConfig, setting: Setting) {
    let next = !flag(config, setting);
    match setting {
        Setting::QuickStart => config.show_quick_start_button = next,
        Setting::EnableHud => config.enable_hud = next,
        Setting::ShowMultipleCards => config.show_multiple_cards = next,
        Setting::CardTeamOutlines => config.card_team_outlines = next,
        Setting::HideOnF3 => config.hide_on_f3 = next,
        Setting::HideOnChat => config.hide_on_chat = next,
        Setting::CardPausesGame => config.card_pauses_game = next,
        Setting::MessageFromOtherTeams => config.message_from_other_teams = next,
        _ => {}
    }
}

/// Clicking an enum cycles it, in the enum's declaration order, as YACL's `EnumController` does.
fn cycle(config: &mut ClientConfig, setting: Setting) {
    match setting {
        Setting::CardAlignment => {
            config.card_alignment = match config.card_alignment {
                CardAlignment::TopLeft => CardAlignment::TopRight,
                CardAlignment::TopRight => CardAlignment::BottomLeft,
                CardAlignment::BottomLeft => CardAlignment::BottomRight,
                CardAlignment::BottomRight => CardAlignment::TopLeft,
            };
        }
        Setting::CardOverlap => {
            config.card_overlap = match config.card_overlap {
                CardOverlap::Above => CardOverlap::Underneath,
                CardOverlap::Underneath => CardOverlap::Above,
            };
        }
        _ => {}
    }
}

fn choice_key(config: &ClientConfig, setting: Setting) -> &'static str {
    match setting {
        Setting::CardAlignment => match config.card_alignment {
            CardAlignment::TopLeft => "bingo.config.card_alignment.top_left",
            CardAlignment::TopRight => "bingo.config.card_alignment.top_right",
            CardAlignment::BottomLeft => "bingo.config.card_alignment.bottom_left",
            CardAlignment::BottomRight => "bingo.config.card_alignment.bottom_right",
        },
        _ => match config.card_overlap {
            CardOverlap::Above => "bingo.config.card_overlap.above",
            CardOverlap::Underneath => "bingo.config.card_overlap.underneath",
        },
    }
}

/// The slider of a setting: its lowest value, highest value and step. The offsets top out at half the GUI.
fn range(setting: Setting, gui: (i32, i32)) -> Option<(f32, f32, f32)> {
    Some(match setting {
        Setting::MessageDuration => (MESSAGE_DURATION_RANGE.0 as f32, MESSAGE_DURATION_RANGE.1 as f32, 1.0),
        Setting::CardOffsetX => (OFFSET_MIN as f32, (gui.0 / 2) as f32, 1.0),
        Setting::CardOffsetY => (OFFSET_MIN as f32, (gui.1 / 2) as f32, 1.0),
        Setting::MessageScale | Setting::CardScale => (SCALE_RANGE.0, SCALE_RANGE.1, 0.05),
        Setting::SoundVolume(_) => (SOUND_VOLUME_RANGE.0, SOUND_VOLUME_RANGE.1, 0.01),
        _ => return None,
    })
}

fn value(config: &ClientConfig, setting: Setting) -> f32 {
    match setting {
        Setting::MessageDuration => config.message_duration_seconds as f32,
        Setting::MessageScale => config.message_scale,
        Setting::CardScale => config.card_scale,
        Setting::CardOffsetX => config.card_offset_x as f32,
        Setting::CardOffsetY => config.card_offset_y as f32,
        Setting::SoundVolume(id) => config.sound_volume(id),
        _ => 0.0,
    }
}

fn set_value(config: &mut ClientConfig, setting: Setting, v: f32) {
    match setting {
        Setting::MessageDuration => config.message_duration_seconds = v.round() as i64,
        Setting::MessageScale => config.message_scale = v,
        Setting::CardScale => config.card_scale = v,
        Setting::CardOffsetX => config.card_offset_x = v.round() as i32,
        Setting::CardOffsetY => config.card_offset_y = v.round() as i32,
        Setting::SoundVolume(id) => {
            config.sound_volumes.insert(id.to_string(), v);
        }
        _ => {}
    }
}

/// A value moved to the nearest step of its slider, and kept inside the slider's range.
fn snap(v: f32, (min, max, step): (f32, f32, f32)) -> f32 {
    let steps = ((v - min) / step).round();
    let stepped = min + steps * step;
    // Two decimals is the finest step any slider has, so this keeps `1.75` from becoming `1.7500001`.
    let quantised = (stepped * 100.0).round() / 100.0;
    quantised.max(min).min(max)
}

/// Where a value sits along its slider, from 0 at the lowest to 1 at the highest.
fn fraction(v: f32, min: f32, max: f32) -> f32 {
    if max > min { ((v - min) / (max - min)).max(0.0).min(1.0) } else { 0.0 }
}

/// The value under the mouse on a slider: the handle's centre follows the mouse, inside the track less the handle.
fn value_at(rect: Rect, mouse_x: f64, (min, max, step): (f32, f32, f32)) -> f32 {
    let travel = (rect.w - HANDLE_W) as f64;
    let t = if travel > 0.0 {
        ((mouse_x - rect.x as f64 - (HANDLE_W / 2) as f64) / travel).max(0.0).min(1.0)
    } else {
        0.0
    };
    snap(min + t as f32 * (max - min), (min, max, step))
}

/// Sets a setting from a slider's position under the mouse. True when the value moved.
fn set_from_mouse(config: &mut ClientConfig, setting: Setting, rect: Rect, mouse_x: f64, r: (f32, f32, f32)) -> bool {
    let before = value(config, setting);
    set_value(config, setting, value_at(rect, mouse_x, r));
    value(config, setting) != before
}

/// The text a slider shows: a percentage for the scales (rounded) and for the sounds (cut, as Kotlin's `toInt`), the
/// number otherwise.
fn value_text(setting: Setting, v: f32) -> String {
    match setting {
        Setting::MessageScale | Setting::CardScale => format!("{}%", (v * 100.0).round() as i64),
        Setting::SoundVolume(_) => format!("{}%", (v * 100.0) as i64),
        _ => format!("{}", v as i64),
    }
}

/// A row's widget, for a setting. Sliders draw a track and a handle; toggles and enums are buttons with their text.
#[allow(clippy::too_many_arguments)]
fn draw_setting(
    out: &mut Vec<UiDraw>,
    view: &SettingsView<'_>,
    config: &ClientConfig,
    setting: Setting,
    rect: Rect,
    hovered: bool,
    dragging: bool,
    gui: (i32, i32),
) {
    if let Some((min, max, _)) = range(setting, gui) {
        let v = value(config, setting);
        out.push(UiDraw::NineSlice {
            name: SLIDER.into(),
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            slice: 1,
            tex: Some(WIDGET_TEX),
        });
        let handle_x = rect.x + (fraction(v, min, max) * (rect.w - HANDLE_W) as f32).round() as i32;
        let lit = hovered || dragging;
        out.push(UiDraw::NineSlice {
            name: if lit { HANDLE_HIGHLIGHTED } else { HANDLE }.into(),
            x: handle_x,
            y: rect.y,
            w: HANDLE_W,
            h: rect.h,
            slice: 2,
            tex: Some(HANDLE_TEX),
        });
        centred(out, view, rect, value_text(setting, v), WHITE);
    } else if is_toggle(setting) {
        let on = flag(config, setting);
        let label = tr(view, if on { "options.on" } else { "options.off" });
        button_box(out, view, rect, hovered, label);
    } else if is_choice(setting) {
        let label = tr(view, choice_key(config, setting));
        button_box(out, view, rect, hovered, label);
    }
}

/// A 150 x 20 button with its text centred on it (`drawCenteredString`), lit under the mouse.
fn button_box(out: &mut Vec<UiDraw>, view: &SettingsView<'_>, rect: Rect, hovered: bool, label: String) {
    out.push(UiDraw::NineSlice {
        name: if hovered { BUTTON_HIGHLIGHTED } else { BUTTON }.into(),
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
        slice: 3,
        tex: Some(WIDGET_TEX),
    });
    centred(out, view, rect, label, WHITE);
}

/// Text centred in a rectangle: horizontally on its middle, vertically as a vanilla button's label.
fn centred(out: &mut Vec<UiDraw>, view: &SettingsView<'_>, rect: Rect, label: String, argb: u32) {
    let width = (view.text_width)(&label);
    out.push(UiDraw::Text {
        x: (rect.x + rect.w / 2 - width / 2) as f32,
        y: (rect.y + (rect.h - BUTTON_TEXT_H) / 2) as f32,
        text: label,
        argb,
        shadow: true,
        obfuscated: false,
    });
}

/// Text with its top left corner at (x, y), or centred on `cx` when it is the title or a header.
fn text(view: &SettingsView<'_>, x_or_centre: i32, y: i32, label: String, argb: u32, centre: bool) -> UiDraw {
    let x = if centre { x_or_centre - (view.text_width)(&label) / 2 } else { x_or_centre };
    UiDraw::Text { x: x as f32, y: y as f32, text: label, argb, shadow: true, obfuscated: false }
}

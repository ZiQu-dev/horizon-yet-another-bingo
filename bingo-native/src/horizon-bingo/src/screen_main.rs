//! The card screen (`client/common/hud/screen/BingoHudScreen.kt`): its layout, tab strip, buttons, the title texts
//! around the card, and the rules for opening, closing, Escape and the ready vote. Pure: a [`ScreenState`] and a
//! [`MainView`] (what the Kotlin reads from the HUD state, config and packets that frame) go in, and come out as
//! [`Layout`], [`Hit`]s, [`ScreenDraw`]s and [`Effect`]s. Nothing is drawn or clicked here.
//!
//! The end-screen tabs are the two that exist in the mod: CARDS and SCORES (`GameOverPacket.EndScreenTab`). The
//! message history is the sidebar's scrolling list, not a tab. Sibling modules draw the things this screen holds
//! a slot for: the card (`screen_cards`), the scores (`screen_end`), the message history (no module yet), and the
//! firework (no module yet). Those are named in `TODO(integration)` comments.
//!
//! Not yet ported, listed in the report: the scrolling of a button label wider than its button (drawn from the left
//! instead), the window's own
//! `Interpolate` of the card's position towards its HUD slot (the card's job), and the screen's own `onClose`
//! callbacks of the yacl settings screen.

use std::f32::consts::PI;

use crate::card_draw::{Draw, CARD_HEIGHT};
use crate::game_over::{EndScreenTab, GameOverPacket};
use crate::game_status::JavaDuration;
use crate::ready_update::{ReadyUpdatePacket, VoteOption};
use crate::text_plain::{flatten, format_template, Lang};

/// `BingoHudScreen.sidebarWidth`: `(width * 0.2).toInt() + 100`.
pub const SIDEBAR_FRACTION: f32 = 0.2;
pub const SIDEBAR_BASE: i32 = 100;
/// `TabsWidget.height`, a constant of the platform widget.
pub const TABS_HEIGHT: i32 = 24;
/// `MenuTabBar`'s `MAX_WIDTH` and `MARGIN`: the strip is at most `TABS_MAX_WIDTH` wide, less `TABS_MARGIN` at each side.
pub const TABS_MAX_WIDTH: i32 = 400;
pub const TABS_MARGIN: i32 = 14;
/// `Screen.MENU_BACKGROUND` (`widget/menu_background`'s texture): black at 25% alpha, under the open tab.
pub const MENU_TILE: u32 = 0x40_000000;
/// `Screen.HEADER_SEPARATOR`: a 32 x 2 texture, white at 20% over black at 75%, along the strip's bottom two rows.
pub const SEPARATOR_LIGHT: u32 = 0x33_ffffff;
pub const SEPARATOR_DARK: u32 = 0xbf_000000;
/// `marginX` and `marginY` of `updatePositions`.
pub const MARGIN_X: i32 = 12;
pub const MARGIN_Y: i32 = 4;
/// The height of a generic button (`size.y = 20`), and of a vote or the primary button (`40`).
pub const BUTTON_HEIGHT: i32 = 20;
pub const PRIMARY_HEIGHT: i32 = 40;
/// `buttonY = height - closeButton.size.y - 12`.
pub const BOTTOM_GAP: i32 = 12;
/// `VOTES_MAX`: the most marks a vote button shows.
pub const VOTES_MAX: i32 = 8;
/// `Interpolate(duration = 150.milliseconds)`: the sidebar slides in and out over this long.
pub const ANIM_MS: u64 = 150;
/// `NEW_BEST_DURATION = 2.seconds`: how long "New best time!" takes to grow in.
pub const NEW_BEST_MS: f64 = 2000.0;
/// `ClientCardRenderer.winColors`, cycled once per second.
pub const WIN_COLORS: [u32; 6] = [0xfff5c0a3, 0xffa3f5a3, 0xffa3f5c7, 0xffa3abf5, 0xffcca3f5, 0xfff5a3d6];

const WHITE: u32 = 0xffff_ffff;
/// Text of a button that is not active. TODO(verify): vanilla's disabled-button grey, from memory.
const GREY: u32 = 0xffa0_a0a0;
/// The sidebar's subtitle colour (`0xff888888`).
const SUBTITLE: u32 = 0xff88_8888;
/// The RGB of the game duration (`0xA3F5A3`, mint), the best time (`0xCCA3F5`) and the win streak (`0xF5A3D6`).
const DURATION_RGB: u32 = 0x00a3_f5a3;
const BEST_RGB: u32 = 0x00cc_a3f5;
const STREAK_RGB: u32 = 0x00f5_a3d6;

// Translation keys (`StringKey`), as the mod's `en_us.json` names them.
pub const KEY_CLOSE: &str = "bingo.card.close";
pub const KEY_SETTINGS: &str = "bingo.card.settings_button";
pub const KEY_MORE: &str = "bingo.options.more";
pub const KEY_RETURN: &str = "bingo.world.return_to_lobby";
pub const KEY_TAB_CARDS: &str = "bingo.game.end.tab.cards";
pub const KEY_TAB_SCORES: &str = "bingo.game.end.tab.scores";
pub const KEY_DURATION: &str = "bingo.stats.game_duration";
pub const KEY_BEST: &str = "bingo.stats.best_time";
pub const KEY_BEST_NEW: &str = "bingo.stats.best_time.new";
pub const KEY_STREAK: &str = "bingo.stats.win_streak";
pub const KEY_STREAK_MAX: &str = "bingo.stats.win_streak.max";
pub const KEY_STREAK_NEW: &str = "bingo.stats.win_streak.new_max";

/// The end-screen tabs in the order of the tab strip (`GameOverPacket.EndScreenTab.entries`).
pub const TABS: [EndScreenTab; 2] = [EndScreenTab::Cards, EndScreenTab::Scores];
const TAB_KEYS: [&str; 2] = [KEY_TAB_CARDS, KEY_TAB_SCORES];

/// The three sprites of a button: `texture`, its `focused` version and its `inactive` version (`ButtonTextures`).
struct Textures {
    texture: &'static str,
    focused: &'static str,
    inactive: &'static str,
}
const BUTTON_GENERIC: Textures = Textures {
    texture: "bingo/button_game_generic",
    focused: "bingo/button_game_generic_focused",
    inactive: "bingo/button_game_generic_inactive",
};
const BUTTON_CONTINUE: Textures = Textures {
    texture: "bingo/button_game_continue",
    focused: "bingo/button_game_continue_focused",
    inactive: "bingo/button_game_continue_inactive",
};
/// `BUTTON_GAME_GENERIC` and `BUTTON_GAME_CONTINUE` are drawn from 200 x 20 and 200 x 40 textures, nine-sliced by 8.
const SLICE: i32 = 8;

/// An axis-aligned rectangle in GUI pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// The card's slot. The position is a float, as `cardsWidget.x`/`y` are in the Kotlin. The card draws itself there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardSlot {
    pub x: f32,
    pub y: f32,
    pub w: i32,
    pub h: i32,
}

/// Which easing an [`Interp`] uses. Only the two this screen uses are modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    ExpOut,
    ExpOutReverse,
}

/// `Interpolate` from `client/common/utils/Interpolate.kt`, with time passed in rather than read from a clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interp {
    pub from: f32,
    pub to: f32,
    pub duration_ms: u64,
    pub started_at_ms: u64,
    pub easing: Easing,
    /// `isFinished`: set by [`Interp::update`], read by the next one.
    finished: bool,
}

impl Interp {
    pub fn new(from: f32, to: f32, duration_ms: u64, started_at_ms: u64, easing: Easing) -> Self {
        Interp { from, to, duration_ms, started_at_ms, easing, finished: false }
    }

    fn progress(&self, now_ms: u64) -> f32 {
        let since = now_ms as f64 - self.started_at_ms as f64;
        let t = (since / self.duration_ms as f64).clamp(0.0, 1.0) as f32;
        let exp_out = 1.0 - 2f32.powf(-10.0 * t);
        match self.easing {
            Easing::ExpOut => exp_out,
            Easing::ExpOutReverse => 1.0 - exp_out,
        }
    }

    /// `get()`: the value at `now_ms`.
    pub fn get(&self, now_ms: u64) -> f32 {
        let p = self.progress(now_ms);
        self.from * (1.0 - p) + self.to * p
    }

    /// `isDone()`: strictly after the end, as the Kotlin compares `now > startedAt + duration`.
    pub fn is_done(&self, now_ms: u64) -> bool {
        now_ms > self.started_at_ms + self.duration_ms
    }

    /// `update()`: true on the frame the value must be read again. Returns true while the previous frame had not
    /// yet seen the end, so it reports the frame after the animation has finished too, as the Kotlin does.
    pub fn update(&mut self, now_ms: u64) -> bool {
        let ret = !self.finished;
        self.finished = self.is_done(now_ms);
        ret
    }
}

/// What the screen remembers between frames (the Kotlin fields of `BingoHudScreen` that change after construction).
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenState {
    /// The selected tab. The caller keeps `GameOver.tab` in step with it, as `onTabChanged` does.
    pub tab: EndScreenTab,
    /// `shouldCloseOnEsc()`: the screen was opened with `canEscape`.
    pub can_escape: bool,
    /// `isClosing`.
    pub closing: bool,
    pub more_options_open: bool,
    /// `sendingReadyPacket`: set once a ready vote has been sent, which stops the game being paused.
    pub sending_ready: bool,
    /// `VoteOption.clientSelected` of the option that was voted for last, if any.
    pub client_selected: Option<u128>,
    /// The primary button's text once a pending vote's title has been shown. It is never reset (in the mod it is
    /// not), so it keeps the last title. `None` is the default "Return to Lobby".
    pub primary_title: Option<String>,
    /// `primaryButton.active`. Set only on frames where the primary branch runs, so it keeps its last value otherwise.
    pub primary_active: bool,
    /// `screenInterpolation`: the value the layout uses. It changes only on frames where [`Interp::update`] said so.
    pub screen_interpolation: f32,
    pub interp: Interp,
}

/// What [`ScreenState::frame`] found out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// The sidebar moved: lay out again.
    pub relayout: bool,
    /// The closing animation has finished: the host closes the screen.
    pub close: bool,
}

/// What a click on a [`Hit`] means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The close button: folds the more-options list away if it is open, else closes the screen.
    Close,
    /// The settings button: the host opens the config screen.
    Settings,
    MoreOptions,
    /// The primary button (with the pending vote, if any) or a vote option (with its id). `None` is "no vote".
    Ready(Option<u128>),
    /// A tab of the strip, by its index in [`TABS`].
    Tab(usize),
}

/// A clickable rectangle and what a click there does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub action: Action,
}

/// Something the host must do because of a click. The screen's own state has already changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `SetReadyPacket(true, vote)`, sent as V2 when the server supports it, else V1.
    SendReady(Option<u128>),
    /// `client.screen = yacl.buildConfigScreen(...)`.
    // TODO(integration): the host opens the config screen; the yacl integration is Horizon's, not this crate's.
    OpenSettings,
}

/// The GUI-side view of one frame: everything the Kotlin reads from the HUD state, the config and the packets.
pub struct MainView<'a> {
    pub width: i32,
    pub height: i32,
    /// `state.now`, in the host's milliseconds.
    pub now_ms: u64,
    /// The mouse, for hover.
    pub mouse: (i32, i32),
    /// The screen's `gameOver`: its packet, when the screen was opened from the end of a game.
    pub game_over: Option<&'a GameOverPacket>,
    /// `state.gameOver != null`: when this is false while `game_over` is set, the screen closes itself.
    pub state_has_game_over: bool,
    /// `state.ready`: the ready timer, the vote options and the pending vote.
    pub ready: Option<&'a ReadyUpdatePacket>,
    /// `yacl.isInstalled()`: the settings button exists.
    pub settings_installed: bool,
    /// `packets.readySetV2.isSupported()`: the more-options button exists.
    pub ready_vote_supported: bool,
    pub lang: Lang<'a>,
    /// `Font.getTextWidth`.
    pub text_width: &'a dyn Fn(&str) -> i32,
}

impl MainView<'_> {
    fn options(&self) -> &[VoteOption] {
        match self.ready {
            Some(ready) => ready.vote_options.as_slice(),
            None => &[],
        }
    }
}

/// `Math.round` (half up), which `roundToInt` uses.
fn round_half_up(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// `setAlpha(color, a)`: the colour's RGB with `a` as its alpha.
fn set_alpha(rgb: u32, a: i32) -> u32 {
    (((a as u32) & 0xff) << 24) | (rgb & 0x00ff_ffff)
}

fn tr(lang: Lang<'_>, key: &str) -> String {
    lang(key).unwrap_or_else(|| key.to_string())
}

fn tr_args(lang: Lang<'_>, key: &str, args: &[String]) -> String {
    format_template(&tr(lang, key), args)
}

/// `primaryVoteOption`: the option whose id is the pending vote.
fn primary_vote<'a>(view: &MainView<'a>) -> Option<&'a VoteOption> {
    let ready = view.ready?;
    let pending = ready.vote_pending?;
    ready.vote_options.iter().find(|option| option.id == pending)
}

fn total_nanos(d: JavaDuration) -> i128 {
    d.seconds as i128 * 1_000_000_000 + d.nanos as i128
}

fn from_nanos(n: i128) -> JavaDuration {
    JavaDuration { seconds: n.div_euclid(1_000_000_000) as i64, nanos: n.rem_euclid(1_000_000_000) as i32 }
}

/// `Duration.formatString()`: "3d 2h 01m 32s", the day and hour parts left out when zero.
pub fn format_duration(d: JavaDuration) -> String {
    let n = total_nanos(d);
    let sign = if n < 0 { "-" } else { "" };
    let secs = n.abs() / 1_000_000_000;
    let (days, hours) = (secs / 86_400, secs % 86_400 / 3_600);
    let (minutes, seconds) = (secs % 3_600 / 60, secs % 60);
    let mut out = sign.to_string();
    if days > 0 {
        out.push_str(&format!("{days}d "));
    }
    if hours > 0 {
        out.push_str(&format!("{hours}h "));
    }
    out.push_str(&format!("{minutes:02}m {seconds:02}s"));
    out
}

/// `Duration.formatStringSmall()`: "2h", "1m", "10s", or "0.500s" under a second (`%.3f`, half up).
pub fn format_duration_small(d: JavaDuration) -> String {
    let n = total_nanos(d);
    let sign = if n < 0 { "-" } else { "" };
    let abs = n.abs();
    let secs = abs / 1_000_000_000;
    let (days, hours) = (secs / 86_400, secs % 86_400 / 3_600);
    let (minutes, seconds) = (secs % 3_600 / 60, secs % 60);
    let body = if days > 0 {
        format!("{days}d")
    } else if hours > 0 {
        format!("{hours}h")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else if seconds > 0 {
        format!("{seconds}s")
    } else {
        let ms = (abs % 1_000_000_000 + 500_000) / 1_000_000;
        format!("{}.{:03}s", ms / 1000, ms % 1000)
    };
    format!("{sign}{body}")
}

/// `ScreenState::open`: the screen as the Kotlin constructor leaves it. The tab comes from `GameOver.tab`.
impl ScreenState {
    pub fn open(now_ms: u64, tab: EndScreenTab, can_escape: bool) -> Self {
        ScreenState {
            tab,
            can_escape,
            closing: false,
            more_options_open: false,
            sending_ready: false,
            client_selected: None,
            primary_title: None,
            primary_active: true,
            screen_interpolation: 0.0,
            interp: Interp::new(0.0, 1.0, ANIM_MS, now_ms, Easing::ExpOut),
        }
    }

    /// `requestClose`: starts the closing animation once. A second request does not restart it.
    pub fn request_close(&mut self, now_ms: u64) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.interp.easing = Easing::ExpOutReverse;
        self.interp.started_at_ms = now_ms;
    }

    /// The Escape key. `shouldCloseOnEsc()` is `canEscape`; it closes the screen even with the more-options list
    /// open (the framework's `onClose`, which the Kotlin does not route through the fold-away button).
    /// Returns whether the key closed the screen.
    pub fn escape(&mut self, now_ms: u64) -> bool {
        if self.can_escape {
            self.request_close(now_ms);
            true
        } else {
            false
        }
    }

    /// `shouldPause()`: `config.client.cardPausesGame && !sendingReadyPacket`.
    pub fn should_pause(&self, card_pauses_game: bool) -> bool {
        card_pauses_game && !self.sending_ready
    }

    /// `getBlurriness()`: the screen's own interpolation at `now_ms`.
    pub fn blurriness(&self, now_ms: u64) -> f32 {
        self.interp.get(now_ms)
    }

    /// Selects a tab by its index. Selecting the tab already shown changes nothing.
    pub fn select_tab(&mut self, index: usize) {
        if let Some(tab) = TABS.get(index) {
            self.tab = *tab;
        }
    }

    /// One frame's bookkeeping, in the order `BingoHudScreen.render` does it.
    pub fn frame(&mut self, view: &MainView<'_>) -> Frame {
        let now = view.now_ms;
        if view.game_over.is_some() && !view.state_has_game_over {
            self.request_close(now);
        }
        let relayout = self.interp.update(now);
        if relayout {
            self.screen_interpolation = self.interp.get(now);
        }
        if self.more_options_open {
            // No options left: the list closes itself, and the primary branch does not run this frame.
            if view.options().is_empty() {
                self.more_options_open = false;
            }
        } else {
            match primary_vote(view) {
                Some(vote) => {
                    self.primary_title = Some(flatten(&vote.title, view.lang));
                    self.primary_active = self.client_selected != Some(vote.id);
                }
                None => self.primary_active = view.ready.is_some_and(|ready| !ready.is_ready),
            }
        }
        Frame { relayout, close: self.closing && self.interp.is_done(now) }
    }

    /// A click on a button or tab, by its [`Action`]. Changes the state and says what the host must do.
    pub fn click(&mut self, action: Action, view: &MainView<'_>) -> Vec<Effect> {
        match action {
            Action::Close => {
                if self.more_options_open {
                    self.more_options_open = false;
                } else {
                    self.request_close(view.now_ms);
                }
                Vec::new()
            }
            Action::Settings => vec![Effect::OpenSettings],
            Action::MoreOptions => {
                self.more_options_open = true;
                Vec::new()
            }
            Action::Ready(vote) => {
                // `sendReady`: the chosen option is selected, every other one is not.
                self.client_selected = vote;
                self.sending_ready = true;
                vec![Effect::SendReady(vote)]
            }
            Action::Tab(index) => {
                self.select_tab(index);
                Vec::new()
            }
        }
    }
}

/// `isShowingTabs && !isClosing`: whether the tab strip is on screen.
pub fn tabs_shown(state: &ScreenState, view: &MainView<'_>) -> bool {
    let Some(game_over) = view.game_over else { return false };
    !state.closing
        && (game_over.scores.len() > 1 || state.tab != EndScreenTab::Cards || game_over.default_tab != EndScreenTab::Cards)
}

/// Where every piece of the screen is. `None` is a piece that is not on screen (the Kotlin does not add it).
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub sidebar_width: i32,
    pub sidebar_x: i32,
    /// One rectangle per tab of [`TABS`] when the strip is shown, else empty.
    pub tabs: Vec<Rect>,
    /// `cardsWidget`, present on the CARDS tab.
    // TODO(integration): `screen_cards` draws the card in this slot; the slide from its HUD position
    // (`originalCardPos`, `interpolateX/Y/Scale`) is the card's job, and `screen_placement` if it owns that slide.
    pub card: Option<CardSlot>,
    /// `scoresWidget`, present on the SCORES tab while not closing.
    // TODO(integration): `screen_end` draws the end scores in this slot, with its own scroll and clicks.
    pub scores: Option<Rect>,
    /// The sidebar's scrolling message history, its height up to the buttons.
    // TODO(integration): the message history is drawn in this slot by a history module (no module is named for it yet;
    // `screen_cards` or a new `screen_history`). Its scroll wheel and clicks route here too.
    pub messages: Rect,
    pub close: Rect,
    pub settings: Option<Rect>,
    pub more: Option<Rect>,
    pub primary: Option<Rect>,
    /// The vote options, from the top of the list down.
    pub options: Vec<(u128, Rect)>,
    /// `lastButtonY`: the top of the lowest button, where the subtitle sits.
    pub last_button_y: i32,
}

/// `Mth.roundToward(value, factor)`: `value` up to the next multiple of `factor`.
fn round_toward(value: i32, factor: i32) -> i32 {
    -((-value).div_euclid(factor)) * factor
}

/// `updatePositions()`, for the frame's state.
pub fn layout(state: &ScreenState, view: &MainView<'_>) -> Layout {
    let (w, h) = (view.width, view.height);
    let sidebar_width = (w as f32 * SIDEBAR_FRACTION) as i32 + SIDEBAR_BASE;
    let sidebar_x = w - round_half_up(state.screen_interpolation * sidebar_width as f32);
    let main_w = w - sidebar_width;
    let tabs_half = TABS_HEIGHT / 2;

    let tabs = if tabs_shown(state, view) {
        // `MenuTabBar.arrangeElements`: the strip is at most 400 wide, less a 14 px margin at each side. The tabs
        // share it evenly, each rounded up to an even width, and the row sits at the rounded-up half of the rest.
        let n = TABS.len() as i32;
        let inner = main_w.min(TABS_MAX_WIDTH) - 2 * TABS_MARGIN;
        let tab_w = round_toward(inner / n, 2);
        let x0 = round_toward((main_w - inner) / 2, 2);
        (0..n).map(|i| Rect { x: x0 + i * tab_w, y: 0, w: tab_w, h: TABS_HEIGHT }).collect()
    } else {
        Vec::new()
    };

    let card = (state.tab == EndScreenTab::Cards).then(|| CardSlot {
        x: main_w as f32 / 2.0,
        y: h as f32 / 2.0 + tabs_half as f32,
        w: main_w,
        h: CARD_HEIGHT,
    });

    let scores = (state.tab == EndScreenTab::Scores && !state.closing && view.game_over.is_some()).then(|| Rect {
        x: (main_w as f32 * 0.1) as i32,
        y: h / 2 - CARD_HEIGHT / 2 + tabs_half,
        w: (main_w as f32 * 0.8) as i32,
        h: CARD_HEIGHT,
    });

    let button_w = sidebar_width - MARGIN_X * 2;
    let settings_visible = view.settings_installed && !state.more_options_open;
    let close_w = if settings_visible { button_w / 2 - 1 } else { button_w };
    let mut y = h - BUTTON_HEIGHT - BOTTOM_GAP;
    let close = Rect { x: sidebar_x + MARGIN_X, y, w: close_w, h: BUTTON_HEIGHT };
    let settings =
        settings_visible.then(|| Rect { x: sidebar_x + MARGIN_X + close_w + 2, y, w: close_w, h: BUTTON_HEIGHT });

    let more_visible = view.game_over.is_some()
        && view.ready_vote_supported
        && view.options().len() > 1
        && !state.more_options_open;
    let more = more_visible.then(|| {
        y -= MARGIN_Y + BUTTON_HEIGHT;
        Rect { x: sidebar_x + MARGIN_X, y, w: button_w, h: BUTTON_HEIGHT }
    });

    let vote_available = view.game_over.is_some_and(|g| g.is_ready_vote_available)
        || view.ready.is_some_and(|ready| ready.can_send_ready);
    let primary_visible = view.game_over.is_some() && vote_available && !state.more_options_open;
    let primary = primary_visible.then(|| {
        y -= MARGIN_Y + PRIMARY_HEIGHT;
        Rect { x: sidebar_x + MARGIN_X, y, w: button_w, h: PRIMARY_HEIGHT }
    });

    let mut options = Vec::new();
    if state.more_options_open {
        // `reversed()`: the last option is the lowest, so the first option ends up at the top.
        for vote in view.options().iter().rev() {
            y -= MARGIN_Y + PRIMARY_HEIGHT;
            options.push((vote.id, Rect { x: sidebar_x + MARGIN_X, y, w: button_w, h: PRIMARY_HEIGHT }));
        }
        options.reverse();
    }

    Layout {
        sidebar_width,
        sidebar_x,
        tabs,
        card,
        scores,
        messages: Rect { x: sidebar_x + MARGIN_X, y: 0, w: sidebar_width - MARGIN_X * 2, h: y - 10 },
        close,
        settings,
        more,
        primary,
        options,
        last_button_y: y,
    }
}

/// The clickable rectangles of the frame, in the order the Kotlin adds them. Inactive buttons do not react: the
/// primary button while not active, and a vote option the client already chose.
pub fn hits(state: &ScreenState, view: &MainView<'_>, layout: &Layout) -> Vec<Hit> {
    let mut out = Vec::new();
    let mut push = |r: Rect, action: Action| out.push(Hit { x: r.x, y: r.y, w: r.w, h: r.h, action });
    for (i, r) in layout.tabs.iter().enumerate() {
        push(*r, Action::Tab(i));
    }
    if let Some(r) = layout.primary {
        if state.primary_active {
            push(r, Action::Ready(primary_vote(view).map(|v| v.id)));
        }
    }
    if let Some(r) = layout.more {
        push(r, Action::MoreOptions);
    }
    for (id, r) in &layout.options {
        if state.client_selected != Some(*id) {
            push(*r, Action::Ready(Some(*id)));
        }
    }
    push(layout.close, Action::Close);
    if let Some(r) = layout.settings {
        push(r, Action::Settings);
    }
    out
}

/// The draw commands of this screen. The card and the scores are not here: their slots are in the [`Layout`].
// TODO(integration): `BingoEndFireworkRenderer` (the win fireworks over the card on the first open of a won game) has no
// module yet; it draws at the card's centre, `((width - sidebar) / 2, height / 2)`, when `gameOver.isFirstOpen`.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenDraw {
    /// A command the card drawing already has (a fill, a sprite, text at a whole pixel, an item).
    Base(Draw),
    /// `drawNinePatch`: a sprite stretched in nine slices, `slice` pixels at each edge of a `texture_w x texture_h`
    /// texture.
    NinePatch { name: String, x: i32, y: i32, w: i32, h: i32, slice: i32, texture_w: i32, texture_h: i32 },
    /// Text drawn at `anchor + R(rotation) * (scale * offset)`, shadowed when `shadow`. A text in a `matrices` push
    /// is drawn at the offset in the frame of its scales and rotation (`rotation` in radians, as the Kotlin's `rotate`).
    Label {
        text: String,
        anchor: (f32, f32),
        offset: (f32, f32),
        scale: f32,
        rotation: f32,
        argb: u32,
        shadow: bool,
    },
    /// A square of half side `half` centred on (`cx`, `cy`), turned by `rotation` radians.
    // TODO(integration): the host needs a rotated fill; `Draw::Fill` is axis-aligned only.
    Square { cx: f32, cy: f32, half: f32, rotation: f32, argb: u32 },
}

/// The scale of the end-screen titles: `((titleY - 24) / 14)` between 1 and 2.
fn title_scale(anchor_y: i32) -> f32 {
    ((anchor_y - TABS_HEIGHT) as f32 / 14.0).clamp(1.0, 2.0)
}

/// The y of the duration and win-streak group: `titleY - 20`, where `titleY` is the card's top times 3/4 (or two
/// lines above the card), plus the tab strip's half height.
fn title_anchor(height: i32) -> i32 {
    let card_top = height / 2 - CARD_HEIGHT / 2;
    let title_y = round_half_up(card_top as f32 * 0.75).min(card_top - 11 * 2) + TABS_HEIGHT / 2;
    title_y - 20
}

/// `(now - endedAt) / 2 s`, clamped and squared: how far the "New best" line has grown in.
fn grow_in(now_ms: u64, ended_at_ms: i64) -> f32 {
    let since = now_ms as i64 - ended_at_ms;
    let t = (since as f64 / NEW_BEST_MS).clamp(0.0, 1.0) as f32;
    t * t
}

/// `ClientCardRenderer.getWinColor(now)`: one colour a second.
fn win_color(now_ms: u64) -> u32 {
    WIN_COLORS[((now_ms / 1000) % WIN_COLORS.len() as u64) as usize]
}

/// `text.centred`: a string drawn with its width's half to the left of the anchor, as `-width / 2`.
fn centred_offset(width: i32) -> f32 {
    ((-width) / 2) as f32
}

fn label(text: String, anchor: (f32, f32), offset: (f32, f32), scale: f32, rotation: f32, argb: u32) -> ScreenDraw {
    ScreenDraw::Label { text, anchor, offset, scale, rotation, argb, shadow: true }
}

/// `renderGameDuration`: the duration, its title and the best time, rotated by -0.25 rad, left of the card.
fn duration_group(out: &mut Vec<ScreenDraw>, state: &ScreenState, view: &MainView<'_>, game_over: &GameOverPacket, x: f32, anchor: i32, scale: f32, alpha: i32) {
    let rot = -0.25;
    let ay = anchor as f32;
    let duration = format_duration(game_over.duration);
    let w = (view.text_width)(&duration);
    out.push(label(duration, (x, ay), (centred_offset(w), 0.0), scale, rot, set_alpha(DURATION_RGB, alpha)));

    let half = scale * 0.5;
    let title = tr(view.lang, KEY_DURATION);
    let tw = (view.text_width)(&title);
    out.push(label(title, (x, ay), (centred_offset(tw), 18.0), half, rot, set_alpha(WHITE, alpha)));

    if let Some(prev) = game_over.prev_best_time {
        if game_over.is_best_time {
            let anim = grow_in(view.now_ms, game_over.ended_at_millis);
            let best_scale = 2.0 - anim;
            let diff = format_duration_small(from_nanos(total_nanos(game_over.duration) - total_nanos(prev)));
            let text = tr_args(view.lang, KEY_BEST_NEW, &[diff]);
            let bw = (view.text_width)(&text);
            let a = ((state.screen_interpolation * anim * 255.0) as i32).max(6);
            out.push(label(text, (x, ay), (centred_offset(bw), 30.0), half * best_scale, rot, set_alpha(win_color(view.now_ms), a)));
        } else {
            let text = tr_args(view.lang, KEY_BEST, &[format_duration(prev)]);
            let bw = (view.text_width)(&text);
            out.push(label(text, (x, ay), (centred_offset(bw), 30.0), half, rot, set_alpha(BEST_RGB, alpha)));
        }
    }
}

/// `renderWinStreak`: the streak, its title and the best streak, rotated by +0.25 rad, right of the card; and the
/// spinning square for a streak above one that won the game.
fn streak_group(out: &mut Vec<ScreenDraw>, state: &ScreenState, view: &MainView<'_>, game_over: &GameOverPacket, x: f32, anchor: i32, scale: f32, alpha: i32) {
    let Some(streak) = game_over.win_streak else { return };
    let rot = 0.25;
    let ay = anchor as f32;
    let text = streak.to_string();
    let w = (view.text_width)(&text);
    out.push(label(text, (x, ay), (centred_offset(w), 0.0), scale, rot, set_alpha(STREAK_RGB, alpha)));

    let half = scale * 0.5;
    let title = tr(view.lang, KEY_STREAK);
    let tw = (view.text_width)(&title);
    out.push(label(title, (x, ay), (centred_offset(tw), 18.0), half, rot, set_alpha(WHITE, alpha)));

    if let Some(best) = game_over.best_win_streak {
        if game_over.is_best_win_streak {
            let anim = grow_in(view.now_ms, game_over.ended_at_millis);
            let best_scale = 2.0 - anim;
            let text = tr(view.lang, KEY_STREAK_NEW);
            let bw = (view.text_width)(&text);
            let a = ((state.screen_interpolation * anim * 255.0) as i32).max(6);
            out.push(label(text, (x, ay), (centred_offset(bw), 30.0), half * best_scale, rot, set_alpha(win_color(view.now_ms), a)));
        } else {
            let text = tr_args(view.lang, KEY_STREAK_MAX, &[best.to_string()]);
            let bw = (view.text_width)(&text);
            out.push(label(text, (x, ay), (centred_offset(bw), 30.0), half, rot, set_alpha(BEST_RGB, alpha)));
        }
    }

    if streak > 1 && game_over.is_winner {
        // `now mod 2000 / 2000 * 2 PI`: one turn every two seconds. A 4 px square scaled by 4 * scale, 2 px off centre.
        let angle = (view.now_ms % 2000) as f32 / 2000.0 * 2.0 * PI;
        out.push(ScreenDraw::Square {
            cx: x,
            cy: ay + 12.0,
            half: 8.0 * scale,
            rotation: angle,
            argb: set_alpha(STREAK_RGB, alpha * 30 / 255),
        });
    }
}

/// The title texts of the end screen (`renderGameDuration`, `renderWinStreak`). Nothing without a game over.
fn titles(out: &mut Vec<ScreenDraw>, state: &ScreenState, view: &MainView<'_>, layout: &Layout) {
    let Some(game_over) = view.game_over else { return };
    let anchor = title_anchor(view.height);
    let scale = title_scale(anchor);
    let alpha = (255.0 * state.screen_interpolation) as i32;
    let main_w = view.width - layout.sidebar_width;

    // `decorationMaxWidth`: the duration's width, at least 40. The decoration sits 20 px in from the card's share.
    let duration_w = (view.text_width)(&format_duration(game_over.duration));
    let decoration_max = duration_w.max(40);
    let decoration_x = (main_w as f32 * 0.2 - 20.0).max(decoration_max as f32);
    let streak_x = main_w as f32 - decoration_x;

    duration_group(out, state, view, game_over, decoration_x, anchor, scale, alpha);
    streak_group(out, state, view, game_over, streak_x, anchor, scale, alpha);
}

/// The vote marks of one button: `round(votes * ratio)` dots, joined by spaces, centred under the text.
fn vote_marks(out: &mut Vec<ScreenDraw>, view: &MainView<'_>, rect: Rect, marks: i32) {
    if marks <= 0 {
        return;
    }
    let text = vec!["·"; marks as usize].join(" ");
    let tw = (view.text_width)(&text);
    out.push(ScreenDraw::Base(Draw::Text {
        x: rect.x + rect.w / 2 - tw / 2,
        y: rect.y + rect.h - 10,
        text,
        argb: WHITE,
    }));
}

/// `ButtonFactory.createNinePatchButton` drawn: the sprite by state, then the label centred in it.
fn button(out: &mut Vec<ScreenDraw>, view: &MainView<'_>, rect: Rect, textures: &Textures, texture_h: i32, active: bool, label_text: &str) {
    let hovered = rect.contains(view.mouse.0, view.mouse.1);
    let name = if !active {
        textures.inactive
    } else if hovered {
        textures.focused
    } else {
        textures.texture
    };
    out.push(ScreenDraw::NinePatch {
        name: name.to_string(),
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
        slice: SLICE,
        texture_w: 200,
        texture_h,
    });
    // TODO(verify): the label's offsets are vanilla's `ScrollableText` centring, from memory.
    // TODO(integration): a label wider than the button scrolls in vanilla; it is drawn from the left here.
    let tw = (view.text_width)(label_text);
    let x = rect.x + rect.w / 2 - tw / 2;
    let x = if tw > rect.w - 4 { rect.x + 2 } else { x };
    let y = rect.y + (rect.h - 9) / 2 + 1;
    out.push(ScreenDraw::Base(Draw::Text {
        x,
        y,
        text: label_text.to_string(),
        argb: if active { WHITE } else { GREY },
    }));
}

/// `renderVoteButtonIndicators`' per-option count: `round(votes * maxVotesCapped / max(maxVotes, 1))`.
fn option_marks(options: &[VoteOption]) -> impl Fn(&VoteOption) -> i32 + '_ {
    let max = options.iter().map(|o| o.votes).max().unwrap_or(0);
    let ratio = max.min(VOTES_MAX) as f32 / max.max(1) as f32;
    move |o: &VoteOption| round_half_up(o.votes as f32 * ratio)
}

/// Everything this screen draws, in the Kotlin's order: the titles, the sidebar, the tab labels, the subtitle, the
/// buttons with their labels and vote marks.
pub fn draw(state: &ScreenState, view: &MainView<'_>, layout: &Layout) -> Vec<ScreenDraw> {
    let mut out = Vec::new();
    titles(&mut out, state, view, layout);

    // `sidebarDrawable`: an even width, so the texture lines up.
    let even = if layout.sidebar_width % 2 == 0 { 0 } else { 1 };
    out.push(ScreenDraw::NinePatch {
        name: "bingo/game_sidebar_background".to_string(),
        x: layout.sidebar_x - even,
        y: 0,
        w: layout.sidebar_width + even,
        h: view.height,
        slice: 32,
        texture_w: 128,
        texture_h: 128,
    });

    // `TabButton`'s frame before its label: vanilla's tab sprites (130 x 24, nine-sliced with a 2 px side and top
    // border and no bottom one), not in the mod's atlas, so the host draws them by name. The selected tab is the
    // lighter one; hovering a tab lightens its frame.
    // `MenuTabBar.extractWidgetRenderState`: the header separator is drawn first, so the tabs cover it. It runs from the
    // left edge to the first tab and from the last tab to the strip's right edge, on the strip's bottom two rows.
    if let (Some(first), Some(last)) = (layout.tabs.first(), layout.tabs.last()) {
        let strip_w = view.width - layout.sidebar_width;
        for (x0, x1) in [(0, first.x), (last.x + last.w, strip_w)] {
            if x1 > x0 {
                let y = TABS_HEIGHT - 2;
                out.push(ScreenDraw::Base(Draw::Fill { x: x0, y, w: x1 - x0, h: 1, argb: SEPARATOR_LIGHT }));
                out.push(ScreenDraw::Base(Draw::Fill { x: x0, y: y + 1, w: x1 - x0, h: 1, argb: SEPARATOR_DARK }));
            }
        }
    }

    // `TabButton`s: the frame, then for the open tab its menu tile and focus underline, then the label. Every tab's
    // label is active, so both are white; the open tab's label sits 3 px higher, as the selected offset.
    for (i, rect) in layout.tabs.iter().enumerate() {
        let text = tr(view.lang, TAB_KEYS[i]);
        let tw = (view.text_width)(&text);
        let selected = TABS[i] == state.tab;
        let hovered = rect.contains(view.mouse.0, view.mouse.1);
        let frame = match (selected, hovered) {
            (true, true) => "widget/tab_selected_highlighted",
            (true, false) => "widget/tab_selected",
            (false, true) => "widget/tab_highlighted",
            (false, false) => "widget/tab",
        };
        out.push(ScreenDraw::NinePatch {
            name: frame.to_string(),
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            slice: 2,
            texture_w: 130,
            texture_h: 24,
        });
        if selected {
            // `renderMenuBackground` (2 px in from the left, top and right) and `renderFocusUnderline`: a 1 px line 2 px
            // above the bottom, as wide as the text (at most the tab less 4), centred.
            out.push(ScreenDraw::Base(Draw::Fill {
                x: rect.x + 2,
                y: rect.y + 2,
                w: rect.w - 4,
                h: rect.h - 2,
                argb: MENU_TILE,
            }));
            let line = tw.min(rect.w - 4);
            out.push(ScreenDraw::Base(Draw::Fill {
                x: rect.x + (rect.w - line) / 2,
                y: rect.y + rect.h - 2,
                w: line,
                h: 1,
                argb: WHITE,
            }));
        }
        // `renderLabel`: centred on the tab's middle, vertically between the top (3 px lower when not selected) and the
        // bottom, with a shadow (`acceptScrollingWithDefaultCenter`: `(top + bottom - 9) / 2 + 1`).
        let top = rect.y + if selected { 0 } else { 3 };
        let text_x = rect.x + rect.w / 2 - tw / 2;
        let text_y = (top + rect.y + rect.h - 9) / 2 + 1;
        out.push(label(text, (text_x as f32, text_y as f32), (0.0, 0.0), 1.0, 0.0, WHITE));
    }

    // `state.ready?.takeIf { isRunning }?.subtitle`, centred over the sidebar, just above the lowest button.
    if let Some(ready) = view.ready.filter(|r| r.is_running) {
        if let Some(subtitle) = &ready.subtitle {
            let text = flatten(subtitle, view.lang);
            let tw = (view.text_width)(&text);
            out.push(ScreenDraw::Base(Draw::Text {
                x: layout.sidebar_x + layout.sidebar_width / 2 - tw / 2,
                y: layout.last_button_y - 9,
                text,
                argb: SUBTITLE,
            }));
        }
    }

    let close_label = tr(view.lang, KEY_CLOSE);
    let close_active = true;
    button(&mut out, view, layout.close, &BUTTON_GENERIC, 20, close_active, &close_label);
    if let Some(r) = layout.settings {
        button(&mut out, view, r, &BUTTON_GENERIC, 20, true, &tr(view.lang, KEY_SETTINGS));
    }
    if let Some(r) = layout.more {
        button(&mut out, view, r, &BUTTON_GENERIC, 20, true, &tr(view.lang, KEY_MORE));
    }
    if let Some(r) = layout.primary {
        let text = state.primary_title.clone().unwrap_or_else(|| tr(view.lang, KEY_RETURN));
        button(&mut out, view, r, &BUTTON_CONTINUE, 40, state.primary_active, &text);
        // The primary button shows its vote's marks, capped at `VOTES_MAX` (not scaled, unlike the options).
        if let Some(vote) = primary_vote(view) {
            vote_marks(&mut out, view, r, vote.votes.min(VOTES_MAX));
        }
    }
    let marks = option_marks(view.options());
    for (id, rect) in &layout.options {
        let Some(vote) = view.options().iter().find(|o| o.id == *id) else { continue };
        let active = state.client_selected != Some(*id);
        button(&mut out, view, *rect, &BUTTON_CONTINUE, 40, active, &flatten(&vote.title, view.lang));
        vote_marks(&mut out, view, *rect, marks(vote));
    }
    out
}

//! The card screen as one object the host drives: it keeps the screen's state between frames, draws the screen's frame,
//! the cards widget, the end-scores tab and the message history through the host, and turns mouse and key events into
//! state changes and [`Effect`]s (a ready packet to send, say). The layouts it calls are the pure ports in `screen_main`,
//! `screen_cards`, `scores_draw` and `history_ctl`; this file only connects them. The settings and card-placement
//! screens are not reachable from here (the original opens them from a config screen this port does not have).

use crate::assets::Assets;
use crate::atlas::Atlas;
use crate::card_draw::truncate_to_width;
use crate::game_over::EndScreenTab;
use crate::history_ctl::HistoryCtl;
use crate::host::Host;
use crate::scores_draw::{self, scores_frame};
use crate::screen_cards::{self, Area, CardSlot, CardsFrame, CardsInput, CardsScroll};
use crate::screen_end::ScoresInput;
use crate::screen_main::{self, Action, Effect, MainView, ScreenState};
use crate::state::HudState;
use crate::text_plain::flatten;
use crate::ui_draw::{run, UiDraw};

#[derive(Debug, Default)]
pub struct ScreenController {
    state: Option<ScreenState>,
    scroll: Option<CardsScroll>,
    opened_at: u64,
    main_hits: Vec<screen_main::Hit>,
    card_hits: Vec<screen_cards::Hit>,
    area: Option<Area>,
    card_count: usize,
    history: HistoryCtl,
    history_area: Option<(i32, i32, i32, i32)>,
    scores_scroll: f32,
    scores_area: Option<(i32, i32, i32, i32)>,
    scores_content_height: i32,
    /// The closing animation has finished: the host must close the mod screen.
    close_requested: bool,
    /// The win fireworks over the card (`BingoEndFireworkRenderer`), on the first open of a game the player's team won.
    fireworks: Option<crate::fireworks::Fireworks>,
    /// The team whose card slides in from the HUD position (`originalCardKey`, `originalCardPos`), when opened from the HUD.
    entrance_team: Option<crate::state::TeamKey>,
}

fn inside(area: (i32, i32, i32, i32), mouse: (i32, i32)) -> bool {
    mouse.0 >= area.0 && mouse.0 < area.0 + area.2 && mouse.1 >= area.1 && mouse.1 < area.1 + area.3
}

/// `interpolateDuration` of the card's slide from the HUD.
const ENTRANCE_MS: u64 = 150;

/// Runs `cmds`, with each span `cmds[a..b]` of `spans` (in order, not overlapping) faded to `opacity`
/// (`cardsWidget.opacity`): the host multiplies the alpha of the vertices drawn in it. No span is faded once no alpha
/// byte would change, which `scale_alpha` gives for `255 * opacity >= 254.5`; then the host is not asked at all.
pub fn run_faded(cmds: &[UiDraw], spans: &[(usize, usize)], opacity: f32, atlas: &Atlas, host: &mut dyn Host) {
    if 255.0 * opacity >= 254.5 {
        run(cmds, atlas, host);
        return;
    }
    let mut at = 0;
    for &(start, end) in spans.iter().filter(|(s, e)| s < e) {
        run(&cmds[at..start], atlas, host);
        let mark = host.begin_alpha();
        run(&cmds[start..end], atlas, host);
        host.end_alpha(mark, opacity);
        at = end;
    }
    run(&cmds[at..], atlas, host);
}

/// The parts of `whole` that `holes` (in order, inside `whole`) do not cover, in order and none of them empty. Empty
/// holes cut nothing.
pub fn outside(whole: (usize, usize), holes: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut parts = Vec::new();
    let mut at = whole.0;
    for &(start, end) in holes.iter().filter(|(s, e)| s < e) {
        if start > at {
            parts.push((at, start));
        }
        at = at.max(end);
    }
    if at < whole.1 {
        parts.push((at, whole.1));
    }
    parts
}

/// The cards' commands of one frame in paint order: each card's draws and then its label, so a card painted later
/// covers the label of one painted before it (`BingoCardsWidget` draws the winner's label inside the card's own pass).
/// The scroll bar's draws come last. Also the parts of those commands that fade with the screen (`outside`): every
/// card but the entrance card, which keeps its full alpha together with its label.
pub fn card_commands(cards: &CardsFrame) -> (Vec<UiDraw>, Vec<(usize, usize)>) {
    let mut cmds = Vec::with_capacity(cards.draws.len() + cards.labels.len());
    let mut holes = Vec::new();
    for (i, span) in cards.card_spans.iter().enumerate() {
        let start = cmds.len();
        cmds.extend(cards.draws[span.draws.0..span.draws.1].iter().map(UiDraw::from));
        cmds.extend(cards.labels[span.labels.0..span.labels.1].iter().map(UiDraw::from));
        if cards.entrance_card == Some(i) {
            holes.push((start, cmds.len()));
        }
    }
    let cards_end = cards.card_spans.last().map_or(0, |span| span.draws.1);
    cmds.extend(cards.draws[cards_end..].iter().map(UiDraw::from));
    let faded = outside((0, cmds.len()), &holes);
    (cmds, faded)
}

fn epoch_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

impl ScreenController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.state.is_some()
    }

    /// `openCardScreen`: the screen opens on a tab, escapable or not (the end screen is not).
    pub fn open(&mut self, now_ms: u64, tab: EndScreenTab, can_escape: bool) {
        self.state = Some(ScreenState::open(now_ms, tab, can_escape));
        self.scroll = None;
        self.history = HistoryCtl::new();
        self.scores_scroll = 0.0;
        self.opened_at = now_ms;
        self.close_requested = false;
        self.entrance_team = None;
    }

    /// The card of `team` slides in from where the HUD draws it (the top left, at scale 1) for 150 ms (`interpolateDuration`).
    pub fn set_entrance(&mut self, team: crate::state::TeamKey) {
        self.entrance_team = Some(team);
    }

    /// Starts the win fireworks (`fireworkRenderer` is created for `gameOver.isFirstOpen && packet.isWinner`).
    pub fn start_fireworks(&mut self, now_ms: u64, seed: u64) {
        self.fireworks = Some(crate::fireworks::Fireworks::new(now_ms, seed));
    }

    /// The host closed the screen (Horizon's own Escape, say): forget it.
    pub fn closed_by_host(&mut self) {
        self.state = None;
        self.scroll = None;
    }

    /// True once after the screen finished closing itself: the host then calls `mod_screen_close`.
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.close_requested)
    }

    /// Whether Escape is handled by the screen (it closes with an animation) rather than by the game.
    pub fn escape(&mut self, now_ms: u64) -> bool {
        match self.state.as_mut() {
            Some(state) => state.escape(now_ms),
            None => false,
        }
    }

    /// Draws one frame inside `screen_render_post`. `atlas` holds the mod's sprites and the card pictures.
    pub fn render(&mut self, hud: &HudState, assets: &Assets, atlas: &Atlas, host: &mut dyn Host, gui: (i32, i32), mouse: (i32, i32)) {
        let Some(mut state) = self.state.take() else { return };
        let now = hud.now();
        let lang = |k: &str| assets.translate(k);
        let width_of = |s: &str| host.text_width(s);
        let view = MainView {
            width: gui.0,
            height: gui.1,
            now_ms: now,
            mouse,
            game_over: hud.game_over.as_ref(),
            state_has_game_over: hud.game_over.is_some(),
            ready: hud.ready.as_ref(),
            settings_installed: false,
            ready_vote_supported: true,
            lang: &lang,
            text_width: &width_of,
        };
        let frame = state.frame(&view);
        if frame.close {
            self.close_requested = true;
            self.scroll = None;
            return;
        }
        let layout = screen_main::layout(&state, &view);
        self.main_hits = screen_main::hits(&state, &view, &layout);
        let mut cmds: Vec<UiDraw> = screen_main::draw(&state, &view, &layout).iter().map(UiDraw::from).collect();

        // The message history list in the sidebar.
        let m = layout.messages;
        let history_area = (m.x, m.y, m.w, m.h);
        self.history_area = Some(history_area);
        cmds.extend(self.history.frame(hud, history_area, mouse, assets, &lang, &width_of).draws);

        self.card_hits.clear();
        self.scores_area = None;
        let mut tooltip = None;
        // The parts of `cmds` the cards drew (and the scroll bar), to be faded by `run_faded`.
        let mut faded: Option<Vec<(usize, usize)>> = None;
        match (layout.card, layout.scores, state.tab) {
            (Some(slot), _, EndScreenTab::Cards) => {
                let area = Area { x: slot.x, y: slot.y, width: slot.w, height: slot.h };
                let order: Vec<_> = hud.screen_cards().iter().collect();
                let formatting = |c: &crate::state::ClientCard| c.display.team_color.clone();
                let colors: Vec<_> = order.iter().map(|(k, c)| assets.team_colors(k.as_deref(), formatting(c).as_deref())).collect();
                let names: Vec<Option<String>> = order
                    .iter()
                    .map(|(_, c)| c.display.team_name.as_ref().map(|t| flatten(t, &lang)).map(|n| truncate_to_width(&n, 70, &width_of)))
                    .collect();
                let slots: Vec<CardSlot<'_>> = order
                    .iter()
                    .enumerate()
                    .map(|(i, (k, c))| CardSlot { team: (*k).clone(), card: c, colors: &colors[i], team_name: names[i].clone() })
                    .collect();
                let outline = |team: &str| assets.team_colors(Some(team), None).outline;
                let known = |name: &str| atlas.contains(name);
                let winner = hud.game_over.as_ref().and_then(|g| g.winner.as_deref());
                // `targetScrollX = initialScollX`: the row starts on the winner and stays there.
                let scroll = *self.scroll.get_or_insert_with(|| CardsScroll::starting_at(screen_cards::initial_scroll(&slots, winner)));
                let input = CardsInput {
                    area,
                    slots: &slots,
                    winner,
                    entrance: self.entrance_team.clone().map(|team| screen_cards::Entrance {
                        team,
                        from_x: 0.0,
                        from_y: 0.0,
                        from_scale: 1.0,
                        duration_ms: ENTRANCE_MS,
                    }),
                    elapsed_ms: now.saturating_sub(self.opened_at),
                    now_ms: now,
                    // The fade is applied to the drawn vertices by `run_faded`. The scroll bar takes its alpha from this
                    // value too, so it stays at full here, or it would be faded twice.
                    opacity: 1.0,
                    // `onViewClick` is null on the end screen: no hover outline, no selecting a card.
                    selectable: slots.len() > 1 && hud.game_over.is_none(),
                    mouse,
                    show_outlines: true,
                    team_outline: &outline,
                    items: assets,
                    images: Some(&known),
                    lang: &lang,
                    text_width: &width_of,
                };
                let cards = screen_cards::frame(&input, &scroll);
                // Each card's draws and its label in paint order, then the scroll bar. The card sliding in from the HUD
                // keeps its full alpha while its entrance runs: the original sets the shader colour only for the cards
                // whose team is not `interpolateKey`. Every other card is faded.
                let (card_cmds, card_faded) = card_commands(&cards);
                let first = cmds.len();
                cmds.extend(card_cmds);
                faded = Some(card_faded.into_iter().map(|(a, b)| (first + a, first + b)).collect());
                self.card_hits = cards.hits;
                // `isMouseOver` needs `opacity > 0.5` in the original, so the tile tooltip waits for that too.
                tooltip = cards.tooltip.filter(|_| state.screen_interpolation > 0.5);
                self.scroll = Some(cards.scroll);
                self.area = Some(area);
                self.card_count = slots.len();
            }
            (_, Some(rect), EndScreenTab::Scores) => {
                if let Some(over) = hud.game_over.as_ref() {
                    let input = ScoresInput { game_over: over, cards: hud.screen_cards(), lang: &lang, text_width: &width_of, width: rect.w, padding: 4 };
                    let area = (rect.x, rect.y, rect.w, rect.h);
                    let frame = scores_frame(&input, area, self.scores_scroll, mouse, epoch_ms());
                    self.scores_content_height = frame.content_height;
                    self.scores_scroll = scores_draw::clamp_scroll(self.scores_scroll, frame.content_height, rect.h);
                    cmds.extend(frame.draws);
                    self.scores_area = Some(area);
                }
            }
            _ => {}
        }
        // `fireworkRenderer?.render(drawService, (width - sidebarWidth) / 2, height / 2, now)`, over everything.
        if let Some(fw) = self.fireworks.as_mut() {
            cmds.extend(fw.draw(now, (gui.0 - layout.sidebar_width) / 2, gui.1 / 2));
            if fw.is_done() {
                self.fireworks = None;
            }
        }
        match faded {
            Some(spans) => run_faded(&cmds, &spans, state.screen_interpolation, atlas, host),
            None => run(&cmds, atlas, host),
        }
        if let Some(t) = tooltip {
            host.draw_tooltip(&t.lines, t.mouse.0 + 12, t.mouse.1 - 12);
        }
        self.state = Some(state);
    }

    /// A click: the first thing under the mouse wins (the screen's own buttons, then the cards).
    pub fn mouse_press(&mut self, hud: &mut HudState, assets: &Assets, width_of: &dyn Fn(&str) -> i32, gui: (i32, i32), mouse: (i32, i32)) -> Vec<Effect> {
        let Some(mut state) = self.state.take() else { return Vec::new() };
        let lang = |k: &str| assets.translate(k);
        let hit = self.main_hits.iter().find(|h| mouse.0 >= h.x && mouse.0 < h.x + h.w && mouse.1 >= h.y && mouse.1 < h.y + h.h).map(|h| h.action);
        let mut effects = Vec::new();
        if let Some(action) = hit {
            let view = MainView {
                width: gui.0,
                height: gui.1,
                now_ms: hud.now(),
                mouse,
                game_over: hud.game_over.as_ref(),
                state_has_game_over: hud.game_over.is_some(),
                ready: hud.ready.as_ref(),
                settings_installed: false,
                ready_vote_supported: true,
                lang: &lang,
                text_width: width_of,
            };
            effects = state.click(action, &view);
            if matches!(action, Action::Tab(_)) {
                self.scroll = None;
                self.scores_scroll = 0.0;
            }
        } else if let Some(area) = self.history_area.filter(|a| inside(*a, mouse)) {
            self.history.drag_start(mouse, area);
        } else if let Some(card) = self.card_hits.iter().find(|h| h.contains(mouse)).map(|h| h.action.clone()) {
            match card {
                screen_cards::Action::SelectCard(team) => hud.selected_team = team,
                screen_cards::Action::Tile { .. } => {}
                screen_cards::Action::ScrollBar => {
                    if let (Some(scroll), Some(area)) = (self.scroll.as_mut(), self.area) {
                        screen_cards::drag(scroll, &area, self.card_count, (mouse.0 as f64, mouse.1 as f64));
                    }
                }
            }
        }
        self.state = Some(state);
        effects
    }

    /// The wheel scrolls what is under the mouse: the message list, the scores, or the row of cards.
    pub fn mouse_wheel(&mut self, mouse: (i32, i32), amount: f64) -> bool {
        if let Some(area) = self.history_area.filter(|a| inside(*a, mouse)) {
            return self.history.wheel(mouse, amount, area);
        }
        if let Some(area) = self.scores_area.filter(|a| inside(*a, mouse)) {
            self.scores_scroll = scores_draw::wheel(self.scores_scroll, amount, self.scores_content_height, area.3);
            return true;
        }
        match (self.scroll.as_mut(), self.area) {
            (Some(scroll), Some(area)) => screen_cards::wheel(scroll, &area, self.card_count, (mouse.0 as f64, mouse.1 as f64), amount),
            _ => false,
        }
    }

    pub fn mouse_drag(&mut self, mouse: (i32, i32)) -> bool {
        if let Some(area) = self.history_area {
            if self.history.drag(mouse, area) {
                return true;
            }
        }
        match (self.scroll.as_mut(), self.area) {
            (Some(scroll), Some(area)) => screen_cards::drag(scroll, &area, self.card_count, (mouse.0 as f64, mouse.1 as f64)),
            _ => false,
        }
    }

    pub fn mouse_release(&mut self) -> bool {
        let history = self.history.release();
        match self.scroll.as_mut() {
            Some(scroll) => screen_cards::release(scroll) || history,
            None => history,
        }
    }

    /// An arrow key (vanilla's key name) on the open screen: moves to the neighbouring card. True when it was used.
    pub fn key(&mut self, name: &str) -> bool {
        let key = match name {
            "key.keyboard.left" => screen_cards::Key::Left,
            "key.keyboard.right" => screen_cards::Key::Right,
            _ => return false,
        };
        match self.scroll.as_mut() {
            Some(scroll) => screen_cards::key(scroll, self.card_count, key),
            None => false,
        }
    }

    /// One animation step of the cards widget's scroll, called each client tick while the screen is open.
    pub fn tick(&mut self) {
        // The scroll steps once per rendered frame (`BingoCardsWidget.render`), not per tick.
    }
}

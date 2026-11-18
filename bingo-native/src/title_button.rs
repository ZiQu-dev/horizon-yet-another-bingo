//! The title screen's BINGO! button, through `host.request_world` (Horizon PR #17246). The mod declares
//! `[native] world_control`. The rule, the wording and the drawing are
//! `horizon_bingo::title_button`; this file asks Horizon and hands the hooks' events to them.
//!
//! A press on the button is cancelled (the title does not see it) and remembered; the next client tick asks Horizon
//! to open the lobby world: the title's next frame asks the player, and opens it on Yes. The answer's hint stays a few seconds on the title.
use super::{Mod, ReadOnly, log};
use horizon_bingo::title_button::{self as button, Hint, WorldAnswer};
use mod_api::{ClientHost, Flow, NativeWorldResult, ScreenMouseButtonPressedPreEvent, ScreenRenderPostEvent};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// How long a hint stays on the title screen.
const HINT_TIME: Duration = Duration::from_secs(6);

struct TitleState {
    /// A press on the button is waiting for the next client tick.
    clicked: bool,
    /// The hint of the last press Horizon did not act on, and when it was given.
    hint: Option<(Hint, Instant)>,
}

static STATE: Mutex<TitleState> = Mutex::new(TitleState { clicked: false, hint: None });

fn state() -> MutexGuard<'static, TitleState> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `NativeWorldResult` as the crate's answers.
fn answer(result: NativeWorldResult) -> WorldAnswer {
    match result {
        NativeWorldResult::Queued => WorldAnswer::Queued,
        NativeWorldResult::NotTitle => WorldAnswer::NotTitle,
        NativeWorldResult::NotPermitted => WorldAnswer::NotPermitted,
        NativeWorldResult::InvalidRequest => WorldAnswer::InvalidRequest,
        NativeWorldResult::Busy => WorldAnswer::Busy,
        _ => WorldAnswer::Unknown,
    }
}

/// The button over the title's main page: its frame, and the hint while it lasts.
pub(super) fn render(this: &Mod, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
    if event.screen.id != "title" {
        return;
    }
    let [_, _, width, height] = event.screen.rect;
    let hint = {
        let mut state = state();
        match &state.hint {
            Some((hint, given)) if given.elapsed() < HINT_TIME => Some(hint.clone()),
            _ => {
                state.hint = None;
                None
            }
        }
    };
    let client = this.lock_client();
    let lang = |key: &str| client.translate(key);
    let label = button::label_text(&lang);
    let hint = hint.map(|hint| button::hint_text(&hint, &lang));
    let mut view = ReadOnly::new(host, event.draw.size());
    let mouse = (event.screen.mouse[0], event.screen.mouse[1]);
    let sprites = client.title_button_sprites();
    button::draw(&mut view, button::button_rect(width, height), mouse, &label, hint.as_deref(), sprites);
    view.draw.flush(&mut *event.draw);
}

/// A left press on the button: cancelled, and remembered for the next tick. None for any other press.
pub(super) fn press(event: &ScreenMouseButtonPressedPreEvent<'_>) -> Option<Flow> {
    if event.screen.id != "title" || event.button != 0 {
        return None;
    }
    let [_, _, width, height] = event.screen.rect;
    let mouse = (event.screen.mouse[0], event.screen.mouse[1]);
    if !button::contains(button::button_rect(width, height), mouse) {
        return None;
    }
    state().clicked = true;
    Some(Flow::Cancel)
}

/// Once a client tick: asks Horizon for the lobby world when a press is waiting, and keeps the hint of its answer.
pub(super) fn tick(host: &mut dyn ClientHost) {
    if !std::mem::take(&mut state().clicked) {
        return;
    }
    let folder = button::LOBBY_FOLDER;
    let answer = answer(host.request_world(folder));
    log(&format!("title button: request_world({folder}) answered {answer:?}"));
    if let Some(hint) = button::hint_for(answer, folder) {
        state().hint = Some((hint, Instant::now()));
    }
}

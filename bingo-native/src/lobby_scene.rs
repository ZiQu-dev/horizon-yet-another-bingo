//! The lobby's in-world parts on a running server: the button panel's text displays and the team pads, and the right
//! clicks on them. What a click means and the command lines are `horizon-bingo`'s `server::lobby_scene`; this file runs them
//! against the mod API. The click hook only queues the click, and the next server tick runs it, so the menu and the team
//! change never start inside the game's own click handling.
use super::guarded;
use super::server;
use horizon_bingo::server::menu;
use horizon_bingo::server::lobby_scene::{self, Click, LOBBY_LEVEL};
use mod_api::{BlockClick, Flow, Hand, InteractionResult, LevelDimension, LogicalSide, ServerHost};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Mutex, PoisonError,
};

/// Clicks heard since the last tick: the player and what the block they clicked means.
static CLICKS: Mutex<Vec<(u128, Click)>> = Mutex::new(Vec::new());
/// The most clicks kept for one tick; more are dropped.
const CLICKS_MAX: usize = 64;
/// Whether the displays were made for this server run.
static SCENE_BUILT: AtomicBool = AtomicBool::new(false);
/// Ticks since the scene was last tried, and how many tries have failed.
static TICKS: AtomicU32 = AtomicU32::new(0);
static FAILED_TRIES: AtomicU32 = AtomicU32::new(0);
/// Ticks between two tries (a try needs the lobby's chunks around the displays loaded).
const TRY_EVERY: u32 = 40;
/// Tries before the scene is given up for this run (logged once).
const MAX_TRIES: u32 = 8;

/// A new server: the displays of the last one are not this one's, and no click is left.
pub fn server_about_to_start() {
    CLICKS.lock().unwrap_or_else(PoisonError::into_inner).clear();
    SCENE_BUILT.store(false, Ordering::Relaxed);
    TICKS.store(0, Ordering::Relaxed);
    FAILED_TRIES.store(0, Ordering::Relaxed);
}

/// The `right_click_block` hook, server side: a click of the main hand on the menu's signs or a team pad in the lobby is
/// taken (queued, and cancelled so the block does nothing else). The off hand's call of the same click is left alone.
pub fn right_click_block(click: &BlockClick<'_>, result: &mut InteractionResult) -> Flow {
    if !matches!(click.side, LogicalSide::Server) || !matches!(click.hand, Hand::Main) {
        return Flow::Continue;
    }
    if !matches!(&click.dimension, LevelDimension::Custom(key) if key.to_string() == LOBBY_LEVEL) {
        return Flow::Continue;
    }
    let Some(action) = lobby_scene::click_at(click.pos, click.hit.point) else {
        return Flow::Continue;
    };
    let mut clicks = CLICKS.lock().unwrap_or_else(PoisonError::into_inner);
    if clicks.len() < CLICKS_MAX {
        clicks.push((click.player, action));
    }
    *result = InteractionResult::Success;
    Flow::Cancel
}

/// One server tick: runs the queued clicks, and makes the displays once a player is in the lobby.
pub fn tick(host: &mut dyn ServerHost) {
    let clicks = std::mem::take(&mut *CLICKS.lock().unwrap_or_else(PoisonError::into_inner));
    for (player, action) in clicks {
        match action {
            Click::Menu => server::open_lobby_menu(host, player),
            Click::Start => server::panel_slot(host, player, menu::ROOT_START_SLOT),
            Click::Reroll => server::panel_slot(host, player, menu::ROOT_REROLL_SLOT),
            Click::Join(team) => {
                let _ = server::perform_command(host, player, &format!("join {team}"));
            }
        }
    }
    guarded("bingo lobby scene", || build_scene(host));
}

/// Makes the displays: the old ones (by tag) are removed first, so nothing doubles. Tried every `TRY_EVERY` ticks while a
/// player is in the lobby, until every display was summoned (a summon in an unloaded chunk fails).
fn build_scene(host: &mut dyn ServerHost) {
    if SCENE_BUILT.load(Ordering::Relaxed) || FAILED_TRIES.load(Ordering::Relaxed) >= MAX_TRIES {
        return;
    }
    if TICKS.fetch_add(1, Ordering::Relaxed) % TRY_EVERY != 0 || !host.runtime_levels().is_open(LOBBY_LEVEL) {
        return;
    }
    let someone_in_lobby = {
        let view: &dyn ServerHost = &*host;
        view.players()
            .server_players()
            .into_iter()
            .any(|player| player.dimension(view).is_some_and(|dimension| dimension.to_string() == LOBBY_LEVEL))
    };
    if !someone_in_lobby {
        return;
    }
    // Nothing to remove the first time: the kill fails then, which is fine.
    let _ = host.commands().run_command(&lobby_scene::clear_command());
    let all_made = lobby_scene::summon_commands()
        .iter()
        .fold(true, |made, line| host.commands().run_command(line) && made);
    if all_made {
        SCENE_BUILT.store(true, Ordering::Relaxed);
        super::log("bingo: the lobby's displays were made");
    } else if FAILED_TRIES.fetch_add(1, Ordering::Relaxed) + 1 >= MAX_TRIES {
        super::log("bingo: the lobby's displays could not be made; giving up for this server run");
    }
}

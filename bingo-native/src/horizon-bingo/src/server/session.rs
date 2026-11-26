//! Session: one bingo game on the server with its lobby and its teams. Before play, the teams and the ready timer
//! decide when the game starts (`start`, `ready`, `join`, `shuffleteams`); during play every team plays the same card,
//! and each team has its own progress, its own win and its own place in the ranking.
//!
//! Mirrors the original's `/bingo` commands, team rules, ready timer and scoring (`orig-teams-commands.md`). Pure logic:
//! everything goes out through [`ServerHost`], and the events of [`Game`] are turned into packets here.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::card_gen::{self, generate_card, load_pool, replace_squares, Pool, Rng};
use super::commands::{self, Command, VoteChoice};
use super::difficulty;
use super::game::{Event, Game, GameOptions, GameSnapshot};
use super::hidden_consume::{self, CardMode, HeldStack, Sight, TileView};
use super::infobook;
use super::host::ServerHost;
use super::kit::{self, KitCommand};
use super::map_items::{self, Flashes, MapItems, SquareState, Want, FILLED_MAP, MAP_IDS_FILE, PREVIEW_KEY};
use super::map_render::{CardView, IconSource, Overlays};
use super::texts::{self, Line, Span};
use super::world_reset::{self, Outcome, WorldPort, WorldReset};
use super::multicard::{
    complete_card, finish_card_on_goal, reroll_card, round_ends, settle_pending, stalemate_action, stalemate_reason,
    team_score, CardCompletion, CardState, Completion, CompletionOutcome, EndCause, Rules, StalemateAction,
    compare_teams, leading_team, StalemateBehavior, TeamRound, WinCondition,
};
use super::objectives::{self, Runtime};
use super::options::{EndWhen, Goal, Mode, Options};
use super::prefs::{self, Setting};
use super::ready::{encode_update_v2, postgame_packet, vote_for_id, ReadyOptions, ReadyTimer};
use super::stats::{self, Captures};
use super::store::{self, HistoryEntry, HistoryPlayer, HistoryTeam};
use super::scoring;
use super::teams::{self, Teams};
use super::types::{Card, Objective, Player, TeamProgress, CARD_SQUARES};
use super::wire;
use crate::buf::decode;
use crate::card_tile::TierLabel;
use crate::game::GameStatus;
use crate::game_over::ScoreRanking;
use crate::ready::SetReadyPacket;
use crate::update_settings::PlayerSettings;
use crate::score_update::TeamScore;
use crate::text::Text;
use serde_json::{json, Map, Value};

/// The entity tag that marks the players spread at the start for the moment the command runs.
const SPREAD_TAG: &str = "bingo_spread";
/// Files in the mod's data folder.
const OPTIONS_FILE: &str = "options.json";
const SETTINGS_FILE: &str = "player-settings.json";
const HISTORY_FILE: &str = "history.jsonl";
/// The game in progress, so a server restart does not lose the round. Written when play begins, after each scored
/// item, after a join during play, after a time limit change and after a resume. Emptied when the game ends or is
/// reset. Only a Playing snapshot is restored (see [`Session::restore_from_json`]).
const GAME_STATE_FILE: &str = "game-state.json";
/// Format version of `game-state.json`.
const GAME_STATE_VERSION: u64 = 1;
/// Milliseconds per server tick, to count the ticks a card has been in play.
const MS_PER_TICK: i64 = 50;
/// The command that readies a player up, as the lobby's hint names it.
const READY_COMMAND: &str = "/bingo ready";
/// The chat line when a stalemate rerolls a card.
const REROLL_NOTICE: &str = "A stalemate: every team on the card gets a new card.";
/// Mixed into the seed of the auto shuffle at the start, so it does not repeat the card's own shuffles.
const AUTO_SHUFFLE_SALT: u64 = 0x5EED_7EA5_0000_0001;
/// Server ticks between two reads of the objective squares that no event reports (stats, advancements): one second.
const OBJECTIVE_POLL_TICKS: u64 = 20;
/// Server ticks a player's lobby book waits, after the join, for the client's settings that tell whether it runs the
/// card HUD: three seconds. A client without the mod never answers, and gets its book when the wait ends.
const BOOK_HUD_WAIT_TICKS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// Seed of the first card. Each reset moves on to the next seed. The shuffles of `shuffleteams` start from it too.
    pub seed: u64,
    pub game: GameOptions,
    pub ready: ReadyOptions,
    /// `(distance, spread)` of `spreadplayers` at the start, used when there are two teams or more. `None` leaves
    /// the players where they stand.
    pub spread: Option<(i32, i32)>,
    /// Rules of the round: goal, mode, end condition and the toggles (changed by the `bingo` option commands).
    pub rules: Options,
    /// Seconds a resumed game is extended by when its time limit has run out (`extendRoundTimeLimitByMinutes`, 15 min).
    pub extend_secs: i32,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            seed: 0,
            game: GameOptions::default(),
            ready: ReadyOptions::default(),
            spread: None,
            rules: Options::default(),
            extend_secs: 15 * 60,
        }
    }
}

pub struct Session {
    options: SessionOptions,
    game: Game,
    pool: Pool,
    /// Seed for the next card made.
    seed: u64,
    /// Generator of the shuffles.
    rng: Rng,
    teams: Teams,
    ready: ReadyTimer,
    /// The lobby flow of a start: the players out of the vanilla worlds, the reset, and the players into the new game.
    world: WorldReset,
    /// Players who joined while the game was Pregame and have not been sent to the lobby yet (the lobby was not open,
    /// or the move was refused). The lobby gate sends them on the next ticks (`gather_in_lobby`).
    lobby_wanted: BTreeSet<u128>,
    /// The cards of the round, in the order they were made. The first is made when play begins; the others are made as
    /// teams need them (`multicard`). A one-card round has one entry, and every team plays it.
    cards: Vec<Card>,
    /// The playing tick each card was made at. A card completed within 3 ticks of it ends a multi-card game.
    card_started: Vec<u64>,
    /// The card index each team plays, by `BingoTeamKey.id`.
    team_card: BTreeMap<String, usize>,
    /// Each team's completions of cards, `(card index, counts as a win)`, in the order they happened.
    completions: BTreeMap<String, Vec<(usize, bool)>>,
    /// Teams that have finished the round, with the time in milliseconds they finished.
    finished: BTreeMap<String, u64>,
    /// Progress of each team that plays, by `BingoTeamKey.id`, on the card it plays.
    progress: BTreeMap<String, TeamProgress>,
    /// `ReplaceGoals`: the score each team has from the goals already replaced (`persistentScore`), by `BingoTeamKey.id`.
    /// Empty in the other rounds.
    persistent: BTreeMap<String, TeamScore>,
    /// Teams added to the server's team list by the start, removed again on reset.
    added_teams: Vec<String>,
    /// Boss bars shown, as (player, bar id), so they can be hidden at the end.
    boss_bars: BTreeSet<(u128, String)>,
    /// Server ticks so far, for the periodic packets.
    ticks: u64,
    /// Rules of the round.
    rules: Options,
    /// Lockout: which team owns each square, one map per card (indexed like `cards`).
    owners: Vec<HashMap<usize, String>>,
    /// Squares each team has seen (`teamsSeen`), by team key: a square is seen once a player of the team holds its item.
    /// It only grows during a round. Hidden-items mode reads it; it is saved only while that mode is on.
    seen: BTreeMap<String, Vec<usize>>,
    /// Teams that reached the goal, in the order they did.
    reached: Vec<String>,
    /// Postgame votes by player.
    votes: BTreeMap<u128, VoteChoice>,
    /// Ticks with nobody online while a game runs, for the reset when everyone has left.
    empty_ticks: u64,
    /// The data files have been read.
    loaded: bool,
    /// The players' saved settings: the file keeps them, `prefs` changes them, and a first join saves the defaults. A
    /// player with none has the defaults.
    settings: HashMap<u128, PlayerSettings>,
    /// Finished games, oldest first, from `history.jsonl`.
    history: Vec<HistoryEntry>,
    /// The game was ended by `/bingo end`, not by a win or the time limit.
    forfeit: bool,
    /// Why the game ends when no team won, for the end's chat (`announce_end`). Set just before the end, taken by it.
    end_reason: Option<Line>,
    /// The game's end as the sidebar shows it (`sidebar::End`): set when the game ends, cleared when it goes back to
    /// Pregame.
    sidebar_end: Option<super::sidebar::End>,
    /// Who won the game that ended (`Some(None)`: nobody), for the end screen sent again to a player who joins in
    /// Postgame. `None` while no game is over.
    end_winner: Option<Option<String>>,
    /// The seed the vanilla worlds were reset with at the last start (the end's chat and screen show it).
    world_seed: Option<u64>,
    /// A game restored from `game-state.json` waits for its players: the empty-server reset does not run until one
    /// of them has joined.
    await_rejoin: bool,
    /// The translatable reply of the last command, when its reply has a translation key (see [`Session::take_reply`]).
    reply: Option<Line>,
    /// Whether the events being dispatched come from the start itself (see `countdown_titles`).
    countdown_first: bool,
    /// The players are held still by the countdown's effects (`lobby_scene::freeze_commands`) and not yet released.
    frozen: bool,
    /// Items each player found this round, for the history and `stats` (see `stats::Captures`).
    captures: Captures,
    /// Each card's objective state (`objectives::Runtime`), indexed like `cards`. Read by a card with objective squares.
    objective_runtime: Vec<Runtime>,
    /// Each player's inventory as last reported (`on_inventory`), by player: what the objective squares are read from.
    held: HashMap<u128, HashMap<String, u32>>,
    /// Players whose inventory has been reported since the session started. An objective round is scored only once every
    /// online member has been reported, so a restored game never reads an unknown inventory as an empty one.
    reported: HashSet<u128>,
    /// The players' map items (`map_items`): the team cards, the lobby preview and the mementos.
    maps: MapItems,
    /// The squares that flash on the team maps, since they were scored.
    flashes: Flashes,
    /// The players given the lobby's tutorial book this session (`infobook`), so each gets one once.
    books_given: HashSet<u128>,
    /// Players whose client has sent its settings since they joined: such a client registered the mod's channels, so it
    /// runs the card HUD (the original's `CardViewService.supportsCardHud`). Not read from the saved settings, which
    /// keep the players of earlier sessions. Cleared when the player leaves.
    hud: HashSet<u128>,
    /// The tick each player joined at, for the lobby book's wait for the client's answer (see `give_lobby_books`).
    joined_at: HashMap<u128, u64>,
    /// Players the night vision controller has given the effect and not taken it back from. Not saved: after a restart a
    /// player keeps the effect they have until the controller next checks them.
    night_vision: BTreeSet<u128>,
    /// The tick each team's score last changed at, for the tie announcement (the original's `scoreUpdatedAt`).
    score_updated: BTreeMap<String, u64>,
}

/// A team's score and the round's leader before its progress changes. The change is announced from these and the
/// score after it (`ScoredItemCheck.updateScores` and `postTickCards`).
#[derive(Debug, Clone)]
struct Standing {
    score: TeamScore,
    leader: Option<String>,
}

/// `BingoConfig.nightVisionInSpectator` and `nightVisionInPostgame`, at their defaults: the port has no config file yet.
const NIGHT_VISION_IN_SPECTATOR: bool = true;
const NIGHT_VISION_IN_POSTGAME: bool = true;
/// `BingoOptions.showCompletedLines`, `showCompletedItems` and `showLeadingTeam`, at their defaults (all shown): the
/// session has no such options yet.
const SHOW_COMPLETED_LINES: bool = true;
const SHOW_COMPLETED_ITEMS: bool = true;
const SHOW_LEADING_TEAM: bool = true;

impl Session {
    /// Replaces the card pool; the adapter installs the pool read from the assets folder (its item tags).
    pub fn set_pool(&mut self, pool: Pool) {
        self.pool = pool;
    }

    /// Installs the pictures the maps are drawn with: the item pictures and the map overlays (see `map_render`).
    pub fn set_map_assets(&mut self, icons: Box<dyn IconSource + Send>, overlays: Overlays) {
        self.maps.set_assets(icons, overlays);
    }

    pub fn new(options: SessionOptions) -> Self {
        let game = Game::new(options.game.clone());
        let seed = options.seed;
        Self {
            game,
            pool: load_pool(),
            seed,
            rng: Rng::new(seed),
            teams: Teams::new(),
            ready: ReadyTimer::new(options.ready),
            world: WorldReset::new(),
            lobby_wanted: BTreeSet::new(),
            cards: Vec::new(),
            card_started: Vec::new(),
            team_card: BTreeMap::new(),
            completions: BTreeMap::new(),
            finished: BTreeMap::new(),
            progress: BTreeMap::new(),
            persistent: BTreeMap::new(),
            added_teams: Vec::new(),
            boss_bars: BTreeSet::new(),
            ticks: 0,
            rules: options.rules.clone(),
            owners: Vec::new(),
            seen: BTreeMap::new(),
            reached: Vec::new(),
            votes: BTreeMap::new(),
            empty_ticks: 0,
            loaded: false,
            settings: HashMap::new(),
            history: Vec::new(),
            forfeit: false,
            end_reason: None,
            sidebar_end: None,
            end_winner: None,
            world_seed: None,
            await_rejoin: false,
            reply: None,
            countdown_first: false,
            frozen: false,
            captures: Captures::default(),
            objective_runtime: Vec::new(),
            held: HashMap::new(),
            reported: HashSet::new(),
            maps: MapItems::new(),
            flashes: Flashes::default(),
            books_given: HashSet::new(),
            hud: HashSet::new(),
            joined_at: HashMap::new(),
            night_vision: BTreeSet::new(),
            score_updated: BTreeMap::new(),
            options,
        }
    }

    /// Reads the saved options, the players' settings, the history and a game in progress, once, on the first call of
    /// any hook. A missing or damaged file leaves the defaults; a damaged game file leaves the lobby.
    fn ensure_loaded(&mut self, host: &mut dyn ServerHost) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Some(text) = host.read_file(OPTIONS_FILE) {
            self.rules = store::options_from_json(&text);
        }
        if let Some(text) = host.read_file(SETTINGS_FILE) {
            self.settings = store::settings_from_json(&text);
        }
        if let Some(text) = host.read_file(HISTORY_FILE) {
            self.history = store::parse_history(&text);
        }
        if let Some(text) = host.read_file(MAP_IDS_FILE) {
            self.maps.load_ids(&text);
        }
        if let Some(text) = host.read_file(GAME_STATE_FILE) {
            // An empty file is no game (see `clear_saved_game`). Anything else that does not restore is warned once.
            if !text.trim().is_empty() {
                let restored = match saved_phase(&text) {
                    Some(GameStatus::Playing) => self.restore_from_json(&text),
                    Some(_) => Err("it is not a game in progress".to_string()),
                    // No readable phase: the snapshot parser names what is wrong.
                    None => Err(parse_snapshot(&text).err().unwrap_or_else(|| "no readable phase".to_string())),
                };
                if let Err(e) = restored {
                    host.warn(&format!("bingo: {GAME_STATE_FILE} not restored, starting with no game: {e}"));
                }
            }
        }
    }

    /// The game in progress as JSON (`game-state.json`): the phase, the timer, the seed, the round rules, the card, each
    /// team's progress, the lockout owners, the teams that reached the goal, the postgame votes, the teams and
    /// spectators by player uuid, and the server teams the start added. Keys are sorted where a map is written, so the
    /// same state always gives the same text.
    pub fn snapshot_json(&self) -> String {
        let game = self.game.snapshot();
        let rules = serde_json::from_str::<Value>(&store::options_to_json(&self.rules)).expect("round options are JSON");
        let card = match self.cards.first() {
            Some(card) => card_value(card),
            None => Value::Null,
        };
        let progress: BTreeMap<&str, &Vec<usize>> =
            self.progress.iter().map(|(team, progress)| (team.as_str(), &progress.found)).collect();
        let seen: BTreeMap<&str, &Vec<usize>> = self.seen.iter().map(|(team, squares)| (team.as_str(), squares)).collect();
        // Squares are numbered over the cards as card * 25 + square, so a one-card round keeps the numbers it always had.
        let owners: BTreeMap<String, &String> = self
            .owners
            .iter()
            .enumerate()
            .flat_map(|(index, map)| map.iter().map(move |(square, team)| ((index * CARD_SQUARES + square).to_string(), team)))
            .collect();
        let votes: BTreeMap<String, &str> =
            self.votes.iter().map(|(&player, &choice)| (store::uuid_hex(player), vote_name(choice))).collect();
        let (members, spectators) = self.teams.parts();
        let members: Vec<Value> =
            members.iter().map(|&(player, team)| json!({ "uuid": store::uuid_hex(player), "team": team })).collect();
        let spectators: Vec<String> = spectators.into_iter().map(store::uuid_hex).collect();
        let mut value = json!({
            "version": GAME_STATE_VERSION,
            "phase": status_name(game.status),
            "countdownTicks": game.countdown_ticks,
            "elapsedTicks": game.playing_ticks,
            "timeLimitSecs": game.time_limit_secs,
            "seed": self.seed,
            "rules": rules,
            "addedTeams": self.added_teams,
            "card": card,
            "progress": progress,
            "owners": owners,
            "reached": self.reached,
            "votes": votes,
            "members": members,
            "spectators": spectators,
        });
        // Written only for a round that is multi-card or has more than one card, so a one-card round keeps its text.
        if self.is_multi() || self.cards.len() > 1 {
            let cards: Vec<Value> = self.cards.iter().map(card_value).collect();
            value["cards"] = json!(cards);
            value["cardStarted"] = json!(self.card_started);
            value["teamCard"] = json!(self.team_card);
            value["completions"] = json!(self.completions);
            value["finished"] = json!(self.finished);
        }
        // Written only by a ReplaceGoals round, so the text of the other rounds does not change.
        if self.rules.win_condition == WinCondition::ReplaceGoals {
            let persistent: BTreeMap<&str, Value> = self
                .persistent
                .iter()
                .map(|(team, score)| (team.as_str(), json!({ "items": score.items, "lines": score.lines })))
                .collect();
            value["persistent"] = json!(persistent);
        }
        if self.rules.hidden_consume.hidden_items {
            value["seen"] = json!(seen);
        }
        value["captures"] = self.captures.to_json();
        // Written only by a round with objective squares, so the text of the plain rounds does not change.
        if self.cards.iter().any(|card| objectives::card_squares(card).is_some()) {
            value["objectiveRuntime"] = Value::Array(self.objective_runtime.iter().map(Runtime::to_json).collect());
        }
        serde_json::to_string_pretty(&value).expect("a JSON value always serializes")
    }

    /// Puts the session back to a snapshot from [`Session::snapshot_json`], in any phase. The text is checked in full
    /// first: when it is not a valid snapshot the error is returned and nothing changes. The restored timer keeps the
    /// saved elapsed time; no events are sent, since the players get the game when they join (`on_join`).
    pub fn restore_from_json(&mut self, text: &str) -> Result<(), String> {
        let saved = parse_snapshot(text)?;
        self.game.restore(saved.game);
        self.seed = saved.seed;
        self.rules = saved.rules;
        self.added_teams = saved.added_teams;
        self.cards = saved.cards;
        self.card_started = saved.card_started;
        self.team_card = saved.team_card;
        self.completions = saved.completions;
        self.finished = saved.finished;
        self.progress = saved.progress;
        self.persistent = saved.persistent;
        self.owners = saved.owners;
        self.seen = saved.seen;
        self.reached = saved.reached;
        self.votes = saved.votes;
        self.teams = saved.teams;
        // Read apart from the snapshot itself: a game saved before the ledger existed has none.
        self.captures = serde_json::from_str::<Value>(text).ok().and_then(|value| value.get("captures").map(Captures::from_json)).unwrap_or_default();
        let saved_value = serde_json::from_str::<Value>(text).ok();
        self.objective_runtime = (0..self.cards.len())
            .map(|index| {
                saved_value
                    .as_ref()
                    .and_then(|value| value.get("objectiveRuntime"))
                    .and_then(|list| list.get(index))
                    .map(Runtime::from_json)
                    .unwrap_or_default()
            })
            .collect();
        self.boss_bars.clear();
        self.forfeit = false;
        self.empty_ticks = 0;
        self.await_rejoin = self.game.status() == GameStatus::Playing;
        Ok(())
    }

    /// Writes the game in progress to `game-state.json`, if it is Playing. Other phases write nothing.
    fn save_playing(&self, host: &mut dyn ServerHost) {
        if self.game.status() == GameStatus::Playing {
            host.write_file(GAME_STATE_FILE, &self.snapshot_json());
        }
    }

    /// Empties `game-state.json`: the game is over, so there is nothing to restore. An empty file is no game.
    fn clear_saved_game(host: &mut dyn ServerHost) {
        host.write_file(GAME_STATE_FILE, "");
    }

    /// A player joins. They get the game status. Before play they get the ready screen; during play they get the whole
    /// current game as it is now: the teams, their tiles, the open card and each team's score.
    pub fn on_join(&mut self, host: &mut dyn ServerHost, player: &Player) {
        self.ensure_loaded(host);
        self.await_rejoin = false;
        self.joined_at.insert(player.uuid, self.ticks);
        if self.game.status() == GameStatus::Pregame {
            // Before play the players wait in the lobby: a joiner goes there now, or as soon as the lobby is open.
            self.lobby_wanted.insert(player.uuid);
            self.gather_in_lobby(host);
        }
        send(host, player.uuid, self.game_status());
        match self.game.status() {
            GameStatus::Pregame => self.send_ready_updates(host),
            GameStatus::Playing => {
                if self.teams.team_of(player.uuid).is_none() && !self.teams.is_spectator(player.uuid) {
                    // Joining a running game without a team: watch until `join <team>`.
                    self.teams.spectate(player.uuid);
                    host.command(&format!("gamemode spectator {}", player.name));
                    self.save_playing(host);
                }
                self.send_round(host, player.uuid);
            }
            GameStatus::Postgame => {
                self.send_game_over(host, player.uuid, true);
                self.send_vote_updates(host);
            }
            GameStatus::Starting => {}
        }
    }

    /// A player leaves. Their ready flag goes. Before play they also leave their team; during play the team stays, so
    /// a player who comes back is on it again.
    pub fn on_leave(&mut self, host: &mut dyn ServerHost, player: u128) {
        self.hud.remove(&player);
        self.joined_at.remove(&player);
        self.lobby_wanted.remove(&player);
        self.ready.remove(player);
        if self.game.status() == GameStatus::Pregame {
            self.teams.leave(player);
            self.send_ready_updates(host);
        }
    }

    /// The lobby gate, Pregame only: sends the players `world_reset::gate_targets` picks to the lobby, the ones who joined
    /// and were not moved yet, and outside a start anyone in a vanilla level. A move the engine refuses is tried again on
    /// the next tick (the player stays remembered).
    fn gather_in_lobby(&mut self, host: &mut dyn ServerHost) {
        let starting = self.starting();
        let Some(port) = host.world_port() else {
            return;
        };
        if !port.lobby_is_open() {
            return;
        }
        let in_vanilla = port.vanilla_players();
        for uuid in world_reset::gate_targets(&self.lobby_wanted, &in_vanilla, starting) {
            if port.teleport_to_lobby(uuid) {
                self.lobby_wanted.remove(&uuid);
            }
        }
    }

    /// The players go back to the lobby: a game is reset, ended by a command, or the vote sends it back. Idempotent: the
    /// lobby flow does nothing when no start or game is running.
    fn send_to_lobby(&mut self, host: &mut dyn ServerHost) {
        if let Some(port) = host.world_port() {
            self.world.return_to_lobby(port);
        }
    }

    /// A player's inventory, as a map from item id to count. Only during play, and only for a player on a team: squares
    /// found now are scored for their team and announced. A team that completes a line wins and ends the game.
    pub fn on_inventory(&mut self, host: &mut dyn ServerHost, player: u128, held: &HashMap<String, u32>) {
        // Kept whatever the phase: the objective squares read it once play begins.
        self.held.insert(player, held.clone());
        self.reported.insert(player);
        self.maps.note_inventory(player, held.get(FILLED_MAP).is_some_and(|&count| count > 0));
        if self.game.status() != GameStatus::Playing {
            return;
        }
        let Some(team) = self.teams.team_of(player) else { return };
        // Only a player on a team scores; the save after the finds covers the squares and the goals reached.
        let team = team.to_string();
        if self.card_objectives(self.card_index_of(&team)).is_some() {
            self.apply_objectives(host, Some(player), &team);
            return;
        }
        let standing = self.standing(&team);
        let newly_seen = self.record_seen(&team, held);
        let index = self.card_index_of(&team);
        let (items, lost) = {
            let Some(card) = self.cards.get(index) else { return };
            let progress = self.progress.entry(team.clone()).or_default();
            let (mut gained, lost) = scoring::score_inventory_mode(card, progress, held, self.rules.mode);
            if self.rules.mode == Mode::Lockout {
                let Some(owners) = self.owners.get_mut(index) else { return };
                let claimed = scoring::claim_lockout(owners, &team, &gained);
                for index in gained.iter().filter(|index| !claimed.contains(index)) {
                    progress.found.retain(|found| found != index);
                }
                gained = claimed;
            }
            self.captures.gained(&team, player, &gained);
            self.captures.lost(&team, &lost);
            self.flashes.note_scored(&team, &gained, self.ticks);
            let items: Vec<String> = gained.into_iter().map(|index| card.squares[index].item.clone()).collect();
            (items, lost.len())
        };
        if items.is_empty() && lost == 0 {
            if newly_seen && self.rules.hidden_consume.hidden_items {
                self.send_tiles_after_find(host, &team);
            }
            return;
        }

        let finder = player_name(host, player);
        self.queue_consume(host, &finder, held, &items);
        self.send_tiles_after_find(host, &team);
        self.send_score_to_all(host, &team);
        for item in items {
            let line = texts::item_captured(&finder, short_name(&item), host.item_name_key(&item));
            self.announce_find(host, &line);
        }
        self.announce_lines(host, &team, &standing);
        self.check_goals(host);
        self.announce_leader(host, &standing);
        self.save_playing(host);
    }

    /// A chat line about a find, for the players who keep `itemMessages` on (the default). It is one broadcast when
    /// everyone does.
    fn announce_find(&self, host: &mut dyn ServerHost, line: &texts::Line) {
        let wants = |uuid: u128| self.settings.get(&uuid).map_or(true, |settings| settings.item_messages);
        let players = host.players();
        if players.iter().all(|player| wants(player.uuid)) {
            host.broadcast_line(line);
        } else {
            for player in players.iter().filter(|player| wants(player.uuid)) {
                host.tell_line(player.uuid, line);
            }
        }
    }

    /// The team's score and the round's leader, read before its progress changes.
    fn standing(&self, team: &str) -> Standing {
        Standing { score: self.score(team), leader: self.leader() }
    }

    /// The leading team of the round (`ScoreService.getLeading`), or `None` when the top is tied.
    fn leader(&self) -> Option<String> {
        let rounds = self.rounds_view();
        leading_team(&rounds, self.rules.goal, false).map(|index| rounds[index].key.clone())
    }

    /// Whether a player gets the line messages of a team: `scoreMessages` is on, and either the player is on that team, a
    /// spectator or on no team, or every team's lines are shown (`GameMessageService.isEnabled`).
    fn wants_line_message(&self, player: u128, team: &str) -> bool {
        let on_team_or_spectator = self.teams.team_of(player).map_or(true, |own| own == team);
        let wants = self.settings.get(&player).map_or(true, |settings| settings.score_messages);
        wants && (SHOW_COMPLETED_LINES || on_team_or_spectator)
    }

    /// After a team's progress changed, before the goals are checked: the team's score is stamped as changed now if it
    /// differs, and a team that scored lines (on a lines goal) is announced to the players who want it, with its total
    /// lines (`ScoredItemCheck.updateScores`, `sendLineCaptured`).
    fn announce_lines(&mut self, host: &mut dyn ServerHost, team: &str, before: &Standing) {
        let after = self.score(team);
        if after != before.score {
            self.score_updated.insert(team.to_string(), self.ticks);
        }
        if !matches!(self.rules.goal, Goal::Lines(_)) || after.lines <= before.score.lines {
            return;
        }
        let lines = texts::count_text(after.lines as u32, "line", "lines");
        let line = texts::scored_line(team_name(team), &lines);
        for player in host.players() {
            if self.wants_line_message(player.uuid, team) {
                host.tell_line(player.uuid, &line);
            }
        }
    }

    /// The team a tie is announced for: the leader when ties are allowed (`getLeading(allowTies = true)`), then among the
    /// teams that tie with it the one whose score changed last (`maxByOrNull`, so the first of equals).
    fn tied_team(&self, rounds: &[TeamRound]) -> Option<String> {
        let goal = self.rules.goal;
        let top = leading_team(rounds, goal, true)?;
        let updated = |index: usize| self.score_updated.get(&rounds[index].key).copied().unwrap_or(0);
        let mut best: Option<usize> = None;
        for (index, round) in rounds.iter().enumerate() {
            if compare_teams(goal, round, &rounds[top]) != std::cmp::Ordering::Equal {
                continue;
            }
            match best {
                Some(current) if updated(index) <= updated(current) => {}
                _ => best = Some(index),
            }
        }
        best.map(|index| rounds[index].key.clone())
    }

    /// After the goals are checked: when the leading team changed while the game still runs, the new leader (or the tie)
    /// is announced to the players who keep `leadingMessages` on. Nothing with a single team, and nothing when the change
    /// ended the game, as `postTickCards` returns before the leader is checked (`sendTeamLeading`, `sendTeamTied`).
    fn announce_leader(&self, host: &mut dyn ServerHost, before: &Standing) {
        if self.game.status() != GameStatus::Playing || !SHOW_LEADING_TEAM || self.teams.teams().len() < 2 {
            return;
        }
        let leader = self.leader();
        if leader == before.leader {
            return;
        }
        let rounds = self.rounds_view();
        let (team, tied) = match leader {
            Some(team) => (team, false),
            None => match self.tied_team(&rounds) {
                Some(team) => (team, true),
                None => return,
            },
        };
        let score = texts::leading_score(self.score(&team), matches!(self.rules.goal, Goal::Lines(_)));
        let line = texts::leading_team(team_name(&team), &score, tied, SHOW_COMPLETED_ITEMS);
        for player in host.players() {
            let wants = self.settings.get(&player.uuid).map_or(true, |settings| settings.leading_messages);
            if wants {
                host.tell_line(player.uuid, &line);
            }
        }
    }

    /// Notes the teams that have reached the goal, and ends the game when the end condition holds. In a one-card round
    /// (`Cards(1)`, `ReplaceGoals`) the first team to reach the goal is the winner. A multi-card round goes to
    /// `check_cards`. Either way the stalemate is checked after the goals (`close_cards`). A `ReplaceGoals` round
    /// replaces the goals scored first, so the goal is read from the total score.
    fn check_goals(&mut self, host: &mut dyn ServerHost) {
        if self.is_multi() {
            return self.check_cards(host);
        }
        if self.rules.win_condition == WinCondition::ReplaceGoals {
            self.replace_scored_goals(host);
        }
        let keys: Vec<String> = self.teams.teams().into_iter().map(str::to_string).collect();
        let at = self.elapsed_ms_u64();
        let mut newly = Vec::new();
        for key in &keys {
            let done = self.goal_met(key);
            if done && !self.reached.contains(key) {
                self.reached.push(key.clone());
                self.finished.insert(key.clone(), at);
                self.completions.entry(key.clone()).or_default().push((0, true));
                newly.push(key.clone());
            } else if !done {
                self.reached.retain(|reached| reached != key);
                self.finished.remove(key);
                if let Some(list) = self.completions.get_mut(key) {
                    list.retain(|&(card, won)| !(card == 0 && won));
                }
            }
        }
        let ends = scoring::game_should_end(self.rules.end_when, self.reached.len(), keys.len());
        if !ends {
            for key in newly {
                host.broadcast_line(&texts::team_reached_goal(team_name(&key)));
            }
        }
        if ends {
            self.end_game(host, self.reached.first().cloned());
            return;
        }
        let rounds = self.rounds_view();
        self.close_cards(host, rounds, Vec::new(), false);
    }

    /// The goal part of a multi-card round. On each card, the first team that plays it and has reached the goal
    /// completes it; then the rest that reached it do, as far as the end rule lets them (`finish_card_on_goal`). A
    /// card completed within 3 ticks of being made ends the game with an error, which forfeits it.
    fn check_cards(&mut self, host: &mut dyn ServerHost) {
        let rules = self.round_rules();
        let at = self.elapsed_ms_u64();
        let mut rounds = self.rounds_view();
        let mut notices = Vec::new();
        let mut immediate = false;
        'cards: for index in 0..self.cards.len() {
            let card_ticks = self.card_ticks(index);
            while let Some(finish) = finish_card_on_goal(&mut rounds, &rules, index, card_ticks, Some(index + 1), at) {
                let outcomes = std::iter::once(finish.outcome).chain(finish.skipped.iter().map(|(_, outcome)| *outcome));
                if outcomes.clone().any(|outcome| outcome == CompletionOutcome::ImmediateEnd) {
                    immediate = true;
                    break 'cards;
                }
                notices.push(completion_notice(&rounds[finish.winner], &rules));
            }
        }
        self.close_cards(host, rounds, notices, immediate);
    }

    /// What follows the goals of a score event, for both kinds of round. Each card's stalemate is checked and its
    /// behaviour carried out (`stalemate_action`). The teams with no card get one (`settle_pending`). The rounds are
    /// written back, and the teams whose card changed are sent theirs. Then the chat lines go out, and the round ends
    /// on a stalemate, on an immediate end, or when its end rule holds.
    fn close_cards(&mut self, host: &mut dyn ServerHost, mut rounds: Vec<TeamRound>, mut notices: Vec<String>, mut immediate: bool) {
        let rules = self.round_rules();
        let now = self.card_clock();
        let at = self.elapsed_ms_u64();
        let behaviour = self.rules.stalemate;
        let mut stalemate_end = None;
        if !immediate {
            for index in 0..self.cards.len() {
                let owners = self.owners.get(index).cloned().unwrap_or_default();
                let reason = stalemate_reason(&self.cards[index], index, &rounds, &owners, &rules, true);
                match stalemate_action(reason.as_ref(), behaviour) {
                    StalemateAction::Nothing => {}
                    StalemateAction::WinCard { team, reason } => {
                        if let Some(i) = rounds.iter().position(|round| round.key == team) {
                            let score = team_score(rules.goal, &rounds[i].progress, rounds[i].count_cards());
                            let completion = Completion {
                                card_index: index,
                                is_winner: true,
                                stalemate: Some(reason),
                                completed_at: at,
                                score,
                            };
                            let outcome = complete_card(&mut rounds[i], &rules, self.card_ticks(index), Some(index + 1), completion);
                            immediate |= outcome == CompletionOutcome::ImmediateEnd;
                            notices.push(completion_notice(&rounds[i], &rules));
                        }
                    }
                    StalemateAction::EndGame(cause) => {
                        stalemate_end = Some(cause);
                        break;
                    }
                    StalemateAction::RerollCard => {
                        let outcomes = reroll_card(&mut rounds, &rules, index, self.card_ticks(index), Some(index + 1), at);
                        immediate |= outcomes.iter().any(|(_, outcome)| *outcome == CompletionOutcome::ImmediateEnd);
                        notices.push(REROLL_NOTICE.to_string());
                    }
                }
            }
        }

        settle_pending(&mut rounds, self.cards.len(), || self.make_card(now));
        let changed = self.apply_rounds(rounds);
        for team in &changed {
            self.send_new_card(host, team);
        }
        for line in &notices {
            host.broadcast(line);
        }
        if let Some(cause) = stalemate_end {
            host.broadcast_line(&stalemate_text(cause));
            self.end_reason = Some(stalemate_end_reason(cause));
            self.end_game(host, None);
            return;
        }
        if immediate {
            self.forfeit = true;
            self.end_reason = Some(texts::game_over_reason());
            self.end_game(host, None);
            return;
        }
        let ends = if self.is_multi() {
            round_ends(self.rules.end_when, &self.rounds_view())
        } else {
            scoring::game_should_end(self.rules.end_when, self.reached.len(), self.teams.teams().len())
        };
        if ends {
            self.end_game(host, self.reached.first().cloned());
        }
    }

    /// Ends the game with `winner` (`None` for nobody), through the game's own end. The players stay where they are, on
    /// the end screen, until the game is reset or a command sends them back.
    fn end_game(&mut self, host: &mut dyn ServerHost, winner: Option<String>) {
        if let Ok(events) = self.game.end(winner) {
            self.dispatch(host, events);
        }
    }

    /// Every team's round as `multicard` takes it: the card it plays (or its finish), its completions and its progress.
    fn rounds_view(&self) -> Vec<TeamRound> {
        self.teams
            .teams()
            .into_iter()
            .map(|key| {
                let state = match self.finished.get(key) {
                    Some(&completed_at) => CardState::Finished { completed_at },
                    None => CardState::Playing(self.card_index_of(key)),
                };
                let mut round = TeamRound::new(key, state);
                round.completed = self
                    .completions
                    .get(key)
                    .map(|list| {
                        list.iter()
                            .map(|&(card_index, is_winner)| CardCompletion {
                                card_index,
                                is_winner,
                                stalemate: None,
                                completed_at: 0,
                                score: TeamScore::default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                round.progress = self.progress.get(key).cloned().unwrap_or_default();
                round
            })
            .collect()
    }

    /// Writes the rounds back into the session. A team whose card changed starts it fresh: its seen squares go. Returns
    /// the teams whose card changed, so that they are sent it.
    fn apply_rounds(&mut self, rounds: Vec<TeamRound>) -> Vec<String> {
        let mut changed = Vec::new();
        for round in rounds {
            let key = round.key;
            match round.state {
                CardState::Playing(index) => {
                    if self.card_index_of(&key) != index {
                        self.seen.remove(&key);
                        changed.push(key.clone());
                    }
                    self.team_card.insert(key.clone(), index);
                }
                CardState::Finished { completed_at } => {
                    self.finished.insert(key.clone(), completed_at);
                    if !self.reached.contains(&key) {
                        self.reached.push(key.clone());
                    }
                }
                CardState::Pending(_) => {}
            }
            let completions = round.completed.iter().map(|done| (done.card_index, done.is_winner)).collect();
            self.completions.insert(key.clone(), completions);
            self.progress.insert(key, round.progress);
        }
        changed
    }

    /// Makes the next card of the round, seeded with the round's seed plus 31 times its index. Returns its index.
    fn make_card(&mut self, now: u64) -> usize {
        let index = self.cards.len();
        let seed = self.seed.wrapping_add(31 * index as u64);
        let pool = self.card_pool();
        self.cards.push(generate_card(seed, &pool, self.rules.tier_counts));
        self.owners.push(HashMap::new());
        self.objective_runtime.push(Runtime::default());
        self.card_started.push(now);
        index
    }

    /// A team's new card: each of its members gets the card reset and the tiles of the card the team now plays.
    fn send_new_card(&self, host: &mut dyn ServerHost, team: &str) {
        for player in host.players() {
            if self.teams.team_of(player.uuid) != Some(team) {
                continue;
            }
            send(host, player.uuid, wire::card_reset());
            if let Some(tiles) = self.tiles_for(&*host, team, false) {
                send(host, player.uuid, tiles);
            }
        }
    }

    /// The elapsed playing time in milliseconds, as an unsigned count.
    fn elapsed_ms_u64(&self) -> u64 {
        self.game.elapsed_ms().max(0) as u64
    }

    /// The playing ticks so far: the clock the cards are timed by.
    fn card_clock(&self) -> u64 {
        self.elapsed_ms_u64() / MS_PER_TICK as u64
    }

    /// The index of the card a team plays (0 for a team with none yet).
    fn card_index_of(&self, team: &str) -> usize {
        self.team_card.get(team).copied().unwrap_or(0)
    }

    /// The card a team plays.
    fn card_of(&self, team: &str) -> Option<&Card> {
        self.cards.get(self.card_index_of(team))
    }

    /// A round whose win condition is more than one card, or endless. `ReplaceGoals` is a one-card round whose scored
    /// goals are replaced (`replace_scored_goals`).
    fn is_multi(&self) -> bool {
        !matches!(self.rules.win_condition, WinCondition::Cards(1) | WinCondition::ReplaceGoals)
    }

    fn round_rules(&self) -> Rules {
        Rules::new(&self.rules, self.rules.win_condition)
    }

    /// The ticks a card has been in play, for the rule that a card completed within 3 ticks ends the game. A one-card
    /// round has no such rule, so its cards get the largest count.
    fn card_ticks(&self, index: usize) -> u64 {
        if !self.is_multi() {
            return u64::MAX;
        }
        self.card_clock().saturating_sub(self.card_started.get(index).copied().unwrap_or(0))
    }

    /// A packet from a player on the channel `yet-another-minecraft-bingo:<path>`. Only the ready packets are handled,
    /// and only before play. A payload that does not decode is ignored. A client's settings packet is answered with the
    /// saved settings instead of taken (see `update_settings` below).
    pub fn on_packet(&mut self, host: &mut dyn ServerHost, player: u128, path: &str, data: &[u8]) {
        if path == "update_settings" {
            if decode(data, PlayerSettings::read).is_ok() {
                // The client sends this once its channels are registered, so it runs the card HUD. Its packet does not
                // replace the saved settings: a client sends the defaults on join, and the saved ones win. The server
                // answers with them (`update_settings`), which is how the client learns them. Only `prefs` changes them.
                // A player with none saved gets the defaults saved, as the original's `PlayerSettingsService` gives them.
                self.hud.insert(player);
                if !self.settings.contains_key(&player) {
                    self.settings.insert(player, PlayerSettings::default());
                    host.write_file(SETTINGS_FILE, &store::settings_to_json(&self.settings));
                }
                // The held book names the card HUD's keybind and the auto hide setting, so it is refreshed with this.
                self.refresh_book(host, player);
                send(host, player, wire::update_settings(&self.settings[&player]));
            }
            return;
        }
        let ready = match path {
            "set_ready" => decode(data, SetReadyPacket::read_v1),
            "set_ready_v2" => decode(data, SetReadyPacket::read_v2),
            _ => return,
        };
        let Ok(packet) = ready else { return };
        let ready = packet.is_ready;
        if self.game.status() == GameStatus::Postgame {
            if ready {
                // The option the button stands for; none (or one this server did not offer) is the pending default.
                let choice = packet.option_id.and_then(vote_for_id).unwrap_or(VoteChoice::NextRound);
                self.vote(host, player, choice);
                if self.game.status() == GameStatus::Postgame {
                    self.send_vote_updates(host);
                }
            }
            return;
        }
        if self.game.status() != GameStatus::Pregame {
            return;
        }
        self.ready.set_ready(player, ready);
        self.send_ready_updates(host);
    }

    /// The round options (`/bingo options`), for the lobby menu.
    pub fn rules(&self) -> &Options {
        &self.rules
    }

    /// The time limit in whole minutes, if one is set (`timelimit`), for the lobby panel's tile.
    pub fn time_limit_minutes(&self) -> Option<u32> {
        self.game.time_limit_secs().map(|secs| u32::try_from(secs / 60).unwrap_or(0))
    }

    /// Whether the player's client runs the card HUD: it sent its settings in this session, after it registered the mod's
    /// channels, and has not left since. The original's `supportsCardHud`. A client without the mod has no HUD, and so
    /// does a player whose settings were only saved by an earlier session.
    pub fn has_card_hud(&self, player: u128) -> bool {
        self.hud.contains(&player)
    }

    /// Whether no game has started yet (`Pregame`): the only time the round options can change.
    pub fn is_pregame(&self) -> bool {
        self.game.status() == GameStatus::Pregame
    }

    /// Whether the game has ended and waits for the players' votes: the only time a plain chat vote counts.
    pub fn is_postgame(&self) -> bool {
        self.game.status() == GameStatus::Postgame
    }

    /// One server tick. Before play, the ready timer runs and starts the game when it ends. Otherwise the game moves
    /// on, and its events go to everyone.
    pub fn tick(&mut self, host: &mut dyn ServerHost) {
        self.tick_world(host);
        self.tick_game(host);
        self.tick_night_vision(host);
    }

    /// The engine's `levels_reset` hook: the vanilla worlds are new, so the players go into the new overworld and the
    /// game begins. Ignored unless a start is waiting for the reset.
    pub fn levels_reset(&mut self, host: &mut dyn ServerHost) {
        if let Some(port) = host.world_port() {
            self.world.levels_reset_done(port);
        }
    }

    /// Whether the lobby flow is bringing the players out, resetting the worlds or taking the players to the new spawn for
    /// a start.
    fn starting(&self) -> bool {
        matches!(
            self.world.state(),
            world_reset::State::WaitingForPlayersOut { .. }
                | world_reset::State::Resetting
                | world_reset::State::AwaitingSpawn { .. }
        )
    }

    /// The lobby flow's part of a tick: a reset that does not come in time ends the start.
    fn tick_world(&mut self, host: &mut dyn ServerHost) {
        let outcome = match host.world_port() {
            Some(port) => self.world.tick(port),
            None => None,
        };
        if let Some(outcome) = outcome {
            self.world_outcome(host, outcome);
        }
    }

    /// What the lobby flow ended with: the game begins, or the start gave up and the players are in the lobby.
    fn world_outcome(&mut self, host: &mut dyn ServerHost, outcome: Outcome) {
        match outcome {
            Outcome::Started => {
                if let Err(error) = self.begin_game(host) {
                    host.warn(&error);
                }
            }
            Outcome::Failed(failure) => {
                host.warn(&format!("bingo: the game did not start: {failure:?}"));
                host.broadcast_line(&texts::start_failed(!self.world.lobby_missed()));
            }
        }
    }

    /// The game's part of a tick: the lobby, the game's own tick and its events.
    fn tick_game(&mut self, host: &mut dyn ServerHost) {
        self.ensure_loaded(host);
        self.ticks += 1;
        if self.game.status() != GameStatus::Pregame {
            // `resetWhenEveryoneDisconnects` (the original's `ResetOnLeaveController`): an ended game that nobody is in goes
            // back to the lobby after a few seconds. A game in progress is not reset: it stays as `game-state.json` has it
            // until its players come back, so a server stop with nobody online keeps the round.
            if self.game.status() == GameStatus::Postgame && host.players().is_empty() && !self.await_rejoin {
                self.empty_ticks += 1;
                if self.empty_ticks >= 100 {
                    self.empty_ticks = 0;
                    let events = self.game.reset();
                    self.dispatch(host, events);
                    return;
                }
            } else {
                self.empty_ticks = 0;
            }
        }
        if self.game.status() == GameStatus::Pregame {
            self.gather_in_lobby(host);
            if !self.starting() {
                self.tick_lobby(host);
            }
            self.tick_maps(host);
            return;
        }
        let events = self.game.tick();
        self.dispatch(host, events);
        if self.game.status() == GameStatus::Playing && self.ticks % OBJECTIVE_POLL_TICKS == 0 {
            self.poll_objectives(host);
        }
        if self.game.status() == GameStatus::Playing && self.ticks % 10 == 0 {
            self.send_boss_bars(host);
        }
        self.tick_maps(host);
    }

    /// The night vision effect (`NightVisionController`), checked on every tick for each online player. A player who should
    /// have it and has not gets an infinite one; a player who has it and should not loses it. Who should: the player's
    /// setting is on, and then a team player while the round starts or plays with the option on, a spectator or a player
    /// on no team while it starts or plays, and everyone after the game.
    fn tick_night_vision(&mut self, host: &mut dyn ServerHost) {
        let status = self.game.status();
        for player in host.players() {
            let setting = self.settings.get(&player.uuid).map_or(true, |settings| settings.night_vision);
            let should = setting
                && match status {
                    GameStatus::Starting | GameStatus::Playing => {
                        if self.teams.team_of(player.uuid).is_some() {
                            self.rules.night_vision
                        } else {
                            NIGHT_VISION_IN_SPECTATOR
                        }
                    }
                    GameStatus::Postgame => NIGHT_VISION_IN_POSTGAME,
                    GameStatus::Pregame => false,
                };
            let has = self.night_vision.contains(&player.uuid);
            if should && !has {
                host.command(&format!("effect give {} night_vision infinite 0 true", player.name));
                self.night_vision.insert(player.uuid);
            } else if !should && has {
                host.command(&format!("effect clear {} night_vision", player.name));
                self.night_vision.remove(&player.uuid);
            }
        }
    }

    /// The players' map items for this tick (`map_items`): each player without the client's HUD holds the item their
    /// phase and team call for, and every map is repainted when its picture changed. The map ids are saved when new ones
    /// were made.
    fn tick_maps(&mut self, host: &mut dyn ServerHost) {
        let status = self.game.status();
        let wants: Vec<(u128, String, Option<Want>)> = host
            .players()
            .into_iter()
            .map(|player| {
                let team = self.teams.team_of(player.uuid);
                let won = team.is_some_and(|key| self.reached.iter().any(|reached| reached == key));
                let want = map_items::want_for_with(
                    status,
                    team,
                    self.teams.is_spectator(player.uuid),
                    // A player whose client runs our HUD needs no map.
                    self.has_card_hud(player.uuid),
                    won,
                    self.rules.preview_card,
                );
                (player.uuid, player.name, want)
            })
            .collect();
        let mut maps = std::mem::take(&mut self.maps);
        maps.tick(host, self.ticks, &wants, &|key| self.map_view(key));
        self.maps = maps;
        if let Some(text) = self.maps.take_ids_to_save() {
            host.write_file(MAP_IDS_FILE, &text);
        }
    }

    /// The card a map shows now, by its key (a team's key or [`PREVIEW_KEY`]). The team's own card while it is played
    /// (and after the game), with its squares as its team has them; the preview card otherwise, in the team's colour.
    /// Hidden squares show the hidden picture, as the viewer sees them (`hidden_consume::tile_views`).
    fn map_view(&self, key: &str) -> Option<CardView> {
        let playing = matches!(self.game.status(), GameStatus::Playing | GameStatus::Postgame) && !self.cards.is_empty();
        if key == PREVIEW_KEY {
            return Some(self.preview_view(None));
        }
        let color = teams::preset(key)?.color;
        if !playing {
            return Some(self.preview_view(Some(color)));
        }
        self.card_of(key)?;
        let views = self.views_for(Some(key));
        let none: &[usize] = &[];
        let own: &[usize] = self.progress.get(key).map_or(none, |progress| progress.found.as_slice());
        let others = self.found_on(self.card_index_of(key));
        let squares: Vec<SquareState> = (0..CARD_SQUARES)
            .map(|index| SquareState {
                found: own.contains(&index),
                achieved_by: others
                    .iter()
                    .filter(|(other, found)| *other != key && found.contains(&index))
                    .filter_map(|(other, _)| teams::preset(other).map(|preset| preset.color.to_string()))
                    .collect(),
                flash_since: self.flashes.flash_since(key, index),
            })
            .collect();
        Some(map_items::card_view(Some(color), &views, &squares, self.rules.mode == Mode::Lockout))
    }

    /// The preview card as the lobby shows it: the card the round will play (same seed), with every square as a player
    /// on no team sees it.
    fn preview_view(&self, team: Option<&str>) -> CardView {
        let card = generate_card(self.seed, &self.card_pool(), self.rules.tier_counts);
        let views = hidden_consume::tile_views(&card, self.rules.hidden_consume, false, None, false);
        map_items::card_view(team, &views, &vec![SquareState::default(); CARD_SQUARES], false)
    }

    /// Runs a command typed by `player` (or the console, `None`) and returns the text to answer with. `line` is what
    /// follows the word `bingo`. Who may run a command is for the caller to check (see [`commands::requires_op`]).
    pub fn command(&mut self, host: &mut dyn ServerHost, player: Option<u128>, line: &str) -> String {
        self.ensure_loaded(host);
        match commands::parse(line) {
            Ok(command) => self.run(host, player, command),
            Err(error) => error,
        }
    }

    /// The command's reply as a translatable line, when its reply has one (the English is what [`Session::command`]
    /// returned). Taken once; the next command clears it.
    pub fn take_reply(&mut self) -> Option<Line> {
        self.reply.take()
    }

    /// The English reply of a command, which is also kept as a translatable line for [`Session::take_reply`].
    fn say(&mut self, line: Line) -> String {
        let text = line.render();
        self.reply = Some(line);
        text
    }

    /// Runs a command already parsed (by [`commands::parse`] or [`commands::chat_shortcut`]), as [`Session::command`]
    /// does after parsing. It does not load the saved files: a server tick does that before any call here.
    pub fn run(&mut self, host: &mut dyn ServerHost, player: Option<u128>, command: Command) -> String {
        self.reply = None;
        match command {
            Command::Help => format!("Usage: bingo {}", commands::usage()),
            Command::Status => self.status_text(),
            Command::Start { ignore_warnings } => match self.start_game(host, ignore_warnings) {
                Ok(()) => self.say(texts::start_success()),
                Err(error) => error,
            },
            Command::End => match self.game.end(None) {
                Ok(events) => {
                    self.forfeit = true;
                    let name = match player {
                        Some(uuid) => host.players().into_iter().find(|p| p.uuid == uuid).map(|p| p.name),
                        None => Some("RCON".to_owned()),
                    };
                    self.end_reason = Some(texts::stopped_reason(&name.unwrap_or_else(|| "[unknown]".to_owned())));
                    self.dispatch(host, events);
                    self.send_to_lobby(host);
                    self.say(texts::end_success())
                }
                Err(_) => "The game is not Playing.".to_string(),
            },
            Command::Reset => {
                // The reset goes back to Pregame through `reset_round`, which sends the players to the lobby.
                let events = self.game.reset();
                self.dispatch(host, events);
                self.say(texts::reset_success())
            }
            Command::Defaults => self.set_rule(host, |rules| *rules = Options::default(), "rules"),
            Command::ShuffleTeams(count) => self.shuffle_teams(host, count),
            Command::Join(team) => self.join_team(host, player, &team),
            Command::Spectator => self.become_spectator(host, player),
            Command::Ready => self.toggle_ready(host, player),
            Command::ReadyCancel => self.cancel_ready(host),
            Command::TimeLimit(minutes) => self.set_time_limit(host, minutes),
            Command::Difficulty(counts) => self.set_rule(host, |rules| rules.tier_counts = counts, "difficulty"),
            Command::Reroll => self.reroll(host, player),
            Command::TeamCount(teams) => self.set_rule(host, |rules| rules.team_count = teams, "team count"),
            Command::Card => self.open_card(host, player),
            Command::Goal(goal) => self.set_rule(host, |rules| rules.goal = goal, "goal"),
            Command::Mode(mode) => self.set_rule(
                host,
                |rules| {
                    rules.mode = mode;
                    // Inventory mode and consume items exclude each other: inventory mode turns consume items off.
                    if mode == Mode::Inventory {
                        rules.hidden_consume.consume_items = false;
                    }
                },
                "mode",
            ),
            Command::ModeOff(mode) => self.set_rule(
                host,
                |rules| {
                    if rules.mode == mode {
                        rules.mode = Mode::Standard;
                    }
                },
                "mode",
            ),
            Command::CardMode(mode, on) => self.set_rule(
                host,
                |rules| {
                    rules.hidden_consume.set(mode, on);
                    // Turning consume items on leaves inventory mode, which it excludes.
                    if on && mode == CardMode::ConsumeItems && rules.mode == Mode::Inventory {
                        rules.mode = Mode::Standard;
                    }
                },
                "card mode",
            ),
            Command::EndWhen(end_when) => self.set_rule(host, |rules| rules.end_when = end_when, "end condition"),
            Command::Toggle(toggle, on) => self.set_rule(host, |rules| rules.set_toggle(toggle, on), "setting"),
            Command::SpawnDistance(chunks) => self.set_rule(host, |rules| rules.spawn_distance = chunks, "spawn distance"),
            Command::PlayTo(win) => self.set_rule(host, |rules| rules.win_condition = win, "win condition"),
            Command::Stalemate(behaviour) => self.set_rule(host, |rules| rules.stalemate = behaviour, "stalemate behaviour"),
            Command::Options => self.options_text(),
            Command::Vote(choice) => match player {
                Some(player) => self.vote(host, player, choice),
                None => "Only a player can vote.".to_string(),
            },
            Command::Resume => self.resume(host),
            Command::Stats => self.stats(host, player),
            Command::StatsOf(name) => self.stats_of_player(host, &name),
            Command::Menu => "The menu opens in a player's game chat: type bingo menu in game.".to_string(),
            Command::Kit(command) => self.kit_command(host, command),
            Command::Book => self.book_command(host, player),
            Command::Prefs { setting, value } => self.prefs(host, player, setting, value),
        }
    }

    /// `bingo prefs <setting> [on|off]`: changes one of the player's settings, the value given or its opposite. As the
    /// original's `BingoPrefsCommand`, the change marks the tutorial as seen, is saved, and goes back to the client
    /// (`update_settings`), which keeps the client's copy the same as the saved one. A bossbar turned off is hidden at once.
    /// The held tutorial book is refreshed for the new values (`refresh_book`), as the original updates every held book.
    /// The scoreboard setting is the client's: the server sends it back, and the client leaves its sidebar out
    /// (`BingoClient::hides_layer`), since the sidebar is one for everyone (`sidebar`).
    fn prefs(&mut self, host: &mut dyn ServerHost, player: Option<u128>, setting: Setting, value: Option<bool>) -> String {
        let Some(uuid) = player else {
            return "Only a player can change the player settings.".to_string();
        };
        let mut settings = self.settings.get(&uuid).cloned().unwrap_or_default();
        let on = value.unwrap_or(!setting.get(&settings));
        setting.set(&mut settings, on);
        settings.seen_tutorial = true;
        self.settings.insert(uuid, settings.clone());
        host.write_file(SETTINGS_FILE, &store::settings_to_json(&self.settings));
        if setting == Setting::Bossbar && !on {
            self.hide_boss_bars_of(host, uuid);
        }
        send(host, uuid, wire::update_settings(&settings));
        self.refresh_book(host, uuid);
        prefs::changed_text(setting, on)
    }

    /// Replaces the contents of the tutorial book a player holds with one written for their settings now, in place, so
    /// the player keeps one book (as the original's `updateBookItem`). False when the player holds none, is not online,
    /// or the host cannot write it; the held book is then left as it was.
    fn refresh_book(&self, host: &mut dyn ServerHost, uuid: u128) -> bool {
        let Some(player) = host.players().into_iter().find(|p| p.uuid == uuid) else {
            return false;
        };
        if !host.held_tagged(uuid, infobook::HELD_TAG).is_some_and(|count| count > 0) {
            return false;
        }
        let snbt = infobook::item_snbt(&player.name, &self.book_pages(host, &player), self.rules.lobby_tutorial_book);
        host.replace_held_tagged(uuid, infobook::HELD_TAG, &snbt)
    }

    /// Hides the boss bars shown to one player, and forgets them.
    fn hide_boss_bars_of(&mut self, host: &mut dyn ServerHost, player: u128) {
        let (hidden, kept): (BTreeSet<(u128, String)>, BTreeSet<(u128, String)>) =
            std::mem::take(&mut self.boss_bars).into_iter().partition(|(member, _)| *member == player);
        self.boss_bars = kept;
        for (member, id) in hidden {
            host.boss_bar(member, &id, "", 0.0, false);
        }
    }

    /// `bingo book`: the tutorial book for the player. Only when the lobby does not give it (`lobby_tutorial_book` off).
    fn book_command(&mut self, host: &mut dyn ServerHost, player: Option<u128>) -> String {
        if self.rules.lobby_tutorial_book {
            return "bingo book is off: the lobby gives the tutorial book (lobby_tutorial_book is on).".to_string();
        }
        let Some(uuid) = player else {
            return "Only a player can get the book.".to_string();
        };
        let Some(player) = host.players().into_iter().find(|p| p.uuid == uuid) else {
            return "You are not on the server.".to_string();
        };
        // A player who holds the book has it updated, not a second one (the original's `giveBookItem`).
        if self.refresh_book(host, uuid) || self.give_book(host, &player, false) {
            "You got the bingo book.".to_string()
        } else {
            "The bingo book could not be given.".to_string()
        }
    }

    /// The pages of a player's tutorial book, written for their settings and operator rights.
    fn book_pages(&self, host: &dyn ServerHost, player: &Player) -> Vec<infobook::Part> {
        let settings = self.settings.get(&player.uuid).cloned().unwrap_or_default();
        let operator = host.is_operator(player.uuid);
        let hud = self.has_card_hud(player.uuid);
        infobook::pages(&infobook::PageView { settings: &settings, operator, hud })
    }

    /// Gives one player the tutorial book, its pages written for their settings and operator rights. False when the host
    /// could not give it.
    fn give_book(&self, host: &mut dyn ServerHost, player: &Player, keep: bool) -> bool {
        let pages = self.book_pages(host, player);
        let components = infobook::components(&player.name, &pages, keep);
        host.give_item(player.uuid, infobook::ITEM, &components)
    }

    /// The lobby's tutorial book (`lobby_tutorial_book`): each online player is given one once this session, the first
    /// time they are in the lobby. Nothing is given while the option is off.
    fn give_lobby_books(&mut self, host: &mut dyn ServerHost, players: &[Player]) {
        if !self.rules.lobby_tutorial_book {
            return;
        }
        for player in players {
            if self.books_given.contains(&player.uuid) {
                continue;
            }
            // The book names the keybind of the card HUD, and a client answers with its settings a moment after it joins.
            // The book waits for that answer, up to `BOOK_HUD_WAIT_TICKS` after the join (a book is never replaced).
            let waiting = !self.has_card_hud(player.uuid)
                && self.joined_at.get(&player.uuid).is_some_and(|joined| self.ticks < joined + BOOK_HUD_WAIT_TICKS);
            if waiting {
                continue;
            }
            // A player who already holds the book (from before a restart, when `books_given` was empty) keeps it, updated,
            // and is not given a second one.
            if self.refresh_book(host, player.uuid) {
                self.books_given.insert(player.uuid);
                continue;
            }
            if self.give_book(host, player, true) {
                self.books_given.insert(player.uuid);
            }
        }
    }

    /// The pool the cards are drawn from: the tiers without the kit's items, which the original keeps off the cards
    /// (`excludeSpawnKitItemsFromCards`). With no kit it is the built-in pool.
    fn card_pool(&self) -> Pool {
        let pool = kit::pool_without(&self.pool, &self.rules.kit.names());
        // `objectives` adds the advanced squares of the original's tier lists; off, the pool is the plain one.
        if self.rules.objectives {
            objectives::with_objectives(&pool)
        } else {
            pool
        }
    }

    /// `bingo kit`: shows the kit, or changes it before the game starts and saves the options.
    fn kit_command(&mut self, host: &mut dyn ServerHost, command: KitCommand) -> String {
        if !command.changes() {
            return self.rules.kit.describe();
        }
        if self.game.status() != GameStatus::Pregame {
            return texts::KIT_ONLY_BEFORE_START.to_string();
        }
        let answer = match command {
            KitCommand::Add { item, count } => {
                if host.item_numeric_id(&item).is_none() {
                    return texts::unknown_item(&item);
                }
                self.rules.kit.add(&item, count).map(|()| texts::kit_added(&item, count))
            }
            KitCommand::Remove(item) => {
                if self.rules.kit.remove(&item) {
                    Ok(texts::kit_removed(&item))
                } else {
                    Err(texts::not_in_kit(&item))
                }
            }
            KitCommand::Clear => {
                self.rules.kit.clear();
                Ok(texts::KIT_CLEARED.to_string())
            }
            KitCommand::Show => Ok(self.rules.kit.describe()),
        };
        match answer {
            Ok(text) => {
                host.write_file(OPTIONS_FILE, &store::options_to_json(&self.rules));
                text
            }
            Err(error) => error,
        }
    }

    /// The lobby: the ready timer, which starts the game if the start rules hold when it ends. Everyone online gets the
    /// ready screen every 10 ticks.
    fn tick_lobby(&mut self, host: &mut dyn ServerHost) {
        let players = host.players();
        self.give_lobby_books(host, &players);
        let online: Vec<u128> = players.iter().map(|player| player.uuid).collect();
        // A lockout warning stops the countdown from running (the original's start waits for the operator too).
        let eligible = match self.plan_start(&players, false) {
            Ok(teams) => self.lockout_warning(&teams).is_none(),
            Err(_) => false,
        };
        if self.ready.tick(&online, eligible) {
            // The checks above ran just now. Only the goal-met warning, which reads the card, can still refuse the start.
            if let Err(warning) = self.start_game(host, false) {
                host.warn(&warning);
            }
            return;
        }
        if self.ticks % 10 == 0 {
            self.send_ready_updates(host);
        }
    }

    /// Checks the start rules and, if they hold, sets up the teams and the server's team list, then starts the countdown.
    /// The teams change only when the start goes ahead.
    /// Where the host has the lobby flow, the start waits for it (`start_from_lobby`): the game begins with `begin_game`
    /// once the players are in the new overworld.
    fn start_game(&mut self, host: &mut dyn ServerHost, ignore_warnings: bool) -> Result<(), String> {
        if self.starting() {
            return Err(self.say(texts::already_starting()));
        }
        if self.game.status() != GameStatus::Pregame {
            return Err(format!("The game is not in Pregame ({:?}).", self.game.status()));
        }
        let players = host.players();
        let teams = self.plan_start(&players, ignore_warnings)?;
        if !ignore_warnings {
            if let Some(warning) = self.lockout_warning(&teams) {
                return Err(warning);
            }
        }
        if let Some(warning) = self.goal_met_warning() {
            return Err(warning);
        }
        self.teams = teams;
        self.ready.clear();
        match host.world_port() {
            None => self.begin_game(host),
            Some(port) => {
                let seed = self.rng.next_u64();
                self.start_from_lobby(port, seed)
            }
        }
    }

    /// Asks the lobby flow for the start: the players go to the lobby, and the vanilla worlds reset with `seed` once
    /// nobody is in them. The game begins later, on `levels_reset`.
    fn start_from_lobby(&mut self, port: &mut dyn WorldPort, seed: u64) -> Result<(), String> {
        // A game that ended without its end (`bingo end`, a reset) may still have its players in the new overworld.
        self.world.return_to_lobby(port);
        self.world_seed = Some(seed);
        match self.world.start(port, &mut || seed) {
            Ok(()) => Ok(()),
            Err(world_reset::StartRefused::LobbyClosed) => Err(self.say(texts::lobby_not_open())),
            Err(world_reset::StartRefused::AlreadyStarted) => Err(self.say(texts::already_starting())),
        }
    }

    /// The game's own start: the gamerules, the teams on the server and the game itself. Runs at once, or once the
    /// lobby flow has the players in the new overworld.
    ///
    /// Delay versus world reset: the original also enters COUNTDOWN (and its 100 tick delay) after the loading state, so
    /// the delay already runs in the new world here, as there. The reset itself is the engine's, not a delay we count.
    fn begin_game(&mut self, host: &mut dyn ServerHost) -> Result<(), String> {
        host.set_game_rule("pvp", self.rules.pvp);
        host.set_game_rule("keep_inventory", self.rules.keep_inventory);
        let players = host.players();
        self.add_teams_to_server(host, &players);
        let events = self.game.start().map_err(str::to_owned)?;
        // With no countdown the start's 0 comes in these events; its title would only repeat the start title.
        self.countdown_first = true;
        self.dispatch(host, events);
        self.countdown_first = false;
        self.announce_start(host);
        Ok(())
    }

    /// The warning for lockout with a lines goal, when more than one team plays and the stalemate behaviour is not
    /// `do nothing` (`GameService.start`, the `CommandStartLockoutWithLinesGoal` warning). It is skipped by
    /// `ignore_warnings`.
    fn lockout_warning(&self, teams: &Teams) -> Option<String> {
        let Goal::Lines(_) = self.rules.goal else { return None };
        let risky = self.rules.mode == Mode::Lockout
            && self.rules.stalemate != StalemateBehavior::Nothing
            && teams.teams().len() > 1;
        risky.then(|| texts::lockout_with_lines_goal(&goal_text(self.rules.goal)))
    }

    /// The warning when the card already meets the goal before play, as the original counts the inverse squares, which
    /// the card holds at the start (`GameService.start`, `CommandStartWillImmediatelyEnd`). Nothing overrides it. The
    /// card is the one the round deals (`begin_round`).
    fn goal_met_warning(&self) -> Option<String> {
        let card = generate_card(self.seed, &self.card_pool(), self.rules.tier_counts);
        let squares = objectives::card_squares(&card)?;
        let held: Vec<usize> = squares
            .iter()
            .enumerate()
            .filter(|(_, square)| matches!(square.objective, objectives::Objective::Inverse { .. }))
            .map(|(index, _)| index)
            .collect();
        let lines = scoring::count_lines(&held) as u32;
        let items = held.len() as u32;
        let met = match self.rules.goal {
            Goal::Lines(n) => n > 0 && n <= lines,
            Goal::Items(n) => n > 0 && n <= items,
            Goal::FullCard => items as usize == CARD_SQUARES,
        };
        let completed = match self.rules.goal {
            Goal::Lines(_) => texts::count_text(lines, "line", "lines"),
            _ => texts::count_text(items, "item", "items"),
        };
        met.then(|| texts::goal_already_met(&goal_text(self.rules.goal), &completed))
    }

    /// The start's announcements (`GameService.start`): a title to every player, and the banner with the options to each
    /// player on a team. The original colours the title in the team's colour; the title here is white.
    fn announce_start(&self, host: &mut dyn ServerHost) {
        for player in host.players() {
            let color = self.teams.team_of(player.uuid).and_then(super::lobby_scene::team_color);
            host.show_title(player.uuid, &texts::start_title(), &texts::start_subtitle(), color.as_deref());
        }
        let options = self.options_text();
        for player in host.players() {
            if self.teams.team_of(player.uuid).is_none() {
                continue;
            }
            host.tell(player.uuid, &texts::separator());
            host.tell_line(player.uuid, &texts::start_banner());
            host.tell(player.uuid, &options);
            host.tell(player.uuid, &texts::separator());
        }
    }

    /// The start rules, worked out on a copy of the teams (the copy is returned, so nothing changes on an error).
    /// Spectators do not play. Nobody to play is an error. When no team exists and exactly one player is on none, that
    /// player joins `bingo_red`. Players on no team are an error unless `ignore_warnings`, which makes them spectators.
    fn plan_start(&self, players: &[Player], ignore_warnings: bool) -> Result<Teams, String> {
        let active: Vec<u128> = players
            .iter()
            .map(|player| player.uuid)
            .filter(|&player| !self.teams.is_spectator(player))
            .collect();
        if active.is_empty() {
            return Err("Nobody to play.".to_string());
        }
        let mut teams = self.teams.clone();
        if self.rules.team_auto_shuffle {
            // Seeded from the round, not the shared generator, so the ready timer's check and the start deal the same teams.
            let mut rng = Rng::new(self.seed ^ AUTO_SHUFFLE_SALT);
            teams.shuffle(&active, self.rules.team_count, &mut rng)?;
        }
        let mut unassigned = teams.unassigned(&active);
        if teams.teams().is_empty() && unassigned.len() == 1 {
            teams.join(unassigned[0], "bingo_red")?;
            unassigned.clear();
        }
        if !unassigned.is_empty() {
            if !ignore_warnings {
                return Err(format!(
                    "{} player(s) are not on a team. Use join <team> or shuffleteams <n>, or start with ignore_warnings to make them spectators.",
                    unassigned.len()
                ));
            }
            for player in unassigned {
                teams.spectate(player);
            }
        }
        if teams.teams().is_empty() {
            return Err("Nobody to play.".to_string());
        }
        Ok(teams)
    }

    /// Commands for the server's team list: each team with members is added and coloured, its online members join it,
    /// and with `spread` the players are spread out.
    fn add_teams_to_server(&mut self, host: &mut dyn ServerHost, players: &[Player]) {
        for key in self.teams.teams() {
            host.command(&format!("team add {key}"));
            let color = teams::preset(key).map_or("WHITE", |preset| preset.color).to_ascii_lowercase();
            host.command(&format!("team modify {key} color {color}"));
            for member in self.teams.members(key) {
                if let Some(player) = players.iter().find(|player| player.uuid == member) {
                    host.command(&format!("team join {key} {}", player.name));
                }
            }
            self.added_teams.push(key.to_string());
        }
        if let Some((_, spread)) = self.options.spread {
            let distance = (self.rules.spawn_distance as i32).saturating_mul(16).max(16);
            let spread = spread.max(distance.saturating_mul(2));
            // Horizon's `spreadplayers ... true` groups remote players into one "team", so the first member of each
            // team is spread on their own and the rest of the team is teleported to them.
            let reps: Vec<(&str, &str)> = self
                .teams
                .teams()
                .into_iter()
                .filter_map(|key| {
                    let rep = self.teams.members(key).into_iter().find_map(|m| players.iter().find(|p| p.uuid == m))?;
                    Some((key, rep.name.as_str()))
                })
                .collect();
            if reps.len() >= 2 {
                for (_, name) in &reps {
                    host.command(&format!("tag {name} add {SPREAD_TAG}"));
                }
                host.command(&format!("spreadplayers 0 0 {distance} {spread} false @a[tag={SPREAD_TAG}]"));
                for (key, name) in &reps {
                    host.command(&format!("tp @a[team={key},tag=!{SPREAD_TAG}] {name}"));
                }
                for (_, name) in &reps {
                    host.command(&format!("tag {name} remove {SPREAD_TAG}"));
                }
            }
        }
    }

    /// Makes the card and shows the game to everyone online.
    fn begin_round(&mut self, host: &mut dyn ServerHost) {
        let pool = self.card_pool();
        self.cards = vec![generate_card(self.seed, &pool, self.rules.tier_counts)];
        self.card_started = vec![0];
        self.team_card = self.teams.teams().into_iter().map(|key| (key.to_string(), 0)).collect();
        self.completions.clear();
        self.finished.clear();
        // The game starts: the first save of it. Scoring, joins and the time limit save again from here on.
        self.save_playing(host);
        self.progress = self.teams.teams().into_iter().map(|key| (key.to_string(), TeamProgress::default())).collect();
        self.captures.clear();
        self.persistent.clear();
        self.flashes.clear();
        self.owners = vec![HashMap::new()];
        self.objective_runtime = vec![Runtime::default()];
        self.seen.clear();
        self.reached.clear();
        self.votes.clear();
        self.forfeit = false;
        self.end_reason = None;
        let players = host.players();
        for player in &players {
            if self.teams.team_of(player.uuid).is_some() {
                host.command(&format!("gamemode survival {}", player.name));
                if self.rules.elytra {
                    host.command(&format!("give {} elytra", player.name));
                }
                for command in self.rules.kit.give_commands(&player.name) {
                    host.command(&command);
                }
            } else {
                host.command(&format!("gamemode spectator {}", player.name));
            }
        }
        for player in &players {
            self.send_round(host, player.uuid);
        }
    }

    /// What a player gets to see the game: every team with online members, the tiles of their own team (none for a
    /// spectator), the open card and the score of every team.
    fn send_round(&self, host: &mut dyn ServerHost, player: u128) {
        let players = host.players();
        let own = self.teams.team_of(player);
        let view: Vec<(&str, &str, &str, Vec<Player>)> = self
            .teams
            .teams()
            .into_iter()
            .filter_map(|key| {
                let preset = teams::preset(key)?;
                let members: Vec<Player> = players
                    .iter()
                    .filter(|candidate| self.teams.team_of(candidate.uuid) == Some(key))
                    .cloned()
                    .collect();
                // A player with a team sees only their own card; a spectator sees every card.
                (!members.is_empty() && own.map_or(true, |own| own == key)).then_some((key, preset.color, preset.name, members))
            })
            .collect();
        send(host, player, wire::display_cards_all(&view));

        if let Some(team) = own {
            if let Some(tiles) = self.tiles_for(&*host, team, false) {
                send(host, player, tiles);
            }
        }
        send(host, player, wire::open_card(own));
        for key in self.teams.teams() {
            send(host, player, wire::score_update(own == Some(key), key, self.score(key)));
        }
    }

    /// The end screen's vote options to everyone online (Postgame): the primary button's text and the More list.
    fn send_vote_updates(&self, host: &mut dyn ServerHost) {
        let players = host.players();
        for player in &players {
            let packet = postgame_packet(player.uuid, &self.votes, players.len());
            send(host, player.uuid, encode_update_v2(&packet));
        }
    }

    /// The ready screen to everyone online, from the ready timer.
    fn send_ready_updates(&self, host: &mut dyn ServerHost) {
        let players = host.players();
        let online: Vec<u128> = players.iter().map(|player| player.uuid).collect();
        for player in &players {
            let packet = self.ready.lobby_packet(player.uuid, &online, "pregame", GameStatus::Pregame, READY_COMMAND);
            send(host, player.uuid, encode_update_v2(&packet));
        }
    }

    fn dispatch(&mut self, host: &mut dyn ServerHost, events: Vec<Event>) {
        for event in events {
            match event {
                Event::StatusChanged(status) => {
                    send_all(host, self.game_status());
                    if status == GameStatus::Starting {
                        // Held for the whole start delay and the countdown (the original rides the players on bats
                        // from entering COUNTDOWN, CountdownController.kt:55-93, 145-148), not only the numbers.
                        let span = self.game.start_span_secs();
                        if span > 0 {
                            // A second over: the thaw at the 0 ends it.
                            self.freeze_players(host, span + 1);
                        }
                    }
                    if status == GameStatus::Playing {
                        self.begin_round(host);
                    }
                }
                Event::Countdown { seconds } => {
                    send_all(host, wire::countdown(seconds));
                    if seconds <= 0 {
                        self.thaw_players(host);
                    }
                    self.countdown_titles(host, seconds);
                }
                Event::Timer { seconds_remaining } => send_all(host, wire::timer(seconds_remaining)),
                Event::Ended { winner } => self.announce_end(host, winner),
                Event::Reset => self.reset_round(host),
                Event::Resumed => {
                    send_all(host, self.game_status());
                    host.broadcast_line(&texts::game_continues());
                }
            }
        }
    }

    /// Holds every player on a team still for the countdown's `seconds` (the original rides them on invisible bats): slowness
    /// and jump boost at their top levels, queued as console commands.
    fn freeze_players(&mut self, host: &mut dyn ServerHost, seconds: i32) {
        for player in host.players() {
            if self.teams.team_of(player.uuid).is_some() {
                for line in super::lobby_scene::freeze_commands(&player.name, seconds) {
                    host.command(&line);
                }
            }
        }
        self.frozen = true;
    }

    /// Lets the players go when the countdown ends or the round is reset during it. Nothing happens if they were not held.
    fn thaw_players(&mut self, host: &mut dyn ServerHost) {
        if !std::mem::take(&mut self.frozen) {
            return;
        }
        for player in host.players() {
            for line in super::lobby_scene::thaw_commands(&player.name) {
                host.command(&line);
            }
        }
    }

    /// The titles of a countdown step (`CountdownService.sendCountdownPacket`): the number for each second left, and at zero
    /// `BINGO` in the team's colour (white without a team) over `Game has started!`. The start's own title
    /// (`announce_start`) goes out with the start; the first number follows after the start delay. With no countdown the 0
    /// comes with the start and shows nothing, the start title stays instead.
    fn countdown_titles(&mut self, host: &mut dyn ServerHost, seconds: i32) {
        if self.countdown_first {
            return;
        }
        for player in host.players() {
            if seconds > 0 {
                host.show_title(player.uuid, &texts::countdown_number(seconds), &texts::no_subtitle(), None);
            } else if seconds == 0 {
                let color = self.teams.team_of(player.uuid).and_then(super::lobby_scene::team_color);
                let color = color.as_deref().unwrap_or("white");
                host.show_title(player.uuid, &texts::start_title(), &texts::started_subtitle(), Some(color));
            }
        }
    }

    /// The game ended: the end screen with the ranking of every team goes to each player, who wins if their team does.
    fn announce_end(&mut self, host: &mut dyn ServerHost, winner: Option<String>) {
        self.hide_boss_bars(host);
        // The sidebar's end (`GameOverService.getTitle` and `getMessage`): a win is `BINGO!`, else why the game ended.
        let reason = match winner {
            Some(_) => texts::bingo_reason(),
            None => self.end_reason.clone().unwrap_or_else(texts::out_of_time),
        };
        let teams = self.sidebar_teams(&host.players());
        let first_win = self.rules.end_when == EndWhen::FirstWin;
        self.sidebar_end = Some(super::sidebar::End {
            reason,
            message: super::sidebar::end_message(&teams, winner.as_deref(), first_win),
        });
        self.record_game(host, winner.as_deref());
        Self::clear_saved_game(host);
        self.end_winner = Some(winner.clone());
        for player in host.players() {
            self.send_game_over(host, player.uuid, false);
        }
        self.send_vote_updates(host);
        // Postgame shows every square, so each team's card is sent again with nothing hidden.
        if self.rules.hidden_consume.hidden_items {
            for player in host.players() {
                if let Some(team) = self.teams.team_of(player.uuid) {
                    if let Some(tiles) = self.tiles_for(&*host, team, false) {
                        send(host, player.uuid, tiles);
                    }
                }
            }
        }
        self.send_end_summary(host);
    }

    /// The end screen's packet to one player (`GameOverService.createPacket`), who wins if their team does. `is_update`
    /// refreshes a screen already open (a player who joins in Postgame), without opening it or playing the sound.
    fn send_game_over(&self, host: &mut dyn ServerHost, uuid: u128, is_update: bool) {
        let Some(winner) = self.end_winner.clone() else { return };
        let is_winner = winner.is_some() && self.teams.team_of(uuid) == winner.as_deref();
        let end = self.end_stats(Some(uuid));
        let extras = wire::EndExtras {
            seed: Some(self.world_seed.unwrap_or(self.seed) as i64),
            ready_vote_available: true,
            is_update,
            title: self.sidebar_end.as_ref().map(|end| (end.reason.key.to_owned(), end.reason.template.to_owned(), end.reason.args.clone())),
            subtitle: self.sidebar_end.as_ref().and_then(|end| end.message.as_ref()).map(|m| {
                let args = m.team.iter().map(|t| t.name.clone()).collect();
                (m.key.to_owned(), m.fallback.to_owned(), args)
            }),
        };
        let packet = wire::game_over_v2_with(winner.as_deref(), is_winner, self.game.elapsed_ms(), self.ranking(), &end, &extras);
        send(host, uuid, packet);
    }

    /// The end's chat for each player on a team (`GameOverController.broadcastGameOver`, `game_end.json`): a yellow rule,
    /// the message (yellow) or else the reason, the duration, the seed, the ranking of the teams after a blank line when
    /// there is more than one (unless the first win ends a game with one win condition), and a yellow rule.
    fn send_end_summary(&mut self, host: &mut dyn ServerHost) {
        self.end_reason = None;
        let Some(end) = self.sidebar_end.clone() else { return };
        let reason = match &end.message {
            Some(message) => {
                let name = message.team.as_ref().map(|team| Span::text(team.name.clone()));
                Span::translate(message.key, message.fallback, name.into_iter().collect()).in_color("yellow")
            }
            None => line_span(&end.reason),
        };
        let clock = texts::duration_text(self.game.elapsed_ms());
        let seed = self.world_seed.unwrap_or(self.seed) as i64;
        let ranking = self.ranking();
        let ranked = !(self.rules.end_when == EndWhen::FirstWin && self.rules.win_condition != WinCondition::Infinite)
            && ranking.len() > 1;
        for player in host.players() {
            if self.teams.team_of(player.uuid).is_none() {
                continue;
            }
            host.tell_spans(player.uuid, &texts::end_rule_spans());
            host.tell_spans(player.uuid, &[Span::text("  "), reason.clone()]);
            host.tell_spans(player.uuid, &texts::end_duration_spans(&clock));
            host.tell_spans(player.uuid, &texts::end_seed_spans(seed));
            if ranked {
                host.tell_spans(player.uuid, &[]);
                for row in &ranking {
                    let score = texts::score_text(row.score);
                    let duration = row.duration.as_deref().map(|d| texts::duration_text(parse_java_duration(d)));
                    let line = texts::end_row_spans(row.index as usize + 1, team_name(&row.key), &score, duration.as_deref());
                    host.tell_spans(player.uuid, &line);
                }
            }
            host.tell_spans(player.uuid, &texts::end_rule_spans());
        }
    }

    /// Back to Pregame: the card, the progress and the ready flags go; the teams stay, but the server's team list drops
    /// the teams the start added.
    fn reset_round(&mut self, host: &mut dyn ServerHost) {
        self.thaw_players(host);
        self.send_to_lobby(host);
        self.hide_boss_bars(host);
        self.sidebar_end = None;
        self.end_winner = None;
        Self::clear_saved_game(host);
        for team in std::mem::take(&mut self.added_teams) {
            host.command(&format!("team remove {team}"));
        }
        send_all(host, wire::game_reset());
        send_all(host, wire::card_reset());
        for player in host.players() {
            host.command(&format!("gamemode survival {}", player.name));
        }
        self.cards.clear();
        self.card_started.clear();
        self.team_card.clear();
        self.completions.clear();
        self.finished.clear();
        self.owners.clear();
        self.seen.clear();
        self.reached.clear();
        self.votes.clear();
        self.progress.clear();
        self.persistent.clear();
        self.objective_runtime.clear();
        self.flashes.clear();
        self.ready.clear();
        self.seed = self.seed.wrapping_add(1);
        self.send_ready_updates(host);
    }

    fn game_status(&self) -> (&'static str, Vec<u8>) {
        wire::game_status(Some(self.game.elapsed_ms()), self.game.remaining_ms())
    }

    /// Score of a team. A team with no progress yet has none. In a `ReplaceGoals` round the score is the total: the
    /// goals replaced so far plus what the card shows.
    fn score(&self, team: &str) -> TeamScore {
        if self.rules.win_condition == WinCondition::ReplaceGoals {
            let mut score = self.total_score(team);
            score.cards = i32::from(self.goal_met(team));
            return score;
        }
        self.progress.get(team).map_or_else(TeamScore::default, |progress| scoring::team_score_goal(progress, self.rules.goal))
    }

    /// The items and lines a team has: those on its card, plus its persistent score (zero outside `ReplaceGoals`).
    fn total_score(&self, team: &str) -> TeamScore {
        let mut score = self.progress.get(team).map(scoring::team_score).unwrap_or_default();
        if let Some(done) = self.persistent.get(team) {
            score.items += done.items;
            score.lines += done.lines;
        }
        score
    }

    /// Whether a team has reached the round's goal. In a `ReplaceGoals` round the goals it had replaced count too, so
    /// the test is on the total: `Lines(n)` lines, `Items(n)` items, and 25 items for a full card.
    fn goal_met(&self, team: &str) -> bool {
        if self.rules.win_condition != WinCondition::ReplaceGoals {
            return self.progress.get(team).is_some_and(|progress| scoring::goal_reached(progress, self.rules.goal));
        }
        let total = self.total_score(team);
        match self.rules.goal {
            Goal::Lines(0) | Goal::Items(0) => false,
            Goal::Lines(n) => i64::from(total.lines) >= i64::from(n),
            Goal::Items(n) => i64::from(total.items) >= i64::from(n),
            Goal::FullCard => i64::from(total.items) >= CARD_SQUARES as i64,
        }
    }

    /// `ReplaceGoals`, run before the goals are read (`ScoredItemCheck.updateScores`): each square a team has scored is
    /// replaced by a new item of its tier, and the team's persistent score takes the squares and the full lines they
    /// made. An items round scores every found square; a lines round scores the squares of the full lines only. A team
    /// that has finished scores nothing more. A replaced square is new, so it loses its progress and its owner for every
    /// team, and the squares that are not replaced stay as they are.
    fn replace_scored_goals(&mut self, host: &mut dyn ServerHost) {
        let goal = self.rules.goal;
        let teams: Vec<String> = self.teams.teams().into_iter().map(str::to_string).collect();
        let mut replaced: BTreeSet<usize> = BTreeSet::new();
        for key in &teams {
            if self.finished.contains_key(key) {
                continue;
            }
            let Some(progress) = self.progress.get(key) else { continue };
            let scored = scored_squares(&progress.found, goal);
            if scored.is_empty() {
                continue;
            }
            let lines = scoring::count_lines(&progress.found);
            let total = self.persistent.entry(key.clone()).or_default();
            total.items += scored.len() as i32;
            total.lines += lines;
            replaced.extend(scored);
        }
        if replaced.is_empty() {
            return;
        }

        let indices: Vec<usize> = replaced.iter().copied().collect();
        // The seed moves on with each batch: the scored items only grow, so no two batches share a seed.
        let batch: u64 = self.persistent.values().map(|score| score.items as u64).sum();
        let seed = self.seed.wrapping_add(batch);
        let pool = self.card_pool();
        if let Some(card) = self.cards.first_mut() {
            replace_squares(seed, &pool, card, &indices);
        }
        if let Some(runtime) = self.objective_runtime.first_mut() {
            runtime.forget(&indices);
        }
        for progress in self.progress.values_mut() {
            progress.found.retain(|square| !replaced.contains(square));
        }
        for owners in self.owners.iter_mut() {
            owners.retain(|square, _| !replaced.contains(square));
        }
        for squares in self.seen.values_mut() {
            squares.retain(|square| !replaced.contains(square));
        }
        for key in &teams {
            self.send_new_card(host, key);
            self.send_score_to_all(host, key);
        }
    }

    /// Every team's found squares on card `index`, in the order of the presets, for [`wire::card_tiles_seen`]. Teams on
    /// another card are left out: their squares are not on this one.
    fn found_on(&self, index: usize) -> Vec<(&'static str, &[usize])> {
        self.teams
            .teams()
            .into_iter()
            .filter(|&key| self.card_index_of(key) == index)
            .filter_map(|key| self.progress.get(key).map(|progress| (key, progress.found.as_slice())))
            .collect()
    }

    /// The tiles of a viewer's team. `None` before play, when there is no card.
    fn tiles_for(&self, host: &dyn ServerHost, viewer: &str, should_notify: bool) -> Option<(&'static str, Vec<u8>)> {
        let index = self.card_index_of(viewer);
        let card = self.cards.get(index)?;
        if let Some(squares) = self.card_objectives(index) {
            let none: &[usize] = &[];
            let found = self.progress.get(viewer).map_or(none, |progress| progress.found.as_slice());
            let item_id = |item: &str| host.item_numeric_id(item);
            return objectives::card_from_objectives(viewer, &squares, found, &item_id, &objectives::Tags::new(), should_notify).ok();
        }
        let found = self.found_on(index);
        let item_id = |item: &str| host.item_numeric_id(item);
        let views = self.views_for(Some(viewer));
        Some(wire::card_tiles_seen(viewer, card, &found, &views, &item_id, should_notify))
    }

    /// Every square as `viewer` sees it (hidden-items mode, see `hidden_consume::tile_views`). `None` is a player on no
    /// team, who sees every square hidden. Postgame shows all.
    fn views_for(&self, viewer: Option<&str>) -> Vec<TileView> {
        let card = match viewer {
            Some(team) => self.card_of(team),
            None => self.cards.first(),
        };
        let Some(card) = card else { return Vec::new() };
        let none: &[usize] = &[];
        let sight = viewer.map(|team| Sight {
            seen: self.seen.get(team).map_or(none, |squares| squares.as_slice()),
            found: self.progress.get(team).map_or(none, |progress| progress.found.as_slice()),
            finished: self.reached.iter().any(|key| key == team),
        });
        let postgame = self.game.status() == GameStatus::Postgame;
        hidden_consume::tile_views(card, self.rules.hidden_consume, false, sight, postgame)
    }

    /// The squares of card `index` when it has objective squares (see `objectives::card_squares`), else `None`.
    fn card_objectives(&self, index: usize) -> Option<Vec<objectives::Square>> {
        self.cards.get(index).and_then(objectives::card_squares)
    }

    /// Scores the objective squares of a team's card from what its members hold now, their statistics and their
    /// advancements (`objectives::update_team`). Lockout gives each new square to the team when no team has it. A
    /// square found is announced and sent to the team, the goals are checked and the game is saved. Nothing happens
    /// until every online member's inventory has been reported.
    fn apply_objectives(&mut self, host: &mut dyn ServerHost, finder: Option<u128>, team: &str) {
        let index = self.card_index_of(team);
        let Some(squares) = self.card_objectives(index) else { return };
        let members = self.teams.members(team);
        let online: Vec<u128> = host.players().iter().map(|player| player.uuid).collect();
        if members.is_empty() || online.iter().any(|uuid| members.contains(uuid) && !self.reported.contains(uuid)) {
            return;
        }
        let standing = self.standing(team);
        let before: Vec<usize> = self.progress.get(team).map(|progress| progress.found.clone()).unwrap_or_default();
        let mut found = before.clone();
        let inventory_mode = self.rules.mode == Mode::Inventory;
        let tags = objectives::Tags::new();
        if self.objective_runtime.len() <= index {
            self.objective_runtime.resize_with(index + 1, Runtime::default);
        }
        let newly = {
            let facts = objectives::HostFacts { held: &self.held, host: &*host };
            let ctx = objectives::Ctx { facts: &facts, tags: &tags };
            let runtime = &mut self.objective_runtime[index];
            objectives::update_team(&squares, &mut found, &members, &ctx, runtime, inventory_mode)
        };
        let claimed: Vec<usize> = if self.rules.mode == Mode::Lockout {
            let Some(owners) = self.owners.get_mut(index) else { return };
            scoring::claim_lockout(owners, team, &newly)
        } else {
            newly.clone()
        };
        found.retain(|square| !newly.contains(square) || claimed.contains(square));
        if found == before {
            return;
        }
        let removed: Vec<usize> = before.iter().filter(|square| !found.contains(square)).copied().collect();
        self.progress.entry(team.to_string()).or_default().found = found;
        let who = finder.unwrap_or(members[0]);
        self.captures.gained(team, who, &claimed);
        self.captures.lost(team, &removed);
        let name = player_name(&*host, who);
        for square in &claimed {
            let label = objectives::describe(&squares[*square].objective);
            self.announce_find(host, &texts::objective_captured(&name, &label));
        }
        self.send_tiles_after_find(host, team);
        self.send_score_to_all(host, team);
        self.announce_lines(host, team, &standing);
        self.check_goals(host);
        self.announce_leader(host, &standing);
        self.save_playing(host);
    }

    /// Scores the objective squares of every team's card: the poll of [`OBJECTIVE_POLL_TICKS`].
    fn poll_objectives(&mut self, host: &mut dyn ServerHost) {
        let teams: Vec<String> = self.teams.teams().into_iter().map(str::to_string).collect();
        for team in teams {
            if self.game.status() != GameStatus::Playing {
                break;
            }
            self.apply_objectives(host, None, &team);
        }
    }

    /// Adds the squares whose items `held` holds to the team's seen squares. Returns whether any square was new.
    fn record_seen(&mut self, team: &str, held: &HashMap<String, u32>) -> bool {
        let Some(card) = self.card_of(team) else { return false };
        let holding: Vec<usize> = card
            .squares
            .iter()
            .enumerate()
            .filter(|(_, square)| held.get(&square.item).copied().unwrap_or(0) >= 1)
            .map(|(index, _)| index)
            .collect();
        let seen = self.seen.entry(team.to_string()).or_default();
        let before = seen.len();
        hidden_consume::record_seen(seen, &holding);
        seen.len() != before
    }

    /// Consume items: the items a team captured with this inventory are taken from the capturing player (`name`), one
    /// `clear` per square. Nothing when the mode is off, or in inventory mode (see `hidden_consume::consume_on_score`).
    /// The server is always in lobby mode here, so that rule does not apply.
    fn queue_consume(&self, host: &mut dyn ServerHost, name: &str, held: &HashMap<String, u32>, items: &[String]) {
        if !self.rules.hidden_consume.consume_items {
            return;
        }
        let mut stacks: Vec<HeldStack> = held
            .iter()
            .map(|(item, &count)| HeldStack { item: item.clone(), count, bingo_ignore: false, nested: Vec::new() })
            .collect();
        let inventory = self.rules.mode == Mode::Inventory;
        for item in items {
            for (taken, count) in hidden_consume::consume_on_score(&mut stacks, item, 1, self.rules.hidden_consume, inventory, true) {
                host.command(&format!("clear {name} {taken} {count}"));
            }
        }
    }

    /// After a find: each player on a team gets the tiles of their team. Only the members of the finding team are told.
    fn send_tiles_after_find(&self, host: &mut dyn ServerHost, finder: &str) {
        for player in host.players() {
            let Some(team) = self.teams.team_of(player.uuid) else { continue };
            if let Some(tiles) = self.tiles_for(&*host, team, team == finder) {
                send(host, player.uuid, tiles);
            }
        }
    }

    /// The score of one team, to everyone online. Only members of that team are "on the team" for their own view.
    fn send_score_to_all(&self, host: &mut dyn ServerHost, team: &str) {
        let score = self.score(team);
        for player in host.players() {
            let on_team = self.teams.team_of(player.uuid) == Some(team);
            send(host, player.uuid, wire::score_update(on_team, team, score.clone()));
        }
    }

    /// The end-screen ranking in the order of `ScoreRankingService`: the teams that finished first, then by cards, lines and
    /// items, a tie sharing its place. A team that finished has the time it took; one that did not has none (`DNF`),
    /// except in infinite mode, where every team has the game's duration.
    fn ranking(&self) -> Vec<ScoreRanking> {
        let teams = self.sidebar_teams(&[]);
        let infinite = self.rules.win_condition == WinCondition::Infinite;
        super::sidebar::ranked(&teams)
            .into_iter()
            .map(|(place, team)| ScoreRanking {
                index: place as i32,
                key: team.key.clone(),
                name: Text::plain(team_name(&team.key)),
                score: self.score(&team.key),
                duration: match team.finished {
                    _ if infinite => Some(java_duration(self.game.elapsed_ms())),
                    Some(ms) => Some(java_duration(ms)),
                    None => None,
                },
            })
            .collect()
    }

    /// The boss bar of each team member online, with the time left, every 10 ticks while the game has a time limit.
    fn send_boss_bars(&mut self, host: &mut dyn ServerHost) {
        let Some(remaining) = self.game.remaining_ms() else { return };
        let total = remaining + self.game.elapsed_ms();
        if total <= 0 {
            return;
        }
        let progress = (remaining as f32 / total as f32).clamp(0.0, 1.0);
        let seconds = remaining / 1000;
        let title = format!("{:02}:{:02} left", seconds / 60, seconds % 60);
        let online: Vec<u128> = host.players().iter().map(|player| player.uuid).collect();
        for key in self.teams.teams() {
            let id = format!("bingo:{key}");
            for member in self.teams.members(key) {
                // A player who sent no settings sees the bar; one who turned `bossbar` off does not.
                let wants = self.settings.get(&member).map_or(true, |settings| settings.bossbar);
                if online.contains(&member) && wants {
                    host.boss_bar(member, &id, &title, progress, true);
                    self.boss_bars.insert((member, id.clone()));
                }
            }
        }
    }

    fn hide_boss_bars(&mut self, host: &mut dyn ServerHost) {
        for (player, id) in std::mem::take(&mut self.boss_bars) {
            host.boss_bar(player, &id, "", 0.0, false);
        }
    }

    /// Team chat is on now: the option is on and the game is Starting or Playing (`chat.rs`).
    pub fn team_chat_on(&self) -> bool {
        self.rules.team_chat && matches!(self.game.status(), GameStatus::Starting | GameStatus::Playing)
    }

    /// Where a chat line of `sender` goes during team chat, and the line (`chat.rs`).
    pub fn team_chat_route(&self, online: &[Player], sender: u128, message: &str) -> super::chat::Routed {
        let name = online.iter().find(|player| player.uuid == sender).map_or("", |player| player.name.as_str());
        let roster: Vec<(u128, Option<&str>)> = online.iter().map(|player| (player.uuid, self.teams.team_of(player.uuid))).collect();
        super::chat::route(name, self.teams.team_of(sender), &roster, message)
    }

    /// The sidebar's lines (`sidebar.rs`), or `None` when the sidebar is off in the options. `online` is who is online:
    /// the lobby's ready count and the name of a team's only player.
    pub fn sidebar_rows(&self, online: &[Player]) -> Option<Vec<super::sidebar::Row>> {
        self.rules.sidebar.then(|| super::sidebar::rows(&self.sidebar_facts(online)))
    }

    /// The teams that play, as the sidebar shows them, in preset order: names, colours, symbols, scores and finish times.
    fn sidebar_teams(&self, online: &[Player]) -> Vec<super::sidebar::Standing> {
        self.teams
            .teams()
            .into_iter()
            .filter_map(|key| {
                let preset = teams::preset(key)?;
                let score = self.score(key);
                let player = match self.teams.members(key).as_slice() {
                    [only] => online.iter().find(|player| player.uuid == *only).map(|player| player.name.clone()),
                    _ => None,
                };
                Some(super::sidebar::Standing {
                    key: key.to_string(),
                    name: preset.name.to_string(),
                    color: preset.color.to_ascii_lowercase(),
                    symbol: Some(preset.symbol.to_string()),
                    player,
                    items: score.items,
                    lines: score.lines,
                    cards: score.cards,
                    finished: self.finished.get(key).map(|&ms| ms as i64),
                })
            })
            .collect()
    }

    fn sidebar_facts(&self, online: &[Player]) -> super::sidebar::Facts {
        use super::sidebar::{Clock, Features, Phase};
        let phase = match self.game.status() {
            GameStatus::Pregame => Phase::Lobby,
            GameStatus::Starting | GameStatus::Playing => Phase::Live,
            GameStatus::Postgame => Phase::Over,
        };
        let clock = match phase {
            Phase::Lobby => match self.game.time_limit_secs() {
                Some(secs) => Clock::Limit(i64::from(secs) * 1000),
                None => Clock::NoLimit,
            },
            Phase::Over => Clock::NoLimit,
            Phase::Live => match self.game.remaining_ms() {
                Some(ms) => Clock::Left(ms),
                None => Clock::Elapsed(self.game.elapsed_ms()),
            },
        };
        let teams = self.sidebar_teams(online);
        let (ready_up, countdown) = match phase {
            Phase::Lobby => {
                let online_ids: Vec<u128> = online.iter().map(|player| player.uuid).collect();
                let (ready_up, countdown) = self.ready.sidebar_lines(&online_ids);
                (ready_up, Some(countdown))
            }
            _ => (None, None),
        };
        let end = match phase {
            Phase::Over => self.sidebar_end.clone(),
            _ => None,
        };
        // The cards still to play after this one (`remainingCards`): a team's own wins are taken off when one team plays.
        let more_cards = match self.rules.win_condition {
            WinCondition::Cards(cards) => {
                let won = match teams.as_slice() {
                    [only] => self.completions.get(only.key.as_str()).map_or(0, |list| {
                        list.iter().filter(|(_, is_winner)| *is_winner).count() as i32
                    }),
                    _ => 0,
                };
                let more = (cards as i32 - 1 - won).max(0);
                (more > 0).then_some(more)
            }
            WinCondition::Infinite | WinCondition::ReplaceGoals => None,
        };
        super::sidebar::Facts {
            phase,
            mode: self.rules.mode,
            hidden_items: self.rules.hidden_consume.hidden_items,
            consume_items: self.rules.hidden_consume.consume_items,
            replace_goals: self.rules.win_condition == WinCondition::ReplaceGoals,
            features: Features {
                elytra: self.rules.elytra,
                night_vision: self.rules.night_vision,
                keep_inventory: self.rules.keep_inventory,
                pvp: self.rules.pvp,
            },
            goal: self.rules.goal,
            infinite: self.rules.win_condition == WinCondition::Infinite,
            more_cards,
            multi_card: match self.rules.win_condition {
                WinCondition::Cards(cards) => cards > 1,
                WinCondition::Infinite => true,
                WinCondition::ReplaceGoals => false,
            },
            clock,
            teams,
            ready_up,
            countdown,
            end,
        }
    }

    /// `/bingo status`: the state of the game, then one line per team that plays.
    fn status_text(&self) -> String {
        let mut lines = vec![format!("Status: {:?}.", self.game.status())];
        if let Some(remaining_ms) = self.game.remaining_ms() {
            lines[0].push_str(&format!(" Time left: {} s.", remaining_ms / 1000));
        }
        for key in self.teams.teams() {
            let score = self.score(key);
            lines.push(format!(
                "{}: {} players, {} items, {} lines",
                team_name(key),
                self.teams.members(key).len(),
                score.items,
                score.lines
            ));
        }
        lines.join("\n")
    }

    /// `/bingo shuffleteams <n>`: deals the online players into `n` teams. Those whose team changes lose their ready flag.
    fn shuffle_teams(&mut self, host: &mut dyn ServerHost, count: usize) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "Teams can be changed only before the game starts.".to_string();
        }
        let online: Vec<u128> = host.players().iter().map(|player| player.uuid).collect();
        let mut teams = self.teams.clone();
        if let Err(error) = teams.shuffle(&online, count, &mut self.rng) {
            return error;
        }
        for &player in &online {
            if self.teams.team_of(player) != teams.team_of(player) {
                self.ready.set_ready(player, false);
            }
        }
        self.teams = teams;
        self.send_ready_updates(host);
        self.say(texts::shuffle_success())
    }

    /// `/bingo join <team>`: the player joins the team, leaving their old one or the spectators.
    fn join_team(&mut self, host: &mut dyn ServerHost, player: Option<u128>, team: &str) -> String {
        let Some(player) = player else {
            return "Only a player can join a team.".to_string();
        };
        if self.game.status() == GameStatus::Playing {
            return self.join_midgame(host, player, team);
        }
        if self.game.status() != GameStatus::Pregame {
            return "Teams can be changed only before the game starts.".to_string();
        }
        let before = self.teams.team_of(player);
        if let Err(error) = self.teams.join(player, team) {
            return error;
        }
        if before != self.teams.team_of(player) {
            self.ready.set_ready(player, false);
            self.send_ready_updates(host);
        }
        host.play_sound(player, super::lobby_scene::SOUND_TEAM_CHANGED);
        let name = player_name(&*host, player);
        self.say(texts::joined_team(&name, team_name(team)))
    }

    /// `/bingo spectator`: the player leaves their team and watches.
    fn become_spectator(&mut self, host: &mut dyn ServerHost, player: Option<u128>) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "Teams can be changed only before the game starts.".to_string();
        }
        let Some(player) = player else {
            return "Only a player can become a spectator.".to_string();
        };
        let was_on_team = self.teams.team_of(player).is_some();
        let was_spectator = self.teams.is_spectator(player);
        self.teams.spectate(player);
        if was_on_team || !was_spectator {
            self.ready.set_ready(player, false);
            self.send_ready_updates(host);
        }
        "You are now a spectator.".to_string()
    }

    /// `/bingo ready`: toggles the caller's ready flag.
    fn toggle_ready(&mut self, host: &mut dyn ServerHost, player: Option<u128>) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "Ready is possible only before the game starts.".to_string();
        }
        let Some(player) = player else {
            return "Only a player can ready up.".to_string();
        };
        // As the original's `ReadyService.setReady`: a player on no team (a spectator too) cannot ready up.
        if self.teams.team_of(player).is_none() {
            return self.say(texts::ready_not_on_team("/spectator"));
        }
        let ready = !self.ready.is_ready(player);
        self.ready.set_ready(player, ready);
        self.send_ready_updates(host);
        let name = player_name(&*host, player);
        self.say(if ready { texts::ready_true(&name) } else { texts::ready_false(&name) })
    }

    /// `/bingo ready cancel`: stops the ready timer and keeps it from starting again until someone is ready.
    fn cancel_ready(&mut self, host: &mut dyn ServerHost) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "The ready timer can be cancelled only before the game starts.".to_string();
        }
        self.ready.cancel();
        self.send_ready_updates(host);
        self.say(texts::ready_cancelled())
    }

    /// `/bingo timelimit <minutes|off>`, in Pregame or Playing.
    fn set_time_limit(&mut self, host: &mut dyn ServerHost, minutes: Option<u32>) -> String {
        if !matches!(self.game.status(), GameStatus::Pregame | GameStatus::Playing) {
            return "The time limit can be changed only in Pregame or Playing.".to_string();
        }
        let answer = match minutes {
            Some(minutes) => {
                let seconds = i32::try_from(minutes.saturating_mul(60)).unwrap_or(i32::MAX);
                self.game.set_time_limit(Some(seconds));
                format!("Time limit set to {minutes} minutes")
            }
            None => {
                self.game.set_time_limit(None);
                "Time limit turned off".to_string()
            }
        };
        self.save_playing(host);
        answer
    }

    /// `/bingo stats`: the caller's statistics, all time and this month, from the history.
    fn stats(&self, host: &dyn ServerHost, player: Option<u128>) -> String {
        let Some(player) = player else {
            return "Only a player has statistics.".to_string();
        };
        self.stats_of(host, player)
    }

    /// `/bingo stats <player>` (operators): the statistics of an online player.
    fn stats_of_player(&self, host: &dyn ServerHost, name: &str) -> String {
        match host.players().into_iter().find(|player| player.name.eq_ignore_ascii_case(name)) {
            Some(player) => self.stats_of(host, player.uuid),
            None => format!("No player named {name} is online."),
        }
    }

    /// The `bingo stats` text of one player: all time, then this month.
    fn stats_of(&self, host: &dyn ServerHost, uuid: u128) -> String {
        let month = stats::year_month(host.now_ms());
        stats::summary_text(
            &player_name(host, uuid),
            &stats::totals(&self.history, uuid, None),
            &stats::totals(&self.history, uuid, Some(month)),
        )
    }

    /// The end screen's numbers for `viewer` (`None`: the game-wide part only), from the round that just ended, which is
    /// the last history entry. Empty when there is none.
    fn end_stats(&self, viewer: Option<u128>) -> stats::EndStats {
        match self.history.split_last() {
            Some((game, prior)) => stats::end_stats(prior, game, viewer),
            None => stats::EndStats::default(),
        }
    }

    /// Records the finished game: one line in the history file, and in memory for `stats`.
    fn record_game(&mut self, host: &mut dyn ServerHost, winner: Option<&str>) {
        let ended_at_ms = host.now_ms();
        let players = host.players();
        let teams = self
            .teams
            .teams()
            .into_iter()
            .map(|key| {
                let score = self.score(key);
                HistoryTeam { key: key.to_string(), name: team_name(key).to_string(), items: score.items, lines: score.lines, won: winner == Some(key) }
            })
            .collect();
        // The roster: everyone on a team at the end, online or not. A name that is not online comes from the history.
        let mut roster = Vec::new();
        for key in self.teams.teams() {
            for uuid in self.teams.members(key) {
                let name = match players.iter().find(|player| player.uuid == uuid) {
                    Some(player) => player.name.clone(),
                    None => self
                        .history
                        .iter()
                        .rev()
                        .flat_map(|entry| entry.players.iter())
                        .find(|known| known.uuid == uuid)
                        .map_or_else(|| "A player".to_string(), |known| known.name.clone()),
                };
                let items = self.captures.items(uuid) as i32;
                roster.push(HistoryPlayer { uuid, name, team: key.to_string(), items });
            }
        }
        let entry = HistoryEntry {
            id: (u128::from(self.seed) << 64) | ended_at_ms.max(0) as u128,
            ended_at_ms,
            duration_ms: self.game.elapsed_ms(),
            winner: winner.map(str::to_string),
            forfeit: self.forfeit,
            teams,
            players: roster,
            rules: Some(stats::rules_key(&self.rules)),
        };
        host.append_line(HISTORY_FILE, &store::entry_to_line(&entry));
        self.history.push(entry);
    }

    /// `join <team>` while the game runs: allowed only for a spectator (or a player with no team) and only onto a team
    /// that already plays. The player becomes a survival player at their team's spawn.
    fn join_midgame(&mut self, host: &mut dyn ServerHost, player: u128, team: &str) -> String {
        if self.teams.team_of(player).is_some() {
            return "Teams can be changed only before the game starts.".to_string();
        }
        let key = format!("bingo_{}", team.to_lowercase().trim_start_matches("bingo_"));
        if !self.progress.contains_key(&key) {
            return format!("{} is not playing in this game.", team_name(&key));
        }
        let teammate = self.teams.members(&key).into_iter().find_map(|member| host.players().into_iter().find(|p| p.uuid == member));
        if let Err(error) = self.teams.join(player, &key) {
            return error;
        }
        let name = player_name(host, player);
        host.command(&format!("team join {key} {name}"));
        host.command(&format!("gamemode survival {name}"));
        if let Some(teammate) = teammate {
            host.command(&format!("tp {name} {}", teammate.name));
        }
        self.send_round(host, player);
        self.save_playing(host);
        let name = player_name(&*host, player);
        self.say(texts::joined_team(&name, team_name(&key)))
    }

    /// `/bingo reroll`: a new seed for the card the game will start with, drawn from the shuffle generator. Only before the
    /// game starts (there is no card to redraw before that: the native lobby shows none). The seed is not saved, as the
    /// round's seed is not saved outside a game in progress.
    fn reroll(&mut self, host: &mut dyn ServerHost, player: Option<u128>) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "The card can be rerolled only before the game starts.".to_string();
        }
        self.seed = self.rng.next_u64();
        if let Some(player) = player {
            host.play_sound(player, super::lobby_scene::SOUND_REROLL);
        }
        "Card re-rolled.".to_string()
    }

    /// Changes one round rule. Only before the game starts.
    fn set_rule(&mut self, host: &mut dyn ServerHost, change: impl FnOnce(&mut Options), what: &str) -> String {
        if self.game.status() != GameStatus::Pregame {
            return format!("The {what} can be changed only before the game starts.");
        }
        change(&mut self.rules);
        host.write_file(OPTIONS_FILE, &store::options_to_json(&self.rules));
        self.options_text()
    }

    /// `/bingo options`: the rules of the round.
    fn options_text(&self) -> String {
        let goal = match self.rules.goal {
            Goal::Lines(n) => format!("{n} line(s)"),
            Goal::Items(n) => format!("{n} item(s)"),
            Goal::FullCard => "full card".to_string(),
        };
        let mode = match self.rules.mode {
            Mode::Standard => "standard",
            Mode::Inventory => "inventory",
            Mode::Lockout => "lockout",
        };
        let end = match self.rules.end_when {
            EndWhen::Never => "never".to_string(),
            EndWhen::FirstWin => "first win".to_string(),
            EndWhen::TeamsWin(n) => format!("{n} teams win"),
            EndWhen::AllWin => "all teams win".to_string(),
        };
        let on = |value: bool| if value { "on" } else { "off" };
        // The end is named only when it is not the default, as the original's summary does.
        let end = if self.rules.end_when == EndWhen::FirstWin { String::new() } else { format!(" End: {end}.") };
        let mut text = format!(
            "Goal: {goal}. Mode: {mode}.{end} PvP: {}. Keep inventory: {}. Elytra: {}. Night vision: {}. Spawn distance: {} chunks.",
            on(self.rules.pvp),
            on(self.rules.keep_inventory),
            on(self.rules.elytra),
            on(self.rules.night_vision),
            self.rules.spawn_distance
        );
        // The time limit is named only when one is set, as the original's summary does (`Time Limit: 1h 30m`).
        if let Some(minutes) = self.time_limit_minutes() {
            text.push_str(&format!(" Time limit: {}h {:02}m.", minutes / 60, minutes % 60));
        }
        if !self.rules.preview_card {
            text.push_str(" Preview card: off.");
        }
        // The win condition and the stalemate behaviour are named only when they are not the defaults.
        if self.rules.win_condition != WinCondition::default() {
            let win = match self.rules.win_condition {
                WinCondition::Cards(n) => format!("{n} cards"),
                WinCondition::Infinite => "infinite".to_string(),
                WinCondition::ReplaceGoals => "replace goals".to_string(),
            };
            text.push_str(&format!(" Win: {win}."));
        }
        if self.rules.stalemate != StalemateBehavior::default() {
            let behaviour = match self.rules.stalemate {
                StalemateBehavior::EndGame => "end game",
                StalemateBehavior::RerollCard => "reroll card",
                StalemateBehavior::Nothing => "do nothing",
            };
            text.push_str(&format!(" Stalemate: {behaviour}."));
        }
        // The difficulty and the auto shuffle are named only when they are not the defaults.
        if self.rules.tier_counts != card_gen::DEFAULT_DISTRIBUTION {
            text.push_str(&format!(" Difficulty: {}.", difficulty::describe(self.rules.tier_counts)));
        }
        if self.rules.team_auto_shuffle {
            text.push_str(&format!(" Auto shuffle: on, {} teams.", self.rules.team_count));
        }
        // The advanced squares are named only when on, as the other rules that are off by default.
        if self.rules.objectives {
            text.push_str(" Objectives: on.");
        }
        text
    }

    /// `/bingo vote`: only in Postgame. When every online player who plays has voted, the option with most votes
    /// is carried out (a tie goes to the next round, then keep playing).
    fn vote(&mut self, host: &mut dyn ServerHost, player: u128, choice: VoteChoice) -> String {
        if self.game.status() != GameStatus::Postgame {
            return "Voting is possible only after the game ends.".to_string();
        }
        self.votes.insert(player, choice);
        let voters: Vec<u128> = host
            .players()
            .iter()
            .map(|p| p.uuid)
            .filter(|&uuid| !self.teams.is_spectator(uuid))
            .collect();
        if voters.iter().all(|uuid| self.votes.contains_key(uuid)) {
            let count = |wanted: VoteChoice| voters.iter().filter(|uuid| self.votes.get(uuid) == Some(&wanted)).count();
            let (next, keep, lobby) = (count(VoteChoice::NextRound), count(VoteChoice::KeepPlaying), count(VoteChoice::ReturnToLobby));
            if keep > next && keep > lobby {
                return self.resume(host);
            }
            let events = self.game.reset();
            self.dispatch(host, events);
            return "The vote is over: back to the lobby.".to_string();
        }
        format!("Vote recorded ({} of {} votes).", self.votes.len().min(voters.len()), voters.len())
    }

    /// `/bingo resume`: Postgame back to Playing. A condition that would end the game again at once is turned off.
    fn resume(&mut self, host: &mut dyn ServerHost) -> String {
        if self.game.status() != GameStatus::Postgame {
            return "The game can be resumed only after it ends.".to_string();
        }
        let ended_by_rule = if self.is_multi() {
            round_ends(self.rules.end_when, &self.rounds_view())
        } else {
            scoring::game_should_end(self.rules.end_when, self.reached.len(), self.teams.teams().len())
        };
        match self.game.resume(self.options.extend_secs) {
            Ok(events) => {
                if ended_by_rule {
                    self.rules.end_when = EndWhen::Never;
                }
                self.votes.clear();
                self.dispatch(host, events);
                self.save_playing(host);
                "The game continues.".to_string()
            }
            Err(error) => error.to_string(),
        }
    }

    /// `/bingo card`: opens the card of the caller's team.
    fn open_card(&self, host: &mut dyn ServerHost, player: Option<u128>) -> String {
        let Some(player) = player else {
            return "Only a player can open a card.".to_string();
        };
        if let Some(team) = self.teams.team_of(player) {
            send(host, player, wire::open_card(Some(team)));
        }
        "Opening your card".to_string()
    }
}

/// A card as `card_value` writes it: its squares, each an item and a tier.
fn card_value(card: &Card) -> Value {
    Value::Array(card.squares.iter().map(|square| json!({ "item": square.item, "tier": square.tier.name() })).collect())
}

/// A card read from its squares, as `card_value` writes them.
fn parse_card(value: &Value) -> Result<Card, String> {
    let squares = value.as_array().ok_or("a card is not a list")?.iter().map(parse_square).collect::<Result<Vec<_>, _>>()?;
    if squares.len() != CARD_SQUARES {
        return Err(format!("a card has {CARD_SQUARES} squares, not {}", squares.len()));
    }
    Ok(Card { squares })
}

/// The chat line when a team completes a card: `Red completed card 1/2!`, or `Red completed card 1!` when the win has
/// no number of cards (`Infinite`).
fn completion_notice(round: &TeamRound, rules: &Rules) -> String {
    let name = team_name(&round.key);
    let done = round.count_cards();
    match rules.win.required_cards() {
        Some(total) => format!("{name} completed card {done}/{total}!"),
        None => format!("{name} completed card {done}!"),
    }
}

/// The reason the end's chat gives for a stalemate that ended the game.
fn stalemate_end_reason(cause: EndCause) -> Line {
    match cause {
        EndCause::Stalemate => texts::stalemate_reason(),
        EndCause::Lockout => texts::lockout_reason(),
        EndCause::ImpossibleGoal => texts::soft_locked_reason(),
    }
}

/// The name of a goal as the warnings write it: `2 lines`, `6 items`, `full card`.
fn goal_text(goal: Goal) -> String {
    match goal {
        Goal::Lines(n) => texts::count_text(n, "line", "lines"),
        Goal::Items(n) => texts::count_text(n, "item", "items"),
        Goal::FullCard => "full card".to_string(),
    }
}

/// The chat line for a stalemate that ends the game.
fn stalemate_text(cause: EndCause) -> Line {
    match cause {
        EndCause::Stalemate => texts::stalemate_no_team_can_win(),
        EndCause::Lockout => texts::stalemate_lockout(),
        EndCause::ImpossibleGoal => texts::stalemate_impossible_goal(),
    }
}

/// A snapshot read and checked in full, ready to be applied to a session.
struct SavedGame {
    game: GameSnapshot,
    seed: u64,
    rules: Options,
    added_teams: Vec<String>,
    cards: Vec<Card>,
    card_started: Vec<u64>,
    team_card: BTreeMap<String, usize>,
    completions: BTreeMap<String, Vec<(usize, bool)>>,
    finished: BTreeMap<String, u64>,
    progress: BTreeMap<String, TeamProgress>,
    persistent: BTreeMap<String, TeamScore>,
    owners: Vec<HashMap<usize, String>>,
    seen: BTreeMap<String, Vec<usize>>,
    reached: Vec<String>,
    votes: BTreeMap<u128, VoteChoice>,
    teams: Teams,
}

/// Reads a snapshot from [`Session::snapshot_json`]. A missing field, a wrong type, an unknown name, a bad uuid, a square
/// off the card, or a card that does not match the phase is an error.
fn parse_snapshot(text: &str) -> Result<SavedGame, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
    let object = root.as_object().ok_or("the snapshot is not a JSON object")?;
    let version = u64_field(object, "version")?;
    if version != GAME_STATE_VERSION {
        return Err(format!("unknown snapshot version {version}"));
    }
    let phase = str_field(object, "phase")?;
    let status = status_from_name(phase).ok_or_else(|| format!("unknown phase \"{phase}\""))?;
    let countdown_ticks = u64_field(object, "countdownTicks")?;
    let playing_ticks = u64_field(object, "elapsedTicks")?;
    let time_limit_secs = match field(object, "timeLimitSecs")? {
        Value::Null => None,
        value => {
            let secs = value.as_i64().and_then(|secs| i32::try_from(secs).ok()).filter(|secs| *secs >= 0);
            Some(secs.ok_or("\"timeLimitSecs\" is not a whole number of seconds")?)
        }
    };
    let seed = u64_field(object, "seed")?;

    let rules_value = field(object, "rules")?;
    let rules = store::options_from_json(&rules_value.to_string());
    let canonical = serde_json::from_str::<Value>(&store::options_to_json(&rules)).expect("round options are JSON");
    if canonical != *rules_value {
        return Err("\"rules\" are not valid round options".to_string());
    }

    let added_teams = str_list(array_field(object, "addedTeams")?)?
        .iter()
        .map(|team| team_key(team))
        .collect::<Result<Vec<_>, _>>()?;

    let card = match field(object, "card")? {
        Value::Null => None,
        value @ Value::Array(_) => Some(parse_card(value)?),
        _ => return Err("\"card\" is neither null nor a list".to_string()),
    };
    let needs_card = matches!(status, GameStatus::Playing | GameStatus::Postgame);
    if card.is_some() != needs_card {
        return Err(format!("phase {phase} does not go with the card the snapshot has"));
    }

    // A round with several cards, or a multi-card round, writes its cards and the state of each team on them. Any
    // other snapshot is a one-card round: its one card is `card`, and the state comes from the older keys.
    let grouped = object.contains_key("cards");
    let cards: Vec<Card> = if grouped {
        array_field(object, "cards")?.iter().map(parse_card).collect::<Result<Vec<_>, _>>()?
    } else {
        card.into_iter().collect()
    };
    if cards.is_empty() == needs_card {
        return Err(format!("phase {phase} does not go with the cards the snapshot has"));
    }
    let card_started: Vec<u64> = if grouped {
        array_field(object, "cardStarted")?
            .iter()
            .map(|tick| tick.as_u64().ok_or("a card's start is not a whole number"))
            .collect::<Result<Vec<u64>, &str>>()?
    } else {
        vec![0; cards.len()]
    };
    if card_started.len() != cards.len() {
        return Err("\"cardStarted\" does not match the cards".to_string());
    }
    let mut team_card = BTreeMap::new();
    if grouped {
        for (team, index) in object_field(object, "teamCard")? {
            let index = index
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&n| n < cards.len())
                .ok_or_else(|| format!("team {team} plays a card that is not in the round"))?;
            team_card.insert(team_key(team)?, index);
        }
    }

    let mut progress = BTreeMap::new();
    for (team, found) in object_field(object, "progress")? {
        let found = found
            .as_array()
            .ok_or("a team's progress is not a list")?
            .iter()
            .map(square_index)
            .collect::<Result<Vec<_>, _>>()?;
        progress.insert(team_key(team)?, TeamProgress { found });
    }

    // Written only by a ReplaceGoals round; a snapshot without it has no persistent scores.
    let mut persistent = BTreeMap::new();
    if let Some(value) = object.get("persistent") {
        let teams = value.as_object().ok_or("\"persistent\" is not an object")?;
        for (team, score) in teams {
            let score = score.as_object().ok_or("a team's persistent score is not an object")?;
            let score = TeamScore { items: score_count(score, "items")?, lines: score_count(score, "lines")?, cards: 0 };
            persistent.insert(team_key(team)?, score);
        }
    }

    // Written only while hidden-items mode is on; a snapshot without it has nothing seen.
    let mut seen = BTreeMap::new();
    if let Some(value) = object.get("seen") {
        let teams = value.as_object().ok_or("\"seen\" is not an object")?;
        for (team, squares) in teams {
            let squares = squares
                .as_array()
                .ok_or("a team's seen squares are not a list")?
                .iter()
                .map(square_index)
                .collect::<Result<Vec<_>, _>>()?;
            seen.insert(team_key(team)?, squares);
        }
    }

    let mut owners: Vec<HashMap<usize, String>> = vec![HashMap::new(); cards.len()];
    for (square, team) in object_field(object, "owners")? {
        let index = square.parse::<usize>().map_err(|_| format!("owner square \"{square}\" is not a number"))?;
        let (card_index, slot) = (index / CARD_SQUARES, index % CARD_SQUARES);
        if card_index >= cards.len() {
            return Err(format!("owner square {index} is off the card"));
        }
        owners[card_index].insert(slot, team_key(team_text(team)?)?);
    }

    let reached = str_list(array_field(object, "reached")?)?
        .iter()
        .map(|team| team_key(team))
        .collect::<Result<Vec<_>, _>>()?;

    // The completions and the finish times are written with the cards; a one-card round has them only as its reached
    // teams, each of which completed card 0 as a win and finished at once (the time is not kept, so it is 0).
    let (completions, finished): (BTreeMap<String, Vec<(usize, bool)>>, BTreeMap<String, u64>) = if grouped {
        let mut completions = BTreeMap::new();
        for (team, list) in object_field(object, "completions")? {
            let mut done = Vec::new();
            for entry in list.as_array().ok_or("a team's completions are not a list")? {
                let pair = entry.as_array().filter(|pair| pair.len() == 2).ok_or("a completion is not a pair")?;
                let index = pair[0]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|&n| n < cards.len())
                    .ok_or("a completion is of a card that is not in the round")?;
                let won = pair[1].as_bool().ok_or("a completion's win is not a flag")?;
                done.push((index, won));
            }
            completions.insert(team_key(team)?, done);
        }
        let mut finished = BTreeMap::new();
        for (team, at) in object_field(object, "finished")? {
            let at = at.as_u64().ok_or("a team's finish time is not a whole number")?;
            finished.insert(team_key(team)?, at);
        }
        (completions, finished)
    } else {
        let completions = reached.iter().map(|team| (team.clone(), vec![(0, true)])).collect();
        let finished = reached.iter().map(|team| (team.clone(), 0)).collect();
        (completions, finished)
    };

    let mut votes = BTreeMap::new();
    for (player, choice) in object_field(object, "votes")? {
        votes.insert(player_uuid(player)?, vote_from_name(team_text(choice)?)?);
    }

    let mut members: Vec<(u128, String)> = Vec::new();
    for entry in array_field(object, "members")? {
        let entry = entry.as_object().ok_or("a member is not an object")?;
        members.push((player_uuid(str_field(entry, "uuid")?)?, team_key(str_field(entry, "team")?)?));
    }
    let member_refs: Vec<(u128, &str)> = members.iter().map(|(player, team)| (*player, team.as_str())).collect();
    let mut spectators = Vec::new();
    for text in str_list(array_field(object, "spectators")?)? {
        spectators.push(player_uuid(&text)?);
    }
    let teams = Teams::from_parts(&member_refs, &spectators)?;

    Ok(SavedGame {
        game: GameSnapshot { status, countdown_ticks, playing_ticks, time_limit_secs },
        seed,
        rules,
        added_teams,
        cards,
        card_started,
        team_card,
        completions,
        finished,
        progress,
        persistent,
        owners,
        seen,
        reached,
        votes,
        teams,
    })
}

/// The squares a team's score is made of in a `ReplaceGoals` round: every found square for an items goal, the squares
/// of the full lines for a lines goal. Sorted, without repeats.
fn scored_squares(found: &[usize], goal: Goal) -> Vec<usize> {
    match goal {
        Goal::Lines(_) => scoring::squares_of_full_lines(found),
        Goal::Items(_) | Goal::FullCard => {
            let mut squares: Vec<usize> = found.iter().copied().filter(|&square| square < CARD_SQUARES).collect();
            squares.sort_unstable();
            squares.dedup();
            squares
        }
    }
}

/// A count of a persistent score in a snapshot: a whole number that fits an `i32`.
fn score_count(object: &Map<String, Value>, key: &str) -> Result<i32, String> {
    i32::try_from(u64_field(object, key)?).map_err(|_| format!("\"{key}\" is too large"))
}

/// The phase a snapshot text names, or `None` when the text is not a snapshot.
fn saved_phase(text: &str) -> Option<GameStatus> {
    let value: Value = serde_json::from_str(text).ok()?;
    status_from_name(value.get("phase")?.as_str()?)
}

fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, String> {
    object.get(key).ok_or_else(|| format!("\"{key}\" is missing"))
}

fn u64_field(object: &Map<String, Value>, key: &str) -> Result<u64, String> {
    field(object, key)?.as_u64().ok_or_else(|| format!("\"{key}\" is not a whole number"))
}

fn str_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    field(object, key)?.as_str().ok_or_else(|| format!("\"{key}\" is not text"))
}

fn array_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Vec<Value>, String> {
    field(object, key)?.as_array().ok_or_else(|| format!("\"{key}\" is not a list"))
}

fn object_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Map<String, Value>, String> {
    field(object, key)?.as_object().ok_or_else(|| format!("\"{key}\" is not an object"))
}

fn team_text(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| "a name in the snapshot is not text".to_string())
}

fn str_list(values: &[Value]) -> Result<Vec<&str>, String> {
    values.iter().map(team_text).collect()
}

/// A team key as the snapshot writes it (`bingo_red`). Any other spelling, or an unknown team, is an error.
fn team_key(text: &str) -> Result<String, String> {
    match teams::preset(text) {
        Some(preset) if preset.key == text => Ok(preset.key.to_string()),
        _ => Err(format!("unknown team \"{text}\"")),
    }
}

fn player_uuid(text: &str) -> Result<u128, String> {
    store::uuid_from(text).ok_or_else(|| format!("\"{text}\" is not a player uuid"))
}

fn square_index(value: &Value) -> Result<usize, String> {
    let index = value.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("a square is not a whole number")?;
    if index >= CARD_SQUARES {
        return Err(format!("square {index} is off the card"));
    }
    Ok(index)
}

fn parse_square(value: &Value) -> Result<Objective, String> {
    let object = value.as_object().ok_or("a square is not an object")?;
    let item = str_field(object, "item")?;
    if item.is_empty() {
        return Err("a square has no item".to_string());
    }
    let tier = tier_from_name(str_field(object, "tier")?)?;
    Ok(Objective { item: item.to_string(), tier })
}

fn tier_from_name(name: &str) -> Result<TierLabel, String> {
    [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D]
        .into_iter()
        .find(|tier| tier.name() == name)
        .ok_or_else(|| format!("unknown tier \"{name}\""))
}

fn status_name(status: GameStatus) -> &'static str {
    match status {
        GameStatus::Pregame => "Pregame",
        GameStatus::Starting => "Starting",
        GameStatus::Playing => "Playing",
        GameStatus::Postgame => "Postgame",
    }
}

fn status_from_name(name: &str) -> Option<GameStatus> {
    GameStatus::ALL.into_iter().find(|&status| status_name(status) == name)
}

fn vote_name(choice: VoteChoice) -> &'static str {
    match choice {
        VoteChoice::NextRound => "next_round",
        VoteChoice::KeepPlaying => "keep_playing",
        VoteChoice::ReturnToLobby => "return_to_lobby",
    }
}

fn vote_from_name(name: &str) -> Result<VoteChoice, String> {
    match name {
        "next_round" => Ok(VoteChoice::NextRound),
        "keep_playing" => Ok(VoteChoice::KeepPlaying),
        "return_to_lobby" => Ok(VoteChoice::ReturnToLobby),
        _ => Err(format!("unknown vote \"{name}\"")),
    }
}

fn send(host: &mut dyn ServerHost, player: u128, (path, payload): (&'static str, Vec<u8>)) {
    host.send(player, path, payload);
}

fn send_all(host: &mut dyn ServerHost, (path, payload): (&'static str, Vec<u8>)) {
    for player in host.players() {
        host.send(player.uuid, path, payload.clone());
    }
}

fn player_name(host: &dyn ServerHost, uuid: u128) -> String {
    host.players()
        .into_iter()
        .find(|player| player.uuid == uuid)
        .map_or_else(|| "A player".to_string(), |player| player.name)
}

/// The team's display name: the preset's name, or the key itself for a team that is not a preset.
fn team_name(team: &str) -> &str {
    teams::preset(team).map_or(team, |preset| preset.name)
}

/// The item's path without the `minecraft:` namespace, for chat.
fn short_name(item: &str) -> &str {
    item.strip_prefix("minecraft:").unwrap_or(item)
}

/// The milliseconds of a `Duration.toString()` text this module wrote (`PT1H2M3.5S`); 0 for anything else.
fn parse_java_duration(text: &str) -> i64 {
    let Some(mut rest) = text.strip_prefix("PT") else { return 0 };
    let mut millis = 0.0;
    while let Some(at) = rest.find(|c: char| c.is_ascii_alphabetic()) {
        let value: f64 = rest[..at].parse().unwrap_or(0.0);
        millis += value
            * match &rest[at..=at] {
                "H" => 3_600_000.0,
                "M" => 60_000.0,
                _ => 1_000.0,
            };
        rest = &rest[at + 1..];
    }
    millis.round() as i64
}

/// A translatable line as a styled piece, its arguments plain.
fn line_span(line: &Line) -> Span {
    Span::translate(line.key, line.template, line.args.iter().map(|arg| Span::text(arg.clone())).collect())
}

/// `Duration.toString()` of a span in milliseconds: `PT5S`, `PT1M`, `PT1H2M3.5S`; zero is `PT0S`.
fn java_duration(millis: i64) -> String {
    let total_secs = millis / 1000;
    let hours = total_secs / 3600;
    let minutes = total_secs % 3600 / 60;
    let secs = total_secs % 60;
    let fraction = format!("{:03}", millis % 1000);
    let fraction = fraction.trim_end_matches('0');

    let mut out = String::from("PT");
    if hours > 0 {
        out.push_str(&format!("{hours}H"));
    }
    if minutes > 0 {
        out.push_str(&format!("{minutes}M"));
    }
    if secs > 0 || !fraction.is_empty() || out == "PT" {
        out.push_str(&secs.to_string());
        if !fraction.is_empty() {
            out.push('.');
            out.push_str(fraction);
        }
        out.push('S');
    }
    out
}

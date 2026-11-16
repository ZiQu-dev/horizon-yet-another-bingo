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
use super::texts::{self, Line};
use super::multicard::{
    complete_card, finish_card_on_goal, reroll_card, round_ends, settle_pending, stalemate_action, stalemate_reason,
    team_score, CardCompletion, CardState, Completion, CompletionOutcome, EndCause, Rules, StalemateAction,
    StalemateBehavior, TeamRound, WinCondition,
};
use super::objectives::{self, Runtime};
use super::options::{EndWhen, Goal, Mode, Options};
use super::ready::{encode_update_v2, ReadyOptions, ReadyTimer};
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
    /// Settings players sent (`update_settings`); a player who sent none has the defaults.
    settings: HashMap<u128, PlayerSettings>,
    /// Finished games, oldest first, from `history.jsonl`.
    history: Vec<HistoryEntry>,
    /// The game was ended by `/bingo end`, not by a win or the time limit.
    forfeit: bool,
    /// Why the game ends when no team won, for the end's chat (`announce_end`). Set just before the end, taken by it.
    end_reason: Option<Line>,
    /// A game restored from `game-state.json` waits for its players: the empty-server reset does not run until one
    /// of them has joined.
    await_rejoin: bool,
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
}

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
            await_rejoin: false,
            captures: Captures::default(),
            objective_runtime: Vec::new(),
            held: HashMap::new(),
            reported: HashSet::new(),
            maps: MapItems::new(),
            flashes: Flashes::default(),
            books_given: HashSet::new(),
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
            GameStatus::Starting | GameStatus::Postgame => {}
        }
    }

    /// A player leaves. Their ready flag goes. Before play they also leave their team; during play the team stays, so
    /// a player who comes back is on it again.
    pub fn on_leave(&mut self, host: &mut dyn ServerHost, player: u128) {
        self.ready.remove(player);
        if self.game.status() == GameStatus::Pregame {
            self.teams.leave(player);
            self.send_ready_updates(host);
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
            self.announce_find(host, &format!("{finder} found {}", short_name(&item)));
        }
        self.check_goals(host);
        self.save_playing(host);
    }

    /// A chat line about a find, for the players who keep `itemMessages` on (the default). It is one broadcast when
    /// everyone does.
    fn announce_find(&self, host: &mut dyn ServerHost, line: &str) {
        let wants = |uuid: u128| self.settings.get(&uuid).map_or(true, |settings| settings.item_messages);
        let players = host.players();
        if players.iter().all(|player| wants(player.uuid)) {
            host.broadcast(line);
        } else {
            for player in players.iter().filter(|player| wants(player.uuid)) {
                host.tell(player.uuid, line);
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
            self.end_reason = Some(texts::stopped_reason());
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

    /// Ends the game with `winner` (`None` for nobody), through the game's own end.
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
    /// and only before play. A payload that does not decode is ignored.
    pub fn on_packet(&mut self, host: &mut dyn ServerHost, player: u128, path: &str, data: &[u8]) {
        if path == "update_settings" {
            if let Ok(settings) = decode(data, PlayerSettings::read) {
                self.settings.insert(player, settings);
                host.write_file(SETTINGS_FILE, &store::settings_to_json(&self.settings));
            }
            return;
        }
        let ready = match path {
            "set_ready" => decode(data, SetReadyPacket::read_v1).map(|packet| packet.is_ready),
            "set_ready_v2" => decode(data, SetReadyPacket::read_v2).map(|packet| packet.is_ready),
            _ => return,
        };
        let Ok(ready) = ready else { return };
        if self.game.status() == GameStatus::Postgame {
            if ready {
                self.vote(host, player, VoteChoice::NextRound);
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
        self.ensure_loaded(host);
        self.ticks += 1;
        if self.game.status() != GameStatus::Pregame {
            // `resetWhenEveryoneDisconnects`: a game nobody is in goes back to the lobby after a few seconds. A restored
            // game waits for its players first.
            if host.players().is_empty() && !self.await_rejoin {
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
            self.tick_lobby(host);
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
                let want = map_items::want_for(
                    status,
                    team,
                    self.teams.is_spectator(player.uuid),
                    // A player who sent the client's settings runs our HUD and needs no map.
                    self.settings.contains_key(&player.uuid),
                    won,
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

    /// Runs a command already parsed (by [`commands::parse`] or [`commands::chat_shortcut`]), as [`Session::command`]
    /// does after parsing. It does not load the saved files: a server tick does that before any call here.
    pub fn run(&mut self, host: &mut dyn ServerHost, player: Option<u128>, command: Command) -> String {
        match command {
            Command::Help => format!("Usage: bingo {}", commands::usage()),
            Command::Status => self.status_text(),
            Command::Start { ignore_warnings } => match self.start_game(host, ignore_warnings) {
                Ok(()) => "Game started.".to_string(),
                Err(error) => error,
            },
            Command::End => match self.game.end(None) {
                Ok(events) => {
                    self.forfeit = true;
                    self.end_reason = Some(texts::stopped_reason());
                    self.dispatch(host, events);
                    "Game ended.".to_string()
                }
                Err(_) => "The game is not Playing.".to_string(),
            },
            Command::Reset => {
                let events = self.game.reset();
                self.dispatch(host, events);
                "Game reset.".to_string()
            }
            Command::ShuffleTeams(count) => self.shuffle_teams(host, count),
            Command::Join(team) => self.join_team(host, player, &team),
            Command::Spectator => self.become_spectator(host, player),
            Command::Ready => self.toggle_ready(host, player),
            Command::ReadyCancel => self.cancel_ready(host),
            Command::TimeLimit(minutes) => self.set_time_limit(host, minutes),
            Command::Difficulty(counts) => self.set_rule(host, |rules| rules.tier_counts = counts, "difficulty"),
            Command::Reroll => self.reroll(),
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
        if self.give_book(host, &player, false) {
            "You got the bingo book.".to_string()
        } else {
            "The bingo book could not be given.".to_string()
        }
    }

    /// Gives one player the tutorial book, its pages written for their settings and operator rights. False when the host
    /// could not give it.
    fn give_book(&self, host: &mut dyn ServerHost, player: &Player, keep: bool) -> bool {
        let settings = self.settings.get(&player.uuid).cloned().unwrap_or_default();
        let operator = host.is_operator(player.uuid);
        let pages = infobook::pages(&infobook::PageView { settings: &settings, operator });
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
    fn start_game(&mut self, host: &mut dyn ServerHost, ignore_warnings: bool) -> Result<(), String> {
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
        host.command(&format!("gamerule pvp {}", self.rules.pvp));
        host.command(&format!("gamerule keep_inventory {}", self.rules.keep_inventory));
        self.add_teams_to_server(host, &players);
        let events = self.game.start().map_err(str::to_owned)?;
        self.dispatch(host, events);
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
            host.title(player.uuid, &texts::start_title(), &texts::start_subtitle());
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
                if self.rules.night_vision {
                    host.command(&format!("effect give {} night_vision infinite 0 true", player.name));
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
                    if status == GameStatus::Playing {
                        self.begin_round(host);
                    }
                }
                Event::Countdown { seconds } => send_all(host, wire::countdown(seconds)),
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

    /// The game ended: the end screen with the ranking of every team goes to each player, who wins if their team does.
    fn announce_end(&mut self, host: &mut dyn ServerHost, winner: Option<String>) {
        self.hide_boss_bars(host);
        self.record_game(host, winner.as_deref());
        Self::clear_saved_game(host);
        let ranking = self.ranking();
        let duration = self.game.elapsed_ms();
        for player in host.players() {
            let is_winner = winner.is_some() && self.teams.team_of(player.uuid) == winner.as_deref();
            let end = self.end_stats(Some(player.uuid));
            send(host, player.uuid, wire::game_over_v2_with(winner.as_deref(), is_winner, duration, ranking.clone(), &end));
        }
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
        let line = match winner.as_deref() {
            Some(team) => texts::team_won_game(team_name(team)),
            None => texts::game_ended(),
        };
        host.broadcast_line(&line);
        self.send_end_summary(host, winner.as_deref());
    }

    /// The end's chat for each player on a team (`GameOverController.broadcastGameOver`): a rule, the reason, the duration,
    /// the ranking of the teams when there is more than one (unless the first win ends a game with one win condition),
    /// and a rule. The reason is the team that completed its card, else why the game ended.
    fn send_end_summary(&mut self, host: &mut dyn ServerHost, winner: Option<&str>) {
        let stopped = self.end_reason.take();
        let reason = match winner {
            Some(team) => texts::completed_card(team_name(team)),
            None => stopped.unwrap_or_else(texts::out_of_time),
        };
        let duration = texts::duration_text(self.game.elapsed_ms());
        let ranking = self.ranking();
        let ranked = !(self.rules.end_when == EndWhen::FirstWin && self.rules.win_condition != WinCondition::Infinite)
            && ranking.len() > 1;
        for player in host.players() {
            if self.teams.team_of(player.uuid).is_none() {
                continue;
            }
            host.tell(player.uuid, &texts::separator());
            host.tell_line(player.uuid, &reason);
            host.tell_line(player.uuid, &texts::game_duration(&duration));
            if ranked {
                for row in &ranking {
                    let score = texts::score_text(row.score);
                    let line = texts::score_row(row.index as usize + 1, team_name(&row.key), &score, &duration);
                    host.tell(player.uuid, &line);
                }
            }
            host.tell(player.uuid, &texts::separator());
        }
    }

    /// Back to Pregame: the card, the progress and the ready flags go; the teams stay, but the server's team list drops
    /// the teams the start added.
    fn reset_round(&mut self, host: &mut dyn ServerHost) {
        self.hide_boss_bars(host);
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
            self.announce_find(host, &format!("{name} found {label}"));
        }
        self.send_tiles_after_find(host, team);
        self.send_score_to_all(host, team);
        self.check_goals(host);
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

    /// The end-screen ranking: every team that plays, by lines and then items, most first. Ties keep the order of the
    /// presets.
    fn ranking(&self) -> Vec<ScoreRanking> {
        let mut rows: Vec<(&'static str, TeamScore)> =
            self.teams.teams().into_iter().map(|key| (key, self.score(key))).collect();
        rows.sort_by(|a, b| (b.1.lines, b.1.items).cmp(&(a.1.lines, a.1.items)));
        let duration = java_duration(self.game.elapsed_ms());
        rows.into_iter()
            .enumerate()
            .map(|(index, (key, score))| ScoreRanking {
                index: index as i32,
                key: key.to_string(),
                name: Text::plain(team_name(key)),
                score,
                duration: Some(duration.clone()),
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

    /// The sidebar's lines (`sidebar.rs`), or `None` when the sidebar is off in the options.
    pub fn sidebar_rows(&self) -> Option<Vec<String>> {
        self.rules.sidebar.then(|| super::sidebar::rows(&self.sidebar_facts()))
    }

    fn sidebar_facts(&self) -> super::sidebar::Facts {
        use super::sidebar::{Clock, Phase, Standing};
        let phase = match self.game.status() {
            GameStatus::Pregame => Phase::Lobby,
            GameStatus::Starting | GameStatus::Playing => Phase::Live,
            GameStatus::Postgame => Phase::Over,
        };
        let clock = match phase {
            Phase::Lobby => match self.game.time_limit_secs() {
                Some(secs) => Clock::Limit(secs.max(0) as u32 / 60),
                None => Clock::NoLimit,
            },
            Phase::Over => Clock::Elapsed(self.game.elapsed_ms()),
            Phase::Live => match self.game.remaining_ms() {
                Some(ms) => Clock::Left(ms),
                None => Clock::Elapsed(self.game.elapsed_ms()),
            },
        };
        let teams = match phase {
            Phase::Lobby => self
                .teams
                .teams()
                .into_iter()
                .map(|key| (team_name(key).to_string(), self.teams.members(key).len()))
                .collect(),
            _ => Vec::new(),
        };
        let ranking = match phase {
            Phase::Lobby => Vec::new(),
            _ => self
                .ranking()
                .into_iter()
                .map(|row| Standing { name: team_name(&row.key).to_string(), lines: row.score.lines, items: row.score.items })
                .collect(),
        };
        super::sidebar::Facts {
            phase,
            mode: self.rules.mode,
            goal: self.rules.goal,
            clock,
            teams,
            ranking,
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
        let placed = online.iter().filter(|&&player| !self.teams.is_spectator(player)).count();
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
        format!("Shuffled {placed} players into {} teams", self.teams.teams().len())
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
        format!("You joined {}.", team_name(team))
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
        let ready = !self.ready.is_ready(player);
        self.ready.set_ready(player, ready);
        self.send_ready_updates(host);
        if ready {
            "You are ready.".to_string()
        } else {
            "You are not ready.".to_string()
        }
    }

    /// `/bingo ready cancel`: stops the ready timer and keeps it from starting again until someone is ready.
    fn cancel_ready(&mut self, host: &mut dyn ServerHost) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "The ready timer can be cancelled only before the game starts.".to_string();
        }
        self.ready.cancel();
        self.send_ready_updates(host);
        "Ready timer cancelled.".to_string()
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
        format!("You joined {}.", team_name(&key))
    }

    /// `/bingo reroll`: a new seed for the card the game will start with, drawn from the shuffle generator. Only before the
    /// game starts (there is no card to redraw before that: the native lobby shows none). The seed is not saved, as the
    /// round's seed is not saved outside a game in progress.
    fn reroll(&mut self) -> String {
        if self.game.status() != GameStatus::Pregame {
            return "The card can be rerolled only before the game starts.".to_string();
        }
        self.seed = self.rng.next_u64();
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
        let mut text = format!(
            "Goal: {goal}. Mode: {mode}. End: {end}. PvP: {}. Keep inventory: {}. Elytra: {}. Night vision: {}. Spawn distance: {} chunks.",
            on(self.rules.pvp),
            on(self.rules.keep_inventory),
            on(self.rules.elytra),
            on(self.rules.night_vision),
            self.rules.spawn_distance
        );
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

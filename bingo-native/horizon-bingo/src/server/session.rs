//! Session: one bingo game on the server with its lobby and its teams. Before play, the teams and the ready timer
//! decide when the game starts (`start`, `ready`, `join`, `shuffleteams`); during play every team plays the same card,
//! and each team has its own progress, its own win and its own place in the ranking.
//!
//! Mirrors the original's `/bingo` commands, team rules, ready timer and scoring (`orig-teams-commands.md`). Pure logic:
//! everything goes out through [`ServerHost`], and the events of [`Game`] are turned into packets here.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::card_gen::{generate_card, load_pool, Pool, Rng, DEFAULT_DISTRIBUTION};
use super::commands::{self, Command, VoteChoice};
use super::game::{Event, Game, GameOptions, GameSnapshot};
use super::hidden_consume::{self, CardMode, HeldStack, Sight, TileView};
use super::host::ServerHost;
use super::options::{EndWhen, Goal, Mode, Options};
use super::ready::{encode_update_v2, ReadyOptions, ReadyTimer};
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
    /// The one card every team plays. Made when play begins.
    card: Option<Card>,
    /// Progress of each team that plays, by `BingoTeamKey.id`.
    progress: BTreeMap<String, TeamProgress>,
    /// Teams added to the server's team list by the start, removed again on reset.
    added_teams: Vec<String>,
    /// Boss bars shown, as (player, bar id), so they can be hidden at the end.
    boss_bars: BTreeSet<(u128, String)>,
    /// Server ticks so far, for the periodic packets.
    ticks: u64,
    /// Rules of the round.
    rules: Options,
    /// Lockout: which team owns each square.
    owners: HashMap<usize, String>,
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
    /// A game restored from `game-state.json` waits for its players: the empty-server reset does not run until one
    /// of them has joined.
    await_rejoin: bool,
}

impl Session {
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
            card: None,
            progress: BTreeMap::new(),
            added_teams: Vec::new(),
            boss_bars: BTreeSet::new(),
            ticks: 0,
            rules: options.rules.clone(),
            owners: HashMap::new(),
            seen: BTreeMap::new(),
            reached: Vec::new(),
            votes: BTreeMap::new(),
            empty_ticks: 0,
            loaded: false,
            settings: HashMap::new(),
            history: Vec::new(),
            forfeit: false,
            await_rejoin: false,
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
        if let Some(text) = host.read_file(GAME_STATE_FILE) {
            if saved_phase(&text) == Some(GameStatus::Playing) {
                let _ = self.restore_from_json(&text);
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
        let card = match &self.card {
            Some(card) => Value::Array(
                card.squares.iter().map(|square| json!({ "item": square.item, "tier": square.tier.name() })).collect(),
            ),
            None => Value::Null,
        };
        let progress: BTreeMap<&str, &Vec<usize>> =
            self.progress.iter().map(|(team, progress)| (team.as_str(), &progress.found)).collect();
        let seen: BTreeMap<&str, &Vec<usize>> = self.seen.iter().map(|(team, squares)| (team.as_str(), squares)).collect();
        let owners: BTreeMap<String, &String> = self.owners.iter().map(|(square, team)| (square.to_string(), team)).collect();
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
        if self.rules.hidden_consume.hidden_items {
            value["seen"] = json!(seen);
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
        self.card = saved.card;
        self.progress = saved.progress;
        self.owners = saved.owners;
        self.seen = saved.seen;
        self.reached = saved.reached;
        self.votes = saved.votes;
        self.teams = saved.teams;
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
        if self.game.status() != GameStatus::Playing {
            return;
        }
        let Some(team) = self.teams.team_of(player) else { return };
        // Only a player on a team scores; the save after the finds covers the squares and the goals reached.
        let team = team.to_string();
        let newly_seen = self.record_seen(&team, held);
        let (items, lost) = {
            let Some(card) = self.card.as_ref() else { return };
            let progress = self.progress.entry(team.clone()).or_default();
            let (mut gained, lost) = scoring::score_inventory_mode(card, progress, held, self.rules.mode);
            if self.rules.mode == Mode::Lockout {
                let claimed = scoring::claim_lockout(&mut self.owners, &team, &gained);
                for index in gained.iter().filter(|index| !claimed.contains(index)) {
                    progress.found.retain(|found| found != index);
                }
                gained = claimed;
            }
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

    /// Notes the teams that have reached the goal (in order), and ends the game when the end condition holds. The first
    /// team to reach the goal is the winner.
    fn check_goals(&mut self, host: &mut dyn ServerHost) {
        let keys: Vec<String> = self.teams.teams().into_iter().map(str::to_string).collect();
        let mut newly = Vec::new();
        for key in &keys {
            let done = self.progress.get(key).is_some_and(|progress| scoring::goal_reached(progress, self.rules.goal));
            if done && !self.reached.contains(key) {
                self.reached.push(key.clone());
                newly.push(key.clone());
            } else if !done {
                self.reached.retain(|reached| reached != key);
            }
        }
        let ends = scoring::game_should_end(self.rules.end_when, self.reached.len(), keys.len());
        if !ends {
            for key in newly {
                host.broadcast(&format!("{} reached the goal!", team_name(&key)));
            }
        }
        if ends {
            let winner = self.reached.first().cloned();
            if let Ok(events) = self.game.end(winner) {
                self.dispatch(host, events);
            }
        }
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
            return;
        }
        let events = self.game.tick();
        self.dispatch(host, events);
        if self.game.status() == GameStatus::Playing && self.ticks % 10 == 0 {
            self.send_boss_bars(host);
        }
    }

    /// Runs a command typed by `player` (or the console, `None`) and returns the text to answer with. `line` is what
    /// follows the word `bingo`. Who may run a command is for the caller to check (see [`commands::requires_op`]).
    pub fn command(&mut self, host: &mut dyn ServerHost, player: Option<u128>, line: &str) -> String {
        self.ensure_loaded(host);
        let command = match commands::parse(line) {
            Ok(command) => command,
            Err(error) => return error,
        };
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
            Command::Options => self.options_text(),
            Command::Vote(choice) => match player {
                Some(player) => self.vote(host, player, choice),
                None => "Only a player can vote.".to_string(),
            },
            Command::Resume => self.resume(host),
            Command::Stats => self.stats(host, player),
        }
    }

    /// The lobby: the ready timer, which starts the game if the start rules hold when it ends. Everyone online gets the
    /// ready screen every 10 ticks.
    fn tick_lobby(&mut self, host: &mut dyn ServerHost) {
        let players = host.players();
        let online: Vec<u128> = players.iter().map(|player| player.uuid).collect();
        let eligible = self.plan_start(&players, false).is_ok();
        if self.ready.tick(&online, eligible) {
            // The same checks ran just now with the same result, so the start cannot fail here.
            let _ = self.start_game(host, false);
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
        self.teams = self.plan_start(&players, ignore_warnings)?;
        self.ready.clear();
        host.command(&format!("gamerule pvp {}", self.rules.pvp));
        host.command(&format!("gamerule keep_inventory {}", self.rules.keep_inventory));
        self.add_teams_to_server(host, &players);
        let events = self.game.start().map_err(str::to_owned)?;
        self.dispatch(host, events);
        Ok(())
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
        self.card = Some(generate_card(self.seed, &self.pool, DEFAULT_DISTRIBUTION));
        // The game starts: the first save of it. Scoring, joins and the time limit save again from here on.
        self.save_playing(host);
        self.progress = self.teams.teams().into_iter().map(|key| (key.to_string(), TeamProgress::default())).collect();
        self.owners.clear();
        self.seen.clear();
        self.reached.clear();
        self.votes.clear();
        self.forfeit = false;
        let players = host.players();
        for player in &players {
            if self.teams.team_of(player.uuid).is_some() {
                host.command(&format!("gamemode survival {}", player.name));
                if self.rules.elytra {
                    host.command(&format!("give {} elytra", player.name));
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
            let packet = self.ready.update_packet(player.uuid, &online, "pregame", GameStatus::Pregame);
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
                    host.broadcast("The game continues.");
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
            send(host, player.uuid, wire::game_over_v2(winner.as_deref(), is_winner, duration, ranking.clone()));
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
            Some(team) => format!("{} won the game!", team_name(team)),
            None => "The game has ended.".to_string(),
        };
        host.broadcast(&line);
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
        self.card = None;
        self.owners.clear();
        self.seen.clear();
        self.reached.clear();
        self.votes.clear();
        self.progress.clear();
        self.ready.clear();
        self.seed = self.seed.wrapping_add(1);
        self.send_ready_updates(host);
    }

    fn game_status(&self) -> (&'static str, Vec<u8>) {
        wire::game_status(Some(self.game.elapsed_ms()), self.game.remaining_ms())
    }

    /// Score of a team. A team with no progress yet has none.
    fn score(&self, team: &str) -> TeamScore {
        self.progress.get(team).map_or_else(TeamScore::default, |progress| scoring::team_score_goal(progress, self.rules.goal))
    }

    /// Every team's found squares, in the order of the presets, for [`wire::card_tiles_for`].
    fn found_by(&self) -> Vec<(&'static str, &[usize])> {
        self.teams
            .teams()
            .into_iter()
            .filter_map(|key| self.progress.get(key).map(|progress| (key, progress.found.as_slice())))
            .collect()
    }

    /// The tiles of a viewer's team. `None` before play, when there is no card.
    fn tiles_for(&self, host: &dyn ServerHost, viewer: &str, should_notify: bool) -> Option<(&'static str, Vec<u8>)> {
        let card = self.card.as_ref()?;
        let found = self.found_by();
        let item_id = |item: &str| host.item_numeric_id(item);
        let views = self.views_for(Some(viewer));
        Some(wire::card_tiles_seen(viewer, card, &found, &views, &item_id, should_notify))
    }

    /// Every square as `viewer` sees it (hidden-items mode, see `hidden_consume::tile_views`). `None` is a player on no
    /// team, who sees every square hidden. Postgame shows all.
    fn views_for(&self, viewer: Option<&str>) -> Vec<TileView> {
        let Some(card) = self.card.as_ref() else { return Vec::new() };
        let none: &[usize] = &[];
        let sight = viewer.map(|team| Sight {
            seen: self.seen.get(team).map_or(none, |squares| squares.as_slice()),
            found: self.progress.get(team).map_or(none, |progress| progress.found.as_slice()),
            finished: self.reached.iter().any(|key| key == team),
        });
        let postgame = self.game.status() == GameStatus::Postgame;
        hidden_consume::tile_views(card, self.rules.hidden_consume, false, sight, postgame)
    }

    /// Adds the squares whose items `held` holds to the team's seen squares. Returns whether any square was new.
    fn record_seen(&mut self, team: &str, held: &HashMap<String, u32>) -> bool {
        let Some(card) = self.card.as_ref() else { return false };
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

    /// `/bingo stats`: the caller's games, wins, best winning time and items, from the history.
    fn stats(&self, host: &dyn ServerHost, player: Option<u128>) -> String {
        let Some(player) = player else {
            return "Only a player has statistics.".to_string();
        };
        store::stats_text(&player_name(host, player), &store::player_stats(&self.history, player))
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
        let players = players
            .iter()
            .filter_map(|player| {
                let team = self.teams.team_of(player.uuid)?;
                let items = self.progress.get(team).map_or(0, |progress| progress.found.len() as i32);
                Some(HistoryPlayer { uuid: player.uuid, name: player.name.clone(), team: team.to_string(), items })
            })
            .collect();
        let entry = HistoryEntry {
            id: (u128::from(self.seed) << 64) | ended_at_ms.max(0) as u128,
            ended_at_ms,
            duration_ms: self.game.elapsed_ms(),
            winner: winner.map(str::to_string),
            forfeit: self.forfeit,
            teams,
            players,
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
        format!(
            "Goal: {goal}. Mode: {mode}. End: {end}. PvP: {}. Keep inventory: {}. Elytra: {}. Night vision: {}. Spawn distance: {} chunks.",
            on(self.rules.pvp),
            on(self.rules.keep_inventory),
            on(self.rules.elytra),
            on(self.rules.night_vision),
            self.rules.spawn_distance
        )
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
        let ended_by_rule = scoring::game_should_end(self.rules.end_when, self.reached.len(), self.teams.teams().len());
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

/// A snapshot read and checked in full, ready to be applied to a session.
struct SavedGame {
    game: GameSnapshot,
    seed: u64,
    rules: Options,
    added_teams: Vec<String>,
    card: Option<Card>,
    progress: BTreeMap<String, TeamProgress>,
    owners: HashMap<usize, String>,
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
        Value::Array(squares) => {
            let squares = squares.iter().map(parse_square).collect::<Result<Vec<_>, _>>()?;
            if squares.len() != CARD_SQUARES {
                return Err(format!("a card has {CARD_SQUARES} squares, not {}", squares.len()));
            }
            Some(Card { squares })
        }
        _ => return Err("\"card\" is neither null nor a list".to_string()),
    };
    let needs_card = matches!(status, GameStatus::Playing | GameStatus::Postgame);
    if card.is_some() != needs_card {
        return Err(format!("phase {phase} does not go with the card the snapshot has"));
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

    let mut owners = HashMap::new();
    for (square, team) in object_field(object, "owners")? {
        let index = square.parse::<usize>().map_err(|_| format!("owner square \"{square}\" is not a number"))?;
        if index >= CARD_SQUARES {
            return Err(format!("owner square {index} is off the card"));
        }
        owners.insert(index, team_key(team_text(team)?)?);
    }

    let reached = str_list(array_field(object, "reached")?)?
        .iter()
        .map(|team| team_key(team))
        .collect::<Result<Vec<_>, _>>()?;

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
        card,
        progress,
        owners,
        seen,
        reached,
        votes,
        teams,
    })
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

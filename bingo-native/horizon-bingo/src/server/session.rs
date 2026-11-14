//! Session: one bingo game on the server with its lobby and its teams. Before play, the teams and the ready timer
//! decide when the game starts (`start`, `ready`, `join`, `shuffleteams`); during play every team plays the same card,
//! and each team has its own progress, its own win and its own place in the ranking.
//!
//! Mirrors the original's `/bingo` commands, team rules, ready timer and scoring (`orig-teams-commands.md`). Pure logic:
//! everything goes out through [`ServerHost`], and the events of [`Game`] are turned into packets here.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::card_gen::{generate_card, load_pool, Pool, Rng, DEFAULT_DISTRIBUTION};
use super::commands::{self, Command};
use super::game::{Event, Game, GameOptions};
use super::host::ServerHost;
use super::ready::{encode_update_v2, ReadyOptions, ReadyTimer};
use super::scoring;
use super::teams::{self, Teams};
use super::types::{Card, Player, TeamProgress};
use super::wire;
use crate::buf::decode;
use crate::game::GameStatus;
use crate::game_over::ScoreRanking;
use crate::ready::SetReadyPacket;
use crate::score_update::TeamScore;
use crate::text::Text;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionOptions {
    /// Seed of the first card. Each reset moves on to the next seed. The shuffles of `shuffleteams` start from it too.
    pub seed: u64,
    pub game: GameOptions,
    pub ready: ReadyOptions,
    /// `(distance, spread)` of `spreadplayers` at the start, used when there are two teams or more. `None` leaves
    /// the players where they stand.
    pub spread: Option<(i32, i32)>,
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
            options,
        }
    }

    /// A player joins. They get the game status. Before play they get the ready screen; during play they get the whole
    /// current game as it is now: the teams, their tiles, the open card and each team's score.
    pub fn on_join(&mut self, host: &mut dyn ServerHost, player: &Player) {
        send(host, player.uuid, self.game_status());
        match self.game.status() {
            GameStatus::Pregame => self.send_ready_updates(host),
            GameStatus::Playing => self.send_round(host, player.uuid),
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
        let items: Vec<String> = {
            let Some(card) = self.card.as_ref() else { return };
            let progress = self.progress.entry(team.to_string()).or_default();
            scoring::score_inventory(card, progress, held)
                .into_iter()
                .map(|index| card.squares[index].item.clone())
                .collect()
        };
        if items.is_empty() {
            return;
        }

        let finder = player_name(host, player);
        self.send_tiles_after_find(host, team);
        self.send_score_to_all(host, team);
        for item in items {
            host.broadcast(&format!("{finder} found {}", short_name(&item)));
        }
        if let Some(winner) = self.winner() {
            if let Ok(events) = self.game.end(Some(winner.to_string())) {
                self.dispatch(host, events);
            }
        }
    }

    /// A packet from a player on the channel `yet-another-minecraft-bingo:<path>`. Only the ready packets are handled,
    /// and only before play. A payload that does not decode is ignored.
    pub fn on_packet(&mut self, host: &mut dyn ServerHost, player: u128, path: &str, data: &[u8]) {
        let ready = match path {
            "set_ready" => decode(data, SetReadyPacket::read_v1).map(|packet| packet.is_ready),
            "set_ready_v2" => decode(data, SetReadyPacket::read_v2).map(|packet| packet.is_ready),
            _ => return,
        };
        let Ok(ready) = ready else { return };
        if self.game.status() != GameStatus::Pregame {
            return;
        }
        self.ready.set_ready(player, ready);
        self.send_ready_updates(host);
    }

    /// One server tick. Before play, the ready timer runs and starts the game when it ends. Otherwise the game moves
    /// on, and its events go to everyone.
    pub fn tick(&mut self, host: &mut dyn ServerHost) {
        self.ticks += 1;
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
            Command::TimeLimit(minutes) => self.set_time_limit(minutes),
            Command::Card => self.open_card(host, player),
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
        if let Some((distance, spread)) = self.options.spread {
            if self.teams.teams().len() >= 2 {
                let names: Vec<&str> = players
                    .iter()
                    .filter(|player| !self.teams.is_spectator(player.uuid))
                    .map(|player| player.name.as_str())
                    .collect();
                host.command(&format!("spreadplayers 0 0 {distance} {spread} true {}", names.join(" ")));
            }
        }
    }

    /// Makes the card and shows the game to everyone online.
    fn begin_round(&mut self, host: &mut dyn ServerHost) {
        self.card = Some(generate_card(self.seed, &self.pool, DEFAULT_DISTRIBUTION));
        self.progress = self.teams.teams().into_iter().map(|key| (key.to_string(), TeamProgress::default())).collect();
        for player in host.players() {
            self.send_round(host, player.uuid);
        }
    }

    /// What a player gets to see the game: every team with online members, the tiles of their own team (none for a
    /// spectator), the open card and the score of every team.
    fn send_round(&self, host: &mut dyn ServerHost, player: u128) {
        let players = host.players();
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
                (!members.is_empty()).then_some((key, preset.color, preset.name, members))
            })
            .collect();
        send(host, player, wire::display_cards_all(&view));

        let own = self.teams.team_of(player);
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
            }
        }
    }

    /// The game ended: the end screen with the ranking of every team goes to each player, who wins if their team does.
    fn announce_end(&mut self, host: &mut dyn ServerHost, winner: Option<String>) {
        self.hide_boss_bars(host);
        let ranking = self.ranking();
        let duration = self.game.elapsed_ms();
        for player in host.players() {
            let is_winner = winner.is_some() && self.teams.team_of(player.uuid) == winner.as_deref();
            send(host, player.uuid, wire::game_over_v2(winner.as_deref(), is_winner, duration, ranking.clone()));
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
        for team in std::mem::take(&mut self.added_teams) {
            host.command(&format!("team remove {team}"));
        }
        send_all(host, wire::game_reset());
        send_all(host, wire::card_reset());
        self.card = None;
        self.progress.clear();
        self.ready.clear();
        self.seed = self.seed.wrapping_add(1);
        self.send_ready_updates(host);
    }

    fn game_status(&self) -> (&'static str, Vec<u8>) {
        wire::game_status(Some(self.game.elapsed_ms()), self.game.remaining_ms())
    }

    /// The first team in the order of the presets whose progress has won the card.
    fn winner(&self) -> Option<&'static str> {
        self.teams.teams().into_iter().find(|key| self.progress.get(*key).is_some_and(scoring::has_won))
    }

    /// Score of a team. A team with no progress yet has none.
    fn score(&self, team: &str) -> TeamScore {
        self.progress.get(team).map_or_else(TeamScore::default, scoring::team_score)
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
        Some(wire::card_tiles_for(viewer, card, &found, &item_id, should_notify))
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
                if online.contains(&member) {
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
        if self.game.status() != GameStatus::Pregame {
            return "Teams can be changed only before the game starts.".to_string();
        }
        let Some(player) = player else {
            return "Only a player can join a team.".to_string();
        };
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
    fn set_time_limit(&mut self, minutes: Option<u32>) -> String {
        if !matches!(self.game.status(), GameStatus::Pregame | GameStatus::Playing) {
            return "The time limit can be changed only in Pregame or Playing.".to_string();
        }
        match minutes {
            Some(minutes) => {
                let seconds = i32::try_from(minutes.saturating_mul(60)).unwrap_or(i32::MAX);
                self.game.set_time_limit(Some(seconds));
                format!("Time limit set to {minutes} minutes")
            }
            None => {
                self.game.set_time_limit(None);
                "Time limit turned off".to_string()
            }
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

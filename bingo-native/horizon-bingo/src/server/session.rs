//! One bingo session on the server: the game state, the one team with its card, and the ready screen. Everyone online
//! belongs to the team. The card is made when play begins, so until then the only packet is the game status.
//!
//! Pure logic: everything goes out through [`ServerHost`], and the events of [`Game`] are turned into packets here.
use std::collections::{HashMap, HashSet};

use super::card_gen::{generate_card, load_pool, Pool, DEFAULT_DISTRIBUTION};
use super::game::{Event, Game, GameOptions};
use super::host::ServerHost;
use super::scoring;
use super::types::{Card, Player, TeamProgress, CARD_SQUARES};
use super::wire;
use crate::buf::decode;
use crate::game::GameStatus;
use crate::game_over::ScoreRanking;
use crate::ready::SetReadyPacket;
use crate::score_update::TeamScore;
use crate::text::Text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// Seed of the first card. Each reset moves on to the next seed.
    pub seed: u64,
    pub game: GameOptions,
    /// `BingoTeamKey.id` of the team, for example `bingo_red`.
    pub team: String,
    /// A `Formatting` name, for example `RED`.
    pub team_color: String,
    /// The team's display name, for example `Red`.
    pub team_name: String,
}

pub struct Session {
    options: SessionOptions,
    game: Game,
    pool: Pool,
    /// Seed for the next card made.
    seed: u64,
    card: Option<Card>,
    progress: TeamProgress,
    ready: HashSet<u128>,
}

impl Session {
    pub fn new(options: SessionOptions) -> Self {
        let game = Game::new(options.game.clone());
        let seed = options.seed;
        Self {
            options,
            game,
            pool: load_pool(),
            seed,
            card: None,
            progress: TeamProgress::default(),
            ready: HashSet::new(),
        }
    }

    /// A player joins. Before play they get the game status. During play they get the whole current card.
    pub fn on_join(&mut self, host: &mut dyn ServerHost, player: &Player) {
        send(host, player.uuid, self.game_status());
        if self.game.status() == GameStatus::Playing {
            self.send_card_set(host, player.uuid);
        }
    }

    /// A player leaves. They are no longer ready, so rejoining does not count as ready.
    pub fn on_leave(&mut self, _host: &mut dyn ServerHost, player: u128) {
        self.ready.remove(&player);
    }

    /// A player's inventory, as a map from item id to count. Only during play: squares found now are scored and
    /// announced, and a completed line ends the game.
    pub fn on_inventory(&mut self, host: &mut dyn ServerHost, player: u128, held: &HashMap<String, u32>) {
        if self.game.status() != GameStatus::Playing {
            return;
        }
        let Some(card) = &self.card else { return };
        let newly_found = scoring::score_inventory(card, &mut self.progress, held);
        if newly_found.is_empty() {
            return;
        }
        let items: Vec<String> = newly_found.iter().map(|&index| card.squares[index].item.clone()).collect();

        let finder = player_name(host, player);
        if let Some(tiles) = self.card_tiles(&*host, true) {
            send_all(host, tiles);
        }
        send_all(host, self.score_update());
        for item in items {
            host.broadcast(&format!("{finder} found {}", short_name(&item)));
        }
        if scoring::has_won(&self.progress) {
            if let Ok(events) = self.game.end(Some(self.options.team.clone())) {
                self.dispatch(host, events);
            }
        }
    }

    /// A packet from a player on the channel `yet-another-minecraft-bingo:<path>`. Only the ready packets are handled.
    /// A payload that does not decode is ignored. When everyone online is ready, the game starts.
    pub fn on_packet(&mut self, host: &mut dyn ServerHost, player: u128, path: &str, data: &[u8]) {
        let ready = match path {
            "set_ready" => decode(data, SetReadyPacket::read_v1).map(|packet| packet.is_ready),
            "set_ready_v2" => decode(data, SetReadyPacket::read_v2).map(|packet| packet.is_ready),
            _ => return,
        };
        let Ok(ready) = ready else { return };
        if ready {
            self.ready.insert(player);
        } else {
            self.ready.remove(&player);
        }
        self.try_start(host);
    }

    /// One server tick: the game moves on and its events go to everyone.
    pub fn tick(&mut self, host: &mut dyn ServerHost) {
        let events = self.game.tick();
        self.dispatch(host, events);
    }

    /// Runs a command typed by `_player` (or the console, `None`) and returns the text to answer with.
    pub fn command(&mut self, host: &mut dyn ServerHost, _player: Option<u128>, line: &str) -> String {
        match line.trim() {
            "start" => match self.game.start() {
                Ok(events) => {
                    self.dispatch(host, events);
                    "Game started.".to_string()
                }
                Err(error) => error.to_string(),
            },
            "end" => match self.game.end(None) {
                Ok(events) => {
                    self.dispatch(host, events);
                    "Game ended.".to_string()
                }
                Err(error) => error.to_string(),
            },
            "reset" => {
                let events = self.game.reset();
                self.dispatch(host, events);
                "Game reset.".to_string()
            }
            "status" => self.status_text(),
            _ => "Unknown command. Use: start, end, reset, status.".to_string(),
        }
    }

    /// Starts the game once everyone online is ready. Nobody online is never enough.
    fn try_start(&mut self, host: &mut dyn ServerHost) {
        if self.game.status() != GameStatus::Pregame {
            return;
        }
        let players = host.players();
        if players.is_empty() || !players.iter().all(|player| self.ready.contains(&player.uuid)) {
            return;
        }
        if let Ok(events) = self.game.start() {
            self.dispatch(host, events);
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
                Event::Reset => {
                    send_all(host, wire::game_reset());
                    send_all(host, wire::card_reset());
                    self.card = None;
                    self.progress = TeamProgress::default();
                    self.seed = self.seed.wrapping_add(1);
                }
            }
        }
    }

    /// Makes the card and shows it to everyone online.
    fn begin_round(&mut self, host: &mut dyn ServerHost) {
        self.card = Some(generate_card(self.seed, &self.pool, DEFAULT_DISTRIBUTION));
        self.progress = TeamProgress::default();
        for player in host.players() {
            self.send_card_set(host, player.uuid);
        }
    }

    /// What a player gets when they see the current card: the team, the tiles, the open card and the score.
    fn send_card_set(&self, host: &mut dyn ServerHost, player: u128) {
        let players = host.players();
        let display = wire::display_cards(
            &self.options.team,
            &self.options.team_color,
            &self.options.team_name,
            &players,
        );
        send(host, player, display);
        if let Some(tiles) = self.card_tiles(&*host, false) {
            send(host, player, tiles);
        }
        send(host, player, wire::open_card(Some(&self.options.team)));
        send(host, player, self.score_update());
    }

    fn announce_end(&mut self, host: &mut dyn ServerHost, winner: Option<String>) {
        // Everyone online is on the team, so the team's win is the win of every viewer.
        let won = winner.as_deref() == Some(self.options.team.as_str());
        let ranking = ScoreRanking {
            index: 0,
            key: self.options.team.clone(),
            name: Text::plain(&self.options.team_name),
            score: self.team_score(),
            duration: Some(java_duration(self.game.elapsed_ms())),
        };
        send_all(host, wire::game_over_v2(winner.as_deref(), won, vec![ranking]));
        let line = if won {
            format!("{} won the game!", self.options.team_name)
        } else {
            "The game has ended.".to_string()
        };
        host.broadcast(&line);
    }

    fn game_status(&self) -> (&'static str, Vec<u8>) {
        wire::game_status(Some(self.game.elapsed_ms()), self.game.remaining_ms())
    }

    fn card_tiles(&self, host: &dyn ServerHost, should_notify: bool) -> Option<(&'static str, Vec<u8>)> {
        let card = self.card.as_ref()?;
        let item_id = |item: &str| host.item_numeric_id(item);
        Some(wire::card_tiles(&self.options.team, card, &self.progress.found, &item_id, should_notify))
    }

    fn score_update(&self) -> (&'static str, Vec<u8>) {
        wire::score_update(true, &self.options.team, self.team_score())
    }

    fn team_score(&self) -> TeamScore {
        scoring::team_score(&self.progress)
    }

    fn status_text(&self) -> String {
        let status = format!("{:?}", self.game.status());
        let found = self.team_score().items;
        let mut text = format!("Status: {status}. Squares found: {found} of {CARD_SQUARES}.");
        if let Some(remaining_ms) = self.game.remaining_ms() {
            text.push_str(&format!(" Time left: {} s.", remaining_ms / 1000));
        }
        text
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

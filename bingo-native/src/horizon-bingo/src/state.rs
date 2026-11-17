//! What the client remembers between packets, and what each packet does to it. It follows the mod's own client
//! (`client/common/hud/BingoHudController.kt`, `state/BingoHudState.kt`, `sound/ScorePacketReceiver.kt`,
//! `settings/ClientSettingsController.kt`), line by line where the behaviour is visible on screen.
//!
//! No drawing and no Horizon here: [`HudState::on_packet`] takes a channel and its payload, updates the state and
//! queues [`ClientEvent`]s (sounds, screens to open) for the host to act on. Time comes from the host through
//! [`HudState::set_now`], never from a clock, so every rule is testable.
//!
//! Differences from the Kotlin client, on purpose: no framebuffers or images are created (the host draws), and
//! `card_image` payloads are kept as bytes (PNG decoding is the host's job).

use std::collections::{BTreeMap, HashMap};
use std::ops::Index;

use crate::buf::{DecodeError, Reader};
use crate::card_tile::CardTile;
use crate::card_tile_action::CardTileAction;
use crate::card_tile_image::CardTileImage;
use crate::display_cards::{CardDisplay, CardDisplayPacket};
use crate::game_message::GameMessagePacket;
use crate::game_over::GameOverPacket;
use crate::game_status::GameStatusPacket;
use crate::ready_update::ReadyUpdatePacket;
use crate::score_update::{ScoredItemPacket, TeamScore};
use crate::shuffle_draw::{shuffled, ShuffleRng};
use crate::stats_index::StatsIndexPacket;
use crate::tooltip_v2::TooltipPacket;
use crate::update_settings::PlayerSettings;

/// `ClientCardBufferRenderer.SHUFFLE_DURATION`: a card is not shuffled again within 280 ms.
pub const SHUFFLE_DURATION_MS: u64 = crate::shuffle_draw::SHUFFLE_DURATION_MS;

/// A team's id (`bingo_red`), or `None` for the card shown when the player has no team.
pub type TeamKey = Option<String>;

/// A card has 25 tiles (`BingoHudController.onCardTiles` builds `MutableList(25)`).
pub const TILES_PER_CARD: usize = 25;

// Limits on what the client keeps from the server, which is not trusted: a hostile server must not make the client
// hold unbounded memory. Normal games stay far below each one. The limits are this port's own choice, not vanilla's.

/// Cards kept at once: the live cards and the end screen's cards each. A new team beyond this is refused.
pub const MAX_CARDS: usize = 64;
/// `card_image` pictures kept at once. A new id beyond this is refused; an id already held can be replaced.
pub const MAX_IMAGES: usize = 64;
/// Bytes of all `card_image` pictures together. A picture that would take the total past this is refused.
pub const MAX_IMAGE_BYTES: usize = 8 << 20;
/// Tooltips kept at once (by entity). A new entity beyond this is refused.
pub const MAX_TOOLTIPS: usize = 256;
/// Teams whose latest score is kept. A new team beyond this is refused.
pub const MAX_TEAM_SCORES: usize = 64;
/// Messages in the list on screen. The oldest is dropped when a new one arrives.
pub const MAX_LIST_MESSAGES: usize = 64;
/// Messages in the history. The oldest (by game time, the order the history shows) is dropped.
pub const MAX_PAST_MESSAGES: usize = 500;

/// `CardTile.EMPTY`.
pub fn empty_tile() -> CardTile {
    CardTile {
        id: None,
        image: CardTileImage::default(),
        image_list: Vec::new(),
        action: CardTileAction::None,
        item_tier: None,
        name: None,
        lore: Vec::new(),
        is_hidden: false,
        is_locked: false,
        is_flashing: false,
        progress: 0.0,
        team_keys: Vec::new(),
    }
}

#[derive(Debug, Clone)]
pub struct ClientCard {
    pub display: CardDisplay,
    pub tiles: Vec<CardTile>,
    /// When each tile last changed, for the flash on the card; `None` when it should not flash.
    pub tile_updated_at: Vec<Option<u64>>,
    /// When the card was last shuffled, in the host's milliseconds.
    pub shuffled_at: Option<u64>,
    /// `shufflePositions`: the grid slot each tile was in before the latest shuffle, indexed by tile. Starts as 0..25
    /// and becomes a new permutation of itself on each shuffle that passes the debounce.
    pub shuffle_positions: Vec<usize>,
    /// Set whenever the card needs to be drawn again; the host clears it.
    pub dirty: bool,
}

impl ClientCard {
    /// A card with no tiles yet, for tests of the drawing code.
    pub fn new_for_tests() -> Self {
        Self::new(CardDisplay::default())
    }

    fn new(display: CardDisplay) -> Self {
        ClientCard {
            display,
            tiles: Vec::new(),
            tile_updated_at: Vec::new(),
            shuffled_at: None,
            shuffle_positions: (0..TILES_PER_CARD).collect(),
            dirty: true,
        }
    }
}

/// The cards in the order they were first shown: the original keeps them in a `LinkedHashMap`, and "the first card"
/// (the one drawn when none is selected) is the first inserted, not the first by name.
#[derive(Debug, Default)]
pub struct Cards {
    items: Vec<(TeamKey, ClientCard)>,
}

impl Cards {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn get(&self, key: &TeamKey) -> Option<&ClientCard> {
        self.items.iter().find(|(k, _)| k == key).map(|(_, c)| c)
    }
    pub fn get_mut(&mut self, key: &TeamKey) -> Option<&mut ClientCard> {
        self.items.iter_mut().find(|(k, _)| k == key).map(|(_, c)| c)
    }
    pub fn contains_key(&self, key: &TeamKey) -> bool {
        self.get(key).is_some()
    }
    /// Replaces the card of an existing key in place, else appends.
    pub fn insert(&mut self, key: TeamKey, card: ClientCard) {
        match self.items.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = card,
            None => self.items.push((key, card)),
        }
    }
    /// `getOrPut`.
    pub fn get_or_insert_with(&mut self, key: TeamKey, make: impl FnOnce() -> ClientCard) -> &mut ClientCard {
        if !self.contains_key(&key) {
            self.items.push((key.clone(), make()));
        }
        self.get_mut(&key).expect("just inserted")
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&TeamKey, &ClientCard) -> bool) {
        self.items.retain(|(k, c)| keep(k, c));
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn iter(&self) -> impl Iterator<Item = (&TeamKey, &ClientCard)> {
        self.items.iter().map(|(k, c)| (k, c))
    }
    pub fn keys(&self) -> impl Iterator<Item = &TeamKey> {
        self.items.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl Iterator<Item = &ClientCard> {
        self.items.iter().map(|(_, c)| c)
    }
}

impl Index<&TeamKey> for Cards {
    type Output = ClientCard;
    fn index(&self, key: &TeamKey) -> &ClientCard {
        self.get(key).expect("no card for that team")
    }
}

/// `BingoHudState.ScoreMessage`.
#[derive(Debug, Clone)]
pub struct ScoreMessage {
    pub created_at: u64,
    pub packet: GameMessagePacket,
}

/// The sounds `ScorePacketReceiver` plays (`ClientSounds.Key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKey {
    ItemScored,
    OpponentScored,
    ItemLost,
    GameWon,
    GameLost,
    TimerTick,
    StartCountdown,
    StartRelease,
}

/// What the host should do because of a packet.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Sound { key: SoundKey, volume: f32, pitch: f32 },
    /// `cardOpenV1`: open the card screen on this team's card.
    OpenCard(TeamKey),
    /// `cardCompletedV1` with `isWinner`: the end fireworks.
    Fireworks,
    /// `gameOverV1/V2` that is not an update: open the end screen.
    OpenEndScreen,
    /// Something the open screens draw changed (`HudStateChangedEvent`).
    HudChanged,
}

/// What [`HudState::on_packet`] did with a payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Decoded and applied. `unread` bytes were left after the packet; the Kotlin readers ignore them, so do we.
    Applied { unread: usize },
    /// A channel this client does not handle (not the mod's, or one it has no client handler for).
    Ignored,
}

#[derive(Debug, Default)]
pub struct HudState {
    now: u64,
    pub cards: Cards,
    pub selected_team: Option<String>,
    /// `None` is `GameStatusPacket.DEFAULT`: no `game_status` has arrived since the last reset. The Kotlin code tests
    /// that by identity, so a received packet with both durations empty is NOT the default.
    pub game_status: Option<GameStatusPacket>,
    pub game_over: Option<GameOverPacket>,
    /// `GameOver.cards`: the cards the end screen shows. They outlive `cards`, which the server empties when the next
    /// game's lobby opens, so the winner is still there to look at.
    pub game_over_cards: Cards,
    pub messages: Vec<ScoreMessage>,
    pub past_messages: Vec<ScoreMessage>,
    /// `card_image` payloads by id (PNG bytes; removed when the packet carries none).
    pub images: HashMap<String, Vec<u8>>,
    pub tooltips_by_entity: HashMap<u128, TooltipPacket>,
    pub ready: Option<ReadyUpdatePacket>,
    pub timer_seconds: Option<i32>,
    pub countdown_seconds: Option<i32>,
    pub settings: Option<PlayerSettings>,
    /// The latest score the server sent for each team.
    pub scores: BTreeMap<String, TeamScore>,
    pub stats_hash: Option<String>,
    pub stats_index: Option<StatsIndexPacket>,
    /// Bytes left over after packets, summed: a growing number means the server speaks a newer format.
    pub unread_bytes: usize,
    /// The order source of the card shuffles (`shufflePositions.shuffled()` in the original).
    shuffle_rng: ShuffleRng,
    events: Vec<ClientEvent>,
}

impl HudState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The host's time in milliseconds (monotonic); call it before packets are applied each tick.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now = now_ms;
    }

    /// Seeds the order of the card shuffles. The same seed and the same packets give the same tile positions.
    pub fn set_shuffle_seed(&mut self, seed: u64) {
        self.shuffle_rng = ShuffleRng::new(seed);
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    /// Takes the events queued since the last call.
    pub fn take_events(&mut self) -> Vec<ClientEvent> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self, event: ClientEvent) {
        self.events.push(event);
    }

    fn sound(&mut self, key: SoundKey, volume: f32, pitch: f32) {
        self.emit(ClientEvent::Sound { key, volume, pitch });
    }

    /// `BingoHudController.getSelectedCard`: the selected team's card if it is shown, else the first one shown.
    /// (`selectedTeam` is a nullable team key: no selection looks up the no-team card, then falls back the same way.)
    pub fn selected_card(&self) -> Option<(&TeamKey, &ClientCard)> {
        let wanted: TeamKey = self.selected_team.clone();
        self.cards.iter().find(|(k, _)| **k == wanted).or_else(|| self.cards.iter().next())
    }

    /// `BingoHudState.reset`.
    pub fn reset(&mut self) {
        self.selected_team = None;
        self.game_over = None;
        self.game_over_cards.clear();
        self.ready = None;
    }

    /// `BingoHudController.updateGameOverViews`: the end screen's cards are the current ones (those of a team, not the
    /// preview card) followed by the ones it already had, a card of the same team only once, the newest first.
    fn update_game_over_views(&mut self) {
        if self.game_over.is_none() {
            return;
        }
        let mut next = Cards::default();
        for (key, card) in self.cards.iter().chain(self.game_over_cards.iter()) {
            if key.is_some() && !next.contains_key(key) && next.len() < MAX_CARDS {
                next.insert(key.clone(), card.clone());
            }
        }
        self.game_over_cards = next;
    }

    /// The cards the card screen shows: the end screen's while the game is over, else the live ones.
    pub fn screen_cards(&self) -> &Cards {
        if self.game_over.is_some() {
            &self.game_over_cards
        } else {
            &self.cards
        }
    }

    /// `BingoHudState.clearMessages`.
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.past_messages.clear();
    }

    /// `BingoHudState.resetAll`.
    pub fn reset_all(&mut self) {
        self.reset();
        self.clear_messages();
        self.game_status = None;
        self.cards.clear();
        self.images.clear();
    }

    /// Applies one plugin-channel payload from the server. A payload that does not decode changes nothing.
    pub fn on_packet(&mut self, channel: &str, data: &[u8]) -> Result<Handled, DecodeError> {
        let Some(path) = channel.strip_prefix(crate::channels::NAMESPACE).and_then(|rest| rest.strip_prefix(':')) else {
            return Ok(Handled::Ignored);
        };
        let mut r = Reader::new(data);
        match path {
            "card_reset" => {
                crate::card_reset::CardResetPacket::read(&mut r)?;
                self.reset();
            }
            "display_cards" => {
                let p = CardDisplayPacket::read(&mut r)?;
                self.on_card_display(p);
            }
            "update_card_tiles" => {
                // The one packet that is applied in part: a card with a tile this crate cannot read still shows the
                // tiles before it (otherwise one unknown data component empties the whole card). The error is still
                // returned so it is logged.
                let (p, failure) = crate::update_card_tiles::CardTilesPacket::read_salvaging(&mut r)?;
                if !p.tiles.is_empty() {
                    self.on_card_tiles(p);
                }
                if let Some(e) = failure {
                    return Err(e);
                }
            }
            "card_shuffled" => {
                let p = crate::card_shuffled::CardShuffledPacket::read(&mut r)?;
                // BingoHudController.onCardShuffled: a card shown and not shuffled within the last 280 ms is shuffled.
                let now = self.now;
                if let Some(card) = self.cards.get_mut(&p.team_key) {
                    if card.shuffled_at.map_or(true, |at| now.saturating_sub(at) > SHUFFLE_DURATION_MS) {
                        card.shuffled_at = Some(now);
                        card.shuffle_positions = shuffled(&card.shuffle_positions, &mut self.shuffle_rng);
                        card.dirty = true;
                    }
                }
            }
            "card_completed" => {
                let p = crate::scoring::CardCompletedPacket::read(&mut r)?;
                if p.is_winner {
                    self.emit(ClientEvent::Fireworks);
                }
                let key = if p.is_winner { SoundKey::GameWon } else { SoundKey::GameLost };
                self.sound(key, 1.0, 1.0);
            }
            "open_card" => {
                let p = crate::open_card::CardOpenPacket::read(&mut r)?;
                self.emit(ClientEvent::OpenCard(p.team_key));
            }
            "card_image" => {
                let p = crate::card_image::CardImagePacket::read(&mut r)?;
                match p.image {
                    Some(bytes) => {
                        if self.image_fits(&p.id, bytes.len()) {
                            self.images.insert(p.id, bytes);
                        }
                    }
                    None => {
                        self.images.remove(&p.id);
                    }
                }
            }
            "game_over_v2" => {
                let p = GameOverPacket::read_v2(&mut r)?;
                self.on_game_over(p);
            }
            "game_over" => {
                let p = GameOverPacket::read_v1(&mut r)?;
                self.on_game_over(p);
            }
            "score_update" => {
                let p = ScoredItemPacket::read(&mut r)?;
                let key = if p.is_viewer_on_team { SoundKey::ItemScored } else { SoundKey::OpponentScored };
                // ScorePacketReceiver: pitch = 0.5 + items / 25.
                let pitch = 0.5 + p.score.items as f32 / 25.0;
                self.sound(key, 1.0, pitch);
                if self.scores.contains_key(&p.team) || self.scores.len() < MAX_TEAM_SCORES {
                    self.scores.insert(p.team, p.score);
                }
            }
            "scored_item_lost" => {
                crate::scoring::ScoredItemLostPacket::read(&mut r)?;
                self.sound(SoundKey::ItemLost, 0.8, 1.5);
            }
            "game_status" => {
                self.game_status = Some(GameStatusPacket::read(&mut r)?);
                self.emit(ClientEvent::HudChanged);
            }
            "game_message" => {
                let p = GameMessagePacket::read(&mut r)?;
                self.on_game_message(p);
            }
            "game_message_clear" => {
                crate::scoring::GameMessageClearPacket::read(&mut r)?;
                self.clear_messages();
                self.emit(ClientEvent::HudChanged);
            }
            "timer" => {
                let p = crate::timer::TimerPacket::read(&mut r)?;
                let s = p.seconds_remaining;
                self.timer_seconds = Some(s);
                if (1..=20).contains(&s) && s % 2 != 0 {
                    let volume = 1.0 - s as f32 / 20.0;
                    self.sound(SoundKey::TimerTick, volume * 0.8, volume * 0.5 + 0.5);
                }
            }
            "countdown" => {
                let p = crate::timer::CountdownPacket::read(&mut r)?;
                self.countdown_seconds = Some(p.seconds_remaining);
                match p.seconds_remaining {
                    1..=3 => self.sound(SoundKey::StartCountdown, 1.0, 1.0),
                    0 => self.sound(SoundKey::StartRelease, 1.0, 1.0),
                    _ => {}
                }
            }
            "tooltip_v2" => {
                let p = TooltipPacket::read(&mut r)?;
                if self.tooltips_by_entity.contains_key(&p.entity_id) || self.tooltips_by_entity.len() < MAX_TOOLTIPS {
                    self.tooltips_by_entity.insert(p.entity_id, p);
                }
            }
            "ready_timer_update_v2" => {
                let p = ReadyUpdatePacket::read_v2(&mut r)?;
                self.on_ready_update(p);
            }
            "ready_timer_update" => {
                let p = ReadyUpdatePacket::read_v1(&mut r)?;
                self.on_ready_update(p);
            }
            "update_settings" => {
                self.settings = Some(PlayerSettings::read(&mut r)?);
            }
            "stats_check" => {
                self.stats_hash = Some(crate::stats_check::StatsCheckPacket::read(&mut r)?.hash_sha512);
            }
            "stats_index" => {
                self.stats_index = Some(StatsIndexPacket::read(&mut r)?);
            }
            // The client has no handler for these (the server sends game_reset to nobody that listens; stats_game and
            // the C2S-only packets are never answered with state here).
            "game_reset" | "stats_game" | "set_ready" | "set_ready_v2" => return Ok(Handled::Ignored),
            _ => return Ok(Handled::Ignored),
        }
        let unread = r.remaining();
        self.unread_bytes += unread;
        Ok(Handled::Applied { unread })
    }

    /// `BingoHudController.onCardDisplay`.
    fn on_card_display(&mut self, p: CardDisplayPacket) {
        let shown: Vec<&TeamKey> = p.display.iter().map(|(k, _)| k).collect();
        self.cards.retain(|key, _| shown.contains(&key));
        let no_teams = p.display.is_empty();
        for (key, display) in p.display {
            if let Some(card) = self.cards.get_mut(&key) {
                card.display = display;
                card.dirty = true;
            } else if self.cards.len() < MAX_CARDS {
                self.cards.insert(key, ClientCard::new(display));
            }
        }
        // "if there are no teams being displayed (BingoMapController sends when entering PREGAME) reset the HUD state"
        if no_teams && self.game_status.is_none() {
            self.reset();
        }
        self.update_game_over_views();
    }

    /// `BingoHudController.onCardTiles`.
    fn on_card_tiles(&mut self, p: crate::update_card_tiles::CardTilesPacket) {
        let now = self.now;
        if !self.cards.contains_key(&p.team_key) && self.cards.len() >= MAX_CARDS {
            return;
        }
        let card = self.cards.get_or_insert_with(p.team_key, || ClientCard::new(CardDisplay::default()));
        let mut tiles = Vec::with_capacity(TILES_PER_CARD);
        let mut updated = Vec::with_capacity(TILES_PER_CARD);
        for index in 0..TILES_PER_CARD {
            let existing = card.tiles.get(index);
            let new_tile = p
                .tiles
                .iter()
                .rev() // `packet.tiles` is a Map: for a repeated index the last entry wins
                .find(|(i, _)| *i == index as i32)
                .map(|(_, t)| t.clone())
                .or_else(|| existing.cloned())
                .unwrap_or_else(empty_tile);
            let updated_at = if !p.should_notify {
                None
            } else if existing.map(|t| &t.id) != Some(&new_tile.id) {
                Some(now)
            } else {
                card.tile_updated_at.get(index).copied().flatten()
            };
            tiles.push(new_tile);
            updated.push(updated_at);
        }
        card.tiles = tiles;
        card.tile_updated_at = updated;
        card.dirty = true;
        self.update_game_over_views();
    }

    /// `BingoHudController.onGameOver` (the screen itself is the host's: only the state and the event are here).
    fn on_game_over(&mut self, p: GameOverPacket) {
        let is_update = p.is_update;
        self.game_over = Some(p);
        self.update_game_over_views();
        if !is_update {
            self.emit(ClientEvent::OpenEndScreen);
        }
    }

    /// Whether a `card_image` of `len` bytes under `id` may be kept: a new id only while fewer than [`MAX_IMAGES`] are
    /// held, and the total of all pictures (the one under `id` replaced) within [`MAX_IMAGE_BYTES`].
    fn image_fits(&self, id: &str, len: usize) -> bool {
        let held: usize = self.images.values().map(Vec::len).sum();
        let replaced = self.images.get(id).map_or(0, Vec::len);
        let count_ok = self.images.contains_key(id) || self.images.len() < MAX_IMAGES;
        count_ok && held - replaced + len <= MAX_IMAGE_BYTES
    }

    /// `BingoHudController.onMessagePacket`.
    fn on_game_message(&mut self, p: GameMessagePacket) {
        let message = ScoreMessage { created_at: self.now, packet: p };
        if !message.packet.is_update {
            self.messages.push(message.clone());
            let extra = self.messages.len().saturating_sub(MAX_LIST_MESSAGES);
            self.messages.drain(..extra);
        }
        let id = message.packet.id;
        self.past_messages.retain(|m| m.packet.id != id);
        self.past_messages.push(message);
        // `sortBy { timeElapsed }` is stable.
        self.past_messages.sort_by_key(|m| (m.packet.time_elapsed.seconds, m.packet.time_elapsed.nanos));
        // The history is sorted by game time, so its oldest entry is the first one.
        let extra = self.past_messages.len().saturating_sub(MAX_PAST_MESSAGES);
        self.past_messages.drain(..extra);
        self.emit(ClientEvent::HudChanged);
    }

    /// `BingoHudController.onReadyUpdate`.
    fn on_ready_update(&mut self, p: ReadyUpdatePacket) {
        let changed = match &self.ready {
            None => true,
            Some(old) => {
                old.can_send_ready != p.can_send_ready
                    || old.is_ready != p.is_ready
                    || old.is_running != p.is_running
                    || old.vote_options != p.vote_options
                    || old.title != p.title
                    || old.subtitle != p.subtitle
            }
        };
        self.ready = Some(p);
        if changed {
            self.emit(ClientEvent::HudChanged);
        }
    }
}

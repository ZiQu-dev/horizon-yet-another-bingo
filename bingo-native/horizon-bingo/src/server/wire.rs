//! Server to client packets, built with the client's codecs. Each function returns the channel path (without the
//! namespace, see `channels.rs`) and the payload bytes, ready for `ServerHost::send`.
//!
//! Preconditions: `card` has exactly `CARD_SQUARES` squares, and names and ids are far below the 64 KiB limit of a
//! string. A write that fails on such input panics with a message rather than sending a truncated payload.

use crate::buf::encode;
use crate::card_reset::CardResetPacket;
use crate::card_tile::{CardTile, TierLabel};
use crate::card_tile_action::CardTileAction;
use crate::card_tile_image::CardTileImage;
use crate::display_cards::{CardDisplay, CardDisplayPacket, PlayerProfile, FORMATTING_NAMES};
use crate::game_over::{GameOverPacket, ScoreRanking};
use crate::game_reset::GameResetPacket;
use crate::game_status::{GameStatusPacket, JavaDuration};
use crate::item::ItemStack;
use crate::nbt::Nbt;
use crate::open_card::CardOpenPacket;
use crate::score_update::{ScoredItemPacket, TeamScore};
use crate::server::types::{Card, Player, CARD_SQUARES};
use crate::text::Text;
use crate::timer::{CountdownPacket, TimerPacket};
use crate::update_card_tiles::CardTilesPacket;

/// `game_status`: the in-game and the remaining time, each `None` when absent. Milliseconds, as the mod sends them.
pub fn game_status(ingame_ms: Option<i64>, remaining_ms: Option<i64>) -> (&'static str, Vec<u8>) {
    let packet = GameStatusPacket {
        ingame_duration: ingame_ms.map(JavaDuration::of_millis),
        remaining_duration: remaining_ms.map(JavaDuration::of_millis),
    };
    ("game_status", encode(|w| packet.write(w)))
}

/// `timer`: seconds remaining until the game ends.
pub fn timer(seconds_remaining: i32) -> (&'static str, Vec<u8>) {
    let packet = TimerPacket { seconds_remaining };
    ("timer", encode(|w| packet.write(w)))
}

/// `countdown`: seconds remaining until the game starts.
pub fn countdown(seconds: i32) -> (&'static str, Vec<u8>) {
    let packet = CountdownPacket { seconds_remaining: seconds };
    ("countdown", encode(|w| packet.write(w)))
}

/// `card_reset`: no payload.
pub fn card_reset() -> (&'static str, Vec<u8>) {
    ("card_reset", encode(|w| CardResetPacket.write(w)))
}

/// `game_reset`: no payload.
pub fn game_reset() -> (&'static str, Vec<u8>) {
    ("game_reset", encode(|w| GameResetPacket.write(w)))
}

/// `open_card`: the team whose card the player opens, or `None`.
pub fn open_card(team: Option<&str>) -> (&'static str, Vec<u8>) {
    let packet = CardOpenPacket { team_key: team.map(str::to_owned) };
    ("open_card", encode(|w| packet.write(w)))
}

/// `score_update`: the team's score, and whether the viewer is on that team.
pub fn score_update(viewer_on_team: bool, team: &str, score: TeamScore) -> (&'static str, Vec<u8>) {
    let packet = ScoredItemPacket { is_viewer_on_team: viewer_on_team, team: team.to_owned(), score };
    ("score_update", encode(|w| packet.write(w)))
}

/// `display_cards`: one entry of the team map, `team` being `BingoTeamKey.id` (for example `bingo_red`).
/// `color` must be a `Formatting` name such as `RED`; the client reads any other name as no colour.
/// `name` is the team's display name as plain text.
pub fn display_cards(team: &str, color: &str, name: &str, players: &[Player]) -> (&'static str, Vec<u8>) {
    debug_assert!(FORMATTING_NAMES.contains(&color), "not a Formatting name: {color}");
    let display = CardDisplay {
        team_color: Some(color.to_owned()),
        team_name: Some(Text::plain(name)),
        players: players.iter().map(|p| PlayerProfile { uuid: p.uuid, name: p.name.clone() }).collect(),
    };
    let packet = CardDisplayPacket { display: vec![(Some(team.to_owned()), display)] };
    ("display_cards", encode(|w| packet.write(w).expect("display_cards: a text component fails to write")))
}

/// `update_card_tiles`: the 25 tiles of a team's card, in square order (index = row * 5 + column).
///
/// Each tile is built the way the mod's `CardViewService` builds one, from what the server knows here:
/// - `id` is the square's item id; `item_tier` is its tier; `is_hidden`, `is_locked` and `is_flashing` are false.
/// - A square whose index is in `found` has `progress` 1.0, `is_achieved` set, and `team_keys` = `[team]`. The mod
///   leaves out the viewing team there (`CardViewService.kt`), so the client sees this team as one more team that has
///   the item. Kept as the brief asked; it is the one place where this differs from the mod.
/// - Other squares have progress 0.0, no team keys and no achievement.
/// - `item_id` gives the item's registry id. A `None` leaves the picture out (`image.item` and the action are
///   `None`), and the tile keeps its id, tier and name.
/// - The picture is a stack of one item with no data components. The mod adds lore and tooltip components there,
///   which the tile does not need, because the lore is sent as a text.
/// - The name is the item's translation key with the id path as fallback. The key is always `item.` because the
///   codec cannot tell a block from an item, so a block shows its id path.
/// - The lore is one line: the tier (coloured as in `TierLabel.kt`), the list name, and the square's coordinate
///   (column `b i n g o`, then row 1 to 5), as on the mod's tiles.
pub fn card_tiles(
    team: &str,
    card: &Card,
    found: &[usize],
    item_id: &dyn Fn(&str) -> Option<i32>,
    should_notify: bool,
) -> (&'static str, Vec<u8>) {
    assert_eq!(card.squares.len(), CARD_SQUARES, "a card has {CARD_SQUARES} squares");
    let tiles = card
        .squares
        .iter()
        .enumerate()
        .map(|(index, objective)| {
            let is_found = found.contains(&index);
            let stack = item_id(&objective.item).map(|id| ItemStack { count: 1, item: id, added: vec![], removed: vec![] });
            let action = match &stack {
                Some(stack) => CardTileAction::Item(stack.clone()),
                None => CardTileAction::None,
            };
            let tile = CardTile {
                id: Some(objective.item.clone()),
                image: CardTileImage { item: stack, is_achieved: is_found, ..CardTileImage::default() },
                image_list: vec![],
                action,
                item_tier: Some(objective.tier),
                name: Some(item_name(&objective.item)),
                lore: vec![tile_lore(objective.tier, index)],
                is_hidden: false,
                is_locked: false,
                is_flashing: false,
                progress: if is_found { 1.0 } else { 0.0 },
                team_keys: if is_found { vec![team.to_owned()] } else { vec![] },
            };
            (index as i32, tile)
        })
        .collect();
    let packet = CardTilesPacket { team_key: Some(team.to_owned()), tiles, should_notify };
    ("update_card_tiles", encode(|w| packet.write(w).expect("update_card_tiles: a tile fails to write")))
}

/// `game_over_v2`: the end screen with the winner and the ranking. Only these fields are set: the title and the
/// subtitle are empty, the duration is zero, `ended_at` is the epoch, and the screen opens (`is_update` false).
/// `is_winner` tells the viewer whether they won.
pub fn game_over_v2(winner: Option<&str>, is_winner: bool, scores: Vec<ScoreRanking>) -> (&'static str, Vec<u8>) {
    let packet = GameOverPacket {
        winner: winner.map(str::to_owned),
        is_winner,
        scores,
        ..GameOverPacket::default()
    };
    ("game_over_v2", encode(|w| packet.write_v2(w).expect("game_over_v2: a text component fails to write")))
}

/// The tile name: a translation key of the vanilla item, with the id path as fallback. A non-vanilla id is
/// plain text.
fn item_name(item: &str) -> Text {
    match item.strip_prefix("minecraft:") {
        Some(path) => Text(Nbt::compound(vec![
            ("translate", Nbt::string(&format!("item.minecraft.{path}"))),
            ("fallback", Nbt::string(path)),
        ])),
        None => Text::plain(item),
    }
}

/// The one-line lore of a tile: tier, list, coordinate, as in the mod's `CardViewService`.
fn tile_lore(tier: TierLabel, index: usize) -> Text {
    let separator = || Nbt::compound(vec![("color", Nbt::string("gray")), ("text", Nbt::string(" • "))]);
    let tier_key = format!("bingo.tier.{}", tier.name().to_lowercase());
    let coordinate = coordinate(index);
    Text(Nbt::compound(vec![
        (
            "extra",
            Nbt::list(vec![
                Nbt::compound(vec![
                    ("color", Nbt::string(tier_color(tier))),
                    ("translate", Nbt::string(&tier_key)),
                ]),
                separator(),
                Nbt::compound(vec![
                    ("color", Nbt::string("blue")),
                    ("translate", Nbt::string("bingo.list.items")),
                ]),
                separator(),
                Nbt::compound(vec![("color", Nbt::string("gray")), ("text", Nbt::string(&coordinate))]),
            ]),
        ),
        ("text", Nbt::string("")),
    ]))
}

/// The `Formatting` name of a tier's colour (`TierLabel.kt`).
fn tier_color(tier: TierLabel) -> &'static str {
    match tier {
        TierLabel::S => "dark_red",
        TierLabel::A => "red",
        TierLabel::B => "gold",
        TierLabel::C => "yellow",
        TierLabel::D => "green",
    }
}

/// `b1` for square 0, `i1` for 1, ... `o5` for 24: the column letter of B-I-N-G-O, then the row from 1.
fn coordinate(index: usize) -> String {
    const COLUMNS: [char; 5] = ['b', 'i', 'n', 'g', 'o'];
    format!("{}{}", COLUMNS[index % 5], index / 5 + 1)
}

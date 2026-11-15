//! Server to client packets, built with the client's codecs. Each function returns the channel path (without the
//! namespace, see `channels.rs`) and the payload bytes, ready for `ServerHost::send`.
//!
//! Preconditions: `card` has exactly `CARD_SQUARES` squares, and names and ids are far below the 64 KiB limit of a
//! string. A write that fails on such input panics with a message rather than sending a truncated payload.

use super::stats::EndStats;
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
use crate::server::hidden_consume::TileView;
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
/// subtitle are empty, `ended_at` is the epoch, and the screen opens (`is_update` false).
/// `is_winner` tells the viewer whether they won; `duration_ms` is the time the game took.
pub fn game_over_v2(
    winner: Option<&str>,
    is_winner: bool,
    duration_ms: i64,
    scores: Vec<ScoreRanking>,
) -> (&'static str, Vec<u8>) {
    game_over_v2_with(winner, is_winner, duration_ms, scores, &EndStats::default())
}

/// `game_over_v2` with the statistics lines: the best time, the win streak and the captured items of the viewer
/// (`stats::end_stats`). A viewer with no `player` numbers gets none of the streak or item lines.
pub fn game_over_v2_with(
    winner: Option<&str>,
    is_winner: bool,
    duration_ms: i64,
    scores: Vec<ScoreRanking>,
    end: &EndStats,
) -> (&'static str, Vec<u8>) {
    let viewer = end.player.as_ref();
    let packet = GameOverPacket {
        winner: winner.map(str::to_owned),
        is_winner,
        duration: JavaDuration::of_millis(duration_ms),
        scores,
        win_streak: viewer.map(|v| v.win_streak),
        best_win_streak: viewer.and_then(|v| v.best_win_streak),
        is_best_win_streak: viewer.is_some_and(|v| v.is_best_win_streak),
        captured_items: viewer.map(|v| v.captured_items),
        best_captured_items: viewer.and_then(|v| v.best_captured_items),
        is_best_captured_items: viewer.is_some_and(|v| v.is_best_captured_items),
        ended_at_millis: end.ended_at_ms,
        is_best_time: end.is_best_time,
        prev_best_time: end.prev_best_time_ms.map(JavaDuration::of_millis),
        ..GameOverPacket::default()
    };
    ("game_over_v2", encode(|w| packet.write_v2(w).expect("game_over_v2: a text component fails to write")))
}

/// The tile name: a translation key of the vanilla item, with the id path as fallback. A non-vanilla id is
/// plain text.
pub(super) fn item_name(item: &str) -> Text {
    match item.strip_prefix("minecraft:") {
        Some(path) => Text(Nbt::compound(vec![
            ("translate", Nbt::string(&format!("item.minecraft.{path}"))),
            ("fallback", Nbt::string(path)),
        ])),
        None => Text::plain(item),
    }
}

/// The one-line lore of a tile: tier, list, coordinate, as in the mod's `CardViewService`.
pub(super) fn tile_lore(tier: TierLabel, index: usize) -> Text {
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

// Several teams on one card (the team session). Appended; the functions above are unchanged.

/// `display_cards` with one entry per team, in the order given: `(key, colour, name, players)`. `colour` is a
/// `Formatting` name such as `RED`, `name` the team's display name, and `players` the members to show.
pub fn display_cards_all(entries: &[(&str, &str, &str, Vec<Player>)]) -> (&'static str, Vec<u8>) {
    let display = entries
        .iter()
        .map(|(key, color, name, players)| {
            debug_assert!(FORMATTING_NAMES.contains(color), "not a Formatting name: {color}");
            let card_display = CardDisplay {
                team_color: Some((*color).to_owned()),
                team_name: Some(Text::plain(name)),
                players: players.iter().map(|p| PlayerProfile { uuid: p.uuid, name: p.name.clone() }).collect(),
            };
            (Some((*key).to_owned()), card_display)
        })
        .collect();
    let packet = CardDisplayPacket { display };
    ("display_cards", encode(|w| packet.write(w).expect("display_cards: a text component fails to write")))
}

/// `update_card_tiles` for one viewer of a shared card: the 25 tiles, as seen by a player of `viewer_team`.
///
/// `found_by` gives every team's found squares, as `(team key, square indices)`. A square is achieved for the viewer
/// (`is_achieved`, progress 1.0) when the viewer's own team found it. `team_keys` lists the other teams that found it,
/// in the order of `found_by`; the viewer's team is left out, as in the mod's `CardViewService`.
/// Everything else is as in [`card_tiles`]: the tile's item, tier, name and lore, and the picture when the item exists.
pub fn card_tiles_for(
    viewer_team: &str,
    card: &Card,
    found_by: &[(&str, &[usize])],
    item_id: &dyn Fn(&str) -> Option<i32>,
    should_notify: bool,
) -> (&'static str, Vec<u8>) {
    let views: Vec<TileView> = card
        .squares
        .iter()
        .enumerate()
        .map(|(index, square)| TileView { index, hidden: false, item: Some(square.item.clone()), tier: Some(square.tier) })
        .collect();
    card_tiles_seen(viewer_team, card, found_by, &views, item_id, should_notify)
}

/// `update_card_tiles` for one viewer when some squares may be hidden (hidden-items mode). `views` is each square as
/// the viewer sees it, in card order (see `hidden_consume::tile_views`). A visible square is built as in
/// [`card_tiles_for`]. A hidden square has no id, picture, action, name, lore or other teams, and no tier unless its
/// view gives one; it is `is_hidden`.
pub fn card_tiles_seen(
    viewer_team: &str,
    card: &Card,
    found_by: &[(&str, &[usize])],
    views: &[TileView],
    item_id: &dyn Fn(&str) -> Option<i32>,
    should_notify: bool,
) -> (&'static str, Vec<u8>) {
    assert_eq!(card.squares.len(), CARD_SQUARES, "a card has {CARD_SQUARES} squares");
    assert_eq!(views.len(), CARD_SQUARES, "a view per square");
    let viewer_found: &[usize] = match found_by.iter().find(|entry| entry.0 == viewer_team) {
        Some(entry) => entry.1,
        None => &[],
    };
    let tiles = card
        .squares
        .iter()
        .zip(views)
        .enumerate()
        .map(|(index, (objective, view))| {
            let is_found = viewer_found.contains(&index);
            let tile = if view.hidden {
                CardTile {
                    id: None,
                    image: CardTileImage::default(),
                    image_list: vec![],
                    action: CardTileAction::None,
                    item_tier: view.tier,
                    name: None,
                    lore: vec![],
                    is_hidden: true,
                    is_locked: false,
                    is_flashing: false,
                    progress: 0.0,
                    team_keys: vec![],
                }
            } else {
                let other_teams: Vec<String> = found_by
                    .iter()
                    .filter(|entry| entry.0 != viewer_team && entry.1.contains(&index))
                    .map(|entry| entry.0.to_string())
                    .collect();
                let stack = item_id(&objective.item).map(|id| ItemStack { count: 1, item: id, added: vec![], removed: vec![] });
                let action = match &stack {
                    Some(stack) => CardTileAction::Item(stack.clone()),
                    None => CardTileAction::None,
                };
                CardTile {
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
                    team_keys: other_teams,
                }
            };
            (index as i32, tile)
        })
        .collect();
    let packet = CardTilesPacket { team_key: Some(viewer_team.to_owned()), tiles, should_notify };
    ("update_card_tiles", encode(|w| packet.write(w).expect("update_card_tiles: a tile fails to write")))
}

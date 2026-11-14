//! Packets for several teams (`wire::display_cards_all` and `wire::card_tiles_for`), decoded with the client's own
//! readers. Expected values are written out by hand from the rules: a square is achieved for a viewer only when the
//! viewer's own team found it, and `team_keys` lists the other teams that found it, in the order given.

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::display_cards::{CardDisplayPacket, PlayerProfile};
use horizon_bingo::server::types::{Card, Objective, Player, CARD_SQUARES};
use horizon_bingo::server::wire;
use horizon_bingo::text::Text;
use horizon_bingo::update_card_tiles::CardTilesPacket;

/// The 25 squares of the test card, row-major. Tiers repeat S, A, B, C, D by square.
const ITEMS: [&str; 25] = [
    "minecraft:oak_log", "minecraft:cobblestone", "minecraft:iron_ingot", "minecraft:gold_ingot",
    "minecraft:diamond", "minecraft:coal", "minecraft:redstone", "minecraft:lapis_lazuli",
    "minecraft:emerald", "minecraft:sand", "minecraft:gravel", "minecraft:clay_ball",
    "minecraft:bread", "minecraft:apple", "minecraft:carrot", "minecraft:potato",
    "minecraft:wheat", "minecraft:bucket", "minecraft:compass", "minecraft:clock",
    "minecraft:map", "minecraft:book", "minecraft:feather", "minecraft:bone", "minecraft:string",
];
const TIERS: [TierLabel; 5] = [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D];

fn fixture_card() -> Card {
    Card {
        squares: ITEMS
            .iter()
            .enumerate()
            .map(|(i, item)| Objective { item: item.to_string(), tier: TIERS[i % 5] })
            .collect(),
    }
}

/// Registry id 1000 + square index for the fixture items; anything else does not exist.
fn item_id(name: &str) -> Option<i32> {
    ITEMS.iter().position(|item| *item == name).map(|i| 1000 + i as i32)
}

fn decode_tiles(bytes: &[u8]) -> CardTilesPacket {
    decode(bytes, CardTilesPacket::read).expect("the payload decodes and every byte is used")
}

/// The team keys of every tile, in square order.
fn team_keys(packet: &CardTilesPacket) -> Vec<Vec<String>> {
    packet.tiles.iter().map(|(_, tile)| tile.team_keys.clone()).collect()
}

fn achieved(packet: &CardTilesPacket) -> Vec<usize> {
    packet.tiles.iter().filter(|(_, tile)| tile.image.is_achieved).map(|(i, _)| *i as usize).collect()
}

fn player(uuid: u128, name: &str) -> Player {
    Player { uuid, name: name.to_string() }
}

#[test]
fn display_cards_all_lists_each_team_in_the_order_given_with_its_players() {
    let red = vec![player(1, "Alice")];
    let blue = vec![player(2, "Bob"), player(3, "Carol")];
    let (path, bytes) = wire::display_cards_all(&[
        ("bingo_red", "RED", "Red", red.clone()),
        ("bingo_blue", "BLUE", "Blue", blue.clone()),
    ]);
    assert_eq!(path, "display_cards");

    let packet = decode(&bytes, CardDisplayPacket::read).expect("decodes, every byte used");
    let keys: Vec<Option<String>> = packet.display.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(keys, [Some("bingo_red".to_string()), Some("bingo_blue".to_string())]);

    let (_, first) = &packet.display[0];
    assert_eq!(first.team_color.as_deref(), Some("RED"));
    assert_eq!(first.team_name, Some(Text::plain("Red")));
    assert_eq!(first.players, vec![PlayerProfile { uuid: 1, name: "Alice".to_string() }]);

    let (_, second) = &packet.display[1];
    assert_eq!(second.team_color.as_deref(), Some("BLUE"));
    assert_eq!(second.team_name.as_ref().and_then(Text::as_plain), Some("Blue"));
    assert_eq!(
        second.players,
        vec![
            PlayerProfile { uuid: 2, name: "Bob".to_string() },
            PlayerProfile { uuid: 3, name: "Carol".to_string() },
        ]
    );
}

#[test]
fn display_cards_all_with_one_entry_is_the_single_team_packet() {
    let players = vec![player(1, "Alice"), player(2, "Bob")];
    let single = wire::display_cards("bingo_red", "RED", "Red", &players).1;
    let all = wire::display_cards_all(&[("bingo_red", "RED", "Red", players.clone())]).1;
    assert_eq!(all, single);
}

#[test]
fn display_cards_all_with_no_entries_is_a_zero_count() {
    let (_, bytes) = wire::display_cards_all(&[]);
    assert_eq!(bytes, [0, 0, 0, 0]);
    assert!(decode(&bytes, CardDisplayPacket::read).unwrap().display.is_empty());
}

#[test]
fn a_team_with_no_players_in_display_cards_all_keeps_an_empty_list() {
    let (_, bytes) = wire::display_cards_all(&[("bingo_gray", "GRAY", "Gray", vec![])]);
    let packet = decode(&bytes, CardDisplayPacket::read).expect("decodes");
    assert_eq!(packet.display.len(), 1);
    assert!(packet.display[0].1.players.is_empty());
}

#[test]
fn a_viewer_sees_its_own_finds_as_achieved_and_the_other_teams_finds_as_their_keys() {
    // Red found squares 0 and 7; blue found 7 and 24.
    let found_by: [(&str, &[usize]); 2] = [("bingo_red", &[0, 7]), ("bingo_blue", &[7, 24])];
    let (path, bytes) = wire::card_tiles_for("bingo_red", &fixture_card(), &found_by, &item_id, true);
    assert_eq!(path, "update_card_tiles");
    let packet = decode_tiles(&bytes);

    assert_eq!(packet.team_key.as_deref(), Some("bingo_red"));
    assert!(packet.should_notify);
    assert_eq!(achieved(&packet), [0, 7]);

    // Square 0: red's own, no other team has it. Square 7: red's own, and blue has it too. Square 24: only blue.
    let tile = |index: usize| &packet.tiles[index].1;
    assert_eq!((tile(0).progress, tile(0).team_keys.clone()), (1.0, vec![]));
    assert_eq!((tile(7).progress, tile(7).team_keys.clone()), (1.0, vec!["bingo_blue".to_string()]));
    assert_eq!((tile(24).progress, tile(24).team_keys.clone()), (0.0, vec!["bingo_blue".to_string()]));
    assert_eq!((tile(3).progress, tile(3).team_keys.clone()), (0.0, vec![]));
}

#[test]
fn the_viewer_of_another_team_sees_the_same_card_from_its_own_side() {
    let found_by: [(&str, &[usize]); 2] = [("bingo_red", &[0, 7]), ("bingo_blue", &[7, 24])];
    let packet = decode_tiles(&wire::card_tiles_for("bingo_blue", &fixture_card(), &found_by, &item_id, false).1);

    assert_eq!(packet.team_key.as_deref(), Some("bingo_blue"));
    assert!(!packet.should_notify);
    assert_eq!(achieved(&packet), [7, 24]);
    assert_eq!(packet.tiles[0].1.team_keys, ["bingo_red"]);
    assert_eq!(packet.tiles[7].1.team_keys, ["bingo_red"]);
    assert!(packet.tiles[24].1.team_keys.is_empty());
}

#[test]
fn team_keys_follow_the_order_of_found_by() {
    let red_then_aqua: [(&str, &[usize]); 3] = [("bingo_red", &[5]), ("bingo_blue", &[5]), ("bingo_aqua", &[5])];
    let packet = decode_tiles(&wire::card_tiles_for("bingo_green", &fixture_card(), &red_then_aqua, &item_id, false).1);
    assert_eq!(packet.tiles[5].1.team_keys, ["bingo_red", "bingo_blue", "bingo_aqua"]);

    let reversed: [(&str, &[usize]); 3] = [("bingo_aqua", &[5]), ("bingo_blue", &[5]), ("bingo_red", &[5])];
    let packet = decode_tiles(&wire::card_tiles_for("bingo_green", &fixture_card(), &reversed, &item_id, false).1);
    assert_eq!(packet.tiles[5].1.team_keys, ["bingo_aqua", "bingo_blue", "bingo_red"]);
}

#[test]
fn a_viewer_not_in_found_by_has_nothing_achieved_but_sees_the_others() {
    let found_by: [(&str, &[usize]); 1] = [("bingo_red", &[2])];
    let packet = decode_tiles(&wire::card_tiles_for("bingo_green", &fixture_card(), &found_by, &item_id, false).1);
    assert!(achieved(&packet).is_empty());
    assert_eq!(packet.tiles[2].1.progress, 0.0);
    assert_eq!(packet.tiles[2].1.team_keys, ["bingo_red"]);
    assert_eq!(packet.team_key.as_deref(), Some("bingo_green"));
}

#[test]
fn a_card_nobody_has_found_is_the_same_packet_as_the_single_team_card() {
    let card = fixture_card();
    let viewer = wire::card_tiles_for("bingo_red", &card, &[], &item_id, true).1;
    assert_eq!(viewer, wire::card_tiles("bingo_red", &card, &[], &item_id, true).1);
}

#[test]
fn card_tiles_for_writes_all_25_squares_with_their_tiers_and_the_notify_flag() {
    let found_by: [(&str, &[usize]); 1] = [("bingo_blue", &[1])];
    let packet = decode_tiles(&wire::card_tiles_for("bingo_red", &fixture_card(), &found_by, &item_id, true).1);
    assert_eq!(packet.tiles.len(), CARD_SQUARES);
    let indices: Vec<i32> = packet.tiles.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, (0..25).collect::<Vec<i32>>());
    assert_eq!(packet.tiles[7].1.item_tier, Some(TierLabel::B), "square 7 is the third tier of the cycle");
    assert_eq!(packet.tiles[7].1.id.as_deref(), Some("minecraft:lapis_lazuli"));
    assert!(packet.should_notify);
    assert_eq!(team_keys(&packet)[1], ["bingo_blue"]);
}

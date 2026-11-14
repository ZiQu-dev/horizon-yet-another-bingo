//! Server-to-client packet builders (`horizon_bingo::server::wire`), checked by decoding their bytes with the client's
//! own readers. Expected values are written out by hand; the real captures give the byte layout of the header.

use horizon_bingo::buf::{decode, Reader};
use horizon_bingo::card_reset::CardResetPacket;
use horizon_bingo::card_tile::{CardTile, TierLabel};
use horizon_bingo::card_tile_action::CardTileAction;
use horizon_bingo::channels::{self, Dir, CHANNELS};
use horizon_bingo::display_cards::{CardDisplayPacket, PlayerProfile, ID as DISPLAY_ID};
use horizon_bingo::game_over::{EndScreenTab, GameOverPacket, ScoreRanking, V2_ID as GAME_OVER_V2_ID};
use horizon_bingo::game_reset::GameResetPacket;
use horizon_bingo::game_status::{GameStatusPacket, JavaDuration};
use horizon_bingo::item::ItemStack;
use horizon_bingo::nbt::Nbt;
use horizon_bingo::open_card::CardOpenPacket;
use horizon_bingo::score_update::{ScoredItemPacket, TeamScore};
use horizon_bingo::server::types::{Card, Objective, Player};
use horizon_bingo::server::wire;
use horizon_bingo::text::Text;
use horizon_bingo::timer::{CountdownPacket, TimerPacket};
use horizon_bingo::update_card_tiles::CardTilesPacket;

const UPDATE_CARD_TILES_CAPTURE: &str = include_str!("captures/update_card_tiles.hex");
const DISPLAY_CARDS_CAPTURE: &str = include_str!("captures/display_cards.hex");
const GAME_STATUS_CAPTURE: &str = include_str!("captures/game_status.hex");

fn unhex(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    assert!(hex.len().is_multiple_of(2), "odd hex length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

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

fn tiles_for(found: &[usize]) -> Vec<u8> {
    wire::card_tiles("bingo_red", &fixture_card(), found, &item_id, true).1
}

fn decode_tiles(bytes: &[u8]) -> CardTilesPacket {
    decode(bytes, CardTilesPacket::read).expect("the payload decodes and every byte is used")
}

fn lore_extra(tile: &CardTile) -> &[Nbt] {
    match tile.lore[0].0.get("extra") {
        Some(Nbt::List { items, .. }) => items,
        other => panic!("lore has no extra list: {other:?}"),
    }
}

fn action_kind(action: &CardTileAction) -> &'static str {
    match action {
        CardTileAction::Item(_) => "item",
        CardTileAction::Advancement(_) => "advancement",
        CardTileAction::None => "none",
    }
}

#[test]
fn every_builder_uses_a_server_to_client_channel_of_the_mod() {
    let cases: [(&str, &str); 10] = [
        (wire::game_status(None, None).0, GameStatusPacket::ID),
        (wire::timer(0).0, TimerPacket::ID),
        (wire::countdown(0).0, CountdownPacket::ID),
        (wire::card_reset().0, CardResetPacket::ID),
        (wire::game_reset().0, GameResetPacket::ID),
        (wire::open_card(None).0, CardOpenPacket::ID),
        (wire::score_update(false, "bingo_red", TeamScore::default()).0, ScoredItemPacket::ID),
        (wire::display_cards("bingo_red", "RED", "Red", &[]).0, DISPLAY_ID),
        (wire::card_tiles("bingo_red", &fixture_card(), &[], &item_id, false).0, CardTilesPacket::ID),
        (wire::game_over_v2(None, false, 0, vec![]).0, GAME_OVER_V2_ID),
    ];
    for (path, id) in cases {
        assert_eq!(channels::id(path), id, "channel of {path}");
        let channel = CHANNELS.iter().find(|c| c.path == path).expect("the path is listed in channels.rs");
        assert_eq!(channel.dir, Dir::S2c, "direction of {path}");
    }
}

#[test]
fn game_status_matches_the_real_capture_and_writes_absent_fields_as_flag_zero() {
    // The real capture: in-game 59.59 s (0xE8C6 ms), no remaining time.
    let (path, bytes) = wire::game_status(Some(59_590), None);
    assert_eq!(path, "game_status");
    assert_eq!(bytes, [0x01, 0, 0, 0, 0, 0, 0, 0xE8, 0xC6, 0x00]);
    assert_eq!(bytes, unhex(GAME_STATUS_CAPTURE));

    assert_eq!(wire::game_status(None, None).1, [0x00, 0x00]);
    // Only the second field present: flag 0, flag 1, then 124 ms as a big-endian long.
    assert_eq!(wire::game_status(None, Some(124)).1, [0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0x7C]);
}

#[test]
fn game_status_splits_negative_and_fractional_milliseconds_like_java_duration() {
    // -1 ms: Duration.ofMillis(-1) is -1 s plus 999 ms, and toMillis gives -1 again.
    // 1500 ms: 1 s plus 500 ms, written back as 1500.
    let bytes = wire::game_status(Some(-1), Some(1_500)).1;
    assert_eq!(
        bytes,
        [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0, 0, 0, 0, 0, 0, 0x05, 0xDC]
    );
    let packet = decode(&bytes, GameStatusPacket::read).expect("decodes");
    assert_eq!(packet.ingame_duration, Some(JavaDuration { seconds: -1, nanos: 999_000_000 }));
    assert_eq!(packet.remaining_duration, Some(JavaDuration { seconds: 1, nanos: 500_000_000 }));
}

#[test]
fn timer_and_countdown_are_one_big_endian_int() {
    assert_eq!(wire::timer(60), ("timer", vec![0, 0, 0, 0x3C]));
    assert_eq!(wire::countdown(5), ("countdown", vec![0, 0, 0, 5]));
    assert_eq!(wire::timer(-1).1, [0xFF; 4]);
    assert_eq!(decode(&wire::timer(-1).1, TimerPacket::read).unwrap().seconds_remaining, -1);
    assert_eq!(decode(&wire::countdown(5).1, CountdownPacket::read).unwrap().seconds_remaining, 5);
}

#[test]
fn reset_packets_have_no_payload() {
    assert_eq!(wire::card_reset(), ("card_reset", vec![]));
    assert_eq!(wire::game_reset(), ("game_reset", vec![]));
}

#[test]
fn open_card_writes_a_presence_flag_then_the_team_string() {
    let mut expected = vec![1, 9];
    expected.extend_from_slice(b"bingo_red");
    assert_eq!(wire::open_card(Some("bingo_red")), ("open_card", expected));
    assert_eq!(wire::open_card(None).1, [0x00]);
    let packet = decode(&wire::open_card(Some("bingo_red")).1, CardOpenPacket::read).unwrap();
    assert_eq!(packet.team_key.as_deref(), Some("bingo_red"));
    assert_eq!(decode(&wire::open_card(None).1, CardOpenPacket::read).unwrap().team_key, None);
}

#[test]
fn score_update_writes_viewer_flag_team_and_three_scores() {
    let (path, bytes) = wire::score_update(true, "bingo_red", TeamScore { items: 3, lines: 1, cards: 0 });
    assert_eq!(path, "score_update");
    let mut expected = vec![1, 9];
    expected.extend_from_slice(b"bingo_red");
    for score in [3i32, 1, 0] {
        expected.extend_from_slice(&score.to_be_bytes());
    }
    assert_eq!(bytes, expected);

    let (_, other) = wire::score_update(false, "bingo_blue", TeamScore { items: 0, lines: 0, cards: 2 });
    let packet = decode(&other, ScoredItemPacket::read).unwrap();
    assert!(!packet.is_viewer_on_team);
    assert_eq!(packet.team, "bingo_blue");
    assert_eq!(packet.score, TeamScore { items: 0, lines: 0, cards: 2 });
}

#[test]
fn display_cards_starts_like_the_real_capture_and_writes_uuids_as_text() {
    let players = [
        Player { uuid: 0xee30_5221_9b0e_3305_b140_17ed_b555_b9fb, name: "RegTest_control".into() },
        Player { uuid: 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff, name: "Alex".into() },
    ];
    let (path, bytes) = wire::display_cards("bingo_red", "RED", "Red", &players);
    assert_eq!(path, "display_cards");

    // Entry count 1, key "bingo_red", colour "RED": the first 20 bytes of the real capture.
    let capture = unhex(DISPLAY_CARDS_CAPTURE);
    assert_eq!(bytes[..20], capture[..20]);

    assert!(bytes.windows(36).any(|w| w == b"00112233-4455-6677-8899-aabbccddeeff"));

    let mut reader = Reader::new(&bytes);
    let packet = CardDisplayPacket::read(&mut reader).expect("decodes");
    assert_eq!(reader.remaining(), 0, "every byte is used");
    assert_eq!(packet.display.len(), 1);
    let (key, display) = &packet.display[0];
    assert_eq!(key.as_deref(), Some("bingo_red"));
    assert_eq!(display.team_color.as_deref(), Some("RED"));
    assert_eq!(display.team_name.as_ref().and_then(Text::as_plain), Some("Red"));
    assert_eq!(
        display.players,
        vec![
            PlayerProfile { uuid: 0xee30_5221_9b0e_3305_b140_17ed_b555_b9fb, name: "RegTest_control".into() },
            PlayerProfile { uuid: 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff, name: "Alex".into() },
        ]
    );
}

#[test]
fn display_cards_with_no_players_writes_a_zero_count_at_the_end() {
    let (_, bytes) = wire::display_cards("bingo_blue", "BLUE", "Blue", &[]);
    assert_eq!(bytes[bytes.len() - 4..], [0, 0, 0, 0]);
    let packet = decode(&bytes, CardDisplayPacket::read).expect("decodes");
    assert!(packet.display[0].1.players.is_empty());
}

#[test]
fn card_tiles_header_and_shape_match_the_real_capture() {
    let capture = unhex(UPDATE_CARD_TILES_CAPTURE);
    let ours = tiles_for(&[0, 7, 24]);

    // Team present, "bingo_red", 25 tiles, first index 0: the first 19 bytes of the real capture.
    assert_eq!(ours[..19], capture[..19]);

    let real = decode_tiles(&capture);
    let packet = decode_tiles(&ours);
    assert_eq!(packet.team_key, real.team_key);
    assert_eq!(packet.tiles.len(), 25);
    let indices: Vec<i32> = packet.tiles.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, (0..25).collect::<Vec<i32>>());
    assert!(packet.should_notify);

    // An unfound tile has the same fields, of the same kinds, as an unfound tile of the real capture.
    let (_, real_tile) = &real.tiles[1];
    let (_, tile) = &packet.tiles[1];
    assert_eq!(tile.id.is_some(), real_tile.id.is_some());
    assert_eq!(tile.image.item.is_some(), real_tile.image.item.is_some());
    assert_eq!(action_kind(&tile.action), action_kind(&real_tile.action));
    assert_eq!(tile.item_tier.is_some(), real_tile.item_tier.is_some());
    assert_eq!(tile.name.is_some(), real_tile.name.is_some());
    assert_eq!(tile.lore.len(), real_tile.lore.len());
    assert_eq!(lore_extra(tile).len(), lore_extra(real_tile).len(), "lore parts");
    assert_eq!(tile.image.texture, real_tile.image.texture);
    assert_eq!(tile.image.decoration, real_tile.image.decoration);
    assert_eq!(tile.image_list.len(), real_tile.image_list.len());
    assert_eq!(tile.progress, real_tile.progress);
    assert!(tile.team_keys.is_empty() && real_tile.team_keys.is_empty());
}

#[test]
fn found_squares_get_full_progress_the_achieved_mark_and_the_team_key() {
    // Index 7 is listed twice and index 30 is off the card: the card still has 25 tiles, and 7 counts once.
    let packet = decode_tiles(&tiles_for(&[0, 7, 24, 7, 30]));
    assert_eq!(packet.tiles.len(), 25);
    for (index, tile) in &packet.tiles {
        if [0, 7, 24].contains(index) {
            assert_eq!(tile.progress, 1.0, "progress of found square {index}");
            assert!(tile.image.is_achieved, "achieved mark of square {index}");
            assert_eq!(tile.team_keys, ["bingo_red"], "team of square {index}");
        } else {
            assert_eq!(tile.progress, 0.0, "progress of square {index}");
            assert!(!tile.image.is_achieved, "achieved mark of square {index}");
            assert!(tile.team_keys.is_empty(), "team keys of square {index}");
        }
    }
}

#[test]
fn no_found_squares_and_should_notify_false_are_written_as_such() {
    let (_, bytes) = wire::card_tiles("bingo_red", &fixture_card(), &[], &item_id, false);
    let packet = decode_tiles(&bytes);
    assert!(!packet.should_notify);
    assert!(packet.tiles.iter().all(|(_, t)| t.progress == 0.0 && !t.image.is_achieved && t.team_keys.is_empty()));
}

#[test]
fn a_tile_carries_its_squares_item_tier_and_a_one_item_stack() {
    let packet = decode_tiles(&tiles_for(&[]));
    let (_, tile) = &packet.tiles[7];
    assert_eq!(tile.id.as_deref(), Some("minecraft:lapis_lazuli"));
    assert_eq!(tile.item_tier, Some(TierLabel::B), "square 7 is the third tier of the cycle");
    let stack = ItemStack { count: 1, item: 1007, added: Vec::new(), removed: Vec::new() };
    assert_eq!(tile.image.item, Some(stack.clone()));
    assert_eq!(tile.action, CardTileAction::Item(stack));
    assert!(tile.image_list.is_empty());
    assert!(!tile.is_hidden && !tile.is_locked && !tile.is_flashing);

    assert_eq!(packet.tiles[24].1.item_tier, Some(TierLabel::D));
    assert_eq!(packet.tiles[0].1.item_tier, Some(TierLabel::S));
}

#[test]
fn lore_gives_the_tier_colour_and_key_the_list_and_the_coordinate() {
    let packet = decode_tiles(&tiles_for(&[]));

    // Square 7 is column 2 (N), row 2: "n2", tier B (gold).
    let extra = lore_extra(&packet.tiles[7].1);
    assert_eq!(extra[0].get("color"), Some(&Nbt::string("gold")));
    assert_eq!(extra[0].get("translate"), Some(&Nbt::string("bingo.tier.b")));
    assert_eq!(extra[2].get("translate"), Some(&Nbt::string("bingo.list.items")));
    assert_eq!(extra[4].get("text"), Some(&Nbt::string("n2")));

    // Square 24 is column 4 (O), row 5: "o5", tier D (green).
    let extra = lore_extra(&packet.tiles[24].1);
    assert_eq!(extra[0].get("color"), Some(&Nbt::string("green")));
    assert_eq!(extra[0].get("translate"), Some(&Nbt::string("bingo.tier.d")));
    assert_eq!(extra[4].get("text"), Some(&Nbt::string("o5")));

    // Square 0 is B1, in the first tier (dark red).
    let extra = lore_extra(&packet.tiles[0].1);
    assert_eq!(extra[0].get("color"), Some(&Nbt::string("dark_red")));
    assert_eq!(extra[4].get("text"), Some(&Nbt::string("b1")));
}

#[test]
fn name_is_the_items_translation_key_with_its_path_as_fallback() {
    let packet = decode_tiles(&tiles_for(&[]));
    let name = packet.tiles[0].1.name.clone().expect("a name");
    assert_eq!(name.0.get("translate"), Some(&Nbt::string("item.minecraft.oak_log")));
    assert_eq!(name.0.get("fallback"), Some(&Nbt::string("oak_log")));
}

#[test]
fn a_square_whose_item_is_unknown_keeps_its_tile_without_a_picture() {
    let mut card = fixture_card();
    card.squares[3].item = "minecraft:missing_thing".to_string();
    let (_, bytes) = wire::card_tiles("bingo_red", &card, &[3], &item_id, false);
    let packet = decode_tiles(&bytes);
    let (_, tile) = &packet.tiles[3];
    assert_eq!(tile.id.as_deref(), Some("minecraft:missing_thing"));
    assert_eq!(tile.image.item, None);
    assert_eq!(tile.action, CardTileAction::None);
    assert_eq!(tile.item_tier, Some(TierLabel::C), "square 3 is the fourth tier of the cycle");
    assert!(tile.name.is_some());
    assert_eq!(tile.lore.len(), 1);
    // The unknown square is still found, so it still has its mark.
    assert_eq!(tile.progress, 1.0);
    assert!(tile.image.is_achieved);
}

#[test]
fn game_over_v2_carries_the_winner_the_viewers_result_and_the_ranking() {
    let scores = vec![
        ScoreRanking {
            index: 0,
            key: "bingo_red".into(),
            name: Text::plain("Red"),
            score: TeamScore { items: 3, lines: 1, cards: 0 },
            duration: None,
        },
        ScoreRanking {
            index: 1,
            key: "bingo_blue".into(),
            name: Text::plain("Blue"),
            score: TeamScore { items: 1, lines: 0, cards: 0 },
            duration: Some("PT1M30S".into()),
        },
    ];
    let (path, bytes) = wire::game_over_v2(Some("bingo_red"), true, 83_500, scores.clone());
    assert_eq!(path, "game_over_v2");
    let packet = decode(&bytes, GameOverPacket::read_v2).expect("decodes");
    assert_eq!(packet.winner.as_deref(), Some("bingo_red"));
    assert!(packet.is_winner);
    assert_eq!(packet.duration.to_millis_or_zero(), 83_500, "the packet carries the game time");
    assert!(!packet.is_update, "the client opens the end screen");
    assert_eq!(packet.scores, scores);
    assert_eq!(packet.default_tab, EndScreenTab::Cards);
    assert_eq!(packet.resume_description, None);
}

#[test]
fn game_over_v2_without_a_winner_has_no_winner_and_no_scores() {
    let (_, bytes) = wire::game_over_v2(None, false, 0, vec![]);
    let packet = decode(&bytes, GameOverPacket::read_v2).expect("decodes");
    assert_eq!(packet.winner, None);
    assert!(!packet.is_winner);
    assert!(packet.scores.is_empty());
}

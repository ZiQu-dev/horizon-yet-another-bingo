//! update_card_tiles and its nested converters against the real mod. Every row of tests/oracle.tsv was written by
//! the mod's own converters (oracle/OracleUpdateCardTiles.java, through RegBuf). Each row is written here and must
//! match byte for byte, and the oracle bytes must read back to the value built below from the Kotlin inputs.
use horizon_bingo::buf::{decode, encode};
use horizon_bingo::item::{component, ComponentValue, ItemStack};
use horizon_bingo::nbt::Nbt;
use horizon_bingo::text::Text;
use horizon_bingo::card_tile::{self, CardTile, TierLabel};
use horizon_bingo::card_tile_action::{self, CardTileAction};
use horizon_bingo::card_tile_image::{self, CardTileImage, Decoration, ItemStyle};
use horizon_bingo::update_card_tiles::CardTilesPacket;

const TSV: &str = include_str!("oracle/update_card_tiles.tsv");

/// The registry id of minecraft:stone (from tests/oracle/component_types.tsv's sibling item ids, the same run).
const STONE: i32 = 1;

enum Case {
    Packet(CardTilesPacket),
    Tile(CardTile),
    Image(CardTileImage),
    Action(CardTileAction),
}

fn s(v: &str) -> Nbt {
    Nbt::string(v)
}

fn plain(v: &str) -> Text {
    Text::plain(v)
}

fn styled(entries: Vec<(&str, Nbt)>) -> Text {
    Text(Nbt::compound(entries))
}

fn stone(count: i32) -> ItemStack {
    ItemStack { count, item: STONE, added: vec![], removed: vec![] }
}

/// `stone x count` with a custom name "N" and two lore lines: "L1", and "L2" in italics.
fn lore_stone(count: i32) -> ItemStack {
    ItemStack {
        count,
        item: STONE,
        added: vec![
            (component::CUSTOM_NAME, ComponentValue::Text(plain("N"))),
            (
                component::LORE,
                ComponentValue::TextList(vec![plain("L1"), styled(vec![("text", s("L2")), ("italic", Nbt::Byte(1))])]),
            ),
        ],
        removed: vec![],
    }
}

fn image(item: Option<ItemStack>, texture: Option<&str>, style: ItemStyle, decoration: Decoration, deco_texture: Option<&str>, achieved: bool) -> CardTileImage {
    CardTileImage {
        item,
        texture: texture.map(str::to_owned),
        item_style: style,
        decoration,
        decoration_texture: deco_texture.map(str::to_owned),
        is_achieved: achieved,
    }
}

fn minimal_image() -> CardTileImage {
    image(None, None, ItemStyle::Normal, Decoration::None, None, false)
}

fn stone_image() -> CardTileImage {
    image(Some(stone(1)), None, ItemStyle::Normal, Decoration::None, None, false)
}

fn minimal_tile() -> CardTile {
    CardTile {
        id: None,
        image: minimal_image(),
        image_list: vec![],
        action: CardTileAction::None,
        item_tier: None,
        name: None,
        lore: vec![],
        is_hidden: false,
        is_locked: false,
        is_flashing: false,
        progress: 0.0,
        team_keys: vec![],
    }
}

fn tier_tile(tier: TierLabel) -> CardTile {
    CardTile {
        id: Some("minecraft:stone".into()),
        image: stone_image(),
        item_tier: Some(tier),
        name: Some(plain(&format!("tier {}", tier.name()))),
        ..minimal_tile()
    }
}

fn full_tile() -> CardTile {
    CardTile {
        id: Some("minecraft:stone".into()),
        image: image(Some(lore_stone(3)), Some("tex/stone"), ItemStyle::OneOf, Decoration::Forbidden, Some("deco/forbidden"), true),
        image_list: vec![
            image(Some(stone(1)), None, ItemStyle::AllOf, Decoration::FreeSpace, None, false),
            image(None, Some("tex/b"), ItemStyle::Normal, Decoration::MultiItem, None, true),
            image(Some(lore_stone(2)), None, ItemStyle::OneOf, Decoration::Advancement, Some("deco/c"), false),
        ],
        action: CardTileAction::Advancement("minecraft:story/root".into()),
        item_tier: Some(TierLabel::S),
        name: Some(styled(vec![("color", s("yellow")), ("text", s("Stone")), ("bold", Nbt::Byte(1))])),
        lore: vec![
            plain("plain line"),
            styled(vec![("color", s("#FF8800")), ("text", s("hex hi")), ("bold", Nbt::Byte(1))]),
            plain("żółw 日本 😀"),
        ],
        is_hidden: true,
        is_locked: true,
        is_flashing: true,
        progress: 0.75,
        team_keys: vec!["bingo_red".into(), "bingo_blue".into()],
    }
}

fn case(name: &str) -> Option<Case> {
    Some(match name {
        // Converter rows.
        "card_tile_minimal" => Case::Tile(minimal_tile()),
        "card_tile_image_minimal" => Case::Image(minimal_image()),
        "card_tile_image_item_stone_1" => Case::Image(image(Some(stone(1)), None, ItemStyle::Normal, Decoration::None, None, false)),
        "card_tile_image_item_stone_64" => Case::Image(image(Some(stone(64)), None, ItemStyle::Normal, Decoration::None, None, false)),
        "card_tile_image_item_count_200" => Case::Image(image(Some(stone(200)), None, ItemStyle::Normal, Decoration::None, None, false)),
        "card_tile_image_item_lore_name_3" => Case::Image(image(Some(lore_stone(3)), None, ItemStyle::Normal, Decoration::None, None, false)),
        "card_tile_image_texture_non_ascii" => Case::Image(image(None, Some("textures/żółw_🙂.png"), ItemStyle::Normal, Decoration::None, Some("deco/x"), true)),
        // mapTexture is never on the wire: the oracle wrote the same bytes as the minimal image.
        "card_tile_image_map_texture_not_written" => Case::Image(minimal_image()),
        "card_tile_image_style_normal" => Case::Image(image(None, None, ItemStyle::Normal, Decoration::None, None, false)),
        "card_tile_image_style_one_of" => Case::Image(image(None, None, ItemStyle::OneOf, Decoration::None, None, false)),
        "card_tile_image_style_all_of" => Case::Image(image(None, None, ItemStyle::AllOf, Decoration::None, None, false)),
        "card_tile_image_deco_none" => Case::Image(image(None, None, ItemStyle::Normal, Decoration::None, None, false)),
        "card_tile_image_deco_advancement" => Case::Image(image(None, None, ItemStyle::Normal, Decoration::Advancement, Some("d"), false)),
        "card_tile_image_deco_multi_item" => Case::Image(image(None, None, ItemStyle::Normal, Decoration::MultiItem, None, false)),
        "card_tile_image_deco_forbidden" => Case::Image(image(None, None, ItemStyle::Normal, Decoration::Forbidden, None, false)),
        "card_tile_image_deco_free_space" => Case::Image(image(None, None, ItemStyle::Normal, Decoration::FreeSpace, None, false)),
        "card_tile_image_all_fields" => Case::Image(image(Some(lore_stone(2)), Some("tex/a"), ItemStyle::AllOf, Decoration::FreeSpace, Some("deco/a"), true)),
        "card_tile_action_none" => Case::Action(CardTileAction::None),
        "card_tile_action_advancement" => Case::Action(CardTileAction::Advancement("minecraft:story/mine_stone".into())),
        "card_tile_action_advancement_empty" => Case::Action(CardTileAction::Advancement(String::new())),
        "card_tile_action_item" => Case::Action(CardTileAction::Item(stone(1))),
        "card_tile_action_item_lore_name" => Case::Action(CardTileAction::Item(lore_stone(5))),
        "card_tile_tier_s" => Case::Tile(tier_tile(TierLabel::S)),
        "card_tile_tier_a" => Case::Tile(tier_tile(TierLabel::A)),
        "card_tile_tier_b" => Case::Tile(tier_tile(TierLabel::B)),
        "card_tile_tier_c" => Case::Tile(tier_tile(TierLabel::C)),
        "card_tile_tier_d" => Case::Tile(tier_tile(TierLabel::D)),
        "card_tile_full" => Case::Tile(full_tile()),
        "card_tile_image_list_3_flashing_null" => Case::Tile(CardTile {
            id: Some("minecraft:stone".into()),
            image: stone_image(),
            image_list: vec![stone_image(), stone_image(), stone_image()],
            ..minimal_tile()
        }),
        "card_tile_progress_extremes" => Case::Tile(CardTile {
            image: minimal_image(),
            action: CardTileAction::Item(stone(3)),
            item_tier: Some(TierLabel::D),
            name: Some(plain("x")),
            is_hidden: true,
            is_locked: true,
            is_flashing: true,
            progress: f32::MAX,
            team_keys: vec!["bingo_red".into()],
            ..minimal_tile()
        }),
        "card_tile_progress_negative_zero" => Case::Tile(CardTile { id: Some("a".into()), progress: -0.0, ..minimal_tile() }),
        "card_tile_progress_fraction" => Case::Tile(CardTile { id: Some("a".into()), progress: 0.75, ..minimal_tile() }),

        // Packet rows.
        "update_tiles_empty" => Case::Packet(CardTilesPacket { team_key: None, tiles: vec![], should_notify: false }),
        "update_tiles_one_minimal_team" => Case::Packet(CardTilesPacket {
            team_key: Some("bingo_red".into()),
            tiles: vec![(0, minimal_tile())],
            should_notify: true,
        }),
        "update_tiles_three_insertion_order" => Case::Packet(CardTilesPacket {
            team_key: Some("bingo_blue".into()),
            tiles: vec![(5, tier_tile(TierLabel::S)), (-1, full_tile()), (i32::MAX, minimal_tile())],
            should_notify: true,
        }),
        "update_tiles_index_extremes_no_team" => Case::Packet(CardTilesPacket {
            team_key: None,
            tiles: vec![(i32::MIN, minimal_tile())],
            should_notify: false,
        }),
        "update_tiles_non_ascii_team" => Case::Packet(CardTilesPacket {
            team_key: Some("bingo_żółw🙂".into()),
            tiles: vec![(7, minimal_tile())],
            should_notify: false,
        }),
        "update_tiles_21_notify_default_false" => Case::Packet(CardTilesPacket {
            team_key: Some("bingo_red".into()),
            tiles: (0..21).map(|i| (i, minimal_tile())).collect(),
            should_notify: false,
        }),
        _ => return None,
    })
}

fn bytes_of(hex: &str) -> Vec<u8> {
    assert!(hex.len().is_multiple_of(2), "odd hex length in {hex:?}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn every_oracle_row_matches_the_mod_byte_for_byte_and_reads_back() {
    let mut rows = 0;
    for line in TSV.lines().filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "malformed oracle row {line:?}");
        let (name, channel, hex) = (fields[0], fields[1], fields[2]);
        let bytes = bytes_of(hex);
        let case = case(name).unwrap_or_else(|| panic!("no Rust case for oracle row {name}"));
        match case {
            Case::Packet(value) => {
                assert_eq!(channel, CardTilesPacket::ID, "{name}: channel id");
                assert_eq!(hex_of(&encode(|w| value.write(w).unwrap())), hex, "{name}: written bytes");
                assert_eq!(decode(&bytes, CardTilesPacket::read), Ok(value), "{name}: read back");
            }
            Case::Tile(value) => {
                assert_eq!(channel, card_tile::ID, "{name}: channel id");
                assert_eq!(hex_of(&encode(|w| value.write(w).unwrap())), hex, "{name}: written bytes");
                assert_eq!(decode(&bytes, CardTile::read), Ok(value), "{name}: read back");
            }
            Case::Image(value) => {
                assert_eq!(channel, card_tile_image::ID, "{name}: channel id");
                assert_eq!(hex_of(&encode(|w| value.write(w).unwrap())), hex, "{name}: written bytes");
                assert_eq!(decode(&bytes, CardTileImage::read), Ok(value), "{name}: read back");
            }
            Case::Action(value) => {
                assert_eq!(channel, card_tile_action::ID, "{name}: channel id");
                assert_eq!(hex_of(&encode(|w| value.write(w).unwrap())), hex, "{name}: written bytes");
                assert_eq!(decode(&bytes, CardTileAction::read), Ok(value), "{name}: read back");
            }
        }
        rows += 1;
    }
    assert_eq!(rows, 38, "the oracle file has a different number of rows than the cases");
}

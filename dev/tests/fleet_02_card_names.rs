//! Names of the advanced objective squares (`horizon_bingo::server::objective_names`, drawn by `objectives`), against
//! the original's lang strings (en_us.json) and the vanilla item and stat names.
use std::collections::HashMap;

use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::nbt::Nbt;
use horizon_bingo::server::objective_names::format_title;
use horizon_bingo::server::objectives::{card_from_objectives, Objective, Square};
use horizon_bingo::text::Text;
use horizon_bingo::text_plain::flatten;
use horizon_bingo::update_card_tiles::CardTilesPacket;

const APPLE: &str = "minecraft:apple";
const COAL: &str = "minecraft:coal";
const FLINT: &str = "minecraft:flint_and_steel";
const MINED_STONE_TYPE: &str = "minecraft:mined";
const STONE: &str = "minecraft:stone";

/// The lang the names read through: the original's strings and vanilla's item names.
fn lang(key: &str) -> Option<String> {
    let text = match key {
        "bingo.objective.inverse" => "Never get %s",
        "bingo.objective.one_of" => "One of: %s",
        "bingo.objective.all_of" => "All of: %s",
        "item.minecraft.flint_and_steel" => "Flint and Steel",
        "item.minecraft.apple" => "Apple",
        "item.minecraft.coal" => "Coal",
        _ => return None,
    };
    Some(text.to_string())
}

/// The name of square `index` of a board built from `squares`, as the client receives it.
fn tile_name(squares: &[Square], index: usize) -> Text {
    let item_id = |_: &str| Some(1);
    let (_, bytes) = card_from_objectives("bingo_red", squares, &[], &item_id, &HashMap::new(), false).unwrap();
    let packet: CardTilesPacket = decode(&bytes, CardTilesPacket::read).unwrap();
    packet.tiles[index].1.name.clone().expect("every tile has a name")
}

fn board(objective: Objective) -> Vec<Square> {
    let mut squares: Vec<Square> = (0..25)
        .map(|_| Square { objective: Objective::Item { item: COAL.into() }, tier: TierLabel::D })
        .collect();
    squares[0] = Square { objective, tier: TierLabel::A };
    squares
}

fn is_italic(text: &Text) -> bool {
    text.0.get("italic") == Some(&Nbt::Byte(1))
}

#[test]
fn inverse_name_reads_never_get_with_the_item_name_in_italics() {
    let squares = board(Objective::Inverse {
        of: Box::new(Objective::Item { item: FLINT.into() }),
        permanent: true,
    });
    let name = tile_name(&squares, 0);
    assert_eq!(flatten(&name, &lang), "Never get Flint and Steel");
    assert!(is_italic(&name));
}

#[test]
fn one_of_name_reads_the_first_entry_then_an_ellipsis() {
    let squares = board(Objective::OneOf { entries: vec![APPLE.into(), COAL.into()], permanent: false });
    let name = tile_name(&squares, 0);
    assert_eq!(flatten(&name, &lang), "One of: Apple, \u{2026}");
    assert!(is_italic(&name));
}

#[test]
fn all_of_name_uses_its_own_lang_string() {
    let squares = board(Objective::AllOf { entries: vec![COAL.into(), APPLE.into()], permanent: false });
    assert_eq!(flatten(&tile_name(&squares, 0), &lang), "All of: Coal, \u{2026}");
}

#[test]
fn stats_name_is_the_stat_title_and_the_minimum() {
    let squares = board(Objective::Stats {
        stat_type: MINED_STONE_TYPE.into(),
        stat_name: Some(STONE.into()),
        min: 1,
        max: i64::MAX,
        relative: true,
        permanent: false,
    });
    let name = tile_name(&squares, 0);
    assert_eq!(flatten(&name, &lang), "Mined Stone: 1");
    assert!(is_italic(&name));
}

#[test]
fn item_square_name_stays_upright() {
    let squares: Vec<Square> = (0..25)
        .map(|_| Square { objective: Objective::Item { item: APPLE.into() }, tier: TierLabel::D })
        .collect();
    let name = tile_name(&squares, 0);
    assert_eq!(flatten(&name, &lang), "Apple");
    assert!(!is_italic(&name));
}

#[test]
fn format_title_matches_the_original() {
    assert_eq!(format_title("mined"), "Mined");
    assert_eq!(format_title("flint_and_steel"), "Flint And Steel");
    assert_eq!(format_title("someThing_x"), "Some Thing X");
}

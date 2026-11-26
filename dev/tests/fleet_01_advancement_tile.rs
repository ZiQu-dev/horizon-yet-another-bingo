//! An advancement square reads as the original's: the advancement's translated title in green, then its description,
//! then the tier, list and coordinate line (AdvancementObjectiveManager.kt, BingoObjectiveManager.kt). The expected keys
//! are the vanilla language keys: `advancements.story.mine_stone.title` for `minecraft:story/mine_stone`.
use horizon_bingo::buf::decode;
use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::nbt::Nbt;
use horizon_bingo::server::objectives::{card_from_objectives, Objective, Square, Tags};
use horizon_bingo::update_card_tiles::CardTilesPacket;

const IRON: &str = "minecraft:iron_ingot";
const MINE_STONE_ADV: &str = "minecraft:story/mine_stone";

#[test]
fn advancement_square_shows_title_then_description_then_tier_line() {
    let mut squares: Vec<Square> = (0..25)
        .map(|_| Square { objective: Objective::Item { item: IRON.into() }, tier: TierLabel::D })
        .collect();
    squares[2] = Square {
        objective: Objective::Advancement { id: MINE_STONE_ADV.into(), permanent: false },
        tier: TierLabel::B,
    };
    let item_id = |_: &str| None;
    let tags = Tags::new();
    let (_, bytes) = card_from_objectives("bingo_red", &squares, &[], &item_id, &tags, true).unwrap();
    let packet: CardTilesPacket = decode(&bytes, CardTilesPacket::read).unwrap();
    let tile = &packet.tiles[2].1;

    let name = tile.name.as_ref().expect("an advancement square has a name");
    assert_eq!(name.0.get("translate"), Some(&Nbt::string("advancements.story.mine_stone.title")));
    assert_eq!(name.0.get("color"), Some(&Nbt::string("green")));

    assert_eq!(tile.lore.len(), 2, "description, then the tier line");
    assert_eq!(
        tile.lore[0].0.get("translate"),
        Some(&Nbt::string("advancements.story.mine_stone.description"))
    );
    let tier_line = &tile.lore[1].0;
    let extra = match tier_line.get("extra") {
        Some(Nbt::List { items, .. }) => items.clone(),
        other => panic!("tier line should carry extra parts, got {other:?}"),
    };
    assert_eq!(extra[0].get("translate"), Some(&Nbt::string("bingo.tier.b")));
}

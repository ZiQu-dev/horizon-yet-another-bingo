//! Forty `update_card_tiles` payloads from eight real games (tests/captures/live3, taken from the live server with
//! the bot): random cards, so banners, shields and every other item the mod puts on a card.
use horizon_bingo::buf::{decode, encode, DecodeError, Reader};
use horizon_bingo::channels::id;
use horizon_bingo::item::{component, ComponentValue};
use horizon_bingo::state::HudState;
use horizon_bingo::update_card_tiles::CardTilesPacket;

fn payloads() -> Vec<Vec<u8>> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/captures/live3");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files
        .into_iter()
        .map(|f| {
            let t = std::fs::read_to_string(f).unwrap();
            let t = t.trim();
            (0..t.len() / 2).map(|i| u8::from_str_radix(&t[2 * i..2 * i + 2], 16).unwrap()).collect()
        })
        .collect()
}

#[test]
fn every_real_card_decodes_completely_and_encodes_back_to_the_same_bytes() {
    let all = payloads();
    assert_eq!(all.len(), 40);
    for (i, bytes) in all.iter().enumerate() {
        let p = decode(bytes, CardTilesPacket::read).unwrap_or_else(|e| panic!("payload {i}: {e}"));
        assert_eq!(p.tiles.len(), 25, "payload {i}");
        assert_eq!(encode(|w| p.write(w).unwrap()), *bytes, "payload {i}");
    }
}

#[test]
fn the_recording_really_contains_banner_patterns_and_base_colours() {
    // Otherwise this test file would not cover what it was added for.
    let mut seen = std::collections::BTreeSet::new();
    for bytes in payloads() {
        let p = decode(&bytes, CardTilesPacket::read).unwrap();
        for (_, tile) in &p.tiles {
            let mut stacks: Vec<_> = tile.image.item.iter().collect();
            stacks.extend(tile.image_list.iter().filter_map(|i| i.item.as_ref()));
            for s in stacks {
                for (cid, v) in &s.added {
                    if let ComponentValue::BannerPatterns(_) = v {
                        assert_eq!(*cid, component::BANNER_PATTERNS);
                    }
                    seen.insert(*cid);
                }
            }
        }
    }
    assert!(seen.contains(&component::BANNER_PATTERNS) && seen.contains(&component::BASE_COLOR), "{seen:?}");
    // and nothing but the components this crate knows (it decoded them all)
    assert!(seen.is_subset(&[6, 9, 11, 18, 74, 75].into_iter().collect()), "{seen:?}");
}

#[test]
fn a_card_cut_short_keeps_the_tiles_before_the_cut_and_reports_the_error() {
    let bytes = payloads().remove(0);
    let full = decode(&bytes, CardTilesPacket::read).unwrap();
    let cut = &bytes[..bytes.len() * 6 / 10];
    let (partial, error) = CardTilesPacket::read_salvaging(&mut Reader::new(cut)).unwrap();
    assert!(matches!(error, Some(DecodeError::UnexpectedEnd { .. })), "{error:?}");
    assert!(!partial.tiles.is_empty() && partial.tiles.len() < 25, "{} tiles", partial.tiles.len());
    assert_eq!(partial.tiles[..], full.tiles[..partial.tiles.len()], "the tiles read are the first ones, unchanged");
    assert!(!partial.should_notify, "a partly known card does not flash");
    // an intact payload is not reported as a failure
    let (whole, none) = CardTilesPacket::read_salvaging(&mut Reader::new(&bytes)).unwrap();
    assert!(none.is_none() && whole == full);
}

#[test]
fn the_state_shows_the_tiles_it_could_read_and_still_logs_the_error() {
    let bytes = payloads().remove(0);
    let cut = &bytes[..bytes.len() * 6 / 10];
    let mut state = HudState::new();
    let result = state.on_packet(&id("update_card_tiles"), cut);
    assert!(result.is_err(), "the failure is reported");
    let card = state.selected_card().expect("the card exists with the tiles that were read").1;
    let known = card.tiles.iter().filter(|t| t.id.is_some()).count();
    assert!((1..25).contains(&known), "{known} tiles");
    assert!(card.tile_updated_at.iter().all(Option::is_none), "no flashing from a partial packet");
    // nothing at all could be read (cut inside the first tile): the state is untouched
    let mut empty = HudState::new();
    assert!(empty.on_packet(&id("update_card_tiles"), &bytes[..20]).is_err());
    assert!(empty.cards.is_empty());
}

#[test]
fn banner_layers_round_trip_both_registered_and_inline_patterns() {
    use horizon_bingo::buf::Reader;
    use horizon_bingo::item::{read_item_stack, write_item_stack, BannerLayer, ItemStack, PatternRef};
    let stack = ItemStack {
        count: 1,
        item: 1446,
        added: vec![(
            component::BANNER_PATTERNS,
            ComponentValue::BannerPatterns(vec![
                BannerLayer { pattern: PatternRef::Registered(7), color: 14 },
                BannerLayer { pattern: PatternRef::Inline { asset_id: "minecraft:flower".into(), translation_key: "block.minecraft.banner.flower".into() }, color: 0 },
            ]),
        )],
        removed: vec![],
    };
    let bytes = encode(|w| write_item_stack(w, Some(&stack)).unwrap());
    // count 1, item 1446 (a6 0b), 1 added, 0 removed, component 74 (0x4a), 2 layers: holder 7, colour 14; holder 0 + 2 strings, colour 0
    assert_eq!(&bytes[..9], [1, 0xa6, 0x0b, 1, 0, 0x4a, 2, 7, 14]);
    assert_eq!(bytes[9], 0, "an inline pattern is holder 0");
    assert_eq!(read_item_stack(&mut Reader::new(&bytes)).unwrap(), Some(stack));
}

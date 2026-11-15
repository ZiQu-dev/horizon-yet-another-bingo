//! The card's map picture (server/map_render.rs). Expected bytes are hand-computed: palette entries from the vanilla
//! brightness formula (base * modifier / 255, truncated), and the nearest palette byte for each drawn colour from the
//! MapColors.kt distance, checked with a separate script. Coordinates are from MapRenderService.kt: the frame is 5
//! pixels, a tile is 22 pixels wide and starts every 24 pixels.
use std::collections::HashMap;

use horizon_bingo::card_tile::TierLabel;
use horizon_bingo::server::map_render::{
    closest_map_color, map_color_index, palette_rgb, render_card, CardView, Decoration, DirtyRect, IconSource,
    MapBuffer, MapTile, MemoryIcons, Overlays, RgbaImage, Style, FileIcons, MAP_SIZE,
};

// Hand-computed palette bytes (see the module note).
const BG: u8 = 89; // (130,130,130)
const MARGIN: u8 = 14; // (199,199,199)
const SHADOW: u8 = 117; // (20,20,20)
const ITEM_SHADOW: u8 = 238; // (100,100,100)
const ACHIEVED: u8 = 78; // (100,255,100)
const FLASH: u8 = 66; // (178,76,216)
const LOCKED: u8 = 19; // (150,0,0)
const WHITE: u8 = 34; // (255,255,255)
const RED: u8 = 18; // (255,0,0)

fn empty_card() -> CardView {
    CardView { team_color: None, tiles: vec![MapTile::default(); 25] }
}

fn solid(width: usize, height: usize, rgb: [u8; 3]) -> RgbaImage {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for _ in 0..width * height {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    RgbaImage::from_rgba(width, height, rgba)
}

fn px(buf: &MapBuffer, x: usize, y: usize) -> u8 {
    buf.pixels()[y * MAP_SIZE + x]
}

/// Icons for the tests: `minecraft:red_wool` is a solid red 16x16 picture.
fn red_icons() -> MemoryIcons {
    let mut m = HashMap::new();
    m.insert("minecraft:red_wool".to_string(), solid(16, 16, [255, 0, 0]));
    MemoryIcons(m)
}

fn item_tile(item: &str) -> MapTile {
    MapTile { item: Some(item.to_string()), ..MapTile::default() }
}

fn render(card: &CardView, icons: &dyn IconSource, overlays: &Overlays, tick: u64, buf: &mut MapBuffer) -> DirtyRect {
    render_card(card, &Style { icons, overlays }, tick, buf)
}

#[test]
fn palette_entries_are_the_shaded_base_colours() {
    // MAP_COLORS[1] = 0x7FB238 = (127,178,56), GRASS. Shades: LOW 180, NORMAL 220, HIGH 255, LOWEST 135.
    assert_eq!(palette_rgb(4), Some([89, 125, 39]));
    assert_eq!(palette_rgb(5), Some([109, 153, 48]));
    assert_eq!(palette_rgb(6), Some([127, 178, 56]));
    assert_eq!(palette_rgb(7), Some([67, 94, 29]));
    // SAND 0xF7E9A3 = (247,233,163), LOW shade.
    assert_eq!(palette_rgb(8), Some([174, 164, 115]));
    assert_eq!(palette_rgb(3), None, "ids 0..=3 are NONE");
    assert_eq!(palette_rgb(248), None);
}

#[test]
fn exact_palette_inputs_map_to_themselves() {
    assert_eq!(closest_map_color([127, 178, 56]), 6);
    assert_eq!(closest_map_color([89, 125, 39]), 4);
    assert_eq!(closest_map_color([255, 255, 255]), 34);
    assert_eq!(closest_map_color([255, 0, 0]), 18);
    // Every palette entry, and pure black, round-trip to an entry of the same colour.
    for byte in 4..=247u8 {
        let rgb = palette_rgb(byte).unwrap();
        assert_eq!(palette_rgb(closest_map_color(rgb)), Some(rgb), "byte {byte}");
    }
    assert_eq!(closest_map_color([0, 0, 0]), 116);
}

#[test]
fn drawn_colours_match_their_nearest_palette_bytes() {
    assert_eq!(closest_map_color([130, 130, 130]), BG);
    assert_eq!(closest_map_color([199, 199, 199]), MARGIN);
    assert_eq!(closest_map_color([20, 20, 20]), SHADOW);
    assert_eq!(closest_map_color([100, 100, 100]), ITEM_SHADOW);
    assert_eq!(closest_map_color([100, 255, 100]), ACHIEVED);
    assert_eq!(closest_map_color([178, 76, 216]), FLASH);
    assert_eq!(closest_map_color([150, 0, 0]), LOCKED);
}

#[test]
fn team_colour_indices_follow_get_map_color() {
    assert_eq!(map_color_index("RED"), 4);
    assert_eq!(map_color_index("GOLD"), 15);
    assert_eq!(map_color_index("GREEN"), 33);
    assert_eq!(map_color_index("GRAY"), 21);
    assert_eq!(map_color_index("DARK_GRAY"), 21);
    assert_eq!(map_color_index("BOLD"), 22, "a name the original does not list gives 22");
}

#[test]
fn border_without_a_team_uses_index_22() {
    let mut buf = MapBuffer::new();
    render(&empty_card(), &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    // Outer edge: light (22*4+2 = 90) on the top and left, dark (91) on the bottom and right.
    assert_eq!(px(&buf, 60, 0), 90);
    assert_eq!(px(&buf, 60, 127), 91);
    assert_eq!(px(&buf, 127, 127), 91);
    assert_eq!(px(&buf, 0, 0), 90);
    // Inner frame at x = 4 (dark) and x = 123 (light).
    assert_eq!(px(&buf, 4, 60), 91);
    assert_eq!(px(&buf, 60, 123), 90);
    // Grid line under row 1 (margin, y = 27) and the shadow line over row 2 (y = 28).
    assert_eq!(px(&buf, 60, 27), MARGIN);
    assert_eq!(px(&buf, 60, 28), SHADOW);
}

#[test]
fn border_takes_the_team_colour() {
    let mut buf = MapBuffer::new();
    let mut card = empty_card();
    card.team_color = Some("RED".to_string()); // index 4: light 18, mid 17, dark 19
    render(&card, &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    assert_eq!(px(&buf, 60, 0), 18);
    assert_eq!(px(&buf, 60, 127), 19);
    assert_eq!(px(&buf, 1, 60), 17, "middle band, column 1");
}

#[test]
fn empty_tile_is_the_background_and_achieved_empty_is_green() {
    let mut buf = MapBuffer::new();
    render(&empty_card(), &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    // Tile 0 spans 5..=26; tile 1 starts at 29.
    assert_eq!(px(&buf, 5, 5), BG);
    assert_eq!(px(&buf, 26, 26), BG);
    assert_eq!(px(&buf, 29, 5), BG);

    let mut card = empty_card();
    card.tiles[0].achieved = true;
    let mut buf = MapBuffer::new();
    render(&card, &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    assert_eq!(px(&buf, 5, 5), ACHIEVED);
    assert_eq!(px(&buf, 26, 26), ACHIEVED);
}

#[test]
fn item_picture_is_scaled_into_the_tile_with_a_shadow() {
    let mut card = empty_card();
    card.tiles[0] = item_tile("minecraft:red_wool");
    let mut buf = MapBuffer::new();
    render(&card, &red_icons(), &Overlays::default(), 0, &mut buf);
    // Picture pixels 0..=15 sit at tile pixels 3..=18, so map pixels 8..=23.
    assert_eq!(px(&buf, 7, 7), BG, "the padding around the picture");
    assert_eq!(px(&buf, 8, 8), RED);
    assert_eq!(px(&buf, 23, 23), RED);
    // Tile pixel 19 is outside the picture and just right of it: the item shadow.
    assert_eq!(px(&buf, 24, 24), ITEM_SHADOW);
    assert_eq!(px(&buf, 25, 25), BG);
}

#[test]
fn flashing_tile_toggles_every_ten_ticks_for_eighty_ticks() {
    // Tile index 7: row 1, column 2, so its origin is (5 + 48, 5 + 24) = (53, 29).
    let mut card = empty_card();
    card.tiles[7].flash_since = Some(100);
    let icons = MemoryIcons::default();
    let overlays = Overlays::default();
    let at = |tick: u64| {
        let mut buf = MapBuffer::new();
        render(&card, &icons, &overlays, tick, &mut buf);
        px(&buf, 53, 29)
    };
    assert_eq!(at(100), FLASH, "tick 100: 10 / 10 is even, so on");
    assert_eq!(at(110), BG, "tick 110: 11 is odd, so off");
    assert_eq!(at(120), FLASH, "tick 120: on again");
    assert_eq!(at(170), BG, "tick 170: off, still inside the 80 ticks");
    assert_eq!(at(160), FLASH, "tick 160: on, last on-half inside the flash");
    assert_eq!(at(180), BG, "tick 180 is exactly 80 ticks after the score: the flash is over");
    assert_eq!(at(99), BG, "before the score: no flash");
}

#[test]
fn dirty_rectangle_is_the_changed_tile_only() {
    let card = empty_card();
    let mut buf = MapBuffer::new();
    let icons = MemoryIcons::default();
    let overlays = Overlays::default();
    assert_eq!(render(&card, &icons, &overlays, 0, &mut buf), DirtyRect::FULL, "a fresh buffer is drawn in full");
    assert!(render(&card, &icons, &overlays, 0, &mut buf).is_empty(), "nothing changed: nothing to send");

    let mut changed = card.clone();
    changed.tiles[8].achieved = true; // row 1, column 3: origin (77, 29), 22 pixels square
    let rect = render(&changed, &icons, &overlays, 0, &mut buf);
    assert_eq!(rect, DirtyRect { x0: 77, y0: 29, x1: 99, y1: 51 });
    assert_eq!(px(&buf, 77, 29), ACHIEVED);
    assert_eq!(px(&buf, 5, 5), BG, "the other tiles are untouched");
}

#[test]
fn changing_the_team_colour_redraws_the_whole_map() {
    let mut buf = MapBuffer::new();
    let icons = MemoryIcons::default();
    let overlays = Overlays::default();
    render(&empty_card(), &icons, &overlays, 0, &mut buf);
    let mut card = empty_card();
    card.team_color = Some("BLUE".to_string());
    assert_eq!(render(&card, &icons, &overlays, 0, &mut buf), DirtyRect::FULL);
    assert_eq!(px(&buf, 60, 0), 12 * 4 + 2);
}

#[test]
fn progress_fills_a_regular_tile_from_the_bottom_in_the_flash_colour() {
    let mut card = empty_card();
    card.tiles[0].progress = 0.5; // rows with inner y > 21 * 0.5 = 10.5, that is 11 and up, are filled
    let mut buf = MapBuffer::new();
    render(&card, &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    assert_eq!(px(&buf, 5, 15), BG, "inner row 10");
    assert_eq!(px(&buf, 5, 16), FLASH, "inner row 11");
}

#[test]
fn locked_tile_has_a_red_cross() {
    let mut card = empty_card();
    card.tiles[0].locked = true;
    let mut buf = MapBuffer::new();
    render(&card, &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    assert_eq!(px(&buf, 5, 5), LOCKED, "diagonal start");
    assert_eq!(px(&buf, 15, 15), LOCKED, "diagonal at inner 10");
    assert_eq!(px(&buf, 14, 15), LOCKED, "the cross is two pixels thick");
    assert_eq!(px(&buf, 5, 26), LOCKED, "the other diagonal, inner (0, 21)");
    assert_eq!(px(&buf, 7, 5), BG, "off the cross, inner (2, 0)");
}

#[test]
fn team_stripes_run_along_the_top_and_left_edges() {
    let mut card = empty_card();
    card.tiles[0].achieved_by = vec!["BLUE".to_string()]; // map colour 12: even pixels 12*4+2, odd 12*4+3
    let mut buf = MapBuffer::new();
    render(&card, &MemoryIcons::default(), &Overlays::default(), 0, &mut buf);
    assert_eq!(px(&buf, 5, 5), 50);
    assert_eq!(px(&buf, 6, 5), 51);
    assert_eq!(px(&buf, 7, 5), 50);
    assert_eq!(px(&buf, 5, 6), 51, "left edge, next pixel");
}

#[test]
fn stack_count_draws_digits_above_one() {
    // numbers.png as a 50 x 7 sheet of opaque white: every glyph pixel is white.
    let overlays = Overlays { numbers: Some(solid(50, 7, [255, 255, 255])), ..Overlays::default() };
    let mut card = empty_card();
    card.tiles[0] = MapTile { count: 3, ..item_tile("minecraft:red_wool") };
    let mut buf = MapBuffer::new();
    render(&card, &red_icons(), &overlays, 0, &mut buf);
    // One digit: starts at x = 15, y = 13. Its first pixel is at tile (15, 13) = map (20, 18).
    assert_eq!(px(&buf, 20, 18), WHITE);
    // The picture is still red where no digit is: tile (14, 13).
    assert_eq!(px(&buf, 19, 18), RED);
    // The last glyph pixel (tile (19, 19)) casts a shadow on tile (20, 20) = map (25, 25).
    assert_eq!(px(&buf, 25, 25), ITEM_SHADOW);
}

#[test]
fn multi_item_tile_has_a_white_plus_at_six() {
    let mut card = empty_card();
    card.tiles[0] = MapTile { decoration: Decoration::MultiItem, ..item_tile("minecraft:red_wool") };
    let mut buf = MapBuffer::new();
    render(&card, &red_icons(), &Overlays::default(), 0, &mut buf);
    // Tile (6, 6) is map (11, 11); the arms reach 2 pixels out.
    assert_eq!(px(&buf, 11, 11), WHITE);
    assert_eq!(px(&buf, 11, 9), WHITE);
    assert_eq!(px(&buf, 11, 13), WHITE);
    assert_eq!(px(&buf, 9, 11), WHITE);
    assert_eq!(px(&buf, 13, 11), WHITE);
    // Three pixels out is not part of the plus.
    assert_eq!(px(&buf, 11, 14), RED);
}

#[test]
fn hidden_tile_uses_the_tier_picture_or_the_generic_one() {
    let overlays = Overlays {
        hidden: Some(solid(16, 16, [255, 0, 0])),
        hidden_tiers: vec![(TierLabel::S, solid(16, 16, [255, 255, 255]))],
        ..Overlays::default()
    };
    let mut card = empty_card();
    card.tiles[0] = MapTile { hidden: true, tier: Some(TierLabel::S), ..MapTile::default() };
    card.tiles[1] = MapTile { hidden: true, tier: None, ..MapTile::default() };
    card.tiles[2] = MapTile { hidden: true, tier: Some(TierLabel::A), ..MapTile::default() };
    let mut buf = MapBuffer::new();
    render(&card, &MemoryIcons::default(), &overlays, 0, &mut buf);
    assert_eq!(px(&buf, 8, 8), WHITE, "tier S picture");
    assert_eq!(px(&buf, 32, 8), RED, "generic picture, tile 1 at x = 29");
    assert_eq!(px(&buf, 56, 8), BG, "tier A has no picture and there is no fallback: the empty tile");
}

#[test]
fn hidden_flag_from_the_server_view_draws_the_hidden_picture() {
    use horizon_bingo::server::hidden_consume::TileView;
    let view = TileView { index: 0, hidden: true, item: None, tier: None };
    let overlays = Overlays { hidden: Some(solid(16, 16, [255, 0, 0])), ..Overlays::default() };
    let mut card = empty_card();
    card.tiles[0] = MapTile::from_view(&view);
    let mut buf = MapBuffer::new();
    render(&card, &MemoryIcons::default(), &overlays, 0, &mut buf);
    assert_eq!(px(&buf, 8, 8), RED);
}

#[test]
fn file_icons_and_overlays_read_the_prepared_pngs() {
    let dir = std::env::temp_dir().join(format!("bingo_map_render_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("icons").join("minecraft")).unwrap();
    std::fs::create_dir_all(dir.join("map")).unwrap();
    let write = |path: &std::path::Path, w: u32, h: u32, rgba: &[u8]| {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(rgba).unwrap();
    };
    let red16: Vec<u8> = [255u8, 0, 0, 255].repeat(16 * 16);
    write(&dir.join("icons").join("red_wool.png"), 16, 16, &red16);
    write(&dir.join("icons").join("minecraft").join("stone.png"), 16, 16, &[7u8, 8, 9, 255].repeat(256));
    write(&dir.join("map").join("numbers.png"), 50, 7, &[255u8, 255, 255, 255].repeat(350));

    let icons = FileIcons { dir: dir.join("icons") };
    let flat = icons.icon("minecraft:red_wool").expect("flat file");
    assert_eq!((flat.width, flat.height), (16, 16));
    assert_eq!(&flat.rgba[0..4], &[255, 0, 0, 255]);
    let nested = icons.icon("minecraft:stone").expect("namespaced file");
    assert_eq!(&nested.rgba[0..4], &[7, 8, 9, 255]);
    assert!(icons.icon("minecraft:missing").is_none());

    let overlays = Overlays::load(&dir.join("map")).expect("overlays load");
    let numbers = overlays.numbers.expect("numbers.png is read");
    assert_eq!((numbers.width, numbers.height), (50, 7));
    assert!(overlays.fallback.is_none(), "a missing file is None, not an error");

    std::fs::remove_dir_all(&dir).unwrap();
}

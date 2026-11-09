//! Card pictures from `card_image` packets (`BingoCardImageController.kt`): decoding, the limits, the change flag, and
//! the packing into the mod's atlas. Expected values are the pixels written into the fixtures, not read from the code.
use std::collections::HashMap;

use horizon_bingo::atlas::{Atlas, PackError, Sprite};
use horizon_bingo::card_images::{sprite_name, tile_image_sprite, ImageStore, MAX_IMAGES, MAX_PNG_BYTES, MAX_SIDE, PIXEL_BUDGET};
use horizon_bingo::card_tile_image::CardTileImage;

fn png_file(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(color);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
    out
}

/// A 1 x 1 RGBA file of one colour.
fn dot(px: [u8; 4]) -> Vec<u8> {
    png_file(1, 1, png::ColorType::Rgba, &px)
}

fn solid_rgba(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    px.iter().copied().cycle().take((w * h * 4) as usize).collect()
}

fn map(entries: &[(&str, Vec<u8>)]) -> HashMap<String, Vec<u8>> {
    entries.iter().map(|(id, bytes)| (id.to_string(), bytes.clone())).collect()
}

fn pixel(a: &Atlas, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * a.width + x) * 4) as usize;
    [a.rgba[i], a.rgba[i + 1], a.rgba[i + 2], a.rgba[i + 3]]
}

/// The pixels of a sprite as the atlas holds them, row by row.
fn sprite_pixels(a: &Atlas, name: &str) -> Vec<u8> {
    let [x, y, w, h] = a.rect(name).unwrap().map(|v| v as u32);
    let mut out = Vec::new();
    for row in 0..h {
        for col in 0..w {
            out.extend_from_slice(&pixel(a, x + col, y + row));
        }
    }
    out
}

fn base() -> Atlas {
    Atlas::pack(
        &[Sprite { name: "bingo/card".into(), width: 3, height: 3, rgba: solid_rgba(3, 3, [200, 0, 0, 255]) }],
        1024,
    )
    .unwrap()
}

/// Patches the IHDR size of a PNG and recomputes its CRC, so only the size is wrong.
fn with_declared_size(file: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = file.to_vec();
    out[16..20].copy_from_slice(&w.to_be_bytes());
    out[20..24].copy_from_slice(&h.to_be_bytes());
    let crc = crc32(&out[12..29]);
    out[29..33].copy_from_slice(&crc.to_be_bytes());
    out
}

/// CRC-32 (IEEE, as PNG uses it), bit by bit.
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

#[test]
fn the_crc_helper_gives_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn an_rgba_picture_is_packed_pixel_exact() {
    let px = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let mut store = ImageStore::default();
    assert!(store.sync(&map(&[("p", png_file(2, 2, png::ColorType::Rgba, &px))])));
    assert!(store.contains("p"));
    let atlas = store.build_atlas(&base()).unwrap();
    assert_eq!(sprite_pixels(&atlas, &sprite_name("p")), px);
}

#[test]
fn an_rgb_picture_gets_full_alpha() {
    let mut store = ImageStore::default();
    store.sync(&map(&[("rgb", png_file(1, 1, png::ColorType::Rgb, &[10, 20, 30]))]));
    let atlas = store.build_atlas(&base()).unwrap();
    assert_eq!(sprite_pixels(&atlas, "img/rgb"), [10, 20, 30, 255]);
}

#[test]
fn a_sixteen_by_sixteen_picture_keeps_every_pixel() {
    let data: Vec<u8> = (0..16u32).flat_map(|y| (0..16u32).flat_map(move |x| [x * 17, y * 17, (x + y) * 8, 200])).map(|v| v as u8).collect();
    let mut store = ImageStore::default();
    store.sync(&map(&[("grad", png_file(16, 16, png::ColorType::Rgba, &data))]));
    let atlas = store.build_atlas(&base()).unwrap();
    assert_eq!(sprite_pixels(&atlas, "img/grad"), data);
    // Corner check against hand-computed values: (15, 0) is [15 * 17, 0, 15 * 8, 200] = [255, 0, 120, 200].
    let r = atlas.rect("img/grad").unwrap();
    assert_eq!(pixel(&atlas, r[0] as u32 + 15, r[1] as u32), [255, 0, 120, 200]);
}

#[test]
fn corrupt_and_truncated_files_are_refused_and_do_not_change_the_set() {
    let good = png_file(4, 4, png::ColorType::Rgba, &solid_rgba(4, 4, [9, 9, 9, 9]));
    let mut store = ImageStore::default();
    assert!(!store.sync(&map(&[("junk", b"definitely not a png".to_vec()), ("cut", good[..30].to_vec())])));
    assert!(!store.contains("junk") && !store.contains("cut"));
    // Same broken bytes again: still no change. Then a good file under the same id counts.
    assert!(!store.sync(&map(&[("junk", b"definitely not a png".to_vec()), ("cut", good[..30].to_vec())])));
    assert!(store.sync(&map(&[("cut", good.clone())])));
    assert!(store.contains("cut"));
}

#[test]
fn a_declared_size_beyond_the_limit_is_refused_before_any_pixels_are_made() {
    let file = dot([1, 2, 3, 4]);
    assert_eq!(crc32(&file[12..29]), u32::from_be_bytes(file[29..33].try_into().unwrap()));
    let mut store = ImageStore::default();
    store.sync(&map(&[("ok", file.clone()), ("huge", with_declared_size(&file, 100_000, 100_000))]));
    assert!(store.contains("ok"), "the unpatched file is a valid 1 x 1 picture");
    assert!(!store.contains("huge"));
}

/// A valid 1 x 1 PNG with a text chunk of `payload` bytes after IHDR (CRC recomputed), so the file is well formed.
fn with_text_chunk(payload: usize) -> Vec<u8> {
    let file = dot([1, 2, 3, 4]);
    let mut data = b"c\0".to_vec();
    data.resize(2 + payload, b'x');
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(&data);
    let mut typed_data = b"tEXt".to_vec();
    typed_data.extend_from_slice(&data);
    chunk.extend_from_slice(&crc32(&typed_data).to_be_bytes());
    let mut out = file[..33].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&file[33..]);
    out
}

#[test]
fn a_file_over_the_byte_limit_is_refused_even_when_it_would_decode() {
    // 1 MiB (1 048 576 bytes) of limit: the files are valid, so only the byte limit can refuse the big one.
    assert_eq!(MAX_PNG_BYTES, 1_048_576, "the expectations below are written for 1 MiB");
    let at_limit = with_text_chunk(1_048_576 - 1_000);
    let over = with_text_chunk(1_048_576 + 1_000);
    assert!(at_limit.len() <= 1_048_576 && over.len() > 1_048_576);
    let mut store = ImageStore::default();
    store.sync(&map(&[("at", at_limit), ("over", over)]));
    assert!(store.contains("at"), "a valid file under the limit is kept");
    assert!(!store.contains("over"));
}

#[test]
fn only_the_first_sixteen_pictures_by_id_are_kept() {
    let ids: Vec<String> = (0..20).map(|i| format!("a{i:02}")).collect();
    let entries: Vec<(&str, Vec<u8>)> = ids.iter().map(|id| (id.as_str(), dot([1, 1, 1, 255]))).collect();
    let mut store = ImageStore::default();
    assert!(store.sync(&map(&entries)));
    assert_eq!(MAX_IMAGES, 16, "the expectations below are written for 16");
    assert!(store.contains("a00") && store.contains("a15"));
    assert!(!store.contains("a16") && !store.contains("a19"));
}

#[test]
fn a_picture_wider_than_the_side_limit_is_refused_and_the_limit_itself_is_accepted() {
    assert_eq!(MAX_SIDE, 512, "the expectations below are written for 512");
    let at = png_file(512, 1, png::ColorType::Rgba, &solid_rgba(512, 1, [5, 5, 5, 5]));
    let over = png_file(513, 1, png::ColorType::Rgba, &solid_rgba(513, 1, [5, 5, 5, 5]));
    let mut store = ImageStore::default();
    store.sync(&map(&[("at", at), ("over", over)]));
    assert!(store.contains("at"));
    assert!(!store.contains("over"));
}

#[test]
fn the_pixel_budget_keeps_four_full_size_pictures_in_id_order() {
    let full_rgba = solid_rgba(256, 256, [7, 7, 7, 255]);
    let full = png_file(256, 256, png::ColorType::Rgba, &full_rgba);
    assert_eq!(PIXEL_BUDGET, 4 * 256 * 256);
    let entries: Vec<(&str, Vec<u8>)> = ["b0", "b1", "b2", "b3", "b4"].iter().map(|id| (*id, full.clone())).collect();
    let mut store = ImageStore::default();
    store.sync(&map(&entries));
    for id in ["b0", "b1", "b2", "b3"] {
        assert!(store.contains(id), "{id}");
    }
    assert!(!store.contains("b4"));
    // The four kept pictures and the base sprite fit the slot, each picture pixel-exact in its own sprite.
    let atlas = store.build_atlas(&base()).expect("the four kept pictures and the base sprite fit the slot");
    assert_eq!(atlas.len(), 5, "the base sprite and the four kept pictures");
    for id in ["b0", "b1", "b2", "b3"] {
        assert_eq!(sprite_pixels(&atlas, &format!("img/{id}")), full_rgba, "{id}");
    }
    assert!(atlas.rect("img/b4").is_none());
}

#[test]
fn the_change_flag_reports_added_replaced_and_removed_pictures_only() {
    let mut store = ImageStore::default();
    assert!(!store.sync(&HashMap::new()));
    assert!(store.sync(&map(&[("a", dot([255, 0, 0, 255]))])));
    assert!(!store.sync(&map(&[("a", dot([255, 0, 0, 255]))])), "same bytes");
    assert!(store.sync(&map(&[("a", dot([0, 0, 255, 255]))])), "replaced bytes");
    assert!(!store.sync(&map(&[("a", dot([0, 0, 255, 255])), ("broken", b"x".to_vec())])), "a refused file is no change");
    assert!(store.sync(&map(&[("broken", b"x".to_vec())])), "the decoded picture went away");
    assert!(!store.sync(&map(&[("broken", b"x".to_vec())])));
}

#[test]
fn the_picture_is_drawn_from_its_own_sprite_and_the_static_sprites_keep_their_pixels() {
    let mut store = ImageStore::default();
    store.sync(&map(&[("x", dot([9, 8, 7, 6])), ("y", dot([1, 2, 3, 4]))]));
    let before = base();
    let atlas = store.build_atlas(&before).unwrap();
    assert_eq!(sprite_pixels(&atlas, "bingo/card"), solid_rgba(3, 3, [200, 0, 0, 255]));
    assert_eq!(sprite_pixels(&atlas, "img/x"), [9, 8, 7, 6]);
    assert_eq!(sprite_pixels(&atlas, "img/y"), [1, 2, 3, 4]);
    assert_eq!(atlas.len(), 3);
}

#[test]
fn with_no_pictures_the_atlas_is_the_static_one_again() {
    let before = base();
    let atlas = ImageStore::default().build_atlas(&before).unwrap();
    assert_eq!((atlas.width, atlas.height), (before.width, before.height));
    assert_eq!(atlas.rect("bingo/card"), before.rect("bingo/card"));
    assert_eq!(atlas.rgba, before.rgba);
}

#[test]
fn atlas_sprites_are_cut_back_out_pixel_exact() {
    let gradient: Vec<u8> = (0..5 * 1 * 4).map(|i| i as u8 * 3).collect();
    let sprites = vec![
        Sprite { name: "bingo/b".into(), width: 5, height: 1, rgba: gradient.clone() },
        Sprite { name: "bingo/a".into(), width: 3, height: 2, rgba: solid_rgba(3, 2, [4, 5, 6, 7]) },
    ];
    let atlas = Atlas::pack(&sprites, 64).unwrap();
    let back = atlas.sprites();
    let names: Vec<&str> = back.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["bingo/a", "bingo/b"]);
    assert_eq!(back[0].rgba, solid_rgba(3, 2, [4, 5, 6, 7]));
    assert_eq!(back[1].rgba, gradient);
    assert_eq!((back[1].width, back[1].height), (5, 1));
    // Packing them again gives the same atlas bytes.
    let again = Atlas::pack(&back, 64).unwrap();
    assert_eq!(again.rgba, atlas.rgba);
}

#[test]
fn a_tile_takes_its_picture_when_the_atlas_has_it() {
    let known = |name: &str| name == "img/pic";
    let with = CardTileImage { texture: Some("pic".into()), ..CardTileImage::default() };
    assert_eq!(tile_image_sprite(&with, &known), Some("img/pic".to_string()));
    let missing = CardTileImage { texture: Some("gone".into()), ..CardTileImage::default() };
    assert_eq!(tile_image_sprite(&missing, &known), None);
    let none = CardTileImage::default();
    assert_eq!(tile_image_sprite(&none, &|_| true), None);
}

#[test]
fn a_picture_that_does_not_fit_the_slot_is_an_error_not_a_partial_atlas() {
    // Two 512 x 512 pictures would be 2 x 262144 pixels; the budget only admits one, so the second is refused and
    // the rest fits. Build a store whose base alone is too big instead, to see the error reach the caller.
    let huge_base = Atlas::pack(&[Sprite { name: "bingo/big".into(), width: 1024, height: 1024, rgba: solid_rgba(1024, 1024, [0; 4]) }], 1024).unwrap();
    let mut store = ImageStore::default();
    store.sync(&map(&[("p", dot([1, 1, 1, 1]))]));
    assert_eq!(store.build_atlas(&huge_base).unwrap_err(), PackError::TooBig);
}

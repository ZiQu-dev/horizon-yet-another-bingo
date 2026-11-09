//! Loading the original mod's files from a prepared folder (built here in a temporary directory).
use horizon_bingo::assets::{parse_color, Assets};
use horizon_bingo::card_draw::ItemNames;
use std::fs;
use std::path::{Path, PathBuf};

fn png(path: &Path, w: u32, h: u32, color: png::ColorType, data: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut enc = png::Encoder::new(fs::File::create(path).unwrap(), w, h);
    enc.set_color(color);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(data).unwrap();
}

fn prepared() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bingo-assets-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = fs::remove_dir_all(&dir);
    png(&dir.join("sprites/bingo/card_red.png"), 2, 2, png::ColorType::Rgba, &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 9, 8, 7, 6]);
    png(&dir.join("sprites/bingo/rgb.png"), 1, 1, png::ColorType::Rgb, &[10, 20, 30]);
    png(&dir.join("sprites/bingo/gray.png"), 1, 2, png::ColorType::GrayscaleAlpha, &[5, 200, 6, 100]);
    fs::write(
        dir.join("card_colors.json"),
        r##"{"teams":{
            "red":{"backgroundColor":"#80573838","textColor":"#ffe3a64b","outlineColor":"#ffff5555","cardTexture":"minecraft:bingo/card_red","textX":14},
            "bingo_special":{"textColor":"#102030","tileAchievedColor":"#ff00ff00"}}}"##,
    )
    .unwrap();
    fs::create_dir_all(dir.join("lang")).unwrap();
    fs::write(dir.join("lang/en_us.json"), r#"{"bingo.team.red":"Red"}"#).unwrap();
    fs::write(dir.join("item_names.txt"), "minecraft:air\nminecraft:stone\nminecraft:granite\n").unwrap();
    dir
}

#[test]
fn colours_parse_with_or_without_alpha() {
    assert_eq!(parse_color("#80686868"), Some(0x80686868));
    assert_eq!(parse_color("#ff0000"), Some(0xffff0000));
    assert_eq!(parse_color("ff0000"), None);
    assert_eq!(parse_color("#12345"), None);
    assert_eq!(parse_color("#zzzzzz"), None);
}

#[test]
fn textures_are_decoded_to_rgba_whatever_their_colour_type_and_packed_by_path() {
    let assets = Assets::load(&prepared()).unwrap();
    assert!(assets.atlas.contains("bingo/card_red") && assets.atlas.contains("bingo/rgb") && assets.atlas.contains("bingo/gray"));
    let px = |name: &str, dx: u32, dy: u32| {
        let r = assets.atlas.rect(name).unwrap();
        let i = (((r[1] as u32 + dy) * assets.atlas.width + r[0] as u32 + dx) * 4) as usize;
        [assets.atlas.rgba[i], assets.atlas.rgba[i + 1], assets.atlas.rgba[i + 2], assets.atlas.rgba[i + 3]]
    };
    assert_eq!(px("bingo/card_red", 0, 0), [255, 0, 0, 255]);
    assert_eq!(px("bingo/card_red", 1, 0), [0, 255, 0, 128]);
    assert_eq!(px("bingo/card_red", 0, 1), [0, 0, 255, 64]);
    assert_eq!(px("bingo/card_red", 1, 1), [9, 8, 7, 6]);
    assert_eq!(px("bingo/rgb", 0, 0), [10, 20, 30, 255], "RGB gets an opaque alpha");
    assert_eq!(px("bingo/gray", 0, 0), [5, 5, 5, 200], "gray + alpha is expanded");
    assert_eq!(px("bingo/gray", 0, 1), [6, 6, 6, 100]);
}

#[test]
fn a_team_takes_its_entry_by_id_then_label_then_colour_and_the_rest_from_the_defaults() {
    let assets = Assets::load(&prepared()).unwrap();
    let red = assets.team_colors(Some("bingo_red"), None); // by label "red"
    assert_eq!(red.card_texture, "bingo/card_red");
    assert_eq!((red.text_color, red.outline, red.text_x, red.text_y), (0xffe3a64b, 0xffff5555, 14, 10));
    assert_eq!(red.tile_achieved, 0xff55ff55, "not in the entry: the default");
    let by_colour = assets.team_colors(Some("bingo_unknown"), Some("RED"));
    assert_eq!(by_colour.card_texture, "bingo/card_red", "the formatting colour's name, lower-cased");
    let by_id = assets.team_colors(Some("bingo_special"), Some("RED"));
    assert_eq!(by_id.tile_achieved, 0xff00ff00);
    assert_eq!(by_id.text_color, 0xff102030, "an opaque colour without alpha");
    assert_eq!(by_id.card_texture, "bingo/card_preview", "an entry without a texture: the default card");
    let none = assets.team_colors(Some("bingo_x"), None);
    assert_eq!(none, horizon_bingo::card_draw::TeamColors::default());
}

#[test]
fn language_and_item_names_are_looked_up_by_key_and_registry_id() {
    let assets = Assets::load(&prepared()).unwrap();
    assert_eq!(assets.translate("bingo.team.red").as_deref(), Some("Red"));
    assert_eq!(assets.translate("nope"), None);
    assert_eq!(assets.item_name(1).as_deref(), Some("minecraft:stone"));
    assert_eq!(assets.item_name(3), None);
    assert_eq!(assets.item_name(-1), None);
}

#[test]
fn missing_or_broken_files_say_which_one() {
    let empty = std::env::temp_dir().join(format!("bingo-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&empty);
    fs::create_dir_all(empty.join("sprites")).unwrap();
    assert!(Assets::load(&empty).unwrap_err().contains("no sprites"));
    assert!(Assets::load(&empty.join("absent")).unwrap_err().contains("absent"));
    let dir = prepared();
    fs::write(dir.join("card_colors.json"), "{ not json").unwrap();
    assert!(Assets::load(&dir).unwrap_err().contains("card_colors.json"));
    let dir = prepared();
    fs::remove_file(dir.join("item_names.txt")).unwrap();
    assert!(Assets::load(&dir).unwrap_err().contains("item_names.txt"));
    let dir = prepared();
    fs::write(dir.join("sprites/bingo/bad.png"), b"not a png").unwrap();
    assert!(Assets::load(&dir).unwrap_err().contains("bad.png"));
}

/// With BINGO_ASSETS_DIR pointing at a folder made by tools/prepare_assets.py from the real mod, checks the real files.
#[test]
fn the_real_prepared_folder_loads_when_one_is_given() {
    let Ok(dir) = std::env::var("BINGO_ASSETS_DIR") else { return };
    let assets = Assets::load(Path::new(&dir)).unwrap();
    assert!(assets.atlas.width <= 1024 && assets.atlas.height <= 1024);
    for name in ["bingo/card_red", "bingo/card_preview", "bingo/image_hidden", "bingo/image_frame_locked", "bingo/image_frame_advancement"] {
        assert!(assets.atlas.contains(name), "{name} is missing from the real sprites");
    }
    let red = assets.team_colors(Some("bingo_red"), Some("RED"));
    assert_eq!(red.card_texture, "bingo/card_red");
    assert_eq!(assets.item_name(1).as_deref(), Some("minecraft:stone"));
    assert_eq!(assets.item_name(1050).as_deref(), Some("minecraft:diamond_sword"));
    assert_eq!(assets.translate("bingo.team.red").as_deref(), Some("Red"));
    println!("real assets: {} sprites in a {}x{} atlas", assets.atlas.len(), assets.atlas.width, assets.atlas.height);
}

#[test]
fn the_players_language_overrides_english_key_by_key_and_unsafe_codes_are_ignored() {
    let dir = prepared();
    fs::write(dir.join("lang/pl_pl.json"), r#"{"bingo.team.red":"Czerwona"}"#).unwrap();
    fs::write(dir.join("lang/en_us.json"), r#"{"bingo.team.red":"Red","bingo.only.english":"Only"}"#).unwrap();
    let mut assets = Assets::load(&dir).unwrap();
    assets.set_language(&dir, "pl_pl").unwrap();
    assert_eq!(assets.translate("bingo.team.red").as_deref(), Some("Czerwona"));
    assert_eq!(assets.translate("bingo.only.english").as_deref(), Some("Only"), "a missing key falls back to English");
    // en_us, an empty code, a code with a path in it and a language with no file change nothing
    let mut other = Assets::load(&dir).unwrap();
    for code in ["en_us", "", "../x", "de_de"] {
        other.set_language(&dir, code).unwrap();
    }
    assert_eq!(other.translate("bingo.team.red").as_deref(), Some("Red"));
    fs::write(dir.join("lang/xx_xx.json"), "not json").unwrap();
    assert!(other.set_language(&dir, "xx_xx").unwrap_err().contains("xx_xx"));
}

#[test]
fn vanillas_language_is_the_base_and_the_mods_wording_wins_in_every_language() {
    let dir = prepared();
    fs::create_dir_all(dir.join("vanilla_lang")).unwrap();
    fs::write(dir.join("vanilla_lang/en_us.json"), r#"{"chat.square_brackets":"[%s]","block.minecraft.stone":"Stone","bingo.team.red":"Vanilla red"}"#).unwrap();
    fs::write(dir.join("vanilla_lang/pl_pl.json"), r#"{"chat.square_brackets":"[%s]","block.minecraft.stone":"Kamień"}"#).unwrap();
    let mut assets = Assets::load(&dir).unwrap();
    assert_eq!(assets.translate("chat.square_brackets").as_deref(), Some("[%s]"));
    assert_eq!(assets.translate("block.minecraft.stone").as_deref(), Some("Stone"));
    assert_eq!(assets.translate("bingo.team.red").as_deref(), Some("Red"), "the mod's own text wins over vanilla's");
    assets.set_language(&dir, "pl_pl").unwrap();
    assert_eq!(assets.translate("block.minecraft.stone").as_deref(), Some("Kamień"));
    assert_eq!(assets.translate("bingo.team.red").as_deref(), Some("Red"));
}

#[test]
fn the_real_atlas_still_has_room_for_a_player_face() {
    let Ok(dir) = std::env::var("BINGO_ASSETS_DIR") else { return };
    let assets = Assets::load(std::path::Path::new(&dir)).unwrap();
    let mut store = horizon_bingo::card_images::ImageStore::default();
    assert!(store.set_face(7, vec![255; 8 * 8 * 4]));
    let atlas = store.build_atlas(&assets.atlas).expect("a face fits next to the mod's sprites");
    assert!(atlas.rect(&horizon_bingo::card_images::face_sprite(7)).is_some());
}

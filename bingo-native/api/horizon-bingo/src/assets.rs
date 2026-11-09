//! The original mod's own files, read from a folder the player's machine already has (`prepare_assets.py` fills it
//! from the mod's jar and from Horizon's prepared resources). Nothing from the mod is shipped with this crate: like
//! Horizon with a Minecraft install, it uses what the player has.
//!
//! ```text
//! <dir>/sprites/bingo/card_red.png ...   assets/minecraft/textures/gui/sprites/bingo/*.png of the mod (sprite name = path)
//! <dir>/card_colors.json                 assets/yet-another-minecraft-bingo/gui/card_colors.json
//! <dir>/lang/en_us.json                  assets/yet-another-minecraft-bingo/lang/en_us.json
//! <dir>/item_names.txt                   one item per line in registry order (Horizon's command-registries.json "items")
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::atlas::{Atlas, Sprite};
use crate::card_draw::{ItemNames, TeamColors};

/// The HUD atlas slot Horizon gives a mod, in pixels per side.
pub const SLOT_SIZE: u32 = 1024;

#[derive(Debug)]
pub struct Assets {
    pub atlas: Atlas,
    teams: HashMap<String, Value>,
    pub lang: HashMap<String, String>,
    item_names: Vec<String>,
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `#aarrggbb`, or `#rrggbb` (opaque), as the mod's `ColorType` reads it.
pub fn parse_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#')?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        8 => Some(value),
        6 => Some(0xff00_0000 | value),
        _ => None,
    }
}

fn decode_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or_else(|| format!("{}: too large", path.display()))?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
    buf.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(format!("{}: indexed colour was not expanded", path.display())),
    };
    Ok((info.width, info.height, rgba))
}

fn collect_pngs(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            collect_pngs(&path, &format!("{prefix}{name}/"), out)?;
        } else if let Some(stem) = name.strip_suffix(".png") {
            out.push((format!("{prefix}{stem}"), path));
        }
    }
    Ok(())
}

impl Assets {
    pub fn load(dir: &Path) -> Result<Assets, String> {
        let mut files = Vec::new();
        collect_pngs(&dir.join("sprites"), "", &mut files)?;
        if files.is_empty() {
            return Err(format!("{}: no sprites (run prepare_assets.py)", dir.join("sprites").display()));
        }
        let mut sprites = Vec::new();
        for (name, path) in files {
            let (width, height, rgba) = decode_png(&path)?;
            sprites.push(Sprite { name, width, height, rgba });
        }
        let atlas = Atlas::pack(&sprites, SLOT_SIZE).map_err(|e| format!("packing the sprites: {e:?}"))?;

        let colors: Value = serde_json::from_slice(&read(&dir.join("card_colors.json"))?)
            .map_err(|e| format!("card_colors.json: {e}"))?;
        let teams = colors["teams"]
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        // Vanilla's English first (item and block names, chat formats), then the mod's own on top.
        let mut lang: HashMap<String, String> = HashMap::new();
        if let Ok(bytes) = fs::read(dir.join("vanilla_lang").join("en_us.json")) {
            lang = serde_json::from_slice(&bytes).map_err(|e| format!("vanilla_lang/en_us.json: {e}"))?;
        }
        let own: HashMap<String, String> = serde_json::from_slice(&read(&dir.join("lang").join("en_us.json"))?)
            .map_err(|e| format!("lang/en_us.json: {e}"))?;
        lang.extend(own);

        let item_names = String::from_utf8(read(&dir.join("item_names.txt"))?)
            .map_err(|e| format!("item_names.txt: {e}"))?
            .lines()
            .map(str::to_string)
            .collect();
        Ok(Assets { atlas, teams, lang, item_names })
    }

    /// `BingoCardColors.getTeamColors`: the entry for the team's id, else for its label (the id without `bingo_`),
    /// else for its formatting colour's name; what an entry leaves out is the default.
    pub fn team_colors(&self, team: Option<&str>, formatting: Option<&str>) -> TeamColors {
        let entry = team
            .and_then(|t| self.teams.get(t))
            .or_else(|| team.and_then(|t| self.teams.get(t.strip_prefix("bingo_").unwrap_or(t))))
            .or_else(|| formatting.and_then(|f| self.teams.get(&f.to_lowercase())));
        let mut colors = TeamColors::default();
        let Some(entry) = entry else { return colors };
        let color = |key: &str| entry[key].as_str().and_then(parse_color);
        if let Some(c) = color("tileAchievedColor") {
            colors.tile_achieved = c;
        }
        if let Some(c) = color("tileFlashingColor") {
            colors.tile_flashing = c;
        }
        if let Some(c) = color("tileProgressColor") {
            colors.tile_progress = c;
        }
        if let Some(c) = color("textColor") {
            colors.text_color = c;
        }
        if let Some(c) = color("outlineColor") {
            colors.outline = c;
        }
        if let Some(x) = entry["textX"].as_i64() {
            colors.text_x = x as i32;
        }
        if let Some(y) = entry["textY"].as_i64() {
            colors.text_y = y as i32;
        }
        if let Some(texture) = entry["cardTexture"].as_str() {
            // "minecraft:bingo/card_red" -> the sprite "bingo/card_red"
            colors.card_texture = texture.rsplit(':').next().unwrap_or(texture).to_string();
        }
        colors
    }

    /// Reads the player's language over English (`lang/<code>.json`, e.g. `pl_pl`): a key the language lacks keeps its
    /// English text, as in Minecraft. `en_us` itself and a missing file leave the language as it is.
    pub fn set_language(&mut self, dir: &Path, code: &str) -> Result<(), String> {
        if code == "en_us" || code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Ok(());
        }
        // Vanilla's translation first, then the mod's, so the mod's own wording wins as it does in the game.
        for (folder, label) in [("vanilla_lang", "vanilla_lang"), ("lang", "lang")] {
            let path = dir.join(folder).join(format!("{code}.json"));
            if !path.exists() {
                continue;
            }
            let extra: HashMap<String, String> =
                serde_json::from_slice(&read(&path)?).map_err(|e| format!("{label}/{code}.json: {e}"))?;
            self.lang.extend(extra);
        }
        Ok(())
    }

    pub fn translate(&self, key: &str) -> Option<String> {
        self.lang.get(key).cloned()
    }
}

impl ItemNames for Assets {
    fn item_name(&self, registry_id: i32) -> Option<String> {
        usize::try_from(registry_id).ok().and_then(|i| self.item_names.get(i)).cloned()
    }
}

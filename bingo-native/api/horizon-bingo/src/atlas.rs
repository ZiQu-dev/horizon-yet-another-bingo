//! Packs the mod's GUI textures into one RGBA image. Horizon gives a native mod a single HUD atlas slot for its own
//! image (`Host::mod_image`, at most 1024 x 1024 pixels, uploaded once and drawn by source rectangle), so every card
//! texture, tile decoration and hidden-tile image goes into one picture and a sprite is a rectangle of it.
//! Shelf packing: sprites sorted by height (then name, so the layout is the same on every run), left to right, a
//! new shelf when a row is full.

use std::collections::HashMap;

/// A decoded texture: `rgba` is `width * height * 4` bytes.
#[derive(Debug, Clone)]
pub struct Sprite {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// The sprite's pixel buffer is not `width * height * 4` bytes, or one side is zero.
    BadSprite(String),
    /// The sprites do not fit in the given size.
    TooBig,
    /// Two sprites have the same name.
    Duplicate(String),
}

#[derive(Debug, Clone)]
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    rects: HashMap<String, [u32; 4]>,
}

impl Atlas {
    /// Packs `sprites` into an image no larger than `max` pixels on a side; the result is only as large as it needs.
    pub fn pack(sprites: &[Sprite], max: u32) -> Result<Atlas, PackError> {
        let mut seen = std::collections::HashSet::new();
        for s in sprites {
            if s.width == 0 || s.height == 0 || s.rgba.len() != s.width as usize * s.height as usize * 4 {
                return Err(PackError::BadSprite(s.name.clone()));
            }
            if !seen.insert(&s.name) {
                return Err(PackError::Duplicate(s.name.clone()));
            }
            if s.width > max || s.height > max {
                return Err(PackError::TooBig);
            }
        }
        let mut order: Vec<&Sprite> = sprites.iter().collect();
        order.sort_by(|a, b| b.height.cmp(&a.height).then_with(|| a.name.cmp(&b.name)));

        let mut rects = HashMap::new();
        let (mut x, mut y, mut shelf_height, mut used_width) = (0u32, 0u32, 0u32, 0u32);
        for s in &order {
            if x + s.width > max {
                y += shelf_height;
                x = 0;
                shelf_height = 0;
            }
            if y + s.height > max {
                return Err(PackError::TooBig);
            }
            rects.insert(s.name.clone(), [x, y, s.width, s.height]);
            x += s.width;
            used_width = used_width.max(x);
            shelf_height = shelf_height.max(s.height);
        }
        let (width, height) = (used_width.max(1), (y + shelf_height).max(1));
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for s in sprites {
            let [rx, ry, w, h] = rects[&s.name];
            for row in 0..h as usize {
                let from = row * w as usize * 4;
                let to = ((ry as usize + row) * width as usize + rx as usize) * 4;
                rgba[to..to + w as usize * 4].copy_from_slice(&s.rgba[from..from + w as usize * 4]);
            }
        }
        Ok(Atlas { width, height, rgba, rects })
    }

    /// The sprite's rectangle in the atlas, `[x, y, w, h]` in pixels.
    pub fn rect(&self, name: &str) -> Option<[f32; 4]> {
        self.rects.get(name).map(|r| [r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32])
    }

    /// Every sprite of the atlas cut back out of the image, sorted by name: the pixels `pack` was given, so an atlas
    /// can be packed again with more sprites.
    pub fn sprites(&self) -> Vec<Sprite> {
        let mut names: Vec<&String> = self.rects.keys().collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let [x, y, w, h] = self.rects[name];
                let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
                for row in 0..h as usize {
                    let from = ((y as usize + row) * self.width as usize + x as usize) * 4;
                    rgba.extend_from_slice(&self.rgba[from..from + w as usize * 4]);
                }
                Sprite { name: name.clone(), width: w, height: h, rgba }
            })
            .collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.rects.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.rects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }
}

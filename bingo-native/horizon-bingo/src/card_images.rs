//! Pictures the server sends in `card_image` packets (`BingoCardImageController.kt`). The mod gets one picture slot
//! from Horizon, so a picture is packed into the same atlas as the mod's own sprites: [`ImageStore::build_atlas`]
//! returns that atlas, and the host uploads it again whenever [`ImageStore::sync`] reports a change. A tile whose
//! `texture` names a picture is drawn with it instead of its item ([`tile_image_sprite`]).
//!
//! Limits, so a hostile server cannot exhaust memory: at most [`MAX_IMAGES`] pictures (the first by id), each at most
//! [`MAX_SIDE`] pixels a side, files of at most [`MAX_PNG_BYTES`] bytes, and [`PIXEL_BUDGET`] pixels in all. A picture
//! that fails any of them, or does not decode, is never shown and is not decoded again until its bytes change.
//! The limits are the port's own choice: the Kotlin client has none.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::io::Cursor;

use crate::assets::SLOT_SIZE;
use crate::atlas::{Atlas, PackError, Sprite};
use crate::card_tile_image::CardTileImage;

/// Pictures kept at once.
pub const MAX_IMAGES: usize = 16;
/// Largest side of a picture, in pixels.
pub const MAX_SIDE: u32 = 512;
/// Largest PNG file accepted, in bytes.
pub const MAX_PNG_BYTES: usize = 1 << 20;
/// Pixels of all pictures together: four full 256 x 256 pictures, or one 512 x 512.
pub const PIXEL_BUDGET: usize = 512 * 512;

#[derive(Debug, Clone)]
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Picture {
    fn pixels(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

#[derive(Debug, Clone)]
struct Entry {
    /// Length and hash of the file the entry was decoded from; the same bytes are never decoded twice.
    fingerprint: (usize, u64),
    /// `None` when the file was refused (corrupt, too big, over the budget).
    picture: Option<Picture>,
}

/// The decoded pictures by id, in id order.
#[derive(Debug, Default)]
pub struct ImageStore {
    entries: BTreeMap<String, Entry>,
    /// Players' faces, 8 x 8 RGBA, by uuid (the player heads of messages).
    faces: BTreeMap<u128, Vec<u8>>,
}

/// Most faces kept at once (a message list shows few players).
pub const MAX_FACES: usize = 64;

/// The name of a picture's sprite in the atlas.
pub fn sprite_name(id: &str) -> String {
    format!("img/{id}")
}

/// The name of a player's face in the atlas.
pub fn face_sprite(uuid: u128) -> String {
    format!("face/{uuid:032x}")
}

fn fingerprint(bytes: &[u8]) -> (usize, u64) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    (bytes.len(), hasher.finish())
}

/// Decodes a PNG to RGBA the way `assets.rs` does (palette and low bit depths expanded, 16-bit reduced, opaque
/// without alpha). The declared size is checked before any pixel buffer is made.
fn decode(bytes: &[u8]) -> Option<Picture> {
    if bytes.len() > MAX_PNG_BYTES {
        return None;
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = reader.info().size();
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    if rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    Some(Picture { width, height, rgba })
}

impl ImageStore {
    /// Brings the store in line with the pictures the client holds (`HudState.images`). Returns `true` when the set of
    /// decoded pictures changed: one added, removed, or replaced by different bytes. Refused files do not count.
    pub fn sync(&mut self, images: &HashMap<String, Vec<u8>>) -> bool {
        let mut old = std::mem::take(&mut self.entries);
        let mut ids: Vec<&String> = images.keys().collect();
        ids.sort();

        let mut changed = false;
        let mut area = 0usize;
        let mut next = BTreeMap::new();
        for id in ids.into_iter().take(MAX_IMAGES) {
            let bytes = &images[id];
            let fp = fingerprint(bytes);
            let kept = match old.remove(id) {
                Some(prev) if prev.fingerprint == fp => Some(prev),
                prev => {
                    if prev.is_some_and(|p| p.picture.is_some()) {
                        changed = true;
                    }
                    None
                }
            };
            let entry = match kept {
                Some(entry) => entry,
                None => {
                    let picture = decode(bytes).filter(|p| area + p.pixels() <= PIXEL_BUDGET);
                    if picture.is_some() {
                        changed = true;
                    }
                    Entry { fingerprint: fp, picture }
                }
            };
            if let Some(p) = &entry.picture {
                area += p.pixels();
            }
            next.insert(id.clone(), entry);
        }
        // Ids that were dropped (not sent any more, or past the limit) and held a picture.
        if old.values().any(|e| e.picture.is_some()) {
            changed = true;
        }
        self.entries = next;
        changed
    }

    /// Keeps a player's face (8 x 8 RGBA). Returns `true` when the atlas has to be built again: a new face or different
    /// pixels. A wrong size, or a store already holding [`MAX_FACES`] other faces, is refused.
    pub fn set_face(&mut self, uuid: u128, rgba: Vec<u8>) -> bool {
        if rgba.len() != 8 * 8 * 4 || (!self.faces.contains_key(&uuid) && self.faces.len() >= MAX_FACES) {
            return false;
        }
        self.faces.insert(uuid, rgba.clone()).as_ref() != Some(&rgba)
    }

    pub fn has_face(&self, uuid: u128) -> bool {
        self.faces.contains_key(&uuid)
    }

    /// Whether a picture with this id is decoded and can be drawn.
    pub fn contains(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.picture.is_some())
    }

    /// The atlas holding `base` (the static sprites, unchanged) and every decoded picture. The result is packed afresh:
    /// sprite rectangles may move, so look them up again after each call. Fails with `TooBig` when they do not fit in
    /// one slot; the caller then keeps the atlas it has.
    pub fn build_atlas(&self, base: &Atlas) -> Result<Atlas, PackError> {
        let mut sprites = base.sprites();
        for (id, entry) in &self.entries {
            if let Some(p) = &entry.picture {
                sprites.push(Sprite { name: sprite_name(id), width: p.width, height: p.height, rgba: p.rgba.clone() });
            }
        }
        for (uuid, rgba) in &self.faces {
            sprites.push(Sprite { name: face_sprite(*uuid), width: 8, height: 8, rgba: rgba.clone() });
        }
        Atlas::pack(&sprites, SLOT_SIZE)
    }
}

/// The sprite a tile's picture is drawn from, when the tile has one and the atlas (`known`, given the sprite name)
/// holds it. The picture wins over the tile's item, so the caller tries this first.
pub fn tile_image_sprite(image: &CardTileImage, known: &dyn Fn(&str) -> bool) -> Option<String> {
    let name = sprite_name(image.texture.as_deref()?);
    known(&name).then_some(name)
}

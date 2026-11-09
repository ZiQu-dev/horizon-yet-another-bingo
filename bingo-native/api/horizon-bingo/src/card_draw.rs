//! The bingo card as a list of draw commands, ported from the original client's `ClientCardBufferRenderer`,
//! `CardTileRenderer` and `BingoCardColors`. Coordinates are GUI pixels relative to the card's top left corner; the
//! card is 122 x 132, its tiles 18 x 18 at (12 + 20 x, 22 + 20 y) with the item 16 x 16 inside. Pure: nothing is
//! drawn here, so the numbers are testable against the Kotlin source.
//!
//! While a card is shuffled (for 280 ms plus 50 ms of padding after `shuffled_at`) each tile is drawn at its eased
//! position from [`crate::shuffle_draw::tile_origin`]. Items, sprites and decorations keep that exact position;
//! fills, outlines and the achieved overlay are drawn at the nearest whole pixel, as the original's fills are.
//!
//! Not yet drawn, on purpose and listed in DESIGN.md: tile images that arrive as `card_image`
//! pictures (they win over the item in the original), and styled text (one colour only).

use crate::card_tile::{CardTile, TierLabel};
use crate::card_tile_image::{CardTileImage, Decoration, ItemStyle};
use crate::shuffle_draw::tile_origin;
use crate::state::{ClientCard, TILES_PER_CARD};

pub const CARD_WIDTH: i32 = 122;
pub const CARD_HEIGHT: i32 = 132;
/// `BingoCardColors.DEFAULTS`.
pub const TILE_ACHIEVED: u32 = 0xff55ff55;
pub const TILE_FLASHING: u32 = 0xffb24cd8;
pub const TILE_PROGRESS: u32 = 0xff309f30;
/// A tile is flashing for 4 seconds after it changed, blinking every 500 ms.
const REPLACING_MS: u64 = 4_000;
const BLINK_MS: u64 = 500;

/// A team's colours (`BingoCardColors.TeamColors`), read by the host from the mod's `card_colors.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamColors {
    pub tile_achieved: u32,
    pub tile_flashing: u32,
    pub tile_progress: u32,
    pub text_x: i32,
    pub text_y: i32,
    pub text_color: u32,
    pub outline: u32,
    /// The sprite of the card's background, `bingo/card_red`.
    pub card_texture: String,
}

impl Default for TeamColors {
    fn default() -> Self {
        TeamColors {
            tile_achieved: TILE_ACHIEVED,
            tile_flashing: TILE_FLASHING,
            tile_progress: TILE_PROGRESS,
            text_x: 12,
            text_y: 10,
            text_color: 0xff000000,
            outline: 0,
            card_texture: "bingo/card_preview".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Draw {
    /// A texture of the mod's atlas, stretched into the rectangle.
    Sprite { name: String, x: f32, y: f32, w: f32, h: f32 },
    Fill { x: i32, y: i32, w: i32, h: i32, argb: u32 },
    Text { x: i32, y: i32, text: String, argb: u32 },
    /// An item icon by name, `size` pixels square, top left at (x, y).
    Item { name: String, x: f32, y: f32, size: f32 },
    /// A stack count, white with a shadow, whose right edge is at `right`: `GuiGraphics.itemDecorations`.
    Count { text: String, right: i32, y: i32 },
}

/// The item behind a registry id (the packets carry numbers, the game draws by name).
pub trait ItemNames {
    fn item_name(&self, registry_id: i32) -> Option<String>;
}

pub struct CardView<'a> {
    pub card: &'a ClientCard,
    pub colors: &'a TeamColors,
    pub now_ms: u64,
    /// Outlines of the teams that have a tile (`config.client.cardTeamOutlines`, and a single card shown).
    pub show_outlines: bool,
    /// The outline colour of a team, by its key.
    pub team_outline: &'a dyn Fn(&str) -> u32,
    pub items: &'a dyn ItemNames,
    /// Which dynamic picture sprites (`img/<id>`) exist: a tile's texture then wins over its item, as in the original.
    pub images: Option<&'a dyn Fn(&str) -> bool>,
    /// The team's name already flattened and cut to fit (70 px).
    pub team_name: Option<String>,
    /// The card is drawn on a screen (`card.isGui`), not on the HUD: an achieved tile then has no overlay over its item.
    pub is_gui: bool,
}

fn fill(out: &mut Vec<Draw>, ox: i32, oy: i32, x1: i32, y1: i32, x2: i32, y2: i32, argb: u32) {
    // `DrawContext.fill(x1, y1, x2, y2)`: corners, the far one exclusive.
    out.push(Draw::Fill { x: ox + x1, y: oy + y1, w: x2 - x1, h: y2 - y1, argb });
}

/// `DrawContext.drawHorizontalLine(x1, x2, y)`: both ends inclusive, one pixel thick.
fn hline(out: &mut Vec<Draw>, ox: i32, oy: i32, x1: i32, x2: i32, y: i32, argb: u32) {
    let (a, b) = (x1.min(x2), x1.max(x2));
    out.push(Draw::Fill { x: ox + a, y: oy + y, w: b - a + 1, h: 1, argb });
}

fn vline(out: &mut Vec<Draw>, ox: i32, oy: i32, x: i32, y1: i32, y2: i32, argb: u32) {
    let (a, b) = (y1.min(y2), y1.max(y2));
    out.push(Draw::Fill { x: ox + x, y: oy + a, w: 1, h: b - a + 1, argb });
}

/// `Color.mix(Color.BLACK.copy(a = 64))`: the colour with a quarter of black over it.
fn darken(argb: u32) -> u32 {
    let ch = |shift: u32| ((argb >> shift & 0xff) * (255 - 64) / 255) << shift;
    (argb & 0xff00_0000) | ch(16) | ch(8) | ch(0)
}

fn sprite(name: &str, x: i32, y: i32, w: i32, h: i32) -> Draw {
    Draw::Sprite { name: name.to_string(), x: x as f32, y: y as f32, w: w as f32, h: h as f32 }
}

fn tile_image(out: &mut Vec<Draw>, items: &dyn ItemNames, images: Option<&dyn Fn(&str) -> bool>, image: &CardTileImage, list: &[CardTileImage], x: f32, y: f32) {
    let one = |out: &mut Vec<Draw>, img: &CardTileImage, x: f32, y: f32, size: f32| {
        if let Some(name) = images.and_then(|known| crate::card_images::tile_image_sprite(img, known)) {
            out.push(Draw::Sprite { name, x, y, w: size, h: size });
            return;
        }
        if let Some(stack) = &img.item {
            if let Some(name) = items.item_name(stack.item) {
                out.push(Draw::Item { name, x, y, size });
                // itemDecorations: a count other than 1 sits at the icon's bottom right (x + 17, y + 9); a scaled icon has none here.
                // The count is text, a whole-pixel command, so it is rounded like the fills.
                if stack.count != 1 && size == 16.0 {
                    out.push(Draw::Count { text: stack.count.to_string(), right: (x + 17.0).round() as i32, y: (y + 9.0).round() as i32 });
                }
            }
        }
    };
    if list.len() > 1 && image.item_style == ItemStyle::OneOf {
        // scale 12/16, each next image 4/scale/2 further in scaled units = 2 px, the first on top: draw last first.
        for i in (0..3).rev() {
            if let Some(img) = list.get(i) {
                one(out, img, x + 2.0 * i as f32, y + 2.0 * i as f32, 12.0);
            }
        }
    } else if list.len() > 1 && image.item_style == ItemStyle::AllOf {
        // scale 9/16, a 2 x 2 grid whose cells are 12 scaled units = 6.75 px apart.
        for i in 0..4 {
            if let Some(img) = list.get(i) {
                one(out, img, x + (i % 2) as f32 * 6.75, y + (i / 2) as f32 * 6.75, 9.0);
            }
        }
    } else {
        one(out, image, x, y, 16.0);
    }
}

fn decorations(out: &mut Vec<Draw>, image: &CardTileImage) {
    let name = match (&image.decoration_texture, image.decoration) {
        (Some(custom), _) => custom.rsplit(':').next().unwrap_or(custom).to_string(),
        (None, Decoration::Advancement) => "bingo/image_frame_advancement".to_string(),
        (None, Decoration::Forbidden) => "bingo/image_frame_forbidden".to_string(),
        (None, Decoration::FreeSpace) => "bingo/image_frame_free_space".to_string(),
        (None, Decoration::MultiItem | Decoration::None) => return,
    };
    // itemX - 2 with itemX = 1: the 20 x 20 frame sits one pixel outside the tile.
    out.push(sprite(&name, -1, -1, 20, 20));
}

/// The commands that draw the whole card, relative to its top left corner, in the order they must be painted.
pub fn draw_card(view: &CardView<'_>) -> Vec<Draw> {
    let colors = view.colors;
    let mut out = vec![sprite(&colors.card_texture, 0, 0, CARD_WIDTH, CARD_HEIGHT)];
    if let Some(name) = &view.team_name {
        out.push(Draw::Text { x: colors.text_x, y: colors.text_y, text: name.clone(), argb: colors.text_color });
    }
    let blink = (view.now_ms / BLINK_MS) % 2 == 0;
    for index in 0..TILES_PER_CARD.min(view.card.tiles.len()) {
        let tile: &CardTile = &view.card.tiles[index];
        let updated_at = view.card.tile_updated_at.get(index).copied().flatten();
        // `shouldRenderReplacing`: a tile that just changed blinks out of the card for 4 seconds.
        if let Some(at) = updated_at {
            let since = view.now_ms.saturating_sub(at);
            if !tile.is_flashing && since < REPLACING_MS && (since / BLINK_MS) % 2 == 0 {
                continue;
            }
        }
        // The tile's corner: its home slot, or the eased point of the shuffle while the card is animating.
        let (ox, oy) = tile_origin(index, view.card.shuffled_at, view.now_ms, &view.card.shuffle_positions);
        // Fills, outlines and the overlay are whole pixels: the corner rounds to the nearest one (a no-op at home).
        let (rx, ry) = (ox.round() as i32, oy.round() as i32);
        let flashing = blink && tile.is_flashing;
        if flashing {
            fill(&mut out, rx, ry, 0, 0, 18, 18, colors.tile_flashing);
        } else if tile.image.is_achieved {
            fill(&mut out, rx, ry, 0, 0, 18, 18, colors.tile_achieved);
        } else if tile.progress >= 1.0 / 18.0 {
            let y = ((18.0 * (1.0 - tile.progress)) as i32).clamp(0, 18);
            fill(&mut out, rx, ry, 0, y, 18, 18, colors.tile_progress);
        }
        if tile.is_hidden {
            let name = match tile.item_tier {
                Some(t) => format!("bingo/image_hidden_{}", t.name().to_lowercase()),
                None => "bingo/image_hidden".to_string(),
            };
            out.push(Draw::Sprite { name, x: ox + 1.0, y: oy + 1.0, w: 16.0, h: 16.0 });
        }
        let mut tile_draws = Vec::new();
        tile_image(&mut tile_draws, view.items, view.images, &tile.image, &tile.image_list, ox + 1.0, oy + 1.0);
        out.extend(tile_draws);
        if tile.image.is_achieved && !flashing && !view.is_gui {
            // overlayFill with a quarter of the achieved colour's alpha
            let a = (colors.tile_achieved >> 24) / 4;
            fill(&mut out, rx, ry, 0, 0, 18, 18, (a << 24) | (colors.tile_achieved & 0x00ff_ffff));
        }
        let mut deco = Vec::new();
        decorations(&mut deco, &tile.image);
        for d in deco {
            if let Draw::Sprite { name, x, y, w, h } = d {
                out.push(Draw::Sprite { name, x: ox + x, y: oy + y, w, h });
            }
        }
        if tile.is_locked {
            out.push(Draw::Sprite { name: "bingo/image_frame_locked".into(), x: ox - 1.0, y: oy - 1.0, w: 20.0, h: 20.0 });
        }
        if view.show_outlines {
            let n = tile.team_keys.len();
            for (i, team) in tile.team_keys.iter().enumerate() {
                let color = (view.team_outline)(team);
                let dark = darken(color);
                let (start, end) = (i as f32 * 38.0 / n as f32, (i + 1) as f32 * 38.0 / n as f32);
                if start < 19.0 {
                    let (ls, le) = (start as i32, (end as i32).min(19));
                    hline(&mut out, rx, ry, -1 + ls, -1 + le, -1, color);
                    vline(&mut out, rx, ry, -1, -1 + ls, -1 + le, color);
                }
                if end > 19.0 {
                    let (ls, le) = ((start as i32 - 19).max(0), end as i32 - 19);
                    hline(&mut out, rx, ry, -1 + ls, -1 + le, -1 + 19, dark);
                    vline(&mut out, rx, ry, -1 + 19, -1 + ls, -1 + le, dark);
                }
                let (start2, end2) = (i as f32 * 34.0 / n as f32, (i + 1) as f32 * 34.0 / n as f32);
                if start2 < 17.0 {
                    let (ls, le) = (start2 as i32, (end2 as i32).min(17));
                    hline(&mut out, rx, ry, ls, le, 0, dark);
                    vline(&mut out, rx, ry, 0, ls, le, dark);
                }
                if end2 > 17.0 {
                    let (ls, le) = ((start2 as i32 - 17).max(0), end2 as i32 - 17);
                    hline(&mut out, rx, ry, ls, le, 17, color);
                    vline(&mut out, rx, ry, 17, ls, le, color);
                }
            }
        }
    }
    out
}

/// The commands moved by (dx, dy): the message list is laid out relative to an anchor under or beside the card.
pub fn translate(draws: Vec<Draw>, dx: i32, dy: i32) -> Vec<Draw> {
    draws
        .into_iter()
        .map(|d| match d {
            Draw::Sprite { name, x, y, w, h } => Draw::Sprite { name, x: x + dx as f32, y: y + dy as f32, w, h },
            Draw::Fill { x, y, w, h, argb } => Draw::Fill { x: x + dx, y: y + dy, w, h, argb },
            Draw::Text { x, y, text, argb } => Draw::Text { x: x + dx, y: y + dy, text, argb },
            Draw::Item { name, x, y, size } => Draw::Item { name, x: x + dx as f32, y: y + dy as f32, size },
            Draw::Count { text, right, y } => Draw::Count { text, right: right + dx, y: y + dy },
        })
        .collect()
}

/// `Font.substrByWidth`: the longest prefix of `text` that is at most `max_width` wide (no ellipsis).
pub fn truncate_to_width(text: &str, max_width: i32, width_of: &dyn Fn(&str) -> i32) -> String {
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if width_of(&out) > max_width {
            out.pop();
            break;
        }
    }
    out
}

/// A card's team colours from the tile tiers, for tests and the host's tooltips.
pub fn tier_sprite(tier: Option<TierLabel>) -> String {
    match tier {
        Some(t) => format!("bingo/image_hidden_{}", t.name().to_lowercase()),
        None => "bingo/image_hidden".into(),
    }
}

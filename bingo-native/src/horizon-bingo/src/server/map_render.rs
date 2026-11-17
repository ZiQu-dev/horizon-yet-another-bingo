//! The bingo card drawn on a vanilla filled map: a 128 x 128 picture of map colour ids. Port of
//! `map/MapRenderService.kt` (the drawing) and `map/MapColors.kt` (the nearest-colour palette). Pure logic: no game
//! types. The only I/O is `FileIcons` and `Overlays::load`, which read PNG files the prepare script wrote.
//!
//! Palette: vanilla `MapColor.col` by id (62 base colours), copied from horizon-items' `MAP_COLORS`, which the Horizon
//! parity exporter reads from the vanilla 26.3 jar. A packed id is `base * 4 + shade`; the shades are the brightness
//! modifiers 180, 220, 255, 135 (`Brightness` LOW, NORMAL, HIGH, LOWEST). The decompiled MapColor source was not
//! present in this checkout, so the table was not checked against it line by line.
//!
//! Drawing follows `MapRenderService.kt`: the border and grid are drawn once per map; then only tiles whose drawn
//! state changed are repainted, and `render_card` returns the rectangle it touched. Kotlin's `a..b` is inclusive, so
//! every range here that came from `..` is written with `..=`.
//!
//! Known deviations: an icon file that fails to decode is treated as missing (the original draws its fallback
//! picture); a `MapBuffer` caches icons, so one buffer must be drawn with one `Style`.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::card_tile::TierLabel;
use crate::server::hidden_consume::TileView;

/// The picture's side in pixels.
pub const MAP_SIZE: usize = 128;
/// Bytes in one picture (one map colour id per pixel).
pub const MAP_BYTES: usize = MAP_SIZE * MAP_SIZE;
/// The outer frame, in pixels.
pub const MAP_BORDER: usize = 5;
const TILE_IMAGE: usize = 16;
const TILE_PADDING: usize = 3;
const TILE_MARGIN: usize = 2;
/// Distance between two tile origins.
const TILE_SIZE: usize = TILE_IMAGE + TILE_PADDING * 2 + TILE_MARGIN;
/// The side of one tile's picture (16 + 2 * 3).
const IMAGE_SIZE: usize = TILE_IMAGE + TILE_PADDING * 2;
/// The last index inside a tile (`innerMax` in the original).
const INNER_MAX: usize = TILE_SIZE - TILE_MARGIN - 1;
const CARD_TILES: usize = 25;
/// A scored tile flashes for 4 seconds, which is 80 ticks.
pub const FLASH_TICKS: u64 = 80;
/// The flash toggles every 10 ticks.
const FLASH_STEP_TICKS: u64 = 10;
const NUMBER_WIDTH: usize = 5;
const NUMBER_HEIGHT: usize = 7;
/// Width of the digit sheet (`numbers.png` holds ten 5-pixel glyphs side by side in a 50-pixel row).
const NUMBER_SHEET_WIDTH: usize = NUMBER_WIDTH * 10;

/// `MapColor.col` by id; index 0 is NONE. Copied from horizon-items' `MAP_COLORS`.
const BASE_COLORS: [u32; 62] = [
    0, 8368696, 16247203, 13092807, 16711680, 10526975, 10987431, 31744,
    16777215, 10791096, 9923917, 7368816, 4210943, 9402184, 16776437, 14188339,
    11685080, 6724056, 15066419, 8375321, 15892389, 5000268, 10066329, 5013401,
    8339378, 3361970, 6704179, 6717235, 10040115, 1644825, 16445005, 6085589,
    4882687, 55610, 8476209, 7340544, 13742497, 10441252, 9787244, 7367818,
    12223780, 6780213, 10505550, 3746083, 8874850, 5725276, 8014168, 4996700,
    4993571, 5001770, 9321518, 2430480, 12398641, 9715553, 6035741, 1474182,
    3837580, 5647422, 1356933, 6579300, 14200723, 8365974,
];

/// The four shade modifiers, indexed by `packed & 3`.
const SHADE_MODIFIERS: [u32; 4] = [180, 220, 255, 135];

/// The palette entry of a packed id (`4..=247`): base colour times the shade modifier.
pub fn palette_rgb(byte: u8) -> Option<[u8; 3]> {
    if !(4..=247).contains(&byte) {
        return None;
    }
    let col = BASE_COLORS[usize::from(byte >> 2)];
    let modifier = SHADE_MODIFIERS[usize::from(byte & 3)];
    let channel = |shift: u32| ((col >> shift & 255) * modifier / 255) as u8;
    Some([channel(16), channel(8), channel(0)])
}

/// The palette as `(byte, rgb)`, ascending by byte, as `MapColors.mapColors` lists it.
fn palette() -> &'static [(u8, [i32; 3])] {
    static PALETTE: OnceLock<Vec<(u8, [i32; 3])>> = OnceLock::new();
    PALETTE.get_or_init(|| {
        (4..=247u8)
            .filter_map(|b| palette_rgb(b).map(|c| (b, [i32::from(c[0]), i32::from(c[1]), i32::from(c[2])])))
            .collect()
    })
}

/// `colorDistanceEuclidian` (MapColors.kt): the weighted squared distance with the mean red as weight.
fn distance(p: [i32; 3], q: [i32; 3]) -> f64 {
    let r_mean = (p[0] + q[0]) / 2;
    let r_weight = f64::from(2 + r_mean / 256);
    let b_weight = f64::from(2 + (255 - r_mean) / 256);
    let dr = f64::from(p[0] - q[0]);
    let dg = f64::from(p[1] - q[1]);
    let db = f64::from(p[2] - q[2]);
    r_weight * dr * dr + 4.0 * dg * dg + b_weight * db * db
}

/// `closestColorByte`: the palette byte nearest to `rgb`. Pure black is always 116. Ties go to the lower byte.
pub fn closest_map_color(rgb: [u8; 3]) -> u8 {
    if rgb == [0, 0, 0] {
        return 116;
    }
    let p = [i32::from(rgb[0]), i32::from(rgb[1]), i32::from(rgb[2])];
    let mut best = (f64::INFINITY, 116u8);
    for &(byte, q) in palette() {
        let d = distance(p, q);
        if d < best.0 {
            best = (d, byte);
        }
    }
    best.1
}

/// `getMapColor`: the map colour index of a team's text colour, given as a `Formatting` name. `None` (no team) and
/// the colours the original does not list give 22.
pub fn map_color_index(name: &str) -> u8 {
    match name {
        "AQUA" => 31,
        "BLACK" => 29,
        "BLUE" => 12,
        "DARK_AQUA" => 23,
        "DARK_BLUE" => 25,
        "DARK_GRAY" | "GRAY" => 21,
        "DARK_GREEN" => 7,
        "DARK_PURPLE" => 24,
        "DARK_RED" => 28,
        "GOLD" => 15,
        "GREEN" => 33,
        "LIGHT_PURPLE" => 20,
        "RED" => 4,
        "WHITE" => 14,
        "YELLOW" => 30,
        _ => 22,
    }
}

/// The colour constants of MapRenderService.kt, as palette bytes.
struct Colors {
    background: u8,
    item_shadow: u8,
    margin: u8,
    shadow: u8,
    achieved: u8,
    achieved_shadow: u8,
    flashing: u8,
    locked: u8,
    white: u8,
}

fn colors() -> &'static Colors {
    static COLORS: OnceLock<Colors> = OnceLock::new();
    COLORS.get_or_init(|| Colors {
        background: closest_map_color([130, 130, 130]),
        item_shadow: closest_map_color([100, 100, 100]),
        margin: closest_map_color([199, 199, 199]),
        shadow: closest_map_color([20, 20, 20]),
        achieved: closest_map_color([100, 255, 100]),
        achieved_shadow: closest_map_color([80, 120, 80]),
        flashing: closest_map_color([178, 76, 216]),
        locked: closest_map_color([150, 0, 0]),
        white: closest_map_color([255, 255, 255]),
    })
}

/// The `Color.mix` of MapColors.kt: `base` mixed with `other` by `other`'s alpha `a` (0..=255), truncated per channel.
fn mix(base: [u8; 3], other: [u8; 3], a: u8) -> [u8; 3] {
    let alpha = f32::from(a) / 255.0;
    let ch = |i: usize| (f32::from(base[i]) * (1.0 - alpha) + f32::from(other[i]) * alpha) as u8;
    [ch(0), ch(1), ch(2)]
}

/// An icon or overlay picture: RGBA8, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl RgbaImage {
    /// Wraps pixels; panics when `rgba` is not `width * height * 4` bytes.
    pub fn from_rgba(width: usize, height: usize, rgba: Vec<u8>) -> Self {
        assert_eq!(rgba.len(), width * height * 4, "an RGBA picture of {width}x{height}");
        Self { width, height, rgba }
    }
}

/// Where the renderer gets an item's picture (`readTileImage` in the original). `None` means no picture.
pub trait IconSource {
    fn icon(&self, item: &str) -> Option<RgbaImage>;
}

/// Pictures held in memory (tests, or a caller that has already decoded them).
#[derive(Debug, Clone, Default)]
pub struct MemoryIcons(pub HashMap<String, RgbaImage>);

impl IconSource for MemoryIcons {
    fn icon(&self, item: &str) -> Option<RgbaImage> {
        self.0.get(item).cloned()
    }
}

/// PNG icons on disk, as `prepare_assets.py` writes them: `<dir>/<path>.png`, then `<dir>/<namespace>/<path>.png`.
/// The namespace is `minecraft` when the id has none.
#[derive(Debug, Clone)]
pub struct FileIcons {
    pub dir: PathBuf,
}

impl IconSource for FileIcons {
    fn icon(&self, item: &str) -> Option<RgbaImage> {
        let (namespace, path) = match item.split_once(':') {
            Some((ns, path)) => (ns, path),
            None => ("minecraft", item),
        };
        let flat = self.dir.join(format!("{path}.png"));
        if flat.is_file() {
            return read_png(&flat).ok();
        }
        let nested = self.dir.join(namespace).join(format!("{path}.png"));
        if nested.is_file() {
            return read_png(&nested).ok();
        }
        None
    }
}

/// Decodes a PNG file to RGBA8. Any colour type and bit depth is accepted.
pub fn read_png(path: &Path) -> Result<RgbaImage, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
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
    Ok(RgbaImage::from_rgba(info.width as usize, info.height as usize, rgba))
}

/// The map pictures of MapRenderService.kt: fallback, hidden (generic and per tier), the two frames and the digits.
/// Each is optional; a missing one falls back as the original's resource would not (see `hidden_image`).
#[derive(Debug, Clone, Default)]
pub struct Overlays {
    pub fallback: Option<RgbaImage>,
    pub hidden: Option<RgbaImage>,
    pub hidden_tiers: Vec<(TierLabel, RgbaImage)>,
    pub frame_advancement: Option<RgbaImage>,
    pub frame_forbidden: Option<RgbaImage>,
    pub numbers: Option<RgbaImage>,
}

impl Overlays {
    /// Reads `image_fallback.png`, `image_hidden.png`, `image_hidden_{s,a,b,c,d}.png`,
    /// `image_frame_advancement.png`, `image_frame_forbidden.png` and `numbers.png` from `dir`. Missing files are
    /// `None`; a file that exists but does not decode is an error.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let read = |name: &str| -> Result<Option<RgbaImage>, String> {
            let path = dir.join(name);
            if path.is_file() { read_png(&path).map(Some) } else { Ok(None) }
        };
        let mut hidden_tiers = Vec::new();
        for tier in [TierLabel::S, TierLabel::A, TierLabel::B, TierLabel::C, TierLabel::D] {
            if let Some(img) = read(&format!("image_hidden_{}.png", tier.name().to_lowercase()))? {
                hidden_tiers.push((tier, img));
            }
        }
        Ok(Self {
            fallback: read("image_fallback.png")?,
            hidden: read("image_hidden.png")?,
            hidden_tiers,
            frame_advancement: read("image_frame_advancement.png")?,
            frame_forbidden: read("image_frame_forbidden.png")?,
            numbers: read("numbers.png")?,
        })
    }
}

/// The decoration a tile's picture gets (`Decoration` in the original; `FREE_SPACE` does not occur on a native card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Decoration {
    #[default]
    None,
    Advancement,
    MultiItem,
    Forbidden,
}

/// One square as the renderer sees it (`CardTile` in the original, without the picture bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct MapTile {
    /// `isHidden`: drawn as the hidden picture (tier-specific when `tier` is set).
    pub hidden: bool,
    /// `itemTier`, used only for the hidden picture.
    pub tier: Option<TierLabel>,
    /// The item id whose picture is drawn; `None` draws the empty tile.
    pub item: Option<String>,
    /// The stack count; digits are drawn above 1.
    pub count: u32,
    pub decoration: Decoration,
    /// `image.isAchieved`: drawn in the achieved palette.
    pub achieved: bool,
    /// `isLocked`: a red X across the tile.
    pub locked: bool,
    /// `progress` (0..=1): regular tiles fill from the bottom with the flashing colour.
    pub progress: f32,
    /// Map colour names (`Formatting` names) of the other teams that have the square; each gets a stripe.
    pub achieved_by: Vec<String>,
    /// The tick the square was scored at (`isFlashing` for `FLASHING_DURATION`); `None` when it does not flash.
    pub flash_since: Option<u64>,
}

impl Default for MapTile {
    fn default() -> Self {
        Self {
            hidden: false,
            tier: None,
            item: None,
            count: 1,
            decoration: Decoration::None,
            achieved: false,
            locked: false,
            progress: 0.0,
            achieved_by: Vec::new(),
            flash_since: None,
        }
    }
}

impl MapTile {
    /// A square from `hidden_consume::tile_views`: hidden flag, item and tier. The other fields keep their defaults and
    /// the caller sets them.
    pub fn from_view(view: &TileView) -> Self {
        Self { hidden: view.hidden, item: view.item.clone(), tier: view.tier, ..Self::default() }
    }
}

/// A team's card as the renderer draws it: the team's map colour (`None` for a player on no team) and the 25 squares
/// in row-major order (index = row * 5 + column). Extra squares are ignored; missing ones are left as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct CardView {
    pub team_color: Option<String>,
    pub tiles: Vec<MapTile>,
}

/// What `render_card` draws with: the icon source and the overlay pictures. A `MapBuffer` caches pictures, so one
/// buffer must always be drawn with the same style.
pub struct Style<'a> {
    pub icons: &'a dyn IconSource,
    pub overlays: &'a Overlays,
}

/// The rectangle a draw touched, as `x0..x1` and `y0..y1` (exclusive ends). Empty when nothing changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl DirtyRect {
    pub const EMPTY: DirtyRect = DirtyRect { x0: 0, y0: 0, x1: 0, y1: 0 };
    pub const FULL: DirtyRect = DirtyRect { x0: 0, y0: 0, x1: MAP_SIZE, y1: MAP_SIZE };

    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// The smallest rectangle that holds both.
    pub fn union(self, other: DirtyRect) -> DirtyRect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        DirtyRect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// The 22 x 22 picture of one tile in the three palettes the tile can be drawn in.
#[derive(Debug, Clone)]
struct TileImage {
    regular: Vec<u8>,
    flashing: Vec<u8>,
    achieved: Vec<u8>,
}

impl TileImage {
    /// `BingoTileImage()`: the defaults, which are the background, the flash colour and the achieved colour.
    fn empty() -> Self {
        let c = colors();
        Self {
            regular: vec![c.background; IMAGE_SIZE * IMAGE_SIZE],
            flashing: vec![c.flashing; IMAGE_SIZE * IMAGE_SIZE],
            achieved: vec![c.achieved; IMAGE_SIZE * IMAGE_SIZE],
        }
    }

    /// Writes one pixel into all three palettes, as the decoration callback does.
    fn set_all(&mut self, x: i64, y: i64, color: u8) {
        if (0..IMAGE_SIZE as i64).contains(&x) && (0..IMAGE_SIZE as i64).contains(&y) {
            let i = x as usize + y as usize * IMAGE_SIZE;
            self.regular[i] = color;
            self.flashing[i] = color;
            self.achieved[i] = color;
        }
    }

    /// `BingoTileImage.fromBufferedImage`: the picture scaled to 16 pixels, centred in the 22-pixel tile, with a
    /// one-pixel item shadow and glass blended with the background.
    fn from_rgba(img: &RgbaImage) -> Self {
        let c = colors();
        let mut t = Self::empty();
        let (w, h) = (img.width as i64, img.height as i64);
        let sample = |x: i64, y: i64| -> [u8; 4] {
            if x < 0 || y < 0 || x >= w || y >= h {
                return [0, 0, 0, 0];
            }
            let i = (y as usize * img.width + x as usize) * 4;
            [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2], img.rgba[i + 3]]
        };
        for x in 0..IMAGE_SIZE {
            for y in 0..IMAGE_SIZE {
                let i = x + y * IMAGE_SIZE;
                let nx = (x as i64 - TILE_PADDING as i64) * w / TILE_IMAGE as i64;
                let ny = (y as i64 - TILE_PADDING as i64) * h / TILE_IMAGE as i64;
                let color = sample(nx, ny);
                let prev = sample(nx - 1, ny - 1);
                if color[3] == 0 {
                    if prev[3] != 0 {
                        t.regular[i] = c.item_shadow;
                        t.flashing[i] = c.item_shadow;
                        t.achieved[i] = c.achieved_shadow;
                    }
                    continue;
                }
                let rgb = if color[3] < 255 {
                    let a2 = (u32::from(color[3]) * 2).min(255) as u8;
                    mix([130, 130, 130], [color[0], color[1], color[2]], a2)
                } else {
                    [color[0], color[1], color[2]]
                };
                let regular = closest_map_color(rgb);
                t.regular[i] = regular;
                t.flashing[i] = regular;
                t.achieved[i] = closest_map_color(mix([100, 255, 100], rgb, 200));
            }
        }
        t
    }
}

/// The palette bytes of a picture, in its own row-major layout (`toMinecraftByteArray`): alpha 0 is 0.
fn palette_bytes(img: &RgbaImage) -> Vec<u8> {
    img.rgba
        .chunks_exact(4)
        .map(|p| if p[3] == 0 { 0 } else { closest_map_color([p[0], p[1], p[2]]) })
        .collect()
}

/// `drawTileDecorations`: the frame or cross, the "+" for several ways to score, and the stack count.
fn apply_decorations(t: &mut TileImage, count: u32, decoration: Decoration, overlays: &Overlays) {
    let c = colors();
    let frame = |t: &mut TileImage, img: &RgbaImage| {
        let arr = palette_bytes(img);
        for x in 0..IMAGE_SIZE {
            for y in 0..IMAGE_SIZE {
                let idx = x + y * IMAGE_SIZE;
                if idx < arr.len() && arr[idx] != 0 {
                    t.set_all(x as i64, y as i64, arr[idx]);
                }
            }
        }
    };
    match decoration {
        Decoration::Advancement => {
            if let Some(img) = &overlays.frame_advancement {
                frame(t, img);
            }
        }
        Decoration::MultiItem => {
            let offset = 6i64;
            for k in (offset - 2)..=(offset + 2) {
                t.set_all(offset, k, c.white);
                t.set_all(k, offset, c.white);
            }
        }
        Decoration::Forbidden => {
            if let Some(img) = &overlays.frame_forbidden {
                frame(t, img);
            }
        }
        Decoration::None => {}
    }
    if count > 1 {
        if let Some(img) = &overlays.numbers {
            let nums = palette_bytes(img);
            let digits: Vec<usize> = count.to_string().chars().filter_map(|ch| ch.to_digit(10)).map(|d| d as usize).collect();
            let n = digits.len() as i64;
            for (index, &digit) in digits.iter().enumerate() {
                let start_x = 20 - (n - index as i64) * (NUMBER_WIDTH as i64 + 1) + 1;
                let start_y = 20 - NUMBER_HEIGHT as i64;
                for x in 0..NUMBER_WIDTH {
                    for y in 0..NUMBER_HEIGHT {
                        let idx = digit * NUMBER_WIDTH + x + y * NUMBER_SHEET_WIDTH;
                        if idx >= nums.len() {
                            continue;
                        }
                        let dx = start_x + x as i64;
                        let dy = start_y + y as i64;
                        if !(0..=20).contains(&dx) {
                            continue;
                        }
                        let color = nums[idx];
                        if color != 0 {
                            t.set_all(dx, dy, color);
                            t.set_all(dx + 1, dy + 1, c.item_shadow);
                        }
                    }
                }
            }
        }
    }
}

/// Cached pictures: an item with its count and decoration, or the hidden picture of a tier (`None`: generic).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CacheKey {
    Item { name: String, count: u32, decoration: Decoration },
    Hidden(Option<&'static str>),
}

/// The state of the last drawing of one tile: what was painted, and whether it was in the flash-on half.
#[derive(Debug, Clone, PartialEq)]
struct TileKey {
    tile: MapTile,
    flashing: bool,
}

#[derive(Debug, Clone)]
struct Drawn {
    team_index: u8,
    tiles: Vec<Option<TileKey>>,
}

/// The 128 x 128 map picture of one map, kept between draws. Its pixels are the map colour ids.
#[derive(Debug, Clone)]
pub struct MapBuffer {
    pixels: Vec<u8>,
    drawn: Option<Drawn>,
    cache: HashMap<CacheKey, TileImage>,
}

impl Default for MapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl MapBuffer {
    pub fn new() -> Self {
        Self { pixels: vec![0; MAP_BYTES], drawn: None, cache: HashMap::new() }
    }

    /// The 16 384 map colour ids, row-major.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

fn put(pixels: &mut [u8], x: usize, y: usize, v: u8) {
    pixels[y * MAP_SIZE + x] = v;
}

/// `drawMapBorders`: the team-coloured frame, the grid lines under each row and column.
fn draw_borders(pixels: &mut [u8], team_index: u8) {
    let base = u32::from(team_index) * 4;
    let light = (base + 2) as u8;
    let mid = (base + 1) as u8;
    let dark = (base + 3) as u8;
    let c = colors();
    let last = MAP_SIZE - 1;
    for x in 0..MAP_SIZE {
        put(pixels, x, 0, light);
        put(pixels, 0, x, light);
        put(pixels, x, last, dark);
        put(pixels, last, x, dark);
    }
    for x in 1..last {
        for y in 1..(MAP_BORDER - 1) {
            let y_inv = last - y;
            put(pixels, x, y, mid);
            put(pixels, y, x, mid);
            put(pixels, x, y_inv, mid);
            put(pixels, y_inv, x, mid);
        }
    }
    for x in (MAP_BORDER - 1)..=(MAP_SIZE - MAP_BORDER) {
        put(pixels, x, MAP_BORDER - 1, dark);
        put(pixels, MAP_BORDER - 1, x, dark);
        put(pixels, x, MAP_SIZE - MAP_BORDER, light);
        put(pixels, MAP_SIZE - MAP_BORDER, x, light);
    }
    for x in MAP_BORDER..(MAP_SIZE - MAP_BORDER) {
        for y in 1..=4 {
            let ya = MAP_BORDER + y * TILE_SIZE - TILE_MARGIN + 1;
            put(pixels, x, ya, c.shadow);
            put(pixels, ya, x, c.shadow);
        }
    }
    for x in MAP_BORDER..(MAP_SIZE - MAP_BORDER) {
        for y in 1..=4 {
            let ya = MAP_BORDER + y * TILE_SIZE - TILE_MARGIN;
            put(pixels, x, ya, c.margin);
            put(pixels, ya, x, c.margin);
        }
    }
}

/// The 22 x 22 rectangle of tile `index` (`drawMapTile`'s dirty marks).
fn tile_rect(index: usize) -> DirtyRect {
    let x0 = MAP_BORDER + TILE_SIZE * (index % 5);
    let y0 = MAP_BORDER + TILE_SIZE * (index / 5);
    DirtyRect { x0, y0, x1: x0 + IMAGE_SIZE, y1: y0 + IMAGE_SIZE }
}

fn item_image(cache: &mut HashMap<CacheKey, TileImage>, tile: &MapTile, item: &str, style: &Style<'_>) -> TileImage {
    let key = CacheKey::Item { name: item.to_string(), count: tile.count, decoration: tile.decoration };
    if let Some(found) = cache.get(&key) {
        return found.clone();
    }
    let image = match style.icons.icon(item) {
        Some(icon) => {
            let mut t = TileImage::from_rgba(&icon);
            apply_decorations(&mut t, tile.count, tile.decoration, style.overlays);
            t
        }
        None => TileImage::empty(),
    };
    cache.insert(key, image.clone());
    image
}

fn hidden_image(cache: &mut HashMap<CacheKey, TileImage>, tier: Option<TierLabel>, style: &Style<'_>) -> TileImage {
    let key = CacheKey::Hidden(tier.map(|t| t.name()));
    if let Some(found) = cache.get(&key) {
        return found.clone();
    }
    let overlays = style.overlays;
    let source = match tier {
        Some(t) => overlays
            .hidden_tiers
            .iter()
            .find(|(x, _)| *x == t)
            .map(|(_, img)| img)
            .or(overlays.fallback.as_ref()),
        None => overlays.hidden.as_ref(),
    };
    let image = source.map_or_else(TileImage::empty, TileImage::from_rgba);
    cache.insert(key, image.clone());
    image
}

/// `drawMapTile`: paints one tile (picture, lock, team stripes) at `index`.
fn draw_tile(
    pixels: &mut [u8],
    index: usize,
    tile: &MapTile,
    flashing: bool,
    image: &TileImage,
) {
    let c = colors();
    let tile_x = MAP_BORDER + TILE_SIZE * (index % 5);
    let tile_y = MAP_BORDER + TILE_SIZE * (index / 5);

    let is_regular = !tile.hidden && !flashing && !tile.achieved;
    let base: &[u8] = if tile.hidden {
        &image.regular
    } else if flashing {
        &image.flashing
    } else if tile.achieved {
        &image.achieved
    } else {
        &image.regular
    };

    for inner_y in 0..=INNER_MAX {
        let y = tile_y + inner_y;
        let row_start = tile_x + y * MAP_SIZE;
        let src = if is_regular && (inner_y as f32) > (INNER_MAX as f32) * (1.0 - tile.progress) {
            &image.flashing
        } else {
            base
        };
        let from = inner_y * IMAGE_SIZE;
        pixels[row_start..row_start + IMAGE_SIZE].copy_from_slice(&src[from..from + IMAGE_SIZE]);
    }

    if tile.locked {
        for t in 0..=INNER_MAX {
            let x = tile_x + t;
            let y = tile_y + t;
            put(pixels, x, y, c.locked);
            let y_inv = tile_y + INNER_MAX - t;
            put(pixels, x, y_inv, c.locked);
            if t > 0 {
                put(pixels, x - 1, y, c.locked);
                put(pixels, x - 1, y_inv, c.locked);
            }
        }
    }

    let teams: Vec<u8> = tile.achieved_by.iter().map(|name| map_color_index(name)).collect();
    let n = teams.len();
    if n > 0 {
        for inner_t in 0..=(2 * INNER_MAX + 1) {
            let team_slot = inner_t * n / (2 * INNER_MAX + 1);
            let Some(&team) = teams.get(team_slot) else { continue };
            let variant = if inner_t % 2 == 0 { 2 } else { 3 };
            let color = (u32::from(team) * 4 + variant) as u8;
            if inner_t < INNER_MAX {
                put(pixels, tile_x + inner_t, tile_y, color);
                put(pixels, tile_x, tile_y + inner_t, color);
            } else {
                put(pixels, tile_x + inner_t - INNER_MAX, tile_y + INNER_MAX, color);
                put(pixels, tile_x + INNER_MAX, tile_y + inner_t - INNER_MAX, color);
            }
        }
    }
}

/// Draws `card` into `buf` for the tick `tick` and returns the rectangle that changed. A fresh buffer, or one whose
/// team colour changed, is drawn in full (`FULL`); otherwise only tiles whose drawn state changed are repainted, and
/// the result is `EMPTY` when nothing changed. A scored tile toggles between its flash colour and its normal
/// picture every 10 ticks for 80 ticks after the score (`(ticks / 10) % 2 == 0` is the flash-on half).
pub fn render_card(card: &CardView, style: &Style<'_>, tick: u64, buf: &mut MapBuffer) -> DirtyRect {
    let team_index = card.team_color.as_deref().map_or(22, map_color_index);
    let mut dirty = DirtyRect::EMPTY;

    let MapBuffer { pixels, drawn, cache } = buf;
    let fresh = !matches!(drawn, Some(d) if d.team_index == team_index);
    if fresh {
        draw_borders(pixels, team_index);
        *drawn = Some(Drawn { team_index, tiles: vec![None; CARD_TILES] });
        dirty = DirtyRect::FULL;
    }
    let state = drawn.as_mut().expect("drawn is set above");

    for (index, tile) in card.tiles.iter().enumerate().take(CARD_TILES) {
        let flashing = tile.flash_since.is_some_and(|since| tick >= since && tick - since < FLASH_TICKS)
            && (tick / FLASH_STEP_TICKS) % 2 == 0;
        let mut plain = tile.clone();
        plain.flash_since = None;
        let key = TileKey { tile: plain, flashing };
        if state.tiles[index].as_ref() == Some(&key) {
            continue;
        }
        let image = if tile.hidden {
            hidden_image(cache, tile.tier, style)
        } else if let Some(item) = &tile.item {
            item_image(cache, tile, item, style)
        } else {
            TileImage::empty()
        };
        draw_tile(pixels, index, &key.tile, flashing, &image);
        state.tiles[index] = Some(key);
        dirty = dirty.union(tile_rect(index));
    }
    dirty
}

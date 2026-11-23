//! The lobby's in-world parts (`MenuController` of the original): the button panel on the wall at the menu spot, the
//! eight team pads, the text displays that label them, and the console commands for titles and sounds. Pure logic: which
//! clicked block means what, and the command lines to run. The adapter (`bingo-native/src/lobby_scene.rs`) runs them.
//!
//! The spots are the original's lobby POIs (`lobby_poi.json.gz`): `menu` at (0, 17, -5) and one `team` entry per pad.
use super::teams::{preset, PRESETS};
use super::texts::{self, Line};

/// The tag every text display of the lobby carries, so the scene can be removed and made again without duplicates.
pub const UI_TAG: &str = "bingo_lobby_ui";
/// The lobby's runtime level.
pub const LOBBY_LEVEL: &str = "bingo:lobby";

/// The block of the menu spot (POI `menu`, yaw 0): the panel hangs in front of the wall around it.
pub const MENU_POS: [i32; 3] = [0, 17, -5];
/// The wall blocks a click counts on, inclusive: x -2..3, y 17..20 on the wall's row z -5 (the three oak wall signs and the
/// stone around them). The panel's tiles stand in front of this wall, so a click ray ends on one of these blocks.
const WALL_MIN: [i32; 3] = [-2, 17, -5];
const WALL_MAX: [i32; 3] = [3, 20, -5];
/// The panel's plane: displays stand just in front of the wall's south face (z -4), where the signs hang.
pub const PANEL_Z: f64 = -3.9;
/// The rotation `[yaw, pitch]` of the panel's displays. Entity yaw 0 faces south (+z) and a text display's readable side is
/// its front, so 0 should read from a player standing south of the wall. Flip to `[180.0, 0.0]` if a live test shows mirrored
/// or missing text.
pub const PANEL_ROTATION: [f32; 2] = [0.0, 0.0];

/// A tile of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Title,
    Players,
    Teams,
    Settings,
    Reroll,
    Start,
}

/// The tiles and their rectangles `[x0, y0, x1, y1]` in world coordinates on the panel plane, 5 blocks wide (x -2..3) and
/// 3 high (y 17.05..19.95). The summon commands and the click mapping both read this table. Left column: title, teams,
/// settings, reroll; right column: players, and the tall start tile.
pub const TILES: [(Tile, [f64; 4]); 6] = [
    (Tile::Title, [-2.0, 19.30, 0.95, 19.95]),
    (Tile::Teams, [-2.0, 18.55, 0.95, 19.20]),
    (Tile::Settings, [-2.0, 17.80, 0.95, 18.45]),
    (Tile::Reroll, [-2.0, 17.05, 0.95, 17.70]),
    (Tile::Players, [1.05, 19.30, 3.0, 19.95]),
    (Tile::Start, [1.05, 17.05, 3.0, 19.20]),
];

/// The tile under the point `(x, y)` of the panel's plane, if any. The edges belong to the tile above and to the right.
pub fn tile_at(x: f64, y: f64) -> Option<Tile> {
    TILES
        .iter()
        .find(|(_, [x0, y0, x1, y1])| x >= *x0 && x < *x1 && y >= *y0 && y < *y1)
        .map(|(tile, _)| *tile)
}

/// The team pads (POI `team`): the team's key and the block of the pad. The carpet is the block above.
pub const PADS: [(&str, [i32; 3]); 8] = [
    ("bingo_pink", [5, 25, -16]),
    ("bingo_green", [-15, 28, -20]),
    ("bingo_orange", [-17, 24, -4]),
    ("bingo_red", [-17, 23, 14]),
    ("bingo_gray", [-2, 21, 18]),
    ("bingo_aqua", [16, 25, 17]),
    ("bingo_yellow", [18, 23, 6]),
    ("bingo_blue", [16, 23, -9]),
];

/// What a right click on a block of the lobby does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Opens the lobby menu on its root page (the settings tile, a click on no tile, and the teams tile: the menu has no
    /// page of its own for teams, its team count and auto shuffle are on the root page).
    Menu,
    /// Starts the game, as the menu's start slot does.
    Start,
    /// Rerolls the card, as the menu's reroll slot does.
    Reroll,
    /// Joins the team with this key (`bingo_red`).
    Join(&'static str),
}

/// The action of a right click on the block `pos` whose hit point is `point` in the lobby: a block of the panel's wall
/// means the tile under the hit's `x, y` (the ray ends on the wall behind the tiles), the menu when no tile is there; a pad's
/// block or the block above it (its carpet) joins the pad's team. Any other block does nothing.
pub fn click_at(pos: [i32; 3], point: [f64; 3]) -> Option<Click> {
    if (0..3).all(|axis| (WALL_MIN[axis]..=WALL_MAX[axis]).contains(&pos[axis])) {
        return Some(match tile_at(point[0], point[1]) {
            Some(Tile::Start) => Click::Start,
            Some(Tile::Reroll) => Click::Reroll,
            Some(Tile::Teams | Tile::Settings | Tile::Title | Tile::Players) | None => Click::Menu,
        });
    }
    PADS.iter()
        .find(|(_, pad)| pos == *pad || pos == [pad[0], pad[1] + 1, pad[2]])
        .map(|(team, _)| Click::Join(team))
}

/// The chat colour name of a team (`red`, `light_purple`), for a title or a text. `None` for an unknown team.
pub fn team_color(team: &str) -> Option<String> {
    preset(team).map(|p| p.color.to_ascii_lowercase())
}

/// A string as a quoted literal of JSON, or of SNBT (double quotes, backslash escapes).
fn quoted(text: &str, snbt: bool) -> String {
    if !snbt {
        return serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned());
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A compound with already rendered values, its keys bare in SNBT and quoted in JSON.
fn object(snbt: bool, fields: &[(&str, String)]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|(key, value)| if snbt { format!("{key}:{value}") } else { format!("\"{key}\":{value}") })
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// A literal text component.
fn literal(text: &str, color: Option<&str>, bold: bool, snbt: bool) -> String {
    let mut fields = vec![("text", quoted(text, snbt))];
    if let Some(color) = color {
        fields.push(("color", quoted(color, snbt)));
    }
    if bold {
        fields.push(("bold", "true".to_owned()));
    }
    object(snbt, &fields)
}

/// A line as a text component: the translation with the English as its fallback and the arguments as literals, so a client
/// without the key shows the English. The empty line (no template) is an empty literal.
fn component(line: &Line, color: Option<&str>, bold: bool, snbt: bool) -> String {
    if line.template.is_empty() {
        return literal("", None, false, snbt);
    }
    let mut fields = vec![("translate", quoted(line.key, snbt)), ("fallback", quoted(line.template, snbt))];
    if !line.args.is_empty() {
        let args: Vec<String> = line.args.iter().map(|arg| quoted(arg, snbt)).collect();
        fields.push(("with", format!("[{}]", args.join(","))));
    }
    if let Some(color) = color {
        fields.push(("color", quoted(color, snbt)));
    }
    if bold {
        fields.push(("bold", "true".to_owned()));
    }
    object(snbt, &fields)
}

/// Fade in, stay and fade out of a countdown number (no subtitle), in ticks: each replaces the one before a second later.
const COUNT_TIMES: [u32; 3] = [0, 25, 5];
/// Fade in, stay and fade out of a title with a subtitle (the start's two).
const STATE_TIMES: [u32; 3] = [5, 40, 10];

/// The console commands that show `title` and `subtitle` to the player called `name`, the title in `color` (a chat colour
/// name) when given. A title with no subtitle is a countdown number and stays a shorter time. The subtitle goes first, as
/// `/title` shows both once the title is set.
pub fn title_commands(name: &str, title: &Line, subtitle: &Line, color: Option<&str>) -> Vec<String> {
    let times = if subtitle.template.is_empty() { COUNT_TIMES } else { STATE_TIMES };
    vec![
        format!("title {name} times {} {} {}", times[0], times[1], times[2]),
        format!("title {name} subtitle {}", component(subtitle, None, false, false)),
        format!("title {name} title {}", component(title, color, false, false)),
    ]
}

/// The console command that plays `sound` to the player called `name`, at their own place.
pub fn sound_command(name: &str, sound: &str) -> String {
    format!("playsound {sound} master {name}")
}

/// The wooden button's click, when a slot of the lobby menu is clicked.
pub const SOUND_BUTTON: &str = "minecraft:block.wooden_button.click_on";
/// The lodestone compass lock, when a player changes team.
pub const SOUND_TEAM_CHANGED: &str = "minecraft:item.lodestone_compass.lock";
/// A primed TNT, when the card is rerolled.
pub const SOUND_REROLL: &str = "minecraft:entity.tnt.primed";

/// A dark, translucent tile background (ARGB) and the green one of the info tiles.
const BACKGROUND_DARK: u32 = 0xB020_2428;
const BACKGROUND_GREEN: u32 = 0xB012_3A1A;
/// A pad label's background.
const BACKGROUND_LABEL: u32 = 0x8000_0000;
/// The width of a panel tile's text against its height, so a long label fits its tile.
const TEXT_ASPECT: f64 = 0.6;
/// A text display's line height and a space's width, in pixels (40 pixels are a block at scale 1).
const LINE_PX: f64 = 9.0;
const SPACE_PX: f64 = 4.0;

/// The command for one text display in the lobby, at `pos`, with `text` (an SNBT component). `scale` is `[x, y]`; `billboard`
/// is `fixed` (then `rotation` is its `[yaw, pitch]`) or `center` (faces the viewer).
fn summon_display(pos: [f64; 3], scale: [f64; 2], billboard: &str, rotation: [f32; 2], background: u32, text: &str) -> String {
    format!(
        "execute in {LOBBY_LEVEL} run summon minecraft:text_display {} {} {} {{Tags:[\"{UI_TAG}\"],billboard:\"{billboard}\",\
         Rotation:[{:?}f,{:?}f],alignment:\"center\",background:{},shadow:true,brightness:{{block:15,sky:15}},\
         transformation:{{scale:[{:.3}f,{:.3}f,1f],translation:[0f,0f,0f],left_rotation:[0f,0f,0f,1f],right_rotation:[0f,0f,0f,1f]}},\
         text:{text}}}",
        pos[0], pos[1], pos[2], rotation[0], rotation[1], background as i32, scale[0], scale[1]
    )
}

/// The width of a text in pixels, roughly: a space is 4, any other glyph 6.
fn text_px(text: &str) -> f64 {
    text.chars().map(|c| if c == ' ' { SPACE_PX } else { 6.0 }).sum()
}

/// One panel tile as a summon command: the label centred in `rect` on the panel plane. The label has blank lines above and
/// below (`blank_lines` each) and spaces beside it, so the display's background about fills the rectangle; the scale makes
/// the text block as high as the rectangle.
fn tile_command(rect: [f64; 4], line: &Line, color: &str, background: u32, blank_lines: usize) -> String {
    let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
    let lines = 1 + 2 * blank_lines;
    let scale_y = height * 40.0 / (lines as f64 * LINE_PX + 2.0);
    let scale_x = scale_y * TEXT_ASPECT;
    let wanted_px = width * 40.0 / scale_x;
    let pad = ((wanted_px - text_px(line.template)) / (2.0 * SPACE_PX)).floor().max(0.0) as usize;
    let spaces = literal(&" ".repeat(pad), None, false, true);
    let newlines = literal(&"\n".repeat(blank_lines), None, false, true);
    let mut parts = Vec::new();
    if blank_lines > 0 {
        parts.push(newlines.clone());
    }
    parts.push(spaces.clone());
    parts.push(component(line, Some(color), false, true));
    parts.push(spaces);
    if blank_lines > 0 {
        parts.push(newlines);
    }
    let text = object(true, &[("text", quoted("", true)), ("extra", format!("[{}]", parts.join(",")))]);
    summon_display(
        [(rect[0] + rect[2]) / 2.0, rect[1], PANEL_Z],
        [scale_x, scale_y],
        "fixed",
        PANEL_ROTATION,
        background,
        &text,
    )
}

/// The command that removes every display of the scene, so making it again never duplicates.
pub fn clear_command() -> String {
    format!("execute in {LOBBY_LEVEL} run kill @e[type=minecraft:text_display,tag={UI_TAG}]")
}

/// The commands that summon the displays: one for each tile of the panel and, for each pad, the team's `[symbol name]` 1.4
/// blocks above the pad block and `Click to join!` 1.1 above it. `clear_command` is not among them.
pub fn summon_commands() -> Vec<String> {
    let mut commands = Vec::new();
    for (tile, rect) in TILES {
        let (line, color, background, blank) = match tile {
            Tile::Title => (texts::lobby_panel_title(), "green", BACKGROUND_GREEN, 0),
            Tile::Players => (texts::lobby_panel_players(), "green", BACKGROUND_GREEN, 0),
            Tile::Teams => (texts::lobby_panel_teams(), "white", BACKGROUND_DARK, 0),
            Tile::Settings => (texts::lobby_panel_settings(), "white", BACKGROUND_DARK, 0),
            Tile::Reroll => (texts::lobby_panel_reroll(), "white", BACKGROUND_DARK, 0),
            Tile::Start => (texts::lobby_menu_start(), "white", BACKGROUND_DARK, 3),
        };
        commands.push(tile_command(rect, &line, color, background, blank));
    }
    for (key, pad) in PADS {
        let Some(team) = PRESETS.iter().find(|p| p.key == key) else { continue };
        let color = team.color.to_ascii_lowercase();
        let bare = key.strip_prefix("bingo_").unwrap_or(key);
        let name_key = format!("bingo.team.{bare}");
        let name = object(
            true,
            &[
                ("text", quoted(&format!("[{} ", team.symbol), true)),
                ("color", quoted(&color, true)),
                (
                    "extra",
                    format!(
                        "[{},{}]",
                        object(
                            true,
                            &[
                                ("translate", quoted(&name_key, true)),
                                ("fallback", quoted(team.name, true)),
                                ("color", quoted(&color, true)),
                            ]
                        ),
                        literal("]", Some(&color), false, true)
                    ),
                ),
            ],
        );
        let (x, y, z) = (f64::from(pad[0]) + 0.5, f64::from(pad[1]), f64::from(pad[2]) + 0.5);
        commands.push(summon_display([x, y + 1.4, z], [1.0, 1.0], "center", [0.0, 0.0], BACKGROUND_LABEL, &name));
        commands.push(summon_display(
            [x, y + 1.1, z],
            [1.0, 1.0],
            "center",
            [0.0, 0.0],
            BACKGROUND_LABEL,
            &component(&texts::click_to_join(), Some("gray"), false, true),
        ));
    }
    commands
}

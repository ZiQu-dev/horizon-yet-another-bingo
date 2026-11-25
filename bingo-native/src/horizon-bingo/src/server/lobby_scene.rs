//! The lobby's in-world parts (`MenuController` of the original): the button panel on the wall at the menu spot, the
//! eight team pads, the text displays that label them, and the console commands for titles and sounds. Pure logic: which
//! clicked block means what, and the command lines to run. The adapter (`bingo-native/src/lobby_scene.rs`) runs them.
//!
//! The spots are the original's lobby POIs (`lobby_poi.json.gz`): `menu` at (0, 17, -5) and one `team` entry per pad.
//!
//! The panel is the original's `createRootPage`, from the data dump of the real mod (`docs/original-ledger/panel-dump.md`):
//! five columns from x -4.8 to 5.8 (Game Mode, Goal and Time Limit, Difficulty, Shufflificator, the start column). Every
//! tile is a `block_display` of concrete (lime header, gray off, white on) with `text_display` labels and glyphs in front
//! of it; a state change only rewrites a tile's block, texts and brightness, never its place. The click is mapped from the
//! hit point on the wall behind the panel to a tile rectangle (the original puts interaction entities in front).
use super::difficulty;
use super::menu;
use super::options::{Goal, Mode, DEFAULT_TEAM_COUNT};
use super::teams::{preset, PRESETS};
use super::texts::{self, Line, PanelMode};

/// The tag every text display of the lobby carries, so the scene can be removed and made again without duplicates.
pub const UI_TAG: &str = "bingo_lobby_ui";
/// The lobby's runtime level.
pub const LOBBY_LEVEL: &str = "bingo:lobby";

/// The block of the menu spot (POI `menu`, yaw 0): the panel hangs in front of the wall around it.
pub const MENU_POS: [i32; 3] = [0, 17, -5];
/// The wall blocks a click counts on, inclusive. The panel hangs in the recess of the lobby's wall: its tiles span x -4.8 to
/// 5.8 and y 16.4 to 18.8 and stand 0.05 in front of the wall's face at z -5, so a click ray ends on one of the blocks at
/// x -5..5, z -6 (or -5, when the cell is solid), y 16..21 behind them. `click_at` then maps the hit's `x, y` to a tile.
const WALL_MIN: [i32; 3] = [-5, 16, -7];
const WALL_MAX: [i32; 3] = [5, 21, -5];
/// The z of the panel's text (the original's anchor `(0.5, 19, -4.95)`): the tiles' blocks are 0.051 behind it, the
/// hollow marker 0.005 in front of it.
pub const PANEL_Z: f64 = -4.95;

/// A tile of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    /// The green header over a column.
    HeaderMode,
    Mode(PanelMode),
    Features,
    HeaderGoal,
    GoalFewer,
    GoalValue,
    GoalMore,
    /// `Lines` / `Items`: switches the goal between the two.
    GoalKind,
    HeaderTime,
    TimeFewer,
    TimeValue,
    TimeMore,
    More,
    HeaderDifficulty,
    /// One of the four presets (0 easy .. 3 extreme).
    Difficulty(usize),
    Customize,
    HeaderTeams,
    /// Auto shuffle's toggle.
    AutoShuffle,
    /// One team fewer, the `-` beside the count.
    TeamsFewer,
    /// The `N teams` tile.
    Teams,
    /// One team more, the `+` beside the count.
    TeamsMore,
    Shuffle,
    Reroll,
    BestTime,
    Players,
    Start,
    ResetDefaults,
}

/// How a tile is drawn (the original's `registerTitlePanel`, `registerNumberInput`, `registerTileButton`,
/// `registerIconButton`).
#[derive(Debug, Clone, Copy)]
enum Style {
    /// A green header: lime block (sky 3), pale green text.
    Header,
    /// The `-` / `+` square of a number input: always white, black text.
    Stepper,
    /// The value between two steppers: gray block, white text with a shadow.
    Value,
    /// A switch or one choice of several: the hollow and the filled diamond, white when on and gray when off.
    Toggle,
    /// A game mode tile: a toggle with a big icon (the glyph) and a small caption.
    Mode(&'static str),
    /// A plain button, never on: an optional corner glyph, and the block's sky brightness (14, or 8 for the alt ones).
    Button { glyph: Option<&'static str>, sky: u8 },
}

/// A tile's place and style, in hundredths of a block relative to the menu block `(0, 17, -5)`: the centre x, the bottom y,
/// the width and the height. The numbers are the original's (`docs/original-ledger/panel-dump.md`).
struct Def {
    tile: Tile,
    cx: i32,
    y: i32,
    w: i32,
    h: i32,
    style: Style,
}

const fn def(tile: Tile, cx: i32, y: i32, w: i32, h: i32, style: Style) -> Def {
    Def { tile, cx, y, w, h, style }
}

/// Hundredths of a block as a number of blocks.
const fn blocks(hundredths: i32) -> f64 {
    hundredths as f64 / 100.0
}

const BUTTON: Style = Style::Button { glyph: None, sky: 14 };
const fn alt(glyph: &'static str) -> Style {
    Style::Button { glyph: Some(glyph), sky: 8 }
}

/// The 33 tiles of the root page, by column (Game Mode, Goal and Time Limit, Difficulty, Shufflificator, the start column).
const DEFS: [Def; 33] = [
    def(Tile::HeaderMode, -355, 150, 250, 30, Style::Header),
    def(Tile::Mode(PanelMode::Lockout), -420, 70, 120, 70, Style::Mode("\u{1F512}")),
    def(Tile::Mode(PanelMode::Inventory), -290, 70, 120, 70, Style::Mode("\u{1F392}")),
    def(Tile::Mode(PanelMode::HiddenItems), -420, -10, 120, 70, Style::Mode("\u{2753}")),
    def(Tile::Mode(PanelMode::ConsumeItems), -290, -10, 120, 70, Style::Mode("\u{2B1A}")),
    def(Tile::Features, -355, -60, 250, 40, alt("\u{22EF}")),
    def(Tile::HeaderGoal, -120, 150, 200, 30, Style::Header),
    def(Tile::GoalFewer, -205, 110, 30, 30, Style::Stepper),
    def(Tile::GoalValue, -120, 110, 120, 30, Style::Value),
    def(Tile::GoalMore, -35, 110, 30, 30, Style::Stepper),
    def(Tile::GoalKind, -120, 70, 200, 30, Style::Button { glyph: Some("\u{21C4}"), sky: 14 }),
    def(Tile::HeaderTime, -120, 30, 200, 30, Style::Header),
    def(Tile::TimeFewer, -205, -10, 30, 30, Style::Stepper),
    def(Tile::TimeValue, -120, -10, 120, 30, Style::Value),
    def(Tile::TimeMore, -35, -10, 30, 30, Style::Stepper),
    def(Tile::More, -120, -60, 200, 40, alt("\u{22EF}")),
    def(Tile::HeaderDifficulty, 70, 150, 160, 30, Style::Header),
    def(Tile::Difficulty(0), 70, 110, 160, 30, Style::Toggle),
    def(Tile::Difficulty(1), 70, 70, 160, 30, Style::Toggle),
    def(Tile::Difficulty(2), 70, 30, 160, 30, Style::Toggle),
    def(Tile::Difficulty(3), 70, -10, 160, 30, Style::Toggle),
    def(Tile::Customize, 70, -60, 160, 40, alt("\u{22EF}")),
    def(Tile::HeaderTeams, 260, 150, 200, 30, Style::Header),
    def(Tile::AutoShuffle, 260, 110, 200, 30, Style::Toggle),
    def(Tile::TeamsFewer, 175, 70, 30, 30, Style::Stepper),
    def(Tile::Teams, 260, 70, 120, 30, Style::Value),
    def(Tile::TeamsMore, 345, 70, 30, 30, Style::Stepper),
    def(Tile::Shuffle, 260, 30, 200, 30, BUTTON),
    def(Tile::Reroll, 260, -60, 200, 40, alt("\u{2684}")),
    def(Tile::BestTime, 475, 150, 210, 30, Style::Header),
    def(Tile::Players, 475, 110, 210, 30, Style::Header),
    def(Tile::Start, 475, -10, 210, 110, Style::Button { glyph: None, sky: 8 }),
    def(Tile::ResetDefaults, 475, -60, 210, 40, alt("\u{27F3}")),
];

/// The rectangle `[x0, y0, x1, y1]` of a tile in world coordinates (x -4.8 to 5.8, y 16.4 to 18.8).
const fn rect(d: &Def) -> [f64; 4] {
    [blocks(d.cx - d.w / 2), blocks(1700 + d.y), blocks(d.cx + d.w / 2), blocks(1700 + d.y + d.h)]
}

/// The tiles and their rectangles `[x0, y0, x1, y1]` in world coordinates: the click mapping and the summon commands both
/// read the same table. The tiles are 0.1 apart, and a click in the gap is no tile.
pub const TILES: [(Tile, [f64; 4]); 33] = {
    let mut out = [(Tile::HeaderMode, [0.0; 4]); 33];
    let mut i = 0;
    while i < 33 {
        out[i] = (DEFS[i].tile, rect(&DEFS[i]));
        i += 1;
    }
    out
};

/// The tag of a tile's display, so it alone can be made again when what it shows changes: `bingo_lobby_tile_` and the
/// tile's name in lower case (`bingo_lobby_tile_difficulty_2`).
pub fn tile_tag(tile: Tile) -> String {
    let name: String = format!("{tile:?}")
        .chars()
        .filter(|c| *c != ')')
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    format!("bingo_lobby_tile_{name}")
}

/// What the panel's live tiles show: the rules and the players that the tiles read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelState {
    pub auto_shuffle: bool,
    pub teams: usize,
    pub goal: Goal,
    /// The time limit in minutes, `None` when off.
    pub time_limit: Option<u32>,
    /// Which of the difficulty presets the card's tier counts equal (an index into `difficulty::PRESETS`), if any.
    pub difficulty: Option<usize>,
    pub mode: Mode,
    pub hidden_items: bool,
    pub consume_items: bool,
    /// Players online and the most the server takes.
    pub players: usize,
    pub max_players: u32,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            auto_shuffle: false,
            teams: DEFAULT_TEAM_COUNT,
            goal: Goal::Lines(1),
            time_limit: None,
            difficulty: Some(0),
            mode: Mode::Standard,
            hidden_items: false,
            consume_items: false,
            players: 0,
            max_players: 8,
        }
    }
}

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

/// A tile that changes a rule by a command that depends on what the rule is now (`panel_command` builds the line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    GoalFewer,
    GoalMore,
    GoalKind,
    TimeFewer,
    TimeMore,
    /// One of the four presets shown (0 easy .. 3 extreme).
    Difficulty(usize),
    Mode(PanelMode),
}

/// A page of the chest menu that a tile opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPage {
    Features,
    Goal,
    Items,
}

/// What a right click on a block of the lobby does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Opens a page of the chest menu: the tiles of the features the panel has no tile row for.
    Open(PanelPage),
    /// A rule tile: the command comes from the rule's current value, through the menu's own gate.
    Panel(PanelAction),
    /// Starts the game, as the menu's start slot does.
    Start,
    /// Rerolls the card, as the menu's reroll slot does.
    Reroll,
    /// Toggles the auto shuffle rule, as the menu's slot does.
    AutoShuffle,
    /// One team fewer in the auto shuffle (the menu's slot).
    TeamsFewer,
    /// One team more in the auto shuffle (the menu's slot).
    TeamsMore,
    /// Shuffles the teams (the menu's shuffle slot).
    Shuffle,
    /// Every rule back to its default (the menu's slot).
    ResetDefaults,
    /// Joins the team with this key (`bingo_red`).
    Join(&'static str),
}

/// What a click on `tile` does, if it is a button: the headers and the value tiles only show.
pub fn tile_click(tile: Tile) -> Option<Click> {
    Some(match tile {
        Tile::Mode(mode) => Click::Panel(PanelAction::Mode(mode)),
        Tile::Features => Click::Open(PanelPage::Features),
        Tile::GoalFewer => Click::Panel(PanelAction::GoalFewer),
        Tile::GoalMore => Click::Panel(PanelAction::GoalMore),
        Tile::GoalKind => Click::Panel(PanelAction::GoalKind),
        Tile::TimeFewer => Click::Panel(PanelAction::TimeFewer),
        Tile::TimeMore => Click::Panel(PanelAction::TimeMore),
        Tile::More => Click::Open(PanelPage::Goal),
        Tile::Difficulty(index) => Click::Panel(PanelAction::Difficulty(index)),
        Tile::Customize => Click::Open(PanelPage::Items),
        Tile::AutoShuffle => Click::AutoShuffle,
        Tile::TeamsFewer => Click::TeamsFewer,
        Tile::TeamsMore => Click::TeamsMore,
        Tile::Shuffle => Click::Shuffle,
        Tile::Reroll => Click::Reroll,
        Tile::Start => Click::Start,
        Tile::ResetDefaults => Click::ResetDefaults,
        Tile::HeaderMode
        | Tile::HeaderGoal
        | Tile::GoalValue
        | Tile::HeaderTime
        | Tile::TimeValue
        | Tile::HeaderDifficulty
        | Tile::HeaderTeams
        | Tile::Teams
        | Tile::BestTime
        | Tile::Players => return None,
    })
}

/// The most lines and items a goal takes (`BingoGoal.MAX_LINES`, `MAX_ITEMS`), and the time limit's range and step in
/// minutes (`Timer.kt`).
const MAX_LINES: u32 = 11;
const MAX_ITEMS: u32 = 25;
const MAX_TIME: u32 = 360;
const TIME_STEP: u32 = 15;
/// The presets the panel shows: the original's first four (impossible is left to the chest).
const PANEL_DIFFICULTIES: usize = 4;

/// The command line a rule tile runs for the rules as they are now, or `None` when the click changes nothing (a goal
/// already at its end, a time limit already off). The same line the chest menu would run, so the menu's gate decides who
/// may click.
pub fn panel_command(action: PanelAction, state: &PanelState) -> Option<String> {
    let goal = |goal: Goal| match goal {
        Goal::Lines(n) => format!("goal lines {n}"),
        Goal::Items(n) if n >= MAX_ITEMS => "goal full_card".to_owned(),
        Goal::Items(n) => format!("goal items {n}"),
        Goal::FullCard => "goal full_card".to_owned(),
    };
    let time = |minutes: u32| if minutes == 0 { "timelimit off".to_owned() } else { format!("timelimit {minutes}") };
    let minutes = state.time_limit.unwrap_or(0);
    let on_off = |on: bool| if on { "off" } else { "on" };
    match action {
        PanelAction::GoalFewer => match state.goal {
            Goal::Lines(n) if n > 1 => Some(goal(Goal::Lines(n - 1))),
            Goal::Items(n) if n > 1 => Some(goal(Goal::Items(n - 1))),
            Goal::FullCard => Some(goal(Goal::Items(MAX_ITEMS - 1))),
            _ => None,
        },
        PanelAction::GoalMore => match state.goal {
            Goal::Lines(n) if n < MAX_LINES => Some(goal(Goal::Lines(n + 1))),
            Goal::Items(n) if n < MAX_ITEMS => Some(goal(Goal::Items(n + 1))),
            _ => None,
        },
        PanelAction::GoalKind => Some(match state.goal {
            Goal::Lines(n) => goal(Goal::Items(n)),
            Goal::Items(n) => goal(Goal::Lines(n.min(MAX_LINES))),
            Goal::FullCard => goal(Goal::Lines(MAX_LINES)),
        }),
        PanelAction::TimeFewer => (minutes > 0).then(|| time((minutes - 1) / TIME_STEP * TIME_STEP)),
        PanelAction::TimeMore => (minutes < MAX_TIME).then(|| time((minutes / TIME_STEP + 1) * TIME_STEP)),
        PanelAction::Difficulty(index) => difficulty::PRESETS.get(index).filter(|_| index < PANEL_DIFFICULTIES).map(|(name, _)| format!("difficulty {name}")),
        PanelAction::Mode(PanelMode::Lockout) => Some(format!("mode lockout {}", on_off(state.mode == Mode::Lockout))),
        PanelAction::Mode(PanelMode::Inventory) => Some(format!("mode inventory {}", on_off(state.mode == Mode::Inventory))),
        PanelAction::Mode(PanelMode::HiddenItems) => Some(format!("mode hidden_items {}", on_off(state.hidden_items))),
        PanelAction::Mode(PanelMode::ConsumeItems) => Some(format!("mode consume_items {}", on_off(state.consume_items))),
    }
}

/// The root page's slot a button tile clicks, for the tiles that are a slot of the chest's root page.
pub fn root_slot(click: Click) -> Option<usize> {
    match click {
        Click::Start => Some(menu::ROOT_START_SLOT),
        Click::Reroll => Some(menu::ROOT_REROLL_SLOT),
        Click::AutoShuffle => Some(menu::ROOT_AUTO_SHUFFLE_SLOT),
        Click::TeamsFewer => Some(menu::ROOT_TEAMS_FEWER_SLOT),
        Click::TeamsMore => Some(menu::ROOT_TEAMS_MORE_SLOT),
        Click::Shuffle => Some(menu::ROOT_SHUFFLE_SLOT),
        Click::ResetDefaults => Some(menu::ROOT_DEFAULTS_SLOT),
        _ => None,
    }
}

/// The action of a right click on the block `pos` whose hit point is `point` in the lobby: a block of the panel's wall
/// means the tile under the hit's `x, y` (the ray ends on the wall behind the tiles), nothing when no tile is there or the
/// tile only shows; a pad's block or the block above it (its carpet) joins the pad's team. Any other block does nothing.
pub fn click_at(pos: [i32; 3], point: [f64; 3]) -> Option<Click> {
    if (0..3).all(|axis| (WALL_MIN[axis]..=WALL_MAX[axis]).contains(&pos[axis])) {
        return tile_at(point[0], point[1]).and_then(tile_click);
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

/// A pad label's `Click to join!` background: the entity's default shade, which the original writes out.
const BACKGROUND_JOIN: i32 = 0x4000_0000;
/// The marker glyphs of a toggle: hollow always, filled (dark green) when on.
const DIAMOND_HOLLOW: &str = "\u{25C7}";
const DIAMOND_FILLED: &str = "\u{25C6}";
/// The header text colour (pale green).
const TEXT_HEADER: &str = "#99FF99";
/// The text scales of the original (`MENU_TEXT_SCALE`, the mode tiles' icon and caption).
const SCALE_TEXT: f64 = 0.7;
const SCALE_ICON: f64 = 1.6;
const SCALE_CAPTION: f64 = 0.4;
/// The z of a tile's block (0.001 behind the wall's face), and of the hollow marker (0.005 in front of the label).
const Z_BLOCK: f64 = -5.001;
const Z_MARKER: f64 = -4.945;
/// The thickness of a tile's block.
const BLOCK_DEPTH: f64 = 0.05;

/// One display entity of the panel or of a pad.
#[derive(Debug, Clone, PartialEq)]
enum Display {
    /// A tile's background: a `block_display` at its bottom left corner, `size` wide and high.
    Block { pos: [f64; 3], size: [f64; 2], block: &'static str, sky: u8 },
    /// A `text_display`: `text` is an SNBT component.
    Text {
        pos: [f64; 3],
        scale: f64,
        line_width: i32,
        shadow: bool,
        billboard: &'static str,
        background: i32,
        text: String,
    },
}

/// A translated text component with its English fallback, and `with` arguments that are already rendered components.
fn translated(key: &str, fallback: &str, with: &[String], color: Option<&str>) -> String {
    let mut fields = vec![("translate", quoted(key, true)), ("fallback", quoted(fallback, true))];
    if !with.is_empty() {
        fields.push(("with", format!("[{}]", with.join(","))));
    }
    if let Some(color) = color {
        fields.push(("color", quoted(color, true)));
    }
    object(true, &fields)
}

/// A line of `texts` as a component, its arguments as plain literals.
fn line_component(line: &Line, color: Option<&str>) -> String {
    let with: Vec<String> = line.args.iter().map(|arg| quoted(arg, true)).collect();
    translated(line.key, line.template, &with, color)
}

/// A colored literal.
fn colored(text: &str, color: &str) -> String {
    object(true, &[("color", quoted(color, true)), ("text", quoted(text, true))])
}

/// A label: the component after an empty text in `color`, as the original builds a tile's text (`{color, extra: [..], text: ""}`).
fn extra(inner: &str, color: &str) -> String {
    object(true, &[("color", quoted(color, true)), ("extra", format!("[{inner}]")), ("text", quoted("", true))])
}

/// The goal as the value tile shows it, with the keys of the original's lang (`bingo.game.line_count_3`, up to 9).
fn goal_value_component(goal: Goal) -> String {
    let counted = |n: u32, one: &str, many: &str, key: &str, generic: &str| {
        if n == 1 {
            translated(&format!("{key}_1"), one, &[], None)
        } else if n <= 9 {
            translated(&format!("{key}_{n}"), &format!("{n} {many}"), &[], None)
        } else {
            translated(key, generic, &[quoted(&n.to_string(), true)], None)
        }
    };
    match goal {
        Goal::Lines(n) => counted(n, "1 line", "lines", "bingo.game.line_count", "%s lines"),
        Goal::Items(n) => counted(n, "1 item", "items", "bingo.game.item_count", "%s items"),
        Goal::FullCard => line_component(&Line::new("bingo.goal.full_card", "Full Card", &[]), None),
    }
}

/// The team count as the value tile shows it (`bingo.game.team_count_3`, `3 teams`).
fn team_count_component(teams: usize) -> String {
    match teams {
        1 => translated("bingo.game.team_count_1", "1 team", &[], None),
        2..=9 => translated(&format!("bingo.game.team_count_{teams}"), &format!("{teams} teams"), &[], None),
        n => translated("bingo.game.team_count", "%s teams", &[quoted(&n.to_string(), true)], None),
    }
}

/// The text of a tile's label as the original writes it, and whether it is a header's (which have their own shape).
fn tile_label(tile: Tile, state: &PanelState, ink: &str) -> String {
    let plain = |line: Line| extra(&line_component(&line, None), ink);
    let header = |line: Line| line_component(&line, Some(TEXT_HEADER));
    match tile {
        Tile::HeaderMode => header(texts::lobby_panel_game_mode()),
        Tile::HeaderGoal => header(texts::lobby_panel_goal()),
        Tile::HeaderTime => header(texts::lobby_panel_time_limit()),
        Tile::HeaderDifficulty => header(texts::lobby_panel_difficulty()),
        Tile::HeaderTeams => header(texts::lobby_panel_teams_header()),
        Tile::BestTime => translated(
            "bingo.stats.best_time",
            "Best time: %s",
            &[translated("bingo.stats.best_time.not_set", "[Not Set]", &[], Some("gray"))],
            Some(TEXT_HEADER),
        ),
        Tile::Players => translated(
            "bingo.options.player_count",
            "Players: %s",
            &[colored(&format!("{} / {}", state.players, state.max_players), "green")],
            Some(TEXT_HEADER),
        ),
        Tile::GoalFewer | Tile::TimeFewer | Tile::TeamsFewer => extra(&quoted("-", true), "black"),
        Tile::GoalMore | Tile::TimeMore | Tile::TeamsMore => extra(&quoted("+", true), "black"),
        Tile::GoalValue => goal_value_component(state.goal),
        Tile::TimeValue => line_component(&texts::lobby_panel_time_value(state.time_limit), None),
        Tile::Teams => team_count_component(state.teams),
        Tile::Mode(mode) => plain(texts::lobby_panel_mode(mode)),
        Tile::Features => plain(texts::lobby_panel_features()),
        Tile::GoalKind => extra(
            &match state.goal {
                Goal::Lines(_) => translated("bingo.goal.lines", "Lines", &[], None),
                Goal::Items(_) | Goal::FullCard => translated("bingo.goal.items", "Items", &[], None),
            },
            ink,
        ),
        Tile::More => plain(texts::lobby_panel_more()),
        Tile::Difficulty(index) => plain(texts::lobby_panel_difficulty_name(index)),
        Tile::Customize => plain(texts::lobby_panel_customize()),
        Tile::AutoShuffle => plain(texts::lobby_panel_auto_shuffle_label()),
        Tile::Shuffle => plain(texts::lobby_panel_shuffle_now()),
        Tile::Reroll => plain(texts::lobby_panel_reroll_card()),
        Tile::Start => extra(&translated("bingo.options.start", "START", &[], None), ink),
        Tile::ResetDefaults => plain(texts::lobby_panel_reset_defaults()),
    }
}

/// Whether a switch tile is on for the panel's `state`.
fn tile_on(tile: Tile, state: &PanelState) -> bool {
    match tile {
        Tile::Mode(PanelMode::Lockout) => state.mode == Mode::Lockout,
        Tile::Mode(PanelMode::Inventory) => state.mode == Mode::Inventory,
        Tile::Mode(PanelMode::HiddenItems) => state.hidden_items,
        Tile::Mode(PanelMode::ConsumeItems) => state.consume_items,
        Tile::Difficulty(index) => state.difficulty == Some(index),
        Tile::AutoShuffle => state.auto_shuffle,
        _ => false,
    }
}

/// The `line_width` of a label of a tile `w` hundredths wide at `scale` (hundredths too): `(w * 30 / scale)` truncated, as
/// the original does it (`1.6 * 30 / 0.7 = 68.57` is 68).
fn line_width(w: i32, scale_hundredths: i32) -> i32 {
    w * 30 / scale_hundredths
}

/// Every entity of one tile for the panel's `state`, in the original's order: the block, the markers, the label.
fn tile_displays(def: &Def, state: &PanelState) -> Vec<Display> {
    let on = tile_on(def.tile, state);
    let (x, bottom, w, h) = (blocks(def.cx), def.y, def.w, def.h);
    let (block, sky) = match def.style {
        Style::Header => ("minecraft:lime_concrete", 3),
        Style::Stepper => ("minecraft:white_concrete", 15),
        Style::Value => ("minecraft:gray_concrete", 14),
        Style::Toggle | Style::Mode(_) if on => ("minecraft:white_concrete", 15),
        Style::Toggle | Style::Mode(_) => ("minecraft:gray_concrete", 14),
        Style::Button { sky, .. } => ("minecraft:gray_concrete", sky),
    };
    let ink = if on { "black" } else { "gray" };
    let text = |x: f64, y: i32, z: f64, scale: f64, line_width: i32, shadow: bool, text: String| Display::Text {
        pos: [x, blocks(1700 + y), z],
        scale,
        line_width,
        shadow,
        billboard: "fixed",
        background: 0,
        text,
    };
    let mut out = vec![Display::Block {
        pos: [blocks(def.cx - w / 2), blocks(1700 + bottom), Z_BLOCK],
        size: [blocks(w), blocks(h)],
        block,
        sky,
    }];
    let corner_x = blocks(def.cx - w / 2 + 15);
    let corner_y = bottom + h / 2 - 10;
    let label_y = bottom + h / 2 - 10;
    let label = tile_label(def.tile, state, ink);
    match def.style {
        Style::Header => out.push(text(x, label_y, PANEL_Z, SCALE_TEXT, 200, false, label)),
        Style::Stepper => out.push(text(x, label_y, PANEL_Z, SCALE_TEXT, line_width(w, 70), false, label)),
        Style::Value => out.push(text(x, label_y, PANEL_Z, SCALE_TEXT, 200, true, label)),
        Style::Toggle => {
            out.push(text(corner_x, corner_y, Z_MARKER, SCALE_TEXT, 200, false, colored(DIAMOND_HOLLOW, ink)));
            out.push(text(corner_x, corner_y, PANEL_Z, SCALE_TEXT, 200, false, colored(if on { DIAMOND_FILLED } else { "" }, "dark_green")));
            out.push(text(x, label_y, PANEL_Z, SCALE_TEXT, line_width(w, 70), false, label));
        }
        Style::Mode(icon) => {
            out.push(text(corner_x, corner_y, Z_MARKER, SCALE_TEXT, 200, false, colored(DIAMOND_HOLLOW, ink)));
            out.push(text(corner_x, corner_y, PANEL_Z, SCALE_TEXT, 200, false, colored(if on { DIAMOND_FILLED } else { "" }, "dark_green")));
            out.push(text(x, bottom + h / 2 - 15, PANEL_Z, SCALE_ICON, 200, false, colored(icon, ink)));
            out.push(text(x, bottom + h / 2 - 25, PANEL_Z, SCALE_CAPTION, w * 65 / 100, false, label));
        }
        Style::Button { glyph, .. } => {
            if let Some(glyph) = glyph {
                out.push(text(corner_x, corner_y, Z_MARKER, SCALE_TEXT, 200, false, colored(glyph, ink)));
            }
            out.push(text(x, label_y, PANEL_Z, SCALE_TEXT, line_width(w, 70), false, label));
        }
    }
    out
}

/// The command for one display, tagged with every tag of `tags`.
fn display_command(display: &Display, tags: &[&str]) -> String {
    let tags: Vec<String> = tags.iter().map(|tag| format!("\"{tag}\"")).collect();
    let tags = tags.join(",");
    let matrix = |scale: [f64; 3]| {
        format!(
            "transformation:{{left_rotation:[0f,0f,0f,1f],right_rotation:[0f,0f,0f,1f],translation:[0f,0f,0f],scale:[{}f,{}f,{}f]}}",
            scale[0], scale[1], scale[2]
        )
    };
    match display {
        Display::Block { pos, size, block, sky } => format!(
            "execute in {LOBBY_LEVEL} run summon minecraft:block_display {:.3} {:.3} {:.3} {{Tags:[{tags}],Rotation:[0f,0f],\
             billboard:\"fixed\",block_state:{{Name:\"{block}\"}},brightness:{{sky:{sky},block:0}},view_range:1f,{}}}",
            pos[0],
            pos[1],
            pos[2],
            matrix([size[0], size[1], BLOCK_DEPTH])
        ),
        Display::Text { pos, scale, line_width, shadow, billboard, background, text } => format!(
            "execute in {LOBBY_LEVEL} run summon minecraft:text_display {:.3} {:.3} {:.3} {{Tags:[{tags}],Rotation:[0f,0f],\
             billboard:\"{billboard}\",alignment:\"center\",background:{background},default_background:0b,shadow:{shadow},\
             see_through:false,line_width:{line_width},view_range:1f,{},text:{text}}}",
            pos[0],
            pos[1],
            pos[2],
            matrix([*scale, *scale, *scale])
        ),
    }
}

/// The command that removes every display of the scene (blocks, texts and interactions alike: anything but a player that
/// carries the tag), so making it again never duplicates.
pub fn clear_command() -> String {
    format!("execute in {LOBBY_LEVEL} run kill @e[type=!minecraft:player,tag={UI_TAG}]")
}

/// The commands that take the map's three oak wall signs off the panel's wall (the original removes them when it draws
/// its menu): they would stand in front of the tiles. A block that is already air stays air.
pub fn sign_removal_commands() -> Vec<String> {
    (17..=19).map(|y| format!("execute in {LOBBY_LEVEL} run setblock 0 {y} -5 minecraft:air")).collect()
}

/// The command that removes the displays of one tile.
fn clear_tile_command(tile: Tile) -> String {
    format!("execute in {LOBBY_LEVEL} run kill @e[type=!minecraft:player,tag={}]", tile_tag(tile))
}

/// The summon commands of one tile for the panel's `state`: the displays all carry the scene's tag and the tile's.
fn tile_summons(def: &Def, state: &PanelState) -> Vec<String> {
    let tile = tile_tag(def.tile);
    tile_displays(def, state).iter().map(|display| display_command(display, &[UI_TAG, &tile])).collect()
}

/// The commands that make again the tiles whose displays differ between `old` and `new`: each old tile's entities are
/// removed by its tag first, so running this twice leaves one set each. Nothing for a state that looks the same.
pub fn refresh_commands(old: &PanelState, new: &PanelState) -> Vec<String> {
    let mut commands = Vec::new();
    for def in &DEFS {
        if tile_displays(def, old) != tile_displays(def, new) {
            commands.push(clear_tile_command(def.tile));
            commands.extend(tile_summons(def, new));
        }
    }
    commands
}

/// The pad's two labels (`MenuController.spawnTeamEntity`): the name in brackets in the team colour 1.5 above the pad block's
/// middle, `Click to join!` 1.0 above it, both turning about the vertical axis to face the player.
fn pad_displays(key: &str, pad: [i32; 3]) -> Vec<Display> {
    let Some(team) = PRESETS.iter().find(|p| p.key == key) else { return Vec::new() };
    let color = team.color.to_ascii_lowercase();
    let bare = key.strip_prefix("bingo_").unwrap_or(key);
    let team_name = translated(&format!("bingo.team.{bare}"), team.name, &[], None);
    let full_name = translated("bingo.team.name", "%s Team", &[team_name], None);
    let inner = object(
        true,
        &[("color", quoted(&color, true)), ("extra", format!("[{},{full_name}]", quoted(&format!("{} ", team.symbol), true))), ("text", quoted("", true))],
    );
    let name = translated("chat.square_brackets", "[%s]", &[inner], None);
    let (x, y, z) = (f64::from(pad[0]) + 0.5, f64::from(pad[1]), f64::from(pad[2]) + 0.5);
    vec![
        Display::Text { pos: [x, y + 1.5, z], scale: 1.0, line_width: 200, shadow: true, billboard: "vertical", background: 0, text: name },
        Display::Text {
            pos: [x, y + 1.0, z],
            scale: 1.0,
            line_width: 200,
            shadow: false,
            billboard: "vertical",
            background: BACKGROUND_JOIN,
            text: translated("bingo.lobby.click_to_join", "Click to join!", &[], None),
        },
    ]
}

/// The commands that summon the displays: for each tile of the panel its block and texts (the live tiles showing `state`)
/// and, for each pad, the team's `[symbol name]` and `Click to join!`. `clear_command` is not among them.
pub fn summon_commands(state: &PanelState) -> Vec<String> {
    let mut commands: Vec<String> = DEFS.iter().flat_map(|def| tile_summons(def, state)).collect();
    for (key, pad) in PADS {
        commands.extend(pad_displays(key, pad).iter().map(|display| display_command(display, &[UI_TAG])));
    }
    commands
}

/// The console commands that hold the player called `name` still for `seconds`: slowness at its top level stops walking and
/// jump boost at a level past any jump stops jumping (the original rides players on invisible bats). The players wait
/// for the start in spectator mode, which flies and which no effect holds, so they are put in adventure mode first (the
/// game's own modes are given back when the round begins). Particles are hidden.
pub fn freeze_commands(name: &str, seconds: i32) -> Vec<String> {
    let seconds = seconds.max(1);
    vec![
        format!("gamemode adventure {name}"),
        format!("effect give {name} minecraft:slowness {seconds} 255 true"),
        format!("effect give {name} minecraft:jump_boost {seconds} 128 true"),
    ]
}

/// The console commands that release `name` from `freeze_commands`: only those two effects are cleared.
pub fn thaw_commands(name: &str) -> Vec<String> {
    vec![
        format!("effect clear {name} minecraft:slowness"),
        format!("effect clear {name} minecraft:jump_boost"),
    ]
}

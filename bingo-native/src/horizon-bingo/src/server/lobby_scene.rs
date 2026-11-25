//! The lobby's in-world parts (`MenuController` of the original): the button panel on the wall at the menu spot, the
//! eight team pads, the text displays that label them, and the console commands for titles and sounds. Pure logic: which
//! clicked block means what, and the command lines to run. The adapter (`bingo-native/src/lobby_scene.rs`) runs them.
//!
//! The spots are the original's lobby POIs (`lobby_poi.json.gz`): `menu` at (0, 17, -5) and one `team` entry per pad.
//!
//! The panel is laid out like the original's `createRootPage`: five columns of small tiles, six rows high. Game Mode (four
//! icon-sized mode tiles over Features), Goal (lines or items with `-` and `+`, the time limit with `-` and `+`, More),
//! Difficulty (four presets, Customize), Shufflificator (auto shuffle, team count with `-` and `+`, Shuffle!, Re-roll
//! Card) and the start column (best time, players, a tall START, Reset Defaults). Every tile is one text display; the
//! tile that is on or chosen shows a light background with dark text, the others a dark one, and the headers are green.
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
/// The wall blocks a click counts on, inclusive: x -2..3 (the blocks span x -2 to 4), y 16..21 on the wall's row z -5 (the
/// three oak wall signs and the stone around them). The panel's tiles stand in front of this wall, so a click ray ends on
/// one of these blocks.
const WALL_MIN: [i32; 3] = [-2, 16, -7];
const WALL_MAX: [i32; 3] = [3, 21, -5];
/// The panel's plane: displays stand just in front of the wall's south face (z -4), where the signs hang.
pub const PANEL_Z: f64 = -3.9;
/// The rotation `[yaw, pitch]` of the panel's displays. Entity yaw 0 faces south (+z): a live test showed the text reads
/// correctly from a player standing south of the wall with 0.
pub const PANEL_ROTATION: [f32; 2] = [0.0, 0.0];

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

/// The five columns `[x0, x1]` of the panel (6 blocks wide, x -2..4, with 0.06 between them).
const COLUMNS: [[f64; 2]; 5] = [[-2.0, -0.5], [-0.44, 0.76], [0.82, 1.82], [1.88, 2.98], [3.04, 4.0]];
/// The top of row 0, the height of a tile of one row, and the distance between two rows (a 0.06 gap).
const ROW_TOP: f64 = 19.5;
const ROW_HEIGHT: f64 = 0.36;
const ROW_PITCH: f64 = 0.42;

/// The rectangle `[x0, y0, x1, y1]` from column span `[x0, x1]` and rows `first..=last` (a tile can be several rows high).
const fn cell(x: [f64; 2], first: usize, last: usize) -> [f64; 4] {
    [x[0], ROW_TOP - ROW_PITCH * last as f64 - ROW_HEIGHT, x[1], ROW_TOP - ROW_PITCH * first as f64]
}

/// Splits a column span in two halves with a 0.06 gap (the Game Mode column's two icon tiles).
const fn half(x: [f64; 2], right: bool) -> [f64; 2] {
    let mid = (x[0] + x[1]) / 2.0;
    if right { [mid + 0.03, x[1]] } else { [x[0], mid - 0.03] }
}

/// The `-` / value / `+` split of a column span: square steppers `step` wide at both ends.
const fn stepper(x: [f64; 2], part: usize, step: f64) -> [f64; 2] {
    match part {
        0 => [x[0], x[0] + step],
        1 => [x[0] + step + 0.06, x[1] - step - 0.06],
        _ => [x[1] - step, x[1]],
    }
}

/// The tiles and their rectangles `[x0, y0, x1, y1]` in world coordinates on the panel plane, 6 blocks wide (x -2..4) and
/// 2.46 high (y 17.04..19.5). The summon commands and the click mapping both read this table.
pub const TILES: [(Tile, [f64; 4]); 33] = [
    (Tile::HeaderMode, cell(COLUMNS[0], 0, 0)),
    (Tile::Mode(PanelMode::Lockout), cell(half(COLUMNS[0], false), 1, 2)),
    (Tile::Mode(PanelMode::Inventory), cell(half(COLUMNS[0], true), 1, 2)),
    (Tile::Mode(PanelMode::HiddenItems), cell(half(COLUMNS[0], false), 3, 4)),
    (Tile::Mode(PanelMode::ConsumeItems), cell(half(COLUMNS[0], true), 3, 4)),
    (Tile::Features, cell(COLUMNS[0], 5, 5)),
    (Tile::HeaderGoal, cell(COLUMNS[1], 0, 0)),
    (Tile::GoalFewer, cell(stepper(COLUMNS[1], 0, 0.24), 1, 1)),
    (Tile::GoalValue, cell(stepper(COLUMNS[1], 1, 0.24), 1, 1)),
    (Tile::GoalMore, cell(stepper(COLUMNS[1], 2, 0.24), 1, 1)),
    (Tile::GoalKind, cell(COLUMNS[1], 2, 2)),
    (Tile::HeaderTime, cell(COLUMNS[1], 3, 3)),
    (Tile::TimeFewer, cell(stepper(COLUMNS[1], 0, 0.24), 4, 4)),
    (Tile::TimeValue, cell(stepper(COLUMNS[1], 1, 0.24), 4, 4)),
    (Tile::TimeMore, cell(stepper(COLUMNS[1], 2, 0.24), 4, 4)),
    (Tile::More, cell(COLUMNS[1], 5, 5)),
    (Tile::HeaderDifficulty, cell(COLUMNS[2], 0, 0)),
    (Tile::Difficulty(0), cell(COLUMNS[2], 1, 1)),
    (Tile::Difficulty(1), cell(COLUMNS[2], 2, 2)),
    (Tile::Difficulty(2), cell(COLUMNS[2], 3, 3)),
    (Tile::Difficulty(3), cell(COLUMNS[2], 4, 4)),
    (Tile::Customize, cell(COLUMNS[2], 5, 5)),
    (Tile::HeaderTeams, cell(COLUMNS[3], 0, 0)),
    (Tile::AutoShuffle, cell(COLUMNS[3], 1, 1)),
    (Tile::TeamsFewer, cell(stepper(COLUMNS[3], 0, 0.24), 2, 2)),
    (Tile::Teams, cell(stepper(COLUMNS[3], 1, 0.24), 2, 2)),
    (Tile::TeamsMore, cell(stepper(COLUMNS[3], 2, 0.24), 2, 2)),
    (Tile::Shuffle, cell(COLUMNS[3], 3, 3)),
    (Tile::Reroll, cell(COLUMNS[3], 5, 5)),
    (Tile::BestTime, cell(COLUMNS[4], 0, 0)),
    (Tile::Players, cell(COLUMNS[4], 1, 1)),
    (Tile::Start, cell(COLUMNS[4], 2, 4)),
    (Tile::ResetDefaults, cell(COLUMNS[4], 5, 5)),
];

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

/// The tile backgrounds (ARGB): a dark translucent tile, the light one of a tile that is on or chosen, the darker one of a
/// value (the goal, the time, the team count), and the dark green header.
const BACKGROUND_TILE: u32 = 0xA026_2B2B;
const BACKGROUND_ACTIVE: u32 = 0xE0C4_C8C8;
const BACKGROUND_VALUE: u32 = 0xB012_1616;
const BACKGROUND_HEADER: u32 = 0xB00E_2A12;
/// The text colours: gray on a dark tile, black on a light one, white on a value, the original's light green on a header.
const TEXT_TILE: &str = "#A8A8A8";
const TEXT_ACTIVE: &str = "#000000";
const TEXT_VALUE: &str = "#FFFFFF";
const TEXT_HEADER: &str = "#99FF99";
/// The marker before the label of a tile that is a switch or one choice of several: filled dark green when on, hollow gray
/// when off, as the original's icon corner.
const MARKER_ON: &str = "\u{25C6} ";
const MARKER_OFF: &str = "\u{25C7} ";
/// A pad label's background.
const BACKGROUND_LABEL: u32 = 0x8000_0000;
/// The text scale a tile is drawn at: the original's tile text is vanilla size at about this scale in the world, which
/// makes the panel's rows about as high as a line of text. START is drawn larger.
const TEXT_SCALE: f64 = 0.5;
const TEXT_SCALE_START: f64 = 1.0;
/// A text display's line height in pixels (40 pixels are a block at scale 1).
const LINE_PX: f64 = 9.0;
/// The share of a tile's width its label may fill.
const FILL: f64 = 0.92;

/// The command for one text display in the lobby, at `pos`, with `text` (an SNBT component). `scale` is `[x, y]`; `billboard`
/// is `fixed` (then `rotation` is its `[yaw, pitch]`) or `center` (faces the viewer).
fn summon_display(
    pos: [f64; 3],
    scale: [f64; 2],
    billboard: &str,
    rotation: [f32; 2],
    background: u32,
    text: &str,
    extra_tag: Option<&str>,
) -> String {
    let tags = extra_tag.map_or(String::new(), |tag| format!(",\"{tag}\""));
    format!(
        "execute in {LOBBY_LEVEL} run summon minecraft:text_display {} {} {} {{Tags:[\"{UI_TAG}\"{tags}],billboard:\"{billboard}\",\
         Rotation:[{:?}f,{:?}f],alignment:\"center\",background:{},shadow:true,brightness:{{block:15,sky:15}},\
         transformation:{{scale:[{:.3}f,{:.3}f,1f],translation:[0f,0f,0f],left_rotation:[0f,0f,0f,1f],right_rotation:[0f,0f,0f,1f]}},\
         text:{text}}}",
        pos[0], pos[1], pos[2], rotation[0], rotation[1], background as i32, scale[0], scale[1]
    )
}

/// The width of one character in pixels, roughly as vanilla's font sets it (with its pixel of spacing).
fn char_px(c: char) -> f64 {
    match c {
        ' ' => 4.0,
        'i' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' => 3.0,
        'l' | 'I' | 't' => 4.0,
        'f' | 'k' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '*' => 5.0,
        '@' | '~' => 7.0,
        c if c.is_ascii() => 6.0,
        _ => 9.0,
    }
}

/// The width of a text in pixels, roughly.
fn text_px(text: &str) -> f64 {
    text.chars().map(char_px).sum()
}

/// How a tile looks: its label, the colours, the marker (`Some(on)` for a switch) and the text scale it is drawn at.
#[derive(Debug, Clone, PartialEq)]
struct Look {
    line: Line,
    color: &'static str,
    background: u32,
    marker: Option<bool>,
    scale: f64,
}

impl Look {
    fn tile(line: Line) -> Self {
        Self { line, color: TEXT_TILE, background: BACKGROUND_TILE, marker: None, scale: TEXT_SCALE }
    }

    fn value(line: Line) -> Self {
        Self { line, color: TEXT_VALUE, background: BACKGROUND_VALUE, marker: None, scale: TEXT_SCALE }
    }

    fn header(line: Line) -> Self {
        Self { line, color: TEXT_HEADER, background: BACKGROUND_HEADER, marker: None, scale: TEXT_SCALE }
    }

    /// A tile that is on or chosen: light with dark text, and a marker when `marked`.
    fn choice(line: Line, on: bool, marked: bool) -> Self {
        let marker = marked.then_some(on);
        if on {
            Self { line, color: TEXT_ACTIVE, background: BACKGROUND_ACTIVE, marker, scale: TEXT_SCALE }
        } else {
            Self { marker, ..Self::tile(line) }
        }
    }
}

/// The look of a tile for the panel's `state`.
fn tile_look(tile: Tile, state: &PanelState) -> Look {
    match tile {
        Tile::HeaderMode => Look::header(texts::lobby_panel_game_mode()),
        Tile::Mode(mode) => {
            let on = match mode {
                PanelMode::Lockout => state.mode == Mode::Lockout,
                PanelMode::Inventory => state.mode == Mode::Inventory,
                PanelMode::HiddenItems => state.hidden_items,
                PanelMode::ConsumeItems => state.consume_items,
            };
            Look::choice(texts::lobby_panel_mode(mode), on, true)
        }
        Tile::Features => Look::tile(texts::lobby_panel_features()),
        Tile::HeaderGoal => Look::header(texts::lobby_panel_goal()),
        Tile::GoalFewer | Tile::TimeFewer | Tile::TeamsFewer => Look::tile(texts::lobby_panel_minus()),
        Tile::GoalMore | Tile::TimeMore | Tile::TeamsMore => Look::tile(texts::lobby_panel_plus()),
        Tile::GoalValue => Look::value(texts::lobby_panel_goal_value(state.goal)),
        Tile::GoalKind => Look::tile(texts::lobby_panel_goal_kind(state.goal)),
        Tile::HeaderTime => Look::header(texts::lobby_panel_time_limit()),
        Tile::TimeValue => Look::value(texts::lobby_panel_time_value(state.time_limit)),
        Tile::More => Look::tile(texts::lobby_panel_more()),
        Tile::HeaderDifficulty => Look::header(texts::lobby_panel_difficulty()),
        Tile::Difficulty(index) => Look::choice(texts::lobby_panel_difficulty_name(index), state.difficulty == Some(index), true),
        Tile::Customize => Look::tile(texts::lobby_panel_customize()),
        Tile::HeaderTeams => Look::header(texts::lobby_panel_teams_header()),
        Tile::AutoShuffle => Look::choice(texts::lobby_panel_auto_shuffle_label(), state.auto_shuffle, true),
        Tile::Teams => Look::value(texts::lobby_panel_team_count(state.teams)),
        Tile::Shuffle => Look::tile(texts::lobby_panel_shuffle_now()),
        Tile::Reroll => Look::tile(texts::lobby_panel_reroll_card()),
        Tile::BestTime => Look::header(texts::lobby_panel_best_time_not_set()),
        Tile::Players => Look::header(texts::lobby_panel_player_count(state.players, state.max_players)),
        Tile::Start => Look { scale: TEXT_SCALE_START, ..Look::tile(texts::lobby_menu_start()) },
        Tile::ResetDefaults => Look::tile(texts::lobby_panel_reset_defaults()),
    }
}

/// One panel tile as a summon command: the label centred in `rect` on the panel plane. The label has blank lines above and
/// below and spaces beside it, so the display's background about fills the rectangle; the text scale is the look's, made
/// smaller for a long label, and the display is scaled the same in both directions so the glyphs keep their shape.
fn tile_command(tile: Tile, rect: [f64; 4], look: &Look) -> String {
    let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
    let marker = match look.marker {
        Some(true) => MARKER_ON,
        Some(false) => MARKER_OFF,
        None => "",
    };
    let label_px = text_px(marker) + text_px(&look.line.render());
    let wanted = look.scale.min(width * 40.0 * FILL / label_px);
    // An odd number of lines puts the label's own line in the middle.
    let mut lines = (height * 40.0 / (LINE_PX * wanted)).round().max(1.0) as usize;
    if lines % 2 == 0 {
        lines -= 1;
    }
    let blank_lines = lines / 2;
    let scale = height * 40.0 / (lines as f64 * LINE_PX + 2.0);
    let pad = ((width * 40.0 / scale - label_px) / (2.0 * char_px(' '))).floor().max(0.0) as usize;
    let spaces = literal(&" ".repeat(pad), None, false, true);
    let newlines = literal(&"\n".repeat(blank_lines), None, false, true);
    let mut parts = Vec::new();
    if blank_lines > 0 {
        parts.push(newlines.clone());
    }
    parts.push(spaces.clone());
    if let Some(on) = look.marker {
        parts.push(literal(marker, Some(if on { "dark_green" } else { "gray" }), false, true));
    }
    parts.push(component(&look.line, Some(look.color), false, true));
    parts.push(spaces);
    if blank_lines > 0 {
        parts.push(newlines);
    }
    let text = object(true, &[("text", quoted("", true)), ("extra", format!("[{}]", parts.join(",")))]);
    summon_display(
        [(rect[0] + rect[2]) / 2.0, rect[1], PANEL_Z],
        [scale, scale],
        "fixed",
        PANEL_ROTATION,
        look.background,
        &text,
        Some(&tile_tag(tile)),
    )
}

/// The command that removes every display of the scene, so making it again never duplicates.
pub fn clear_command() -> String {
    format!("execute in {LOBBY_LEVEL} run kill @e[type=minecraft:text_display,tag={UI_TAG}]")
}

/// The summon command of one tile for the panel's `state`.
fn tile_summon(tile: Tile, rect: [f64; 4], state: &PanelState) -> String {
    tile_command(tile, rect, &tile_look(tile, state))
}

/// The commands that make again the tiles whose look differs between `old` and `new`: each old display is removed by its
/// tag first, so running this twice leaves one display each. Nothing for a state that looks the same.
pub fn refresh_commands(old: &PanelState, new: &PanelState) -> Vec<String> {
    let mut commands = Vec::new();
    for (tile, rect) in TILES {
        if tile_look(tile, old) != tile_look(tile, new) {
            commands.push(format!("execute in {LOBBY_LEVEL} run kill @e[type=minecraft:text_display,tag={}]", tile_tag(tile)));
            commands.push(tile_summon(tile, rect, new));
        }
    }
    commands
}

/// The commands that summon the displays: one for each tile of the panel (its live tiles showing `state`) and, for each pad,
/// the team's `[symbol name]` 1.4 blocks above the pad block and `Click to join!` 1.1 above it. `clear_command` is not
/// among them.
pub fn summon_commands(state: &PanelState) -> Vec<String> {
    let mut commands = Vec::new();
    for (tile, rect) in TILES {
        commands.push(tile_summon(tile, rect, state));
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
        commands.push(summon_display([x, y + 1.4, z], [1.0, 1.0], "center", [0.0, 0.0], BACKGROUND_LABEL, &name, None));
        commands.push(summon_display(
            [x, y + 1.1, z],
            [1.0, 1.0],
            "center",
            [0.0, 0.0],
            BACKGROUND_LABEL,
            &component(&texts::click_to_join(), Some("gray"), false, true),
            None,
        ));
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

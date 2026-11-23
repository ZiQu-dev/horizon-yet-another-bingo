//! The lobby's in-world parts (`MenuController` of the original): the menu spot where the original draws its button menu, the
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

/// The block of the menu spot (POI `menu`, yaw 0): the three oak wall signs stand around it.
pub const MENU_POS: [i32; 3] = [0, 17, -5];
/// The signs' blocks, inclusive: x -1..1, y 17..19, z -6..-4 around the menu spot.
const MENU_MIN: [i32; 3] = [-1, 17, -6];
const MENU_MAX: [i32; 3] = [1, 19, -4];

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
    /// Opens the lobby menu.
    Menu,
    /// Joins the team with this key (`bingo_red`).
    Join(&'static str),
}

/// The action of a right click on `pos` in the lobby: the signs of the menu spot open the menu, a pad's block or the block
/// above it (its carpet) joins the pad's team. Any other block does nothing.
pub fn click_at(pos: [i32; 3]) -> Option<Click> {
    if (0..3).all(|axis| (MENU_MIN[axis]..=MENU_MAX[axis]).contains(&pos[axis])) {
        return Some(Click::Menu);
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

/// The command for one text display in the lobby, at `pos`, with `text` (an SNBT component).
fn summon_display(pos: [f64; 3], text: &str) -> String {
    format!(
        "execute in {LOBBY_LEVEL} run summon minecraft:text_display {} {} {} {{Tags:[\"{UI_TAG}\"],billboard:\"center\",\
         alignment:\"center\",background:0,shadow:true,text:{text}}}",
        pos[0], pos[1], pos[2]
    )
}

/// The command that removes every display of the scene, so making it again never duplicates.
pub fn clear_command() -> String {
    format!("execute in {LOBBY_LEVEL} run kill @e[type=minecraft:text_display,tag={UI_TAG}]")
}

/// The commands that summon the displays: the menu's text above its signs and, for each pad, the team's name 1.5 blocks
/// above the pad block and `Click to join!` 1 block above it. `clear_command` is not among them.
pub fn summon_commands() -> Vec<String> {
    let mut commands = Vec::new();
    let menu = object(
        true,
        &[
            ("text", quoted("", true)),
            (
                "extra",
                format!(
                    "[{},{},{},{},{}]",
                    literal("BINGO", Some("gold"), true, true),
                    literal("\n", None, false, true),
                    component(&texts::lobby_menu_hint(), None, false, true),
                    literal("\n", None, false, true),
                    component(&texts::lobby_menu_start(), Some("green"), false, true),
                ),
            ),
        ],
    );
    commands.push(summon_display(
        [f64::from(MENU_POS[0]) + 0.5, f64::from(MENU_POS[1]) + 3.5, f64::from(MENU_POS[2]) + 0.5],
        &menu,
    ));
    for (key, pad) in PADS {
        let Some(team) = PRESETS.iter().find(|p| p.key == key) else { continue };
        let color = team.color.to_ascii_lowercase();
        let bare = key.strip_prefix("bingo_").unwrap_or(key);
        let name_key = format!("bingo.team.{bare}");
        let name = object(
            true,
            &[
                ("text", quoted(&format!("{} ", team.symbol), true)),
                ("color", quoted(&color, true)),
                (
                    "extra",
                    format!(
                        "[{}]",
                        object(
                            true,
                            &[
                                ("translate", quoted(&name_key, true)),
                                ("fallback", quoted(team.name, true)),
                                ("color", quoted(&color, true)),
                            ]
                        )
                    ),
                ),
            ],
        );
        let (x, y, z) = (f64::from(pad[0]) + 0.5, f64::from(pad[1]), f64::from(pad[2]) + 0.5);
        commands.push(summon_display([x, y + 1.5, z], &name));
        commands.push(summon_display([x, y + 1.0, z], &component(&texts::click_to_join(), None, false, true)));
    }
    commands
}

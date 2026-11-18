//! The title screen's "BINGO!" button: the original mod's main-menu button, which opens the bingo lobby world. This
//! module is the pure part: where the button sits, what it says, which folder it opens and what each answer of
//! `host.request_world` tells the player. The adapter (`bingo-native/api`, feature `world-request`) asks Horizon and
//! draws through the `Host` seam; nothing here needs the game.
use crate::host::{Argb, Host};
use crate::text_plain::Lang;

/// The lobby world's folder under `saves/`: the name the original's button expects ("bingo").
pub const LOBBY_FOLDER: &str = "bingo";
/// The button's translation key and its English fallback, as the mod's other texts take them.
pub const LABEL_KEY: &str = "bingo.title.button";
pub const LABEL: &str = "BINGO!";
/// The hint's translation key; `{folder}` is the lobby folder's name.
pub const HINT_CREATE_KEY: &str = "bingo.title.hint.create_world";
pub const HINT_CREATE: &str = "No world named \"{folder}\" yet: create one, then press again.";
/// The hint when Horizon refuses the request for lack of the mod's permission (a build that is not allowed to open worlds).
pub const HINT_DENIED_KEY: &str = "bingo.title.hint.not_permitted";
pub const HINT_DENIED: &str = "This build of BINGO! may not open worlds from the title screen.";

/// The button's size in GUI pixels: the original's 80 x 50 sprite, bottom left (`x = 10`, `y = height - 70`).
pub const BUTTON_W: f32 = 80.0;
pub const BUTTON_H: f32 = 50.0;
/// How wide the hint's lines may be: the button is narrow, the hint is not wrapped to it.
pub const HINT_W: i32 = 180;
/// The original's button sprites in the mod's atlas (`sprites/bingo/button.png`, `button_focused.png`).
pub const SPRITE: &str = "bingo/button";
pub const SPRITE_FOCUSED: &str = "bingo/button_focused";

/// Where the button's two pictures lie in the mod's uploaded atlas, `[x, y, w, h]` each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonSprites {
    pub normal: [f32; 4],
    pub focused: [f32; 4],
}

/// The answers of `host.request_world`, as the adapter maps `NativeWorldResult` (plus one for an answer added later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldAnswer {
    Queued,
    NotTitle,
    NotPermitted,
    InvalidRequest,
    Busy,
    Unknown,
}

/// What the player is told after a press that Horizon did not act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hint {
    /// No world of that name exists: create one, then press again.
    CreateWorld { folder: String },
    /// Horizon refuses the request for the permission.
    NotPermitted,
}

/// The hint for an answer, or None when there is nothing to say: `Queued` opens the world, `NotTitle` means another
/// screen is up (the press came too late), `Busy` means a request is already waiting for the next frame.
pub fn hint_for(answer: WorldAnswer, folder: &str) -> Option<Hint> {
    match answer {
        WorldAnswer::Queued | WorldAnswer::NotTitle | WorldAnswer::Busy | WorldAnswer::Unknown => None,
        WorldAnswer::InvalidRequest => Some(Hint::CreateWorld { folder: folder.to_owned() }),
        WorldAnswer::NotPermitted => Some(Hint::NotPermitted),
    }
}

/// The hint's text in the player's language, with the English wording when the language lacks it.
pub fn hint_text(hint: &Hint, lang: Lang<'_>) -> String {
    match hint {
        Hint::CreateWorld { folder } => tr(lang, HINT_CREATE_KEY, HINT_CREATE).replace("{folder}", folder),
        Hint::NotPermitted => tr(lang, HINT_DENIED_KEY, HINT_DENIED),
    }
}

/// Greedy word wrap: the words of `text` in lines no wider than `width` by `measure`, a word too wide for a line on its
/// own kept whole. Under the button the hint is cut into these lines.
pub fn wrap(text: &str, width: i32, measure: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if line.is_empty() || measure(&candidate) <= width {
            line = candidate;
        } else {
            lines.push(std::mem::take(&mut line));
            line = word.to_owned();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The button's label in the player's language.
pub fn label_text(lang: Lang<'_>) -> String {
    tr(lang, LABEL_KEY, LABEL)
}

fn tr(lang: Lang<'_>, key: &str, fallback: &str) -> String {
    lang(key).unwrap_or_else(|| fallback.to_owned())
}

/// Whether `folder` may be asked for: one plain segment of 1 to 64 characters, no separator, colon or control
/// character and no leading dot. The same rule Horizon applies, so a bad name is caught before the request.
pub fn lobby_folder_ok(folder: &str) -> bool {
    (1..=64).contains(&folder.chars().count())
        && !folder.starts_with('.')
        && folder.chars().all(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
}

/// The button's rectangle `[x, y, w, h]` on a title screen of `width` by `height` GUI pixels.
pub fn button_rect(_width: f32, height: f32) -> [f32; 4] {
    [10.0, height - 70.0, BUTTON_W, BUTTON_H]
}

/// Whether the pointer at (`x`, `y`) is on the rectangle (its edges included on the left and top, as vanilla's widgets).
pub fn contains(rect: [f32; 4], (x, y): (f32, f32)) -> bool {
    let [rx, ry, w, h] = rect;
    x >= rx && x < rx + w && y >= ry && y < ry + h
}

/// The button's draw in the title's frame: the original's sprite (its focused picture under the pointer) when the mod's
/// pictures are up, else the vanilla button sprite; its label centred, and the hint, when there is one, in red above it.
pub fn draw(
    host: &mut dyn Host,
    rect: [f32; 4],
    mouse: (f32, f32),
    label: &str,
    hint: Option<&str>,
    sprites: Option<ButtonSprites>,
) {
    let [x, y, w, h] = rect;
    let over = contains(rect, mouse);
    match sprites {
        Some(s) => host.draw_sprite(if over { s.focused } else { s.normal }, rect),
        None => host.draw_vanilla_nine_slice(if over { "widget/button_highlighted" } else { "widget/button" }, x, y, w, h, WHITE),
    }
    let width = host.text_width(label);
    host.draw_text(x as i32 + (w as i32 - width) / 2, y as i32 + (h as i32 - 8) / 2, label, WHITE, true);
    if let Some(hint) = hint {
        let lines = wrap(hint, HINT_W, &|text: &str| host.text_width(text));
        let top = y as i32 - 4 - 10 * lines.len() as i32;
        for (i, line) in lines.iter().enumerate() {
            host.draw_text(x as i32, top + 10 * i as i32, line, RED, true);
        }
    }
}

const WHITE: Argb = 0xffff_ffff;
const RED: Argb = 0xffff_5555;

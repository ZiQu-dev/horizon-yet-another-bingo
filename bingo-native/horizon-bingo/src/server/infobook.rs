//! The lobby's tutorial book (the original's `InfoBookService`, `InfoBookController` and `InfoBookCommand`): a written book
//! with three pages (the welcome, the player settings and the hints), given to each player in the lobby and by `bingo book`.
//! Pure: a page is a [`Part`] tree that reads as the English a player sees ([`Part::plain`]) and as the text components of
//! the book ([`Part::snbt`]). The session gives the item through `ServerHost::give_item`.
//!
//! The English is the original's `en_us` text, under the original's keys, so a client's translation applies.
//!
//! The card HUD is known from the client's settings (see `Session::has_card_hud`): the hints name its keybind, and the
//! settings page lists the auto hide only without it. Each toggle runs `bingo prefs` (`prefs::click_command`). Not ported:
//! the book's refresh when a setting changes (a given book is never changed in place, and the book is given once, in the
//! lobby, or by `bingo book`), so the toggles show the values the book was given with.
use super::prefs::{self, Setting};
use crate::update_settings::PlayerSettings;

/// The item the book is.
pub const ITEM: &str = "minecraft:written_book";
/// The version the book's text names (`v...`): the original mod's version whose English this follows.
pub const VERSION: &str = "2.15.1";
/// The book's title (`bingo.full_name`).
pub const TITLE: &str = "Yet Another Bingo";
/// The wiki, and its page on what the game is (`URL_WIKI`, `URL_WIKI_WHAT_IS_BINGO` of the original).
pub const URL_WIKI: &str = "https://horrific.dev/bingo/";
pub const URL_WIKI_WHAT_IS_BINGO: &str = "https://horrific.dev/bingo/what-is-bingo/";
/// The key the map's hint names (`key.use`, the right click), and the key that opens the card with the HUD (the mod's
/// `KEYBIND_OPEN_CARD`, `bingo.keybind.open_card`).
const KEYBIND_USE: &str = "key.use";
const KEYBIND_OPEN_CARD: &str = "bingo.keybind.open_card";
/// The check mark of an on setting (U+2611) and of an off one (U+2610), and the information mark after a setting's name.
const ON_MARK: &str = "\u{2611}";
const OFF_MARK: &str = "\u{2610}";
const INFO_MARK: &str = "\u{1F6C8}";
/// The book's custom data: the tags of the original (`NBT_BINGO_INFO_BOOK`, `NBT_BINGO_IGNORE`, `NBT_BINGO_VANISH`,
/// `NBT_BINGO_KEEP`). `keep` is set for the lobby's book, which stays with its player when they die.
const TAG_INFO_BOOK: &str = "bingo_info_book";
const TAG_IGNORE: &str = "bingo_ignore";
const TAG_VANISH: &str = "bingo_vanish";
const TAG_KEEP: &str = "bingo_keep";

/// Who reads the book: the player's settings (which page comes first, and the toggles on the settings page), whether
/// the player is an operator (the configure-game text on the welcome page) and whether the player's client runs the
/// card HUD (the keybind the hints name, and the auto hide setting, which needs no HUD to be useful).
pub struct PageView<'a> {
    pub settings: &'a PlayerSettings,
    pub operator: bool,
    pub hud: bool,
}

/// A piece of text: a plain string, a translatable text with its English and arguments, a keybind, or a list of pieces.
/// Its style applies to it and everything inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    kind: Kind,
    style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Text(String),
    Translate { key: &'static str, fallback: &'static str, with: Vec<Part> },
    Keybind(&'static str),
    Group(Vec<Part>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Style {
    color: Option<&'static str>,
    bold: bool,
    underlined: bool,
    url: Option<&'static str>,
    run: Option<String>,
    hover: Option<Box<Part>>,
}

impl Part {
    /// A plain string.
    pub fn text(text: &str) -> Part {
        Part { kind: Kind::Text(text.to_string()), style: Style::default() }
    }

    /// A translatable text: the key, its English (`%s` takes the next argument) and the arguments.
    pub fn translate(key: &'static str, fallback: &'static str, with: Vec<Part>) -> Part {
        Part { kind: Kind::Translate { key, fallback, with }, style: Style::default() }
    }

    /// A key binding, shown as the key's name.
    pub fn keybind(key: &'static str) -> Part {
        Part { kind: Kind::Keybind(key), style: Style::default() }
    }

    /// Pieces one after another, with no style of their own.
    pub fn group(parts: Vec<Part>) -> Part {
        Part { kind: Kind::Group(parts), style: Style::default() }
    }

    pub fn color(mut self, color: &'static str) -> Part {
        self.style.color = Some(color);
        self
    }

    pub fn bold(mut self) -> Part {
        self.style.bold = true;
        self
    }

    pub fn underlined(mut self) -> Part {
        self.style.underlined = true;
        self
    }

    /// Opens `url` when clicked.
    pub fn open_url(mut self, url: &'static str) -> Part {
        self.style.url = Some(url);
        self
    }

    /// Runs `command` (a slash command) when clicked.
    pub fn run_command(mut self, command: String) -> Part {
        self.style.run = Some(command);
        self
    }

    /// Shows `part` when the mouse hovers over it.
    pub fn hover(mut self, part: Part) -> Part {
        self.style.hover = Some(Box::new(part));
        self
    }

    /// The text a player reads: the English of each translatable text, with its arguments in place.
    pub fn plain(&self) -> String {
        match &self.kind {
            Kind::Text(text) => text.clone(),
            Kind::Translate { fallback, with, .. } => {
                let args: Vec<String> = with.iter().map(Part::plain).collect();
                fill(fallback, &args)
            }
            Kind::Keybind(key) => key.to_string(),
            Kind::Group(parts) => parts.iter().map(Part::plain).collect(),
        }
    }

    /// The text component in SNBT, as a book page or an item component reads it.
    pub fn snbt(&self) -> String {
        let mut fields = match &self.kind {
            Kind::Text(text) => vec![format!("text:{}", snbt_string(text))],
            Kind::Translate { key, fallback, with } => {
                let mut fields = vec![
                    format!("translate:{}", snbt_string(key)),
                    format!("fallback:{}", snbt_string(fallback)),
                ];
                if !with.is_empty() {
                    let args: Vec<String> = with.iter().map(Part::snbt).collect();
                    fields.push(format!("with:[{}]", args.join(",")));
                }
                fields
            }
            Kind::Keybind(key) => vec![format!("keybind:{}", snbt_string(key))],
            Kind::Group(parts) => {
                let inner: Vec<String> = parts.iter().map(Part::snbt).collect();
                vec!["text:\"\"".to_string(), format!("extra:[{}]", inner.join(","))]
            }
        };
        if let Some(color) = self.style.color {
            fields.push(format!("color:\"{color}\""));
        }
        if self.style.bold {
            fields.push("bold:1b".to_string());
        }
        if self.style.underlined {
            fields.push("underlined:1b".to_string());
        }
        if let Some(url) = self.style.url {
            fields.push(format!("click_event:{{action:\"open_url\",url:{}}}", snbt_string(url)));
        }
        if let Some(command) = &self.style.run {
            fields.push(format!("click_event:{{action:\"run_command\",command:{}}}", snbt_string(command)));
        }
        if let Some(hover) = &self.style.hover {
            fields.push(format!("hover_event:{{action:\"show_text\",value:{}}}", hover.snbt()));
        }
        format!("{{{}}}", fields.join(","))
    }
}

/// The book's pages in the order the player reads them: the settings page first when the player has seen the tutorial,
/// the welcome page first otherwise, and the hints last. The settings page is for every player.
pub fn pages(view: &PageView) -> Vec<Part> {
    let welcome = welcome_page(view.operator);
    let settings = settings_page(view.settings, view.hud);
    let hints = hints_page(view.hud);
    if view.settings.seen_tutorial {
        vec![settings, welcome, hints]
    } else {
        vec![welcome, settings, hints]
    }
}

/// The components of the book item, in the `give` command's syntax (`minecraft:written_book[<components>]`): the written
/// book content (title, author and the pages) and the custom data tags.
pub fn components(author: &str, pages: &[Part], keep: bool) -> String {
    let pages: Vec<String> = pages.iter().map(Part::snbt).collect();
    let mut tags = vec![format!("{TAG_INFO_BOOK}:1b"), format!("{TAG_IGNORE}:1b"), format!("{TAG_VANISH}:1b")];
    if keep {
        tags.push(format!("{TAG_KEEP}:1b"));
    }
    format!(
        "minecraft:written_book_content={{title:{},author:{},pages:[{}]}},minecraft:custom_data={{{}}}",
        snbt_string(TITLE),
        snbt_string(author),
        pages.join(","),
        tags.join(",")
    )
}

/// The welcome page (`bingo.intro.*`): the title and version, the configure text for an operator, the wiki link and
/// the line for everyone else.
fn welcome_page(operator: bool) -> Part {
    let link = Part::translate("bingo.intro.welcome_link", "Yet Another Bingo", vec![])
        .color("dark_green")
        .underlined()
        .open_url(URL_WIKI);
    let mut parts = vec![
        Part::translate("bingo.intro.welcome", "Welcome to %s!", vec![link]).bold(),
        Part::text("\n"),
        Part::text(&format!("v{VERSION}")).color("gray"),
    ];
    if operator {
        parts.push(Part::text("\n\n"));
        parts.push(Part::translate(
            "bingo.intro.use_config_menu",
            "Use the menu in front of you to configure the game.",
            vec![],
        ));
    }
    let wiki = Part::translate("bingo.intro.read_how_to_play_link", "how to play", vec![])
        .color("dark_green")
        .underlined()
        .open_url(URL_WIKI_WHAT_IS_BINGO);
    parts.push(Part::text("\n\n"));
    parts.push(Part::translate("bingo.intro.read_how_to_play", "Read %s on the wiki.", vec![wiki]));
    if !operator {
        parts.push(Part::text("\n\n"));
        parts.push(Part::translate(
            "bingo.intro.change_player_settings",
            "Change your player settings on the next page.",
            vec![],
        ));
    }
    parts.push(Part::text("\n\n"));
    parts.push(Part::translate("bingo.intro.have_fun", "Have fun!", vec![]));
    Part::group(parts)
}

/// The hints page: the key that opens the card (with the HUD) or that shows its items on a map (without it), and the
/// coordinates command.
fn hints_page(hud: bool) -> Part {
    let keybind = if hud {
        Part::translate(
            "bingo.intro.keybind_hud",
            "Press [%s] to see the Bingo Card in an inventory view.",
            vec![Part::keybind(KEYBIND_OPEN_CARD).color("dark_green")],
        )
    } else {
        Part::translate(
            "bingo.intro.keybind_map",
            "When holding a Bingo Card, press [%s] to see its items.",
            vec![Part::keybind(KEYBIND_USE).color("dark_green")],
        )
    };
    Part::group(vec![
        Part::translate("bingo.intro.hints", "Hints", vec![]).bold(),
        Part::text("\n\n"),
        keybind,
        Part::text("\n\n"),
        Part::translate(
            "bingo.intro.use_coords",
            "Use \"/coords\" to keep track of coordinates and share them with your team.",
            vec![],
        ),
    ])
}

/// The settings page: the player's own toggles, with a mark for each (on or off) and a hover text for what it does.
/// The scoreboard's auto hide is listed only for a player without the card HUD: with the HUD the scoreboard never
/// auto hides (the original's `ScoreboardService.shouldDisplayScoreboard`).
fn settings_page(settings: &PlayerSettings, hud: bool) -> Part {
    let mut parts = vec![
        Part::translate("bingo.player_settings", "Player Settings", vec![]).bold(),
        Part::text("\n\n"),
        toggle(Setting::Bossbar, settings.bossbar, "bingo.player_settings.bossbar", "Bossbar", "bingo.player_settings.bossbar.description", "Shows the time remaining in a bossbar."),
        Part::text("\n\n"),
        toggle(Setting::Scoreboard, settings.scoreboard, "bingo.player_settings.scoreboard", "Scoreboard", "bingo.player_settings.scoreboard.description", "Shows game info and time remaining in a scoreboard."),
    ];
    if !hud {
        parts.push(Part::text("\n"));
        parts.push(toggle(
            Setting::ScoreboardAutoHide,
            settings.scoreboard_auto_hide,
            "bingo.player_settings.scoreboard_auto_hide",
            "Scoreboard Auto Hide",
            "bingo.player_settings.scoreboard_auto_hide.description",
            "Hides the scoreboard when you aren't holding the bingo card.",
        ));
    }
    parts.extend([
        Part::text("\n\n"),
        Part::translate("bingo.player_settings.messages", "Floating Messages", vec![]),
        Part::text(":\n"),
        toggle(Setting::LeadingMessages, settings.leading_messages, "bingo.player_settings.messages.leading", "Leading Team", "bingo.player_settings.messages.leading.description", "Shows a message when the leading team is changed."),
        Part::text("\n"),
        toggle(Setting::ScoreMessages, settings.score_messages, "bingo.player_settings.messages.lines", "Line Scored", "bingo.player_settings.messages.lines.description", "Shows a message when a line is scored."),
        Part::text("\n"),
        toggle(Setting::ItemMessages, settings.item_messages, "bingo.player_settings.messages.items", "Item Scored", "bingo.player_settings.messages.items.description", "Shows a message when any item is scored."),
        Part::text("\n\n"),
        toggle(Setting::NightVision, settings.night_vision, "bingo.player_settings.night_vision", "Night Vision", "bingo.player_settings.night_vision.description", "Receive night vision when the feature is enabled, spectating, or post-game."),
    ]);
    Part::group(parts)
}

/// One setting's line: its mark (green when on, red when off), its name and an information mark with the description.
fn toggle(
    setting: Setting,
    on: bool,
    name_key: &'static str,
    name: &'static str,
    description_key: &'static str,
    description: &'static str,
) -> Part {
    let (mark, color) = if on { (ON_MARK, "dark_green") } else { (OFF_MARK, "dark_red") };
    Part::group(vec![
        Part::text(mark).color(color),
        Part::text(" "),
        Part::translate(name_key, name, vec![]),
        Part::text(&format!(" {INFO_MARK}"))
            .color("gray")
            .hover(Part::translate(description_key, description, vec![])),
    ])
    // A click on the line sets the setting to its opposite (`BingoPrefsCommand.getCommand`).
    .run_command(prefs::click_command(setting, !on))
}

/// Replaces each `%s` of `template` with the next argument (left as `%s` when there is none). `%%` is a percent sign.
fn fill(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some('s') => {
                chars.next();
                match args.next() {
                    Some(arg) => out.push_str(arg),
                    None => out.push_str("%s"),
                }
            }
            _ => out.push('%'),
        }
    }
    out
}

/// A double-quoted SNBT string: a backslash, a quote and a line break are escaped.
fn snbt_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

//! The names of the advanced objective squares (`objectives.rs`), as the original builds them. Sources in
//! orig/common/src/main/java/me/jfenn/bingo/common/:
//! - one_of / all_of (card/objective/SomeOfObjectiveManager.kt, init): the lang text `bingo.objective.one_of` or
//!   `all_of` with the first entry's name followed by ", ..." (the Unicode ellipsis).
//! - inverse (card/objective/InverseObjectiveManager.kt, init): `bingo.objective.inverse` with the inverse objective's
//!   name ("Never get %s").
//! - stats (card/objective/StatsObjectiveManager.kt, init): the translation `stat.<namespace>.<path>` with the fallback
//!   of the type's and the name's titles ("Mined Stone"), then ": <min>".
//! - every name that is not the item's own name is drawn in italics (card/objective/display/ObjectiveDisplayService.kt).
//! `format_title` is the original's `String.formatTitle` (utils/String.kt).
use crate::nbt::Nbt;
use crate::text::Text;

/// The original's `String.formatTitle`: a space before an upper-case letter that follows a lower-case one, `_` as a
/// space, the first letter and every letter after a space upper-cased.
pub fn format_title(value: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in value.chars() {
        if previous.is_some_and(|p| p.is_ascii_lowercase()) && c.is_ascii_uppercase() {
            spaced.push(' ');
        }
        spaced.push(if c == '_' { ' ' } else { c });
        previous = Some(c);
    }
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for (index, c) in spaced.chars().enumerate() {
        if index == 0 {
            out.extend(c.to_uppercase());
        } else if previous.is_some_and(char::is_whitespace) && c.is_ascii_lowercase() {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
        previous = Some(c);
    }
    out
}

/// The lang text `key` with `args` as its `%s` arguments (a translation with no fallback: the key is shown when the
/// language lacks it, as in the mod).
pub fn translated(key: &str, args: Vec<Text>) -> Text {
    Text(Nbt::compound(vec![("translate", Nbt::string(key)), ("with", Nbt::list(args.into_iter().map(as_compound).collect()))]))
}

/// A component as a compound: NBT lists hold one tag type, so a plain literal is wrapped as `{"text": ...}` when it sits
/// in a list with compounds.
fn as_compound(text: Text) -> Nbt {
    match text.0 {
        Nbt::String(value) => Nbt::compound(vec![("text", Nbt::string(&value))]),
        other => other,
    }
}

/// The parts one after another, as one component with no style of its own.
fn sequence(parts: Vec<Text>) -> Text {
    Text(Nbt::compound(vec![
        ("text", Nbt::string("")),
        ("extra", Nbt::list(parts.into_iter().map(as_compound).collect())),
    ]))
}

/// The name in italics, as the original draws every name that is not an item's own name.
pub fn italic(text: Text) -> Text {
    Text(Nbt::compound(vec![
        ("text", Nbt::string("")),
        ("italic", Nbt::Byte(1)),
        ("extra", Nbt::list(vec![as_compound(text)])),
    ]))
}

/// The contents of a one_of or all_of name: the first entry's name and ", ...". `first` is the first entry's name, or
/// `None` for an empty list, which reads as ", ..." alone, as the original's empty name does.
pub fn contents_name(first: Option<Text>) -> Text {
    sequence(vec![first.unwrap_or_else(|| Text::plain("")), Text::plain(", \u{2026}")])
}

/// A stats square's name: the stat's name (translated, with its title as the fallback) and ": <min>".
pub fn stat_name(stat_type: &str, stat_name: Option<&str>, min: i64) -> Text {
    let type_path = after_colon(stat_type);
    let name_path = stat_name.map(after_colon);
    let namespace = stat_name.unwrap_or(stat_type).split(':').next().unwrap_or_default();
    let fallback = format_title(type_path)
        + &name_path.map(|path| format!(" {}", format_title(path))).unwrap_or_default();
    let name = match name_path {
        Some(path) => Text(Nbt::compound(vec![
            ("translate", Nbt::string(&format!("stat.{namespace}.{path}"))),
            ("fallback", Nbt::string(&fallback)),
        ])),
        None => Text::plain(&fallback),
    };
    sequence(vec![name, Text::plain(&format!(": {min}"))])
}

/// The part of `id` after its first colon, or the whole `id` when it has none (Kotlin's `substringAfter`).
fn after_colon(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, path)| path)
}

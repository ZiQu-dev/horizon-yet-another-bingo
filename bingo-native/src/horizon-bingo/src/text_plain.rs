//! A text component flattened to a string: the literal, a translation filled with its arguments (the key itself when
//! neither the language nor the component's `fallback` has it), a keybind's key, then the siblings. `runs` keeps each
//! stretch's colour; `flatten` is the same text without styles. The language comes from the original mod's own
//! `lang/en_us.json`, read by the host.

use crate::nbt::Nbt;
use crate::text::Text;

pub type Lang<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn flatten(text: &Text, lang: Lang<'_>) -> String {
    runs(text, lang).into_iter().map(|r| r.text).collect()
}

/// A stretch of a flattened component in one colour (`None`: the colour the caller draws with).
#[derive(Debug, Clone, PartialEq)]
pub struct StyledRun {
    pub text: String,
    pub rgb: Option<u32>,
    /// A player's head (the `player` object component) instead of glyphs: `text` is two spaces, the 8 pixels it takes.
    pub head: Option<u128>,
}

/// The component's text in runs of one colour each. A component's colour reaches its `extra` and its translation's
/// arguments unless they set their own, as in `Style.applyTo`.
pub fn runs(text: &Text, lang: Lang<'_>) -> Vec<StyledRun> {
    let mut out: Vec<StyledRun> = Vec::new();
    let mut walk = Walk { lang, chars: MAX_CHARS, steps: MAX_STEPS, expanding: 0 };
    collect(&text.0, &mut walk, None, 0, &mut out);
    out
}

/// Limits on what one `runs` call may do. A component comes from a server, and a translation can write its arguments
/// many times (`%1$s%1$s...` with a `with` that nests the same way), so its output grows exponentially with the depth
/// of the component. These caps stop that without changing the output of any ordinary text. Plain text is written
/// once, so it is not limited: it is as long as the packet that carried it.
/// Characters a translation may write (its template and its arguments) per call.
const MAX_CHARS: usize = 8192;
/// Nesting of `extra`, `with` and translation arguments; anything deeper is not shown.
const MAX_DEPTH: usize = 32;
/// Arguments a single translation template may expand (a placeholder that is repeated counts each time).
const MAX_PLACEHOLDERS: usize = 64;
/// Work per call: a component visited, or a template character read. This bounds the time when the output is empty,
/// which the character budget alone does not.
const MAX_STEPS: usize = 1 << 16;

/// What is left of the limits while one `runs` call walks the component.
struct Walk<'a> {
    lang: Lang<'a>,
    /// Characters a translation may still write.
    chars: usize,
    steps: usize,
    /// Translations being written around the current point; the character budget applies while this is non-zero.
    expanding: usize,
}

impl Walk<'_> {
    /// How many of `n` characters may be written now, and spends them from the budget.
    fn allow(&mut self, n: usize) -> usize {
        if self.expanding == 0 {
            return n;
        }
        let k = n.min(self.chars);
        self.chars -= k;
        k
    }
}

/// `ChatFormatting` colours by name, and `#rrggbb`.
pub fn named_color(name: &str) -> Option<u32> {
    if let Some(hex) = name.strip_prefix('#') {
        return u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6);
    }
    Some(match name {
        "black" => 0x000000,
        "dark_blue" => 0x0000AA,
        "dark_green" => 0x00AA00,
        "dark_aqua" => 0x00AAAA,
        "dark_red" => 0xAA0000,
        "dark_purple" => 0xAA00AA,
        "gold" => 0xFFAA00,
        "gray" => 0xAAAAAA,
        "dark_gray" => 0x555555,
        "blue" => 0x5555FF,
        "green" => 0x55FF55,
        "aqua" => 0x55FFFF,
        "red" => 0xFF5555,
        "light_purple" => 0xFF55FF,
        "yellow" => 0xFFFF55,
        "white" => 0xFFFFFF,
        _ => return None,
    })
}

fn push(walk: &mut Walk<'_>, out: &mut Vec<StyledRun>, text: &str, rgb: Option<u32>) {
    if text.is_empty() {
        return;
    }
    let total = text.chars().count();
    let keep = walk.allow(total);
    if keep == 0 {
        return;
    }
    let text = if keep == total { text } else { &text[..text.char_indices().nth(keep).map_or(text.len(), |(i, _)| i)] };
    match out.last_mut() {
        Some(last) if last.rgb == rgb && last.head.is_none() => last.text.push_str(text),
        _ => out.push(StyledRun { text: text.to_string(), rgb, head: None }),
    }
}

/// The uuid of a `player` object component (`{player: {id: [int; 4]}}`).
fn player_head(nbt: &Nbt) -> Option<u128> {
    match nbt.get("player")?.get("id")? {
        Nbt::IntArray(v) if v.len() == 4 => Some(v.iter().fold(0u128, |acc, &i| (acc << 32) | i as u32 as u128)),
        _ => None,
    }
}

/// Every player head a component shows, in order.
pub fn heads(text: &Text) -> Vec<u128> {
    fn walk(nbt: &Nbt, out: &mut Vec<u128>) {
        match nbt {
            Nbt::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
            Nbt::Compound(entries) => {
                out.extend(player_head(nbt));
                for (key, value) in entries {
                    if key == "extra" || key == "with" {
                        walk(value, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&text.0, &mut out);
    out
}

/// Appends `nbt` to `out`. `depth` counts the nesting of `extra`, `with` and translation arguments; past
/// `MAX_DEPTH`, or once the walk's budget is spent, nothing more is appended.
fn collect(nbt: &Nbt, walk: &mut Walk<'_>, inherited: Option<u32>, depth: usize, out: &mut Vec<StyledRun>) {
    if depth > MAX_DEPTH || walk.steps == 0 || (walk.expanding > 0 && walk.chars == 0) {
        return;
    }
    walk.steps -= 1;
    match nbt {
        Nbt::String(s) => push(walk, out, s, inherited),
        Nbt::Byte(v) => push(walk, out, &v.to_string(), inherited),
        Nbt::Short(v) => push(walk, out, &v.to_string(), inherited),
        Nbt::Int(v) => push(walk, out, &v.to_string(), inherited),
        Nbt::Long(v) => push(walk, out, &v.to_string(), inherited),
        Nbt::Float(v) => push(walk, out, &v.to_string(), inherited),
        Nbt::Double(v) => push(walk, out, &v.to_string(), inherited),
        // A list is a component followed by its siblings.
        Nbt::List { items, .. } => items.iter().for_each(|i| collect(i, walk, inherited, depth, out)),
        Nbt::Compound(_) => {
            let rgb = match nbt.get("color") {
                Some(Nbt::String(c)) => named_color(c).or(inherited),
                _ => inherited,
            };
            if let Some(uuid) = player_head(nbt) {
                // A head is two spaces wide.
                if walk.allow(2) == 2 {
                    out.push(StyledRun { text: "  ".into(), rgb, head: Some(uuid) });
                }
            }
            // Plain text is `text`; the 26.x codec also writes it under the empty key.
            if let Some(Nbt::String(t)) = nbt.get("text").or_else(|| nbt.get("")) {
                push(walk, out, t, rgb);
            } else if let Some(Nbt::String(key)) = nbt.get("translate") {
                walk.expanding += 1;
                let with: &[Nbt] = match nbt.get("with") {
                    Some(Nbt::List { items, .. }) => items,
                    _ => &[],
                };
                let template = (walk.lang)(key)
                    .or_else(|| match nbt.get("fallback") {
                        Some(Nbt::String(f)) => Some(f.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| key.clone());
                walk.steps = walk.steps.saturating_sub(template.len());
                // The arguments go in as marker characters, which are then swapped for their runs. Only the first
                // `MAX_PLACEHOLDERS` arguments have a marker.
                let markers: Vec<String> =
                    (0..with.len().min(MAX_PLACEHOLDERS)).map(|i| format!("{}{i}{}", '\u{E000}', '\u{E001}')).collect();
                let filled = format_template(&template, &markers);
                let mut rest = filled.as_str();
                let mut expanded = 0usize;
                while let Some(start) = rest.find('\u{E000}') {
                    push(walk, out, &rest[..start], rgb);
                    let after = &rest[start + '\u{E000}'.len_utf8()..];
                    let end = after.find('\u{E001}').unwrap_or(after.len());
                    expanded += 1;
                    if expanded <= MAX_PLACEHOLDERS {
                        if let Some(arg) = after[..end].parse::<usize>().ok().and_then(|i| with.get(i)) {
                            // Written where it is used, so a placeholder that repeats writes it again.
                            collect(arg, walk, rgb, depth + 1, out);
                        }
                    }
                    rest = after.get(end + '\u{E001}'.len_utf8()..).unwrap_or("");
                }
                push(walk, out, rest, rgb);
                walk.expanding -= 1;
            } else if let Some(Nbt::String(key)) = nbt.get("keybind") {
                push(walk, out, key, rgb);
            }
            if let Some(Nbt::List { items, .. }) = nbt.get("extra") {
                for item in items {
                    collect(item, walk, rgb, depth + 1, out);
                }
            }
        }
        Nbt::ByteArray(_) | Nbt::IntArray(_) | Nbt::LongArray(_) => {}
    }
}

/// Java's `String.format` as Minecraft's translations use it: `%s`, `%1$s` and `%%`.
pub fn format_template(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut next = 0usize;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some('s') => {
                chars.next();
                out.push_str(args.get(next).map(String::as_str).unwrap_or(""));
                next += 1;
            }
            Some(d) if d.is_ascii_digit() => {
                let mut digits = String::new();
                let mut look = chars.clone();
                while let Some(&x) = look.peek() {
                    if x.is_ascii_digit() {
                        digits.push(x);
                        look.next();
                    } else {
                        break;
                    }
                }
                if look.next() == Some('$') && look.next() == Some('s') {
                    let index: usize = digits.parse().unwrap_or(0);
                    out.push_str(index.checked_sub(1).and_then(|i| args.get(i)).map(String::as_str).unwrap_or(""));
                    chars = look;
                } else {
                    out.push('%');
                }
            }
            _ => out.push('%'),
        }
    }
    out
}

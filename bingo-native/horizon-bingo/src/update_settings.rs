//! `PlayerSettings` (V2, channel `update_settings`). Mirrors
//! common/src/main/java/me/jfenn/bingo/common/config/PlayerSettings.kt.
//!
//! The payload is ONE string holding the settings as JSON. The Kotlin side writes it with
//! `json` from common/utils/Json.kt: `prettyPrint` (4-space indent, `"key": value`, one field per
//! line, no trailing newline) and `encodeDefaults`, so every field is always present, in
//! declaration order. Reading uses the same `json`, which is lenient; this port reads the
//! strict form plus whitespace, unknown keys and missing keys (see REPORT.md).

use crate::buf::{DecodeError, Reader, Result, Writer};

/// `PlayerSettings`: the per-player toggles, in Kotlin declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSettings {
    pub seen_tutorial: bool,
    pub hide_lobby_prompt: bool,
    pub bossbar: bool,
    pub scoreboard: bool,
    pub scoreboard_auto_hide: bool,
    pub leading_messages: bool,
    pub score_messages: bool,
    pub item_messages: bool,
    pub night_vision: bool,
}

/// The Kotlin constructor defaults. `itemMessages` defaults to `scoreMessages`, so it has no
/// fixed default of its own; with the defaults above it is `true`.
impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            seen_tutorial: false,
            hide_lobby_prompt: false,
            bossbar: false,
            scoreboard: true,
            scoreboard_auto_hide: false,
            leading_messages: true,
            score_messages: true,
            item_messages: true,
            night_vision: true,
        }
    }
}

/// The JSON property names, in Kotlin declaration order. These are the wire names.
const NAMES: [&str; 9] = [
    "seenTutorial",
    "hideLobbyPrompt",
    "bossbar",
    "scoreboard",
    "scoreboardAutoHide",
    "leadingMessages",
    "scoreMessages",
    "itemMessages",
    "nightVision",
];

impl PlayerSettings {
    pub const ID: &'static str = "yet-another-minecraft-bingo:update_settings";

    /// `toPacketBuf`: the settings as JSON, in one string.
    pub fn write(&self, dest: &mut Writer) {
        dest.write_string(&self.to_json());
    }

    /// `fromPacketBuf`: one string, decoded as JSON.
    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        let text = src.read_string()?;
        Self::from_json(&text)
    }

    fn values(&self) -> [bool; 9] {
        [
            self.seen_tutorial,
            self.hide_lobby_prompt,
            self.bossbar,
            self.scoreboard,
            self.scoreboard_auto_hide,
            self.leading_messages,
            self.score_messages,
            self.item_messages,
            self.night_vision,
        ]
    }

    /// The exact text kotlinx.serialization writes with `prettyPrint = true`.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        let values = self.values();
        for (i, (name, value)) in NAMES.iter().zip(values).enumerate() {
            out.push_str("    \"");
            out.push_str(name);
            out.push_str("\": ");
            out.push_str(if value { "true" } else { "false" });
            out.push_str(if i + 1 < NAMES.len() { ",\n" } else { "\n" });
        }
        out.push('}');
        out
    }

    /// Reads the settings from JSON. Unknown keys are skipped and missing keys take the Kotlin
    /// default. A later duplicate key replaces an earlier one (a guess, not checked against the
    /// oracle).
    pub fn from_json(text: &str) -> Result<Self> {
        let mut cur = Cursor::new(text);
        let mut found: [Option<bool>; 9] = [None; 9];

        cur.eat(b'{')?;
        cur.ws();
        if cur.peek() == Some(b'}') {
            cur.pos += 1;
        } else {
            loop {
                cur.ws();
                let key = cur.key()?;
                cur.eat(b':')?;
                match NAMES.iter().position(|name| *name == key) {
                    Some(i) => {
                        cur.ws();
                        found[i] = Some(cur.boolean()?);
                    }
                    None => cur.skip_value()?,
                }
                cur.ws();
                match cur.next() {
                    Some(b',') => continue,
                    Some(b'}') => break,
                    _ => return Err(DecodeError::InvalidJson("expected ',' or '}'")),
                }
            }
        }
        cur.ws();
        if cur.pos != cur.bytes.len() {
            return Err(DecodeError::InvalidJson("text after the object"));
        }

        let d = Self::default();
        let score_messages = found[6].unwrap_or(d.score_messages);
        Ok(Self {
            seen_tutorial: found[0].unwrap_or(d.seen_tutorial),
            hide_lobby_prompt: found[1].unwrap_or(d.hide_lobby_prompt),
            bossbar: found[2].unwrap_or(d.bossbar),
            scoreboard: found[3].unwrap_or(d.scoreboard),
            scoreboard_auto_hide: found[4].unwrap_or(d.scoreboard_auto_hide),
            leading_messages: found[5].unwrap_or(d.leading_messages),
            score_messages,
            item_messages: found[7].unwrap_or(score_messages),
            night_vision: found[8].unwrap_or(d.night_vision),
        })
    }
}

/// A byte cursor over the JSON text. Only what `from_json` needs.
struct Cursor<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, bytes: text.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, want: u8) -> Result<()> {
        self.ws();
        match self.next() {
            Some(b) if b == want => Ok(()),
            _ => Err(DecodeError::InvalidJson("unexpected character")),
        }
    }

    /// A property name: a quoted string with no escapes (every wire name is plain ASCII).
    fn key(&mut self) -> Result<&'a str> {
        if self.next() != Some(b'"') {
            return Err(DecodeError::InvalidJson("expected a quoted key"));
        }
        let start = self.pos;
        loop {
            match self.next() {
                Some(b'"') => return Ok(&self.text[start..self.pos - 1]),
                Some(b'\\') => return Err(DecodeError::InvalidJson("escapes in keys")),
                Some(_) => {}
                None => return Err(DecodeError::InvalidJson("unterminated key")),
            }
        }
    }

    fn boolean(&mut self) -> Result<bool> {
        let rest = &self.bytes[self.pos..];
        if rest.starts_with(b"true") {
            self.pos += 4;
            Ok(true)
        } else if rest.starts_with(b"false") {
            self.pos += 5;
            Ok(false)
        } else {
            Err(DecodeError::InvalidJson("expected true or false"))
        }
    }

    /// Skips one JSON value of any kind, for keys this version does not know.
    fn skip_value(&mut self) -> Result<()> {
        self.ws();
        match self.peek() {
            Some(b'"') => {
                self.pos += 1;
                self.skip_string_body()
            }
            Some(b'{') | Some(b'[') => {
                let mut depth = 0usize;
                loop {
                    match self.next() {
                        Some(b'"') => self.skip_string_body()?,
                        Some(b'{') | Some(b'[') => depth += 1,
                        Some(b'}') | Some(b']') => {
                            depth -= 1;
                            if depth == 0 {
                                return Ok(());
                            }
                        }
                        Some(_) => {}
                        None => return Err(DecodeError::InvalidJson("unterminated value")),
                    }
                }
            }
            Some(_) => {
                let start = self.pos;
                while matches!(self.peek(), Some(b) if b.is_ascii_alphanumeric() || b"+-.".contains(&b))
                {
                    self.pos += 1;
                }
                if self.pos == start {
                    Err(DecodeError::InvalidJson("expected a value"))
                } else {
                    Ok(())
                }
            }
            None => Err(DecodeError::InvalidJson("expected a value")),
        }
    }

    /// Skips to just past the closing quote of a string whose opening quote was consumed.
    fn skip_string_body(&mut self) -> Result<()> {
        loop {
            match self.next() {
                Some(b'"') => return Ok(()),
                Some(b'\\') => {
                    self.next();
                }
                Some(_) => {}
                None => return Err(DecodeError::InvalidJson("unterminated string")),
            }
        }
    }
}

//! Stats packet. Mirrors common/src/main/java/me/jfenn/bingo/common/stats/packets/StatsGamePacket.kt
//! and the data classes it serialises (common/src/main/java/me/jfenn/bingo/common/stats/data/).
//!
//! The whole packet is one string: `json.encodeToString(PlayerGameSummary)`, where `json` is
//! kotlinx.serialization with `prettyPrint = true` and `encodeDefaults = true`
//! (common/src/main/java/me/jfenn/bingo/common/utils/Json.kt). So the bytes are pretty-printed JSON
//! with 4-space indents and no trailing newline. Field order is declaration order.
//!
//! Java types used by the data classes are mirrored as `Instant` (java.time.Instant, seconds from
//! the epoch plus nanos), `Duration` (java.time.Duration, seconds plus nanos) and `u128` for UUID.
//! Their text forms are the ones Java's `toString`/`parse` give, see the helpers at the bottom.

use crate::buf::{DecodeError, Reader, Writer};

/// Netty's `writeUtf` and `readUtf` refuse a string longer than this many chars (UTF-16 units).
pub const MAX_STRING_CHARS: usize = 32767;

/// `StatsGamePacket` (V1): the game summary the recipient inserts into its database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsGamePacket {
    pub game: PlayerGameSummary,
}

/// `PlayerGameSummary`: the game, the player's team and the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerGameSummary {
    pub game: GameInfo,
    pub team: GameTeamInfo,
    pub player: GamePlayerInfo,
}

/// `GameInfo`. `host_id` defaults to the all-zero UUID when absent on read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInfo {
    pub id: u128,
    /// The bingo options as a JSON string (the mod stores them pretty-printed).
    pub bingo_options: String,
    pub bingo_options_hash: String,
    pub started_at: Instant,
    pub ended_at: Instant,
    pub duration: Duration,
    pub player_count: i32,
    /// True if the game ended due to a timeout and was not won by any team.
    pub is_draw: bool,
    /// True if the game was ended early by "/bingo end".
    pub is_forfeit: bool,
    /// Identifier unique to the server this was played on.
    pub host_id: u128,
}

/// `GameTeamInfo`. `id` is `BingoTeamKey`, a value class that serialises as its bare string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameTeamInfo {
    pub id: String,
    pub game_id: u128,
    pub name: String,
    pub is_winner: bool,
}

/// `GamePlayerInfo`. `team_id` is `BingoTeamKey`, serialised as its bare string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePlayerInfo {
    pub team_id: String,
    pub game_id: u128,
    pub minecraft_id: u128,
    pub minecraft_name: String,
    pub captured_items: i32,
}

/// The default `hostId` (`UUID.fromString("00000000-0000-0000-0000-000000000000")`).
pub const DEFAULT_HOST_ID: u128 = 0;

impl StatsGamePacket {
    /// `StatsGamePacket.V1.id`, `stats_game` in the `yet-another-minecraft-bingo` namespace.
    pub const ID: &'static str = "yet-another-minecraft-bingo:stats_game";

    /// `toPacketBuf`: one string holding the JSON. Fails the way the mod's buffer does when the
    /// JSON text is longer than `MAX_STRING_CHARS` chars (Netty's `EncoderException`).
    pub fn write(&self, dest: &mut Writer) -> Result<(), TooLong> {
        let text = self.to_json();
        let chars = text.encode_utf16().count();
        if chars > MAX_STRING_CHARS {
            return Err(TooLong { chars, max: MAX_STRING_CHARS });
        }
        dest.write_string(&text);
        Ok(())
    }

    /// `fromPacketBuf`: reads the string, then parses the JSON. Unknown keys are skipped and a
    /// missing `hostId` takes its default, as `ignoreUnknownKeys` and the Kotlin default do.
    pub fn read(src: &mut Reader<'_>) -> crate::buf::Result<Self> {
        let text = src.read_string()?;
        let chars = text.encode_utf16().count();
        if chars > MAX_STRING_CHARS {
            return Err(DecodeError::Json(format!(
                "string is {chars} chars, longer than the {MAX_STRING_CHARS} allowed"
            )));
        }
        Self::from_json(&text)
    }

    /// `json.encodeToString(source.game)`, pretty-printed as kotlinx does.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        render(&self.game.to_json(), 0, &mut out);
        out
    }

    pub fn from_json(text: &str) -> crate::buf::Result<Self> {
        let root = parse(text)?;
        let fields = root.as_object("root")?;
        Ok(Self { game: PlayerGameSummary::from_json(fields)? })
    }
}

/// A `write` refused because the JSON text is too long for the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooLong {
    pub chars: usize,
    pub max: usize,
}

impl PlayerGameSummary {
    fn to_json(&self) -> Json {
        Json::Object(vec![
            ("game".into(), self.game.to_json()),
            ("team".into(), self.team.to_json()),
            ("player".into(), self.player.to_json()),
        ])
    }

    fn from_json(fields: &[(String, Json)]) -> crate::buf::Result<Self> {
        Ok(Self {
            game: GameInfo::from_json(req_object(fields, "game")?)?,
            team: GameTeamInfo::from_json(req_object(fields, "team")?)?,
            player: GamePlayerInfo::from_json(req_object(fields, "player")?)?,
        })
    }
}

impl GameInfo {
    fn to_json(&self) -> Json {
        Json::Object(vec![
            ("id".into(), uuid_json(self.id)),
            ("bingoOptions".into(), Json::String(self.bingo_options.clone())),
            ("bingoOptionsHash".into(), Json::String(self.bingo_options_hash.clone())),
            ("startedAt".into(), Json::String(self.started_at.to_string())),
            ("endedAt".into(), Json::String(self.ended_at.to_string())),
            ("duration".into(), Json::String(self.duration.to_string())),
            ("playerCount".into(), Json::Int(i64::from(self.player_count))),
            ("isDraw".into(), Json::Bool(self.is_draw)),
            ("isForfeit".into(), Json::Bool(self.is_forfeit)),
            ("hostId".into(), uuid_json(self.host_id)),
        ])
    }

    fn from_json(fields: &[(String, Json)]) -> crate::buf::Result<Self> {
        let host_id = match field(fields, "hostId") {
            Some(v) => parse_uuid(v.as_str("hostId")?)?,
            None => DEFAULT_HOST_ID,
        };
        Ok(Self {
            id: parse_uuid(req_str(fields, "id")?)?,
            bingo_options: req_str(fields, "bingoOptions")?.to_string(),
            bingo_options_hash: req_str(fields, "bingoOptionsHash")?.to_string(),
            started_at: Instant::parse(req_str(fields, "startedAt")?)?,
            ended_at: Instant::parse(req_str(fields, "endedAt")?)?,
            duration: Duration::parse(req_str(fields, "duration")?)?,
            player_count: req_i32(fields, "playerCount")?,
            is_draw: req_bool(fields, "isDraw")?,
            is_forfeit: req_bool(fields, "isForfeit")?,
            host_id,
        })
    }
}

impl GameTeamInfo {
    fn to_json(&self) -> Json {
        Json::Object(vec![
            ("id".into(), Json::String(self.id.clone())),
            ("gameId".into(), uuid_json(self.game_id)),
            ("name".into(), Json::String(self.name.clone())),
            ("isWinner".into(), Json::Bool(self.is_winner)),
        ])
    }

    fn from_json(fields: &[(String, Json)]) -> crate::buf::Result<Self> {
        Ok(Self {
            id: req_str(fields, "id")?.to_string(),
            game_id: parse_uuid(req_str(fields, "gameId")?)?,
            name: req_str(fields, "name")?.to_string(),
            is_winner: req_bool(fields, "isWinner")?,
        })
    }
}

impl GamePlayerInfo {
    fn to_json(&self) -> Json {
        Json::Object(vec![
            ("teamId".into(), Json::String(self.team_id.clone())),
            ("gameId".into(), uuid_json(self.game_id)),
            ("minecraftId".into(), uuid_json(self.minecraft_id)),
            ("minecraftName".into(), Json::String(self.minecraft_name.clone())),
            ("capturedItems".into(), Json::Int(i64::from(self.captured_items))),
        ])
    }

    fn from_json(fields: &[(String, Json)]) -> crate::buf::Result<Self> {
        Ok(Self {
            team_id: req_str(fields, "teamId")?.to_string(),
            game_id: parse_uuid(req_str(fields, "gameId")?)?,
            minecraft_id: parse_uuid(req_str(fields, "minecraftId")?)?,
            minecraft_name: req_str(fields, "minecraftName")?.to_string(),
            captured_items: req_i32(fields, "capturedItems")?,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// java.time: Instant and Duration, with Java's toString and parse.
// ---------------------------------------------------------------------------------------------

/// `java.time.Instant`: `epoch_second` seconds from 1970-01-01T00:00Z, plus `nano` in 0..1e9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instant {
    pub epoch_second: i64,
    pub nano: u32,
}

impl Instant {
    /// `Instant.toString()`: ISO-8601 UTC, the fraction in 3, 6 or 9 digits, or none.
    /// Years 0..=9999 are four digits; outside that Java adds a sign (`+10000`, `-0001`).
    pub fn to_string(&self) -> String {
        let days = self.epoch_second.div_euclid(86_400);
        let second_of_day = self.epoch_second.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let year = if year > 9999 {
            format!("+{year}")
        } else if year < 0 {
            format!("-{:04}", -year)
        } else {
            format!("{year:04}")
        };
        let mut out = format!(
            "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
            second_of_day / 3600,
            second_of_day % 3600 / 60,
            second_of_day % 60,
        );
        let nano = self.nano;
        if nano > 0 {
            out.push('.');
            if nano % 1_000_000 == 0 {
                out.push_str(&format!("{:03}", nano / 1_000_000));
            } else if nano % 1_000 == 0 {
                out.push_str(&format!("{:06}", nano / 1_000));
            } else {
                out.push_str(&format!("{nano:09}"));
            }
        }
        out.push('Z');
        out
    }

    /// `Instant.parse`, for the form `toString` writes: a four-digit year, optional fraction of
    /// 1 to 9 digits, and a `Z`. Other offsets and extended years are not accepted here.
    pub fn parse(text: &str) -> crate::buf::Result<Self> {
        let bad = || DecodeError::Json(format!("not an Instant: {text:?}"));
        let body = text.strip_suffix('Z').ok_or_else(bad)?;
        let (date, time) = body.split_once('T').ok_or_else(bad)?;
        let mut d = date.split('-');
        let year: i64 = digits(d.next().ok_or_else(bad)?, 4, 4).ok_or_else(bad)?;
        let month: u32 = digits(d.next().ok_or_else(bad)?, 2, 2).ok_or_else(bad)?;
        let day: u32 = digits(d.next().ok_or_else(bad)?, 2, 2).ok_or_else(bad)?;
        if d.next().is_some() {
            return Err(bad());
        }
        let (clock, fraction) = match time.split_once('.') {
            Some((clock, fraction)) => (clock, Some(fraction)),
            None => (time, None),
        };
        let mut t = clock.split(':');
        let hour: i64 = digits(t.next().ok_or_else(bad)?, 2, 2).ok_or_else(bad)?;
        let minute: i64 = digits(t.next().ok_or_else(bad)?, 2, 2).ok_or_else(bad)?;
        let second: i64 = digits(t.next().ok_or_else(bad)?, 2, 2).ok_or_else(bad)?;
        if t.next().is_some() || hour > 23 || minute > 59 || second > 59 {
            return Err(bad());
        }
        let days = days_from_civil(year, month, day);
        // Reject days that do not exist, such as February 30th.
        if civil_from_days(days) != (year, month, day) {
            return Err(bad());
        }
        let nano = match fraction {
            None => 0,
            Some(f) => {
                if f.is_empty() || f.len() > 9 || !f.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(bad());
                }
                let padded = format!("{f:0<9}");
                padded.parse::<u32>().map_err(|_| bad())?
            }
        };
        Ok(Self { epoch_second: days * 86_400 + hour * 3600 + minute * 60 + second, nano })
    }
}

/// `java.time.Duration`: `seconds` (may be negative) plus `nanos` in 0..1e9, normalised as Java
/// stores it. For example `Duration.ofMillis(-1500)` is `seconds: -2, nanos: 500_000_000`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration {
    pub seconds: i64,
    pub nanos: u32,
}

impl Duration {
    /// `Duration.toString()`, ISO-8601 with `PT`, hours, minutes and seconds, as Java writes it.
    pub fn to_string(&self) -> String {
        if self.seconds == 0 && self.nanos == 0 {
            return "PT0S".into();
        }
        let negative = self.seconds < 0;
        let has_nanos = self.nanos > 0;
        let effective = if negative && has_nanos { self.seconds + 1 } else { self.seconds };
        let hours = effective / 3600;
        let minutes = (effective % 3600) / 60;
        let secs = effective % 60;

        let mut buf = String::from("PT");
        if hours != 0 {
            buf.push_str(&format!("{hours}H"));
        }
        if minutes != 0 {
            buf.push_str(&format!("{minutes}M"));
        }
        if secs == 0 && !has_nanos && buf.len() > 2 {
            return buf;
        }
        if negative && has_nanos {
            if secs == 0 {
                buf.push_str("-0");
            } else {
                buf.push_str(&secs.to_string());
            }
        } else {
            buf.push_str(&secs.to_string());
        }
        if has_nanos {
            // Java appends a ten-digit number whose leading '1' becomes the '.', then drops
            // trailing zeros. For negative durations the fraction is 2e9 - nanos.
            let fraction = if negative { 2_000_000_000 - u64::from(self.nanos) } else { u64::from(self.nanos) + 1_000_000_000 };
            let digits = fraction.to_string();
            let frac = digits[1..].trim_end_matches('0');
            buf.push('.');
            buf.push_str(frac);
        }
        buf.push('S');
        buf
    }

    /// `Duration.parse`, for the `PT` forms `to_string` writes, with optional signs on each part.
    pub fn parse(text: &str) -> crate::buf::Result<Self> {
        let bad = || DecodeError::Json(format!("not a Duration: {text:?}"));
        let (sign, rest) = match text.strip_prefix('-') {
            Some(rest) => (-1i128, rest),
            None => (1i128, text),
        };
        let rest = rest.strip_prefix("PT").ok_or_else(bad)?;
        if rest.is_empty() {
            return Err(bad());
        }
        let mut total: i128 = 0;
        let mut remaining = rest;
        let mut parts = 0;
        while !remaining.is_empty() {
            let (part_sign, after_sign) = match remaining.strip_prefix('-') {
                Some(r) => (-1i128, r),
                None => (1i128, remaining),
            };
            let unit_at = after_sign.find(|c: char| !(c.is_ascii_digit() || c == '.')).ok_or_else(bad)?;
            let number = &after_sign[..unit_at];
            let unit = after_sign.as_bytes()[unit_at];
            remaining = &after_sign[unit_at + 1..];
            let scale: i128 = match unit {
                b'H' => 3_600_000_000_000,
                b'M' => 60_000_000_000,
                b'S' => 1_000_000_000,
                _ => return Err(bad()),
            };
            let (whole, frac) = match number.split_once('.') {
                Some((w, f)) => (w, Some(f)),
                None => (number, None),
            };
            if unit != b'S' && frac.is_some() {
                return Err(bad());
            }
            let whole: i128 = whole.parse().map_err(|_| bad())?;
            let mut nanos = whole * scale;
            if let Some(f) = frac {
                if f.is_empty() || f.len() > 9 || !f.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(bad());
                }
                let fraction: i128 = format!("{f:0<9}").parse().map_err(|_| bad())?;
                nanos += fraction;
            }
            total += part_sign * nanos;
            parts += 1;
        }
        if parts == 0 {
            return Err(bad());
        }
        total *= sign;
        // Normalise like Java: seconds rounded down, nanos never negative.
        let seconds = total.div_euclid(1_000_000_000);
        let nanos = total.rem_euclid(1_000_000_000);
        Ok(Self {
            seconds: i64::try_from(seconds).map_err(|_| bad())?,
            nanos: nanos as u32,
        })
    }
}

/// Parses an ASCII-digit field of `min..=max` digits.
fn digits<T: std::str::FromStr>(text: &str, min: usize, max: usize) -> Option<T> {
    if text.len() < min || text.len() > max || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The (year, month, day) of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

// ---------------------------------------------------------------------------------------------
// UUID text form, as java.util.UUID writes and reads it.
// ---------------------------------------------------------------------------------------------

fn uuid_json(value: u128) -> Json {
    Json::String(format_uuid(value))
}

/// `UUID.toString()`: lower-case hex in 8-4-4-4-12 groups. Most significant bits first.
pub fn format_uuid(value: u128) -> String {
    let most = (value >> 64) as u64;
    let least = value as u64;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        most >> 32,
        (most >> 16) & 0xffff,
        most & 0xffff,
        least >> 48,
        least & 0xffff_ffff_ffff,
    )
}

/// `UUID.fromString` for the 36-character form. Java also takes shorter groups; this does not.
pub fn parse_uuid(text: &str) -> crate::buf::Result<u128> {
    let bad = || DecodeError::Json(format!("not a UUID: {text:?}"));
    let bytes = text.as_bytes();
    if bytes.len() != 36 || [8, 13, 18, 23].iter().any(|&i| bytes[i] != b'-') {
        return Err(bad());
    }
    let hex: String = text.chars().filter(|&c| c != '-').collect();
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    u128::from_str_radix(&hex, 16).map_err(|_| bad())
}

// ---------------------------------------------------------------------------------------------
// JSON: kotlinx's pretty printer and a reader for the shape above.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Json {
    Object(Vec<(String, Json)>),
    Array(Vec<Json>),
    String(String),
    /// Written as a plain decimal. Read back only as an integer.
    Int(i64),
    Bool(bool),
    Null,
}

impl Json {
    fn as_object(&self, what: &str) -> crate::buf::Result<&[(String, Json)]> {
        match self {
            Json::Object(fields) => Ok(fields),
            _ => Err(DecodeError::Json(format!("{what} is not an object"))),
        }
    }

    fn as_str(&self, what: &str) -> crate::buf::Result<&str> {
        match self {
            Json::String(s) => Ok(s),
            _ => Err(DecodeError::Json(format!("{what} is not a string"))),
        }
    }
}

/// kotlinx's pretty print: 4 spaces per level, `"key": value`, no trailing newline. An empty
/// object is `{}`.
fn render(value: &Json, depth: usize, out: &mut String) {
    match value {
        Json::Object(fields) if fields.is_empty() => out.push_str("{}"),
        Json::Object(fields) => {
            out.push('{');
            for (i, (key, v)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                indent(out, depth + 1);
                push_escaped(out, key);
                out.push_str(": ");
                render(v, depth + 1, out);
            }
            out.push('\n');
            indent(out, depth);
            out.push('}');
        }
        Json::Array(items) if items.is_empty() => out.push_str("[]"),
        Json::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                indent(out, depth + 1);
                render(v, depth + 1, out);
            }
            out.push('\n');
            indent(out, depth);
            out.push(']');
        }
        Json::String(s) => push_escaped(out, s),
        Json::Int(n) => out.push_str(&n.to_string()),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Null => out.push_str("null"),
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

/// kotlinx escapes `"` and `\`, the controls \b \f \n \r \t by their short forms, and any other
/// char below 0x20 as `\u00xx` in lower-case hex. Everything else, including `/`, DEL and
/// U+2028, goes out as is.
fn push_escaped(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn field<'a>(fields: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn req<'a>(fields: &'a [(String, Json)], key: &str) -> crate::buf::Result<&'a Json> {
    field(fields, key).ok_or_else(|| DecodeError::Json(format!("missing field {key}")))
}

fn req_str<'a>(fields: &'a [(String, Json)], key: &str) -> crate::buf::Result<&'a str> {
    req(fields, key)?.as_str(key)
}

fn req_object<'a>(fields: &'a [(String, Json)], key: &str) -> crate::buf::Result<&'a [(String, Json)]> {
    req(fields, key)?.as_object(key)
}

fn req_bool(fields: &[(String, Json)], key: &str) -> crate::buf::Result<bool> {
    match req(fields, key)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(DecodeError::Json(format!("{key} is not a boolean"))),
    }
}

fn req_i32(fields: &[(String, Json)], key: &str) -> crate::buf::Result<i32> {
    match req(fields, key)? {
        Json::Int(n) => i32::try_from(*n).map_err(|_| DecodeError::Json(format!("{key} is out of Int range"))),
        _ => Err(DecodeError::Json(format!("{key} is not an integer"))),
    }
}

/// Parses one JSON value and nothing after it. Strict JSON: no comments, no unquoted strings,
/// no fractions or exponents (the packet never has them).
fn parse(text: &str) -> crate::buf::Result<Json> {
    let mut p = JsonParser { bytes: text.as_bytes(), text, pos: 0 };
    let value = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(DecodeError::Json(format!("trailing text at byte {}", p.pos)));
    }
    Ok(value)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl JsonParser<'_> {
    fn err(&self, what: &str) -> DecodeError {
        DecodeError::Json(format!("{what} at byte {}", self.pos))
    }

    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> crate::buf::Result<Json> {
        self.skip_ws();
        match self.bytes.get(self.pos) {
            Some(b'{') => self.object(),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b'[') => self.array(),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.integer(),
            _ => Err(self.err("expected a value")),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> crate::buf::Result<Json> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.err("bad literal"))
        }
    }

    fn integer(&mut self) -> crate::buf::Result<Json> {
        let start = self.pos;
        self.eat(b'-');
        let digits_start = self.pos;
        while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == digits_start {
            return Err(self.err("expected digits"));
        }
        if matches!(self.bytes.get(self.pos), Some(b'.' | b'e' | b'E')) {
            return Err(self.err("fractions and exponents are not read"));
        }
        self.text[start..self.pos]
            .parse::<i64>()
            .map(Json::Int)
            .map_err(|_| self.err("integer out of range"))
    }

    fn string(&mut self) -> crate::buf::Result<String> {
        if !self.eat(b'"') {
            return Err(self.err("expected a string"));
        }
        let mut out = String::new();
        loop {
            let Some(&byte) = self.bytes.get(self.pos) else {
                return Err(self.err("unterminated string"));
            };
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(&esc) = self.bytes.get(self.pos) else {
                        return Err(self.err("unterminated escape"));
                    };
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            if (0xD800..0xDC00).contains(&unit) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(self.err("lone high surrogate"));
                                }
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.err("bad low surrogate"));
                                }
                                let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                                out.push(char::from_u32(code).ok_or_else(|| self.err("bad surrogate pair"))?);
                            } else {
                                out.push(char::from_u32(unit).ok_or_else(|| self.err("lone surrogate"))?);
                            }
                        }
                        _ => return Err(self.err("bad escape")),
                    }
                }
                0x00..=0x1F => return Err(self.err("raw control character in a string")),
                _ => {
                    // Copy one whole UTF-8 char; the input is already valid UTF-8.
                    let c = self.text[self.pos..].chars().next().ok_or_else(|| self.err("bad text"))?;
                    out.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    fn hex4(&mut self) -> crate::buf::Result<u32> {
        let slice = self.bytes.get(self.pos..self.pos + 4).ok_or_else(|| self.err("short \\u escape"))?;
        let text = std::str::from_utf8(slice).map_err(|_| self.err("bad \\u escape"))?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(self.err("bad \\u escape"));
        }
        self.pos += 4;
        u32::from_str_radix(text, 16).map_err(|_| self.err("bad \\u escape"))
    }

    fn array(&mut self) -> crate::buf::Result<Json> {
        self.eat(b'[');
        let mut items = Vec::new();
        self.skip_ws();
        if self.eat(b']') {
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Json::Array(items));
            }
            return Err(self.err("expected ',' or ']'"));
        }
    }

    fn object(&mut self) -> crate::buf::Result<Json> {
        self.eat(b'{');
        let mut fields = Vec::new();
        self.skip_ws();
        if self.eat(b'}') {
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            if !self.eat(b':') {
                return Err(self.err("expected ':'"));
            }
            let value = self.value()?;
            fields.push((key, value));
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Json::Object(fields));
            }
            return Err(self.err("expected ',' or '}'"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode};

    /// The minimal valid packet, with the Kotlin default host id left out of the JSON.
    const NO_HOST_ID: &str = r#"{
    "game": {
        "id": "00000000-0000-0000-0000-000000000000",
        "bingoOptions": "",
        "bingoOptionsHash": "",
        "startedAt": "1970-01-01T00:00:00Z",
        "endedAt": "1970-01-01T00:00:00Z",
        "duration": "PT0S",
        "playerCount": 0,
        "isDraw": false,
        "isForfeit": false
    },
    "team": {"id": "", "gameId": "00000000-0000-0000-0000-000000000000", "name": "", "isWinner": false},
    "player": {"teamId": "", "gameId": "00000000-0000-0000-0000-000000000000",
               "minecraftId": "00000000-0000-0000-0000-000000000000", "minecraftName": "", "capturedItems": 0}
}"#;

    #[test]
    fn a_missing_host_id_takes_the_kotlin_default() {
        let packet = StatsGamePacket::from_json(NO_HOST_ID).unwrap();
        assert_eq!(packet.game.game.host_id, 0);
    }

    #[test]
    fn unknown_keys_are_skipped() {
        let with_extra = NO_HOST_ID.replacen("\"isForfeit\": false", "\"isForfeit\": false, \"later\": {\"a\": [1, null]}", 1);
        assert_eq!(StatsGamePacket::from_json(&with_extra), StatsGamePacket::from_json(NO_HOST_ID));
    }

    #[test]
    fn a_missing_required_field_is_an_error() {
        let without_name = NO_HOST_ID.replace("\"name\": \"\", ", "");
        assert!(matches!(StatsGamePacket::from_json(&without_name), Err(DecodeError::Json(_))));
    }

    #[test]
    fn trailing_text_after_the_object_is_an_error() {
        let trailing = format!("{NO_HOST_ID} x");
        assert!(matches!(StatsGamePacket::from_json(&trailing), Err(DecodeError::Json(_))));
    }

    #[test]
    fn a_string_longer_than_the_buffer_cap_is_refused_on_read() {
        // Written raw, past the writer's own check, the way another sender could.
        let long = format!("\"{}\"", "x".repeat(MAX_STRING_CHARS + 1));
        let bytes = encode(|w| w.write_string(&long));
        assert!(decode(&bytes, StatsGamePacket::read).is_err());
    }

    #[test]
    fn duration_text_forms() {
        // Shapes with no oracle row: a bare fraction, and a negative fraction read back.
        assert_eq!(Duration { seconds: 0, nanos: 0 }.to_string(), "PT0S");
        assert_eq!(Duration { seconds: 0, nanos: 100_000_000 }.to_string(), "PT0.1S");
        assert_eq!(Duration::parse("PT-0.1S").unwrap(), Duration { seconds: -1, nanos: 900_000_000 });
    }

    #[test]
    fn instant_text_forms() {
        assert_eq!(Instant { epoch_second: 0, nano: 0 }.to_string(), "1970-01-01T00:00:00Z");
        assert_eq!(Instant { epoch_second: 951_782_400, nano: 0 }.to_string(), "2000-02-29T00:00:00Z");
        assert_eq!(Instant::parse("2000-02-29T00:00:00Z").unwrap(), Instant { epoch_second: 951_782_400, nano: 0 });
        assert!(Instant::parse("2001-02-29T00:00:00Z").is_err());
    }
}

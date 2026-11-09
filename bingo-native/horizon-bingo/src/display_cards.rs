//! Display-cards packet. Mirrors common/src/main/java/me/jfenn/bingo/common/map/CardDisplayPacket.kt
//! (`CardDisplayPacket.V1`) and the nested `CardDisplay.V1` converter in common/.../map/CardView.kt.

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::text::{read_text, write_text_or_empty, Text};

/// Channel id of `CardDisplayPacket.V1`.
pub const ID: &str = "yet-another-minecraft-bingo:display_cards";

/// Channel-like id of the nested `CardDisplay.V1` converter (not a channel of its own).
pub const CARD_DISPLAY_ID: &str = "yet-another-minecraft-bingo:card_view_display";

/// `Formatting` enum names, in ordinal order (platform/src/main/java/me/jfenn/bingo/platform/text/Formatting.kt).
pub const FORMATTING_NAMES: [&str; 22] = [
    "BLACK", "DARK_BLUE", "DARK_GREEN", "DARK_AQUA", "DARK_RED", "DARK_PURPLE", "GOLD", "GRAY", "DARK_GRAY", "BLUE",
    "GREEN", "AQUA", "RED", "LIGHT_PURPLE", "YELLOW", "WHITE", "OBFUSCATED", "BOLD", "STRIKETHROUGH", "UNDERLINE",
    "ITALIC", "RESET",
];

/// `PlayerProfile(uuid: UUID, name: String)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerProfile {
    pub uuid: u128,
    pub name: String,
}

/// `CardDisplay(teamColor: Formatting?, teamName: IText?, players: List<PlayerProfile>)`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CardDisplay {
    /// The `Formatting` enum's name. A name the enum does not have reads as `None`.
    pub team_color: Option<String>,
    /// Written with `writeText`, so `None` goes out as the empty component (never as an absent field).
    /// Reading always yields `Some`: the wire carries no absence, only the empty component.
    pub team_name: Option<Text>,
    pub players: Vec<PlayerProfile>,
}

/// `CardDisplayPacket(display: Map<BingoTeamKey?, CardDisplay>)`. The Kotlin map is written in iteration order;
/// here that order is the Vec's order. The key is `BingoTeamKey.id`, `None` for the null key.
#[derive(Debug, Clone, PartialEq)]
pub struct CardDisplayPacket {
    pub display: Vec<(Option<String>, CardDisplay)>,
}

impl CardDisplayPacket {
    /// `writeList(display.entries) { writeNullable(key?.id, writeString); CardDisplay.V1.toPacketBuf(value) }`.
    pub fn write(&self, dest: &mut Writer) -> Result<()> {
        write_len(dest, self.display.len());
        for (key, display) in &self.display {
            dest.write_bool(key.is_some());
            if let Some(key) = key {
                dest.write_string(key);
            }
            write_card_display(dest, display)?;
        }
        Ok(())
    }

    /// `readList { readNullable(readString)?.let { BingoTeamKey(it) }; CardDisplay.V1.fromPacketBuf }`.
    /// The Kotlin code turns the list into a map, which drops repeated keys; this keeps every entry.
    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        let len = read_len(src)?;
        let mut display = Vec::new();
        for _ in 0..len {
            let key = src.read_nullable(|src| src.read_string())?;
            display.push((key, read_card_display(src)?));
        }
        Ok(Self { display })
    }
}

/// `CardDisplay.V1.toPacketBuf`.
pub fn write_card_display(dest: &mut Writer, value: &CardDisplay) -> Result<()> {
    dest.write_bool(value.team_color.is_some());
    if let Some(name) = &value.team_color {
        dest.write_string(name);
    }
    write_text_or_empty(dest, value.team_name.as_ref())?;
    write_len(dest, value.players.len());
    for player in &value.players {
        write_uuid_string(dest, player.uuid);
        dest.write_string(&player.name);
    }
    Ok(())
}

/// `CardDisplay.V1.fromPacketBuf`. An unknown enum name is read as `None`, as in Kotlin (`valueOf` is caught).
pub fn read_card_display(src: &mut Reader<'_>) -> Result<CardDisplay> {
    let team_color = if src.read_bool()? {
        let name = src.read_string()?;
        FORMATTING_NAMES.iter().find(|known| **known == name).map(|known| (*known).to_string())
    } else {
        None
    };
    let team_name = Some(read_text(src)?);
    let len = read_len(src)?;
    let mut players = Vec::new();
    for _ in 0..len {
        let uuid = read_uuid_string(src)?;
        let name = src.read_string()?;
        players.push(PlayerProfile { uuid, name });
    }
    Ok(CardDisplay { team_color, team_name, players })
}

/// `writeInt(size)`.
fn write_len(dest: &mut Writer, len: usize) {
    dest.write_i32(i32::try_from(len).expect("list longer than i32::MAX"));
}

/// `readInt()` as a list length; a negative length is an error (Kotlin's `List(n)` throws).
fn read_len(src: &mut Reader<'_>) -> Result<usize> {
    let len = src.read_i32()?;
    usize::try_from(len).map_err(|_| DecodeError::NegativeLength(len))
}

/// `UUID.toString()`: lowercase 8-4-4-4-12 hex.
pub fn write_uuid_string(dest: &mut Writer, uuid: u128) {
    let hex = format!("{uuid:032x}");
    let text = format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]);
    dest.write_string(&text);
}

/// `UUID.fromString(readString())`. Only the canonical 8-4-4-4-12 form is accepted (Java also accepts shorter
/// groups; the mod never writes those).
pub fn read_uuid_string(src: &mut Reader<'_>) -> Result<u128> {
    let text = src.read_string()?;
    parse_canonical_uuid(&text).ok_or_else(|| DecodeError::InvalidUuid(text.clone()))
}

fn parse_canonical_uuid(text: &str) -> Option<u128> {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    for (i, &c) in bytes.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if c != b'-' {
                return None;
            }
        } else if !c.is_ascii_hexdigit() {
            return None;
        }
    }
    let digits: String = text.chars().filter(|c| *c != '-').collect();
    u128::from_str_radix(&digits, 16).ok()
}

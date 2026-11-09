//! Stats index packet. Mirrors common/src/main/java/me/jfenn/bingo/common/stats/packets/StatsIndexPacket.kt.

use crate::buf::{DecodeError, Reader, Result, Writer};

/// `StatsIndexPacket.Action`. Written as an int: BROADCAST is 0, REQUEST is 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Broadcast,
    Request,
}

/// `StatsIndexPacket` (V1): an action, and a set of game ids (UUIDs, as `u128`).
///
/// Kotlin's `games` is a `Set<UUID>`, so `games` here is kept duplicate-free in first-occurrence
/// order: `write` and `read` both drop repeats, matching `toSet()` on the read side and the
/// set's own iteration order on the write side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsIndexPacket {
    pub action: Action,
    pub games: Vec<u128>,
}

impl StatsIndexPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:stats_index";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_i32(match self.action {
            Action::Broadcast => 0,
            Action::Request => 1,
        });
        let games = distinct(&self.games);
        dest.write_i32(i32::try_from(games.len()).expect("more than i32::MAX games"));
        for game in games {
            dest.write_string(&format_uuid(game));
        }
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        // Kotlin: any value other than 0 reads as REQUEST.
        let action = match src.read_i32()? {
            0 => Action::Broadcast,
            _ => Action::Request,
        };
        let count = src.read_i32()?;
        // Kotlin's `List(count)` throws on a negative count.
        let count = usize::try_from(count).map_err(|_| DecodeError::NegativeLength(count))?;
        let mut games: Vec<u128> = Vec::new();
        for _ in 0..count {
            let game = parse_uuid(&src.read_string()?)?;
            if !games.contains(&game) {
                games.push(game);
            }
        }
        Ok(Self { action, games })
    }
}

/// The values of `games` without repeats, keeping the first occurrence of each.
fn distinct(games: &[u128]) -> Vec<u128> {
    let mut out: Vec<u128> = Vec::with_capacity(games.len());
    for &game in games {
        if !out.contains(&game) {
            out.push(game);
        }
    }
    out
}

/// `UUID.toString()`: lowercase hex, 8-4-4-4-12 groups.
fn format_uuid(value: u128) -> String {
    let most = (value >> 64) as u64;
    let least = value as u64;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        most >> 32,
        (most >> 16) & 0xFFFF,
        most & 0xFFFF,
        least >> 48,
        least & 0xFFFF_FFFF_FFFF,
    )
}

/// `UUID.fromString` for the canonical 36-character form. Other spellings are rejected.
fn parse_uuid(text: &str) -> Result<u128> {
    let invalid = || DecodeError::InvalidUuid(text.to_string());
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return Err(invalid());
    }
    let mut groups = [0u64; 5];
    let mut at = 0;
    for (index, width) in [8usize, 4, 4, 4, 12].into_iter().enumerate() {
        if index > 0 {
            if bytes[at] != b'-' {
                return Err(invalid());
            }
            at += 1;
        }
        let group = &bytes[at..at + width];
        if !group.iter().all(u8::is_ascii_hexdigit) {
            return Err(invalid());
        }
        // All bytes are ASCII hex digits, so this is valid UTF-8 and parses without overflow.
        let digits = std::str::from_utf8(group).map_err(|_| invalid())?;
        groups[index] = u64::from_str_radix(digits, 16).map_err(|_| invalid())?;
        at += width;
    }
    let most = (groups[0] << 32) | (groups[1] << 16) | groups[2];
    let least = (groups[3] << 48) | groups[4];
    Ok((u128::from(most) << 64) | u128::from(least))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode};

    #[test]
    fn action_is_an_int_and_unknown_values_read_as_request() {
        let empty = StatsIndexPacket { action: Action::Broadcast, games: vec![] };
        assert_eq!(encode(|w| empty.write(w)), [0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(decode(&[0, 0, 0, 0, 0, 0, 0, 0], StatsIndexPacket::read), Ok(empty));
        assert_eq!(
            decode(&[0, 0, 0, 7, 0, 0, 0, 0], StatsIndexPacket::read),
            Ok(StatsIndexPacket { action: Action::Request, games: vec![] })
        );
    }

    #[test]
    fn repeated_games_are_written_once_in_first_occurrence_order() {
        let repeated = StatsIndexPacket { action: Action::Request, games: vec![5, 9, 5] };
        let once = StatsIndexPacket { action: Action::Request, games: vec![5, 9] };
        assert_eq!(encode(|w| repeated.write(w)), encode(|w| once.write(w)));
    }

    #[test]
    fn negative_game_count_is_rejected() {
        assert_eq!(
            decode(&[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF], StatsIndexPacket::read),
            Err(DecodeError::NegativeLength(-1))
        );
    }

    #[test]
    fn malformed_uuid_strings_are_rejected() {
        for bad in [
            "",
            "0123456789abcdeffedcba9876543210",   // no hyphens
            "01234567-89ab-cdef-fedc-ba987654321", // too short
            "01234567_89ab-cdef-fedc-ba9876543210", // wrong separator
            "0123456g-89ab-cdef-fedc-ba9876543210", // not hex
            "0123456\u{e9}-89ab-cdef-fedc-ba9876543210", // multi-byte character
        ] {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
            bytes.push(bad.len() as u8);
            bytes.extend_from_slice(bad.as_bytes());
            assert_eq!(
                decode(&bytes, StatsIndexPacket::read),
                Err(DecodeError::InvalidUuid(bad.to_string())),
                "reading {bad:?}"
            );
        }
    }
}

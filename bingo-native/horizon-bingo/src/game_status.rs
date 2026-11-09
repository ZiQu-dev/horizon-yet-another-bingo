//! `GameStatusPacket` (V1). Mirrors common/src/main/java/me/jfenn/bingo/common/game/GameStatusPacket.kt.
//!
//! Both fields are nullable `Duration`s, written with `IPacketBuf.writeDuration` (a big-endian
//! long of `toMillis()`) and read with `readDuration` (`Duration.ofMillis`).

use crate::buf::{Reader, Result, Writer};

/// A `java.time.Duration`: whole seconds plus a nanosecond part in `0..1_000_000_000`.
/// Java durations can be negative, so `std::time::Duration` does not fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaDuration {
    pub seconds: i64,
    pub nanos: i32,
}

impl JavaDuration {
    /// `Duration.ofMillis(millis)`: `floorDiv` for the seconds, `floorMod` for the nanoseconds.
    pub fn of_millis(millis: i64) -> Self {
        Self {
            seconds: millis.div_euclid(1_000),
            nanos: (millis.rem_euclid(1_000) * 1_000_000) as i32,
        }
    }

    /// `Duration.toMillis()` as `IPacketBuf.writeDuration` sees it: a `toMillis` that overflows
    /// (`ArithmeticException` in Java) is written as 0.
    pub fn to_millis_or_zero(self) -> i64 {
        // Java's toMillis makes the nanoseconds positive first, then truncates them to whole ms.
        let (mut seconds, mut nanos) = (self.seconds, i64::from(self.nanos));
        if seconds < 0 {
            seconds += 1;
            nanos -= 1_000_000_000;
        }
        seconds
            .checked_mul(1_000)
            .and_then(|millis| millis.checked_add(nanos / 1_000_000))
            .unwrap_or(0)
    }
}

/// `GameStatusPacket` (V1). `None` is the Kotlin `null`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameStatusPacket {
    pub ingame_duration: Option<JavaDuration>,
    pub remaining_duration: Option<JavaDuration>,
}

impl GameStatusPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:game_status";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_nullable(self.ingame_duration.as_ref(), |w, d| write_duration(w, d));
        dest.write_nullable(self.remaining_duration.as_ref(), |w, d| write_duration(w, d));
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        let ingame_duration = src.read_nullable(read_duration)?;
        let remaining_duration = src.read_nullable(read_duration)?;
        Ok(Self { ingame_duration, remaining_duration })
    }
}

fn write_duration(dest: &mut Writer, value: &JavaDuration) {
    dest.write_i64(value.to_millis_or_zero());
}

fn read_duration(src: &mut Reader<'_>) -> Result<JavaDuration> {
    Ok(JavaDuration::of_millis(src.read_i64()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode, DecodeError};

    fn ms(millis: i64) -> Option<JavaDuration> {
        Some(JavaDuration::of_millis(millis))
    }

    /// Bytes the real server sent, as given in the brief: 41.893 s in-game, no remaining time.
    /// Byte 0 presence, bytes 1..=8 the long 41893 ms, byte 9 presence (0 = absent).
    const REAL_SERVER_10_BYTES: [u8; 10] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA3, 0xA5, 0x00];

    #[test]
    fn real_server_bytes_name_the_in_game_duration() {
        let packet = decode(&REAL_SERVER_10_BYTES, GameStatusPacket::read).expect("10 bytes decode");
        assert_eq!(
            packet,
            GameStatusPacket {
                ingame_duration: Some(JavaDuration { seconds: 41, nanos: 893_000_000 }),
                remaining_duration: None,
            }
        );
        assert_eq!(encode(|w| packet.write(w)), REAL_SERVER_10_BYTES);
    }

    /// The second example in the brief, `010000000000007c00`, is 9 bytes, not 10. A present
    /// in-game flag takes the 8-byte long, and the remaining flag is then missing. The real mod's
    /// reader fails the same way (IndexOutOfBoundsException, see REPORT.md).
    #[test]
    fn brief_second_example_is_nine_bytes_and_truncated() {
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0x00];
        assert_eq!(bytes.len(), 9);
        assert_eq!(
            decode(&bytes, GameStatusPacket::read),
            Err(DecodeError::UnexpectedEnd { needed: 1, remaining: 0 })
        );
    }

    #[test]
    fn default_packet_is_two_absent_flags() {
        let packet = GameStatusPacket::default();
        assert_eq!(encode(|w| packet.write(w)), [0x00, 0x00]);
        assert_eq!(decode(&[0x00, 0x00], GameStatusPacket::read), Ok(packet));
    }

    #[test]
    fn remaining_only_is_flag_zero_then_flag_one_then_long() {
        let packet = GameStatusPacket { ingame_duration: None, remaining_duration: ms(124) };
        assert_eq!(
            encode(|w| packet.write(w)),
            [0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7C]
        );
        assert_eq!(
            decode(&encode(|w| packet.write(w)), GameStatusPacket::read),
            Ok(packet)
        );
    }

    #[test]
    fn milliseconds_split_into_seconds_and_nanos_like_duration_of_millis() {
        assert_eq!(JavaDuration::of_millis(-1), JavaDuration { seconds: -1, nanos: 999_000_000 });
        assert_eq!(JavaDuration::of_millis(1_500), JavaDuration { seconds: 1, nanos: 500_000_000 });
    }
}

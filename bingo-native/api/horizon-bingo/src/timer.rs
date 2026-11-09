//! Timer packets. Mirrors common/src/main/java/me/jfenn/bingo/common/timer/.

use crate::buf::{Reader, Result, Writer};

/// `TimerPacket` (V1): seconds remaining, as an i32.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerPacket {
    pub seconds_remaining: i32,
}

impl TimerPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:timer";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_i32(self.seconds_remaining);
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { seconds_remaining: src.read_i32()? })
    }
}

/// `CountdownPacket` (V1): seconds remaining, as an i32. Same layout as `TimerPacket`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountdownPacket {
    pub seconds_remaining: i32,
}

impl CountdownPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:countdown";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_i32(self.seconds_remaining);
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { seconds_remaining: src.read_i32()? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode};

    #[test]
    fn timer_of_60_seconds_is_a_big_endian_int() {
        let packet = TimerPacket { seconds_remaining: 60 };
        assert_eq!(encode(|w| packet.write(w)), [0x00, 0x00, 0x00, 0x3C]);
        assert_eq!(decode(&[0x00, 0x00, 0x00, 0x3C], TimerPacket::read), Ok(packet));
    }

    #[test]
    fn timer_round_trips() {
        for seconds_remaining in [-1, 0, 1, i32::MAX] {
            let packet = TimerPacket { seconds_remaining };
            let bytes = encode(|w| packet.write(w));
            assert_eq!(decode(&bytes, TimerPacket::read), Ok(packet));
        }
    }

    #[test]
    fn countdown_of_5_seconds_is_a_big_endian_int() {
        let packet = CountdownPacket { seconds_remaining: 5 };
        assert_eq!(encode(|w| packet.write(w)), [0x00, 0x00, 0x00, 0x05]);
        assert_eq!(decode(&[0x00, 0x00, 0x00, 0x05], CountdownPacket::read), Ok(packet));
    }

    #[test]
    fn countdown_round_trips() {
        for seconds_remaining in [-1, 0, 1, i32::MAX] {
            let packet = CountdownPacket { seconds_remaining };
            let bytes = encode(|w| packet.write(w));
            assert_eq!(decode(&bytes, CountdownPacket::read), Ok(packet));
        }
    }
}

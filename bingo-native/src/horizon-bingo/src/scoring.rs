//! Scoring packets. Mirrors common/src/main/java/me/jfenn/bingo/common/scoring/.

use crate::buf::{Reader, Result, Writer};

/// `CardCompletedPacket` (V1): whether this player won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardCompletedPacket {
    pub is_winner: bool,
}

impl CardCompletedPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:card_completed";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_bool(self.is_winner);
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { is_winner: src.read_bool()? })
    }
}

/// `ScoredItemLostPacket` (V1): no fields, empty payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoredItemLostPacket;

impl ScoredItemLostPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:scored_item_lost";

    pub fn write(&self, _dest: &mut Writer) {}

    pub fn read(_src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
}

/// `GameMessageClearPacket` (V1): no fields, empty payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameMessageClearPacket;

impl GameMessageClearPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:game_message_clear";

    pub fn write(&self, _dest: &mut Writer) {}

    pub fn read(_src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode};

    #[test]
    fn card_completed_is_one_flag_byte() {
        let winner = CardCompletedPacket { is_winner: true };
        assert_eq!(encode(|w| winner.write(w)), [0x01]);
        assert_eq!(decode(&[0x01], CardCompletedPacket::read), Ok(winner));

        let loser = CardCompletedPacket { is_winner: false };
        assert_eq!(encode(|w| loser.write(w)), [0x00]);
        assert_eq!(decode(&[0x00], CardCompletedPacket::read), Ok(loser));
    }

    #[test]
    fn payload_free_packets_encode_to_nothing() {
        assert_eq!(encode(|w| ScoredItemLostPacket.write(w)), Vec::<u8>::new());
        assert_eq!(decode(&[], ScoredItemLostPacket::read), Ok(ScoredItemLostPacket));

        assert_eq!(encode(|w| GameMessageClearPacket.write(w)), Vec::<u8>::new());
        assert_eq!(decode(&[], GameMessageClearPacket::read), Ok(GameMessageClearPacket));
    }
}

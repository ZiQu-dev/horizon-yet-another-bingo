//! Ready-screen packets. Mirrors common/src/main/java/me/jfenn/bingo/common/ready/.

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::game::{GameState, GameStatus};

/// `SetReadyPacket`: the player toggles ready, optionally for a vote option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetReadyPacket {
    pub is_ready: bool,
    /// V2 only. Always `None` after reading V1, which has no option id on the wire.
    pub option_id: Option<u128>,
}

impl SetReadyPacket {
    pub const ID_V1: &'static str = "yet-another-minecraft-bingo:set_ready";
    pub const ID_V2: &'static str = "yet-another-minecraft-bingo:set_ready_v2";

    /// `SetReadyPacket.V1`: the ready flag only. `option_id` is not written.
    pub fn write_v1(&self, dest: &mut Writer) {
        dest.write_bool(self.is_ready);
    }

    pub fn read_v1(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { is_ready: src.read_bool()?, option_id: None })
    }

    /// `SetReadyPacket.V2`: the ready flag, then a nullable UUID.
    pub fn write_v2(&self, dest: &mut Writer) {
        dest.write_bool(self.is_ready);
        dest.write_nullable(self.option_id.as_ref(), |dest, id| dest.write_uuid(*id));
    }

    pub fn read_v2(src: &mut Reader<'_>) -> Result<Self> {
        let is_ready = src.read_bool()?;
        let option_id = src.read_nullable(|src| src.read_uuid())?;
        Ok(Self { is_ready, option_id })
    }
}

/// `ReadyUpdatePacket`: the ready screen's timer and counts.
///
/// Not modelled: `title` and `subtitle` (text components, both versions) and
/// `voteOptions` (V2; each option has a text title). Encoding writes them as absent or empty.
/// Decoding a present title, subtitle or vote option returns `DecodeError::Unsupported`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyUpdatePacket {
    pub is_running: bool,
    pub is_ready: bool,
    pub state: GameState,
    /// Kotlin `Duration`, carried as the millisecond count it is written as.
    pub remaining_millis: i64,
    pub total_millis: i64,
    pub ready_players: i32,
    pub total_players: i32,
    pub can_send_ready: bool,
    /// V2 only. Always `None` after reading V1.
    pub vote_pending: Option<u128>,
}

impl ReadyUpdatePacket {
    pub const ID_V1: &'static str = "yet-another-minecraft-bingo:ready_timer_update";
    pub const ID_V2: &'static str = "yet-another-minecraft-bingo:ready_timer_update_v2";

    /// `ReadyUpdatePacket.V1`. The identifier goes out upper-cased, and `status` and
    /// `vote_pending` are not written.
    pub fn write_v1(&self, dest: &mut Writer) {
        dest.write_bool(self.is_running);
        dest.write_bool(self.is_ready);
        dest.write_string(&self.state.identifier.to_uppercase());
        dest.write_i64(self.remaining_millis);
        dest.write_i64(self.total_millis);
        dest.write_i32(self.ready_players);
        dest.write_i32(self.total_players);
        write_absent_text(dest); // title
        write_absent_text(dest); // subtitle
        dest.write_bool(self.can_send_ready);
    }

    /// Reads `ReadyUpdatePacket.V1`. The identifier comes back lower-cased with status `PREGAME`.
    pub fn read_v1(src: &mut Reader<'_>) -> Result<Self> {
        let is_running = src.read_bool()?;
        let is_ready = src.read_bool()?;
        let identifier = src.read_string()?.to_lowercase();
        let remaining_millis = src.read_i64()?;
        let total_millis = src.read_i64()?;
        let ready_players = src.read_i32()?;
        let total_players = src.read_i32()?;
        reject_present_text(src, "ReadyUpdatePacket.title")?;
        reject_present_text(src, "ReadyUpdatePacket.subtitle")?;
        let can_send_ready = src.read_bool()?;
        Ok(Self {
            is_running,
            is_ready,
            state: GameState { identifier, status: GameStatus::Pregame },
            remaining_millis,
            total_millis,
            ready_players,
            total_players,
            can_send_ready,
            vote_pending: None,
        })
    }

    /// `ReadyUpdatePacket.V2`.
    pub fn write_v2(&self, dest: &mut Writer) {
        dest.write_bool(self.is_running);
        dest.write_bool(self.is_ready);
        dest.write_string(&self.state.identifier);
        dest.write_enum_ordinal(self.state.status.ordinal());
        dest.write_i64(self.remaining_millis);
        dest.write_i64(self.total_millis);
        dest.write_i32(self.ready_players);
        dest.write_i32(self.total_players);
        write_absent_text(dest); // title
        write_absent_text(dest); // subtitle
        dest.write_bool(self.can_send_ready);
        dest.write_i32(0); // voteOptions: always the empty list
        dest.write_nullable(self.vote_pending.as_ref(), |dest, id| dest.write_uuid(*id));
    }

    pub fn read_v2(src: &mut Reader<'_>) -> Result<Self> {
        let is_running = src.read_bool()?;
        let is_ready = src.read_bool()?;
        let identifier = src.read_string()?;
        let status = GameStatus::from_ordinal(src.read_i32()?);
        let remaining_millis = src.read_i64()?;
        let total_millis = src.read_i64()?;
        let ready_players = src.read_i32()?;
        let total_players = src.read_i32()?;
        reject_present_text(src, "ReadyUpdatePacket.title")?;
        reject_present_text(src, "ReadyUpdatePacket.subtitle")?;
        let can_send_ready = src.read_bool()?;
        reject_vote_options(src)?;
        let vote_pending = src.read_nullable(|src| src.read_uuid())?;
        Ok(Self {
            is_running,
            is_ready,
            state: GameState { identifier, status },
            remaining_millis,
            total_millis,
            ready_players,
            total_players,
            can_send_ready,
            vote_pending,
        })
    }
}

/// Writes the presence flag of a nullable text component as absent.
fn write_absent_text(dest: &mut Writer) {
    dest.write_bool(false);
}

/// Reads the presence flag of a nullable text component. Only "absent" is modelled.
fn reject_present_text(src: &mut Reader<'_>, field: &'static str) -> Result<()> {
    if src.read_bool()? {
        Err(DecodeError::Unsupported(field))
    } else {
        Ok(())
    }
}

/// Reads the `voteOptions` count. Only the empty list is modelled.
fn reject_vote_options(src: &mut Reader<'_>) -> Result<()> {
    match src.read_i32()? {
        0 => Ok(()),
        count if count < 0 => Err(DecodeError::NegativeLength(count)),
        _ => Err(DecodeError::Unsupported("ReadyUpdatePacket.voteOptions")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode};

    const UUID: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;

    #[test]
    fn set_ready_v1_is_a_single_flag_byte() {
        let packet = SetReadyPacket { is_ready: true, option_id: None };
        assert_eq!(encode(|w| packet.write_v1(w)), [0x01]);
        assert_eq!(decode(&[0x01], SetReadyPacket::read_v1), Ok(packet));
    }

    #[test]
    fn set_ready_v2_is_flag_then_nullable_uuid() {
        let without_option = SetReadyPacket { is_ready: true, option_id: None };
        assert_eq!(encode(|w| without_option.write_v2(w)), [0x01, 0x00]);
        assert_eq!(decode(&[0x01, 0x00], SetReadyPacket::read_v2), Ok(without_option));

        let with_option = SetReadyPacket { is_ready: false, option_id: Some(UUID) };
        let bytes = [
            0x00, // is_ready
            0x01, // option id present
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, // most significant bits
            0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10, // least significant bits
        ];
        assert_eq!(encode(|w| with_option.write_v2(w)), bytes);
        assert_eq!(decode(&bytes, SetReadyPacket::read_v2), Ok(with_option));
    }

    /// Hand-computed from `ReadyUpdatePacket.V2` field order. Byte positions: 0 isRunning,
    /// 1 isReady, 2..=9 identifier, 10..=13 status, 14..=21 remaining, 22..=29 total,
    /// 30..=33 ready players, 34..=37 total players, 38 title flag, 39 subtitle flag,
    /// 40 canSendReady, 41..=44 voteOptions count, 45 votePending flag.
    const PLAYING_V2_BYTES: [u8; 46] = [
        0x01, // isRunning
        0x00, // isReady
        0x07, b'p', b'l', b'a', b'y', b'i', b'n', b'g', // identifier "playing"
        0x00, 0x00, 0x00, 0x02, // status PLAYING (ordinal 2)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0xDC, // remaining 1500 ms
        0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x93, 0xE0, // total 300000 ms
        0x00, 0x00, 0x00, 0x02, // ready players 2
        0x00, 0x00, 0x00, 0x04, // total players 4
        0x00, // title absent
        0x00, // subtitle absent
        0x01, // canSendReady
        0x00, 0x00, 0x00, 0x00, // voteOptions: empty
        0x00, // votePending absent
    ];

    fn playing() -> ReadyUpdatePacket {
        ReadyUpdatePacket {
            is_running: true,
            is_ready: false,
            state: GameState { identifier: "playing".to_string(), status: GameStatus::Playing },
            remaining_millis: 1_500,
            total_millis: 300_000,
            ready_players: 2,
            total_players: 4,
            can_send_ready: true,
            vote_pending: None,
        }
    }

    #[test]
    fn ready_update_v2_matches_hand_computed_bytes() {
        let packet = playing();
        assert_eq!(encode(|w| packet.write_v2(w)), PLAYING_V2_BYTES);
        assert_eq!(decode(&PLAYING_V2_BYTES, ReadyUpdatePacket::read_v2), Ok(packet));
    }

    #[test]
    fn ready_update_v2_round_trips_vote_pending() {
        let packet = ReadyUpdatePacket { vote_pending: Some(UUID), ..playing() };
        let bytes = encode(|w| packet.write_v2(w));
        assert_eq!(decode(&bytes, ReadyUpdatePacket::read_v2), Ok(packet));
    }

    #[test]
    fn ready_update_v2_rejects_text_and_vote_options() {
        let mut title = PLAYING_V2_BYTES;
        title[38] = 0x01;
        assert_eq!(
            decode(&title, ReadyUpdatePacket::read_v2),
            Err(DecodeError::Unsupported("ReadyUpdatePacket.title"))
        );

        let mut subtitle = PLAYING_V2_BYTES;
        subtitle[39] = 0x01;
        assert_eq!(
            decode(&subtitle, ReadyUpdatePacket::read_v2),
            Err(DecodeError::Unsupported("ReadyUpdatePacket.subtitle"))
        );

        let mut one_vote_option = PLAYING_V2_BYTES;
        one_vote_option[44] = 0x01; // count = 1
        assert_eq!(
            decode(&one_vote_option, ReadyUpdatePacket::read_v2),
            Err(DecodeError::Unsupported("ReadyUpdatePacket.voteOptions"))
        );
    }

    /// Hand-computed from `ReadyUpdatePacket.V1`: the identifier is written as "PREGAME".
    const PREGAME_V1_BYTES: [u8; 37] = [
        0x00, // isRunning
        0x01, // isReady
        0x07, b'P', b'R', b'E', b'G', b'A', b'M', b'E', // identifier "PREGAME"
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xE8, // remaining 1000 ms
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // total 0 ms
        0x00, 0x00, 0x00, 0x00, // ready players 0
        0x00, 0x00, 0x00, 0x03, // total players 3
        0x00, // title absent
        0x00, // subtitle absent
        0x01, // canSendReady
    ];

    #[test]
    fn ready_update_v1_matches_hand_computed_bytes() {
        let packet = ReadyUpdatePacket {
            is_running: false,
            is_ready: true,
            state: GameState { identifier: "pregame".to_string(), status: GameStatus::Pregame },
            remaining_millis: 1_000,
            total_millis: 0,
            ready_players: 0,
            total_players: 3,
            can_send_ready: true,
            vote_pending: None,
        };
        assert_eq!(encode(|w| packet.write_v1(w)), PREGAME_V1_BYTES);
        assert_eq!(decode(&PREGAME_V1_BYTES, ReadyUpdatePacket::read_v1), Ok(packet));
    }

    #[test]
    fn ready_update_v1_drops_status_and_vote_pending() {
        let packet = ReadyUpdatePacket {
            state: GameState { identifier: "countdown".to_string(), status: GameStatus::Starting },
            vote_pending: Some(UUID),
            ..playing()
        };
        let bytes = encode(|w| packet.write_v1(w));
        assert_eq!(&bytes[3..12], b"COUNTDOWN");

        let decoded = decode(&bytes, ReadyUpdatePacket::read_v1).expect("V1 bytes decode");
        assert_eq!(
            decoded.state,
            GameState { identifier: "countdown".to_string(), status: GameStatus::Pregame }
        );
        assert_eq!(decoded.vote_pending, None);
    }
}

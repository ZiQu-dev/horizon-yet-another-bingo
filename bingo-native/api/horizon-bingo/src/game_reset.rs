//! Game reset packet. Mirrors common/src/main/java/me/jfenn/bingo/common/game/GameResetPacket.kt.

use crate::buf::{Reader, Result, Writer};

/// `GameResetPacket` (V1): no fields. The payload is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameResetPacket;

impl GameResetPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:game_reset";

    /// `toPacketBuf` is empty: nothing is written.
    pub fn write(&self, _dest: &mut Writer) {}

    /// `fromPacketBuf` reads nothing and returns a new packet. Bytes after it are left unread.
    pub fn read(_src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
}

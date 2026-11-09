//! Card reset packet. Mirrors common/src/main/java/me/jfenn/bingo/common/map/CardResetPacket.kt.

use crate::buf::{Reader, Result, Writer};

/// `CardResetPacket` (V1): carries no data. The payload is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardResetPacket;

impl CardResetPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:card_reset";

    /// Writes nothing: the Kotlin `toPacketBuf` is a no-op.
    pub fn write(&self, _dest: &mut Writer) {}

    /// Reads nothing: the Kotlin `fromPacketBuf` ignores the buffer. Trailing bytes are rejected by `decode`.
    pub fn read(_src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
}

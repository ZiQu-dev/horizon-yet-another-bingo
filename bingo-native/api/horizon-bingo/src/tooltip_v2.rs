//! Tooltip packet. Mirrors common/src/main/java/me/jfenn/bingo/common/menu/tooltips/TooltipPacket.kt.
//!
//! Field order on the wire (`V1.toPacketBuf`): `writeList(text, writeText)` (an INT length, then each text),
//! then `writeUUID(entityId)` (most significant 64 bits, then least significant 64 bits).

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::text::{read_text, write_text, Text};

/// The channel id of `TooltipPacket.V1` (`Identifier.of(MOD_ID_BINGO, "tooltip_v2")`).
pub const ID: &str = "yet-another-minecraft-bingo:tooltip_v2";

/// `TooltipPacket`: the lines of a tooltip and the entity it belongs to, as a 128-bit UUID.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipPacket {
    /// `text: List<IText>`: written in order, each as a text component (no nullable wrapper).
    pub text: Vec<Text>,
    /// `entityId: UUID`.
    pub entity_id: u128,
}

impl TooltipPacket {
    pub fn write(&self, dest: &mut Writer) -> Result<()> {
        // `writeList` writes `list.size` as a 4-byte INT.
        dest.write_i32(i32::try_from(self.text.len()).expect("more than i32::MAX tooltip lines"));
        for text in &self.text {
            write_text(dest, text)?;
        }
        dest.write_uuid(self.entity_id);
        Ok(())
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        let count = src.read_i32()?;
        let count = usize::try_from(count).map_err(|_| DecodeError::NegativeLength(count))?;
        let mut text = Vec::new();
        for _ in 0..count {
            text.push(read_text(src)?);
        }
        Ok(Self { text, entity_id: src.read_uuid()? })
    }
}

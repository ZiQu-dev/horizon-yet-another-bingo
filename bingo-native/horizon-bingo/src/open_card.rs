//! Card-open packet. Mirrors common/src/main/java/me/jfenn/bingo/common/map/CardOpenPacket.kt.

use crate::buf::{Reader, Result, Writer};

/// `CardOpenPacket` (V1): the team whose card the player opens, or `None`.
/// The Kotlin `BingoTeamKey?` travels as its id string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardOpenPacket {
    pub team_key: Option<String>,
}

impl CardOpenPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:open_card";

    /// `writeNullable(teamKey?.id, writeString)`: a presence flag, then a VarInt-prefixed string.
    pub fn write(&self, dest: &mut Writer) {
        dest.write_nullable(self.team_key.as_ref(), |dest, key| dest.write_string(key));
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { team_key: src.read_nullable(|src| src.read_string())? })
    }
}

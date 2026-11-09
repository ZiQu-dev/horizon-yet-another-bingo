//! Card-shuffled packet. Mirrors common/src/main/java/me/jfenn/bingo/common/map/CardShuffledPacket.kt.

use crate::buf::{Reader, Result, Writer};

/// `CardShuffledPacket` (V1): the team whose card was shuffled, or `None`.
///
/// Kotlin's `BingoTeamKey` is a value class over its `id` string, so the field is carried as that
/// string. The wire form is the string only; the key's `label` is derived and not sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardShuffledPacket {
    /// The team key's `id` (for example `bingo_red`), or `None` for no team.
    pub team_key: Option<String>,
}

impl CardShuffledPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:card_shuffled";

    /// `writeNullable(teamKey?.id, writeString)`: a presence flag, then the string when present.
    pub fn write(&self, dest: &mut Writer) {
        dest.write_nullable(self.team_key.as_ref(), |dest, key| dest.write_string(key));
    }

    /// `readNullable { BingoTeamKey(readString()) }`.
    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { team_key: src.read_nullable(|src| src.read_string())? })
    }
}

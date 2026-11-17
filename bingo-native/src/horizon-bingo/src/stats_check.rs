//! `StatsCheckPacket` (V1). Mirrors common/src/main/java/me/jfenn/bingo/common/stats/packets/StatsCheckPacket.kt.

use crate::buf::{Reader, Result, Writer};

/// `StatsCheckPacket` (V1): the hash of the stats, as one string (`writeString`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsCheckPacket {
    pub hash_sha512: String,
}

impl StatsCheckPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:stats_check";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_string(&self.hash_sha512);
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { hash_sha512: src.read_string()? })
    }
}

//! Score update packet. Mirrors common/src/main/java/me/jfenn/bingo/common/scoring/ScoredItemPacket.kt
//! and the `TeamScore.V1` converter in common/src/main/java/me/jfenn/bingo/common/team/TeamScore.kt.

use crate::buf::{Reader, Result, Writer};

/// `TeamScore` (V1 converter): three big-endian ints in the order items, lines, cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TeamScore {
    pub items: i32,
    pub lines: i32,
    pub cards: i32,
}

impl TeamScore {
    /// Channel id of the `TeamScore.V1` converter. A packet that embeds a score writes its fields without it.
    pub const ID: &'static str = "yet-another-minecraft-bingo:team_score";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_i32(self.items);
        dest.write_i32(self.lines);
        dest.write_i32(self.cards);
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            items: src.read_i32()?,
            lines: src.read_i32()?,
            cards: src.read_i32()?,
        })
    }
}

/// `ScoredItemPacket` (V1): whether the viewer is on the team, the team key, and its score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredItemPacket {
    pub is_viewer_on_team: bool,
    /// `BingoTeamKey.id` (the Kotlin value class is written as its string, prefix included).
    pub team: String,
    pub score: TeamScore,
}

impl ScoredItemPacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:score_update";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_bool(self.is_viewer_on_team);
        dest.write_string(&self.team);
        self.score.write(dest);
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            is_viewer_on_team: src.read_bool()?,
            team: src.read_string()?,
            score: TeamScore::read(src)?,
        })
    }
}

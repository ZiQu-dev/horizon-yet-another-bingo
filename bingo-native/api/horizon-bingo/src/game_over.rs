//! `GameOverPacket` (V1 `game_over`, V2 `game_over_v2`). Mirrors
//! bingo-src/common/src/main/java/me/jfenn/bingo/common/game/GameOverPacket.kt.
//!
//! The two converters write the same fields in the same order with one exception. Right after
//! `isReadyVoteAvailable`, V1 writes `isResumeAvailable` (a boolean) and V2 writes `resumeDescription`
//! (a nullable text). Each converter reads back only its own field; the other one keeps its default
//! (`false` / `None`). Both fields are kept here so a value can carry what either version would drop.
//!
//! `isOperator` is a constructor parameter the mod never writes or reads, so it is not modelled.

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::game_status::JavaDuration;
use crate::score_update::TeamScore;
use crate::text::{read_text, write_text, Text};

/// Channel id of `GameOverPacket.V1`.
pub const V1_ID: &str = "yet-another-minecraft-bingo:game_over";
/// Channel id of `GameOverPacket.V2`.
pub const V2_ID: &str = "yet-another-minecraft-bingo:game_over_v2";

/// `GameOverPacket.EndScreenTab`. The packet carries the constant's name as a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndScreenTab {
    #[default]
    Cards,
    Scores,
}

impl EndScreenTab {
    /// `defaultTab.name`, as written by both converters.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cards => "CARDS",
            Self::Scores => "SCORES",
        }
    }

    /// `EndScreenTab.valueOf(name)`. Kotlin throws `IllegalArgumentException` for an unknown name, and both
    /// readers catch that and use `CARDS`. Matching is case-sensitive, as `valueOf` is.
    pub fn from_name(name: &str) -> Self {
        match name {
            "SCORES" => Self::Scores,
            _ => Self::Cards,
        }
    }
}

/// `GameOverPacket.ScoreRanking`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreRanking {
    pub index: i32,
    /// `BingoTeamKey.id`, written with `writeString`.
    pub key: String,
    pub name: Text,
    pub score: TeamScore,
    /// `Duration.toString()` (for example `PT1M30S`), or `None`. The wire carries the string, which
    /// the mod parses on read. This crate keeps it as it is.
    pub duration: Option<String>,
}

/// `GameOverPacket`, in the fields both channels carry.
#[derive(Debug, Clone, PartialEq)]
pub struct GameOverPacket {
    pub title: Text,
    pub subtitle: Text,
    /// `winner?.id`. `None` when nobody won.
    pub winner: Option<String>,
    pub duration: JavaDuration,
    pub is_ready_vote_available: bool,
    /// V1 only (`isResumeAvailable`, deprecated in the mod). V2 neither writes nor reads it.
    pub is_resume_available: bool,
    /// V2 only (`resumeDescription`). V1 neither writes nor reads it.
    pub resume_description: Option<Text>,
    pub is_winner: bool,
    /// `false`: the client opens the game over screen. `true`: it updates its data without opening it.
    pub is_update: bool,
    pub win_streak: Option<i64>,
    pub best_win_streak: Option<i64>,
    pub is_best_win_streak: bool,
    pub captured_items: Option<i32>,
    pub best_captured_items: Option<i32>,
    pub is_best_captured_items: bool,
    /// `endedAt`, as `writeInstant` writes it: epoch milliseconds.
    pub ended_at_millis: i64,
    pub is_best_time: bool,
    pub prev_best_time: Option<JavaDuration>,
    pub seed: Option<i64>,
    pub scores: Vec<ScoreRanking>,
    pub default_tab: EndScreenTab,
}

impl Default for GameOverPacket {
    /// Every field at its Kotlin default, except `endedAt`: Kotlin's `Instant.MIN` cannot be written
    /// (`toEpochMilli` overflows), so the value here is the epoch.
    fn default() -> Self {
        Self {
            title: Text::empty(),
            subtitle: Text::empty(),
            winner: None,
            duration: JavaDuration { seconds: 0, nanos: 0 },
            is_ready_vote_available: false,
            is_resume_available: false,
            resume_description: None,
            is_winner: false,
            is_update: false,
            win_streak: None,
            best_win_streak: None,
            is_best_win_streak: false,
            captured_items: None,
            best_captured_items: None,
            is_best_captured_items: false,
            ended_at_millis: 0,
            is_best_time: false,
            prev_best_time: None,
            seed: None,
            scores: Vec::new(),
            default_tab: EndScreenTab::Cards,
        }
    }
}

impl GameOverPacket {
    /// `GameOverPacket.V1.toPacketBuf`.
    pub fn write_v1(&self, dest: &mut Writer) -> Result<()> {
        write_text(dest, &self.title)?;
        write_text(dest, &self.subtitle)?;
        dest.write_nullable(self.winner.as_ref(), |d, id| d.write_string(id));
        write_duration(dest, self.duration);
        dest.write_bool(self.is_ready_vote_available);
        dest.write_bool(self.is_resume_available);
        self.write_outcome(dest)
    }

    /// `GameOverPacket.V2.toPacketBuf`.
    pub fn write_v2(&self, dest: &mut Writer) -> Result<()> {
        write_text(dest, &self.title)?;
        write_text(dest, &self.subtitle)?;
        dest.write_nullable(self.winner.as_ref(), |d, id| d.write_string(id));
        write_duration(dest, self.duration);
        dest.write_bool(self.is_ready_vote_available);
        write_nullable_text(dest, self.resume_description.as_ref())?;
        self.write_outcome(dest)
    }

    /// `GameOverPacket.V1.fromPacketBuf`. `resume_description` is left `None`.
    pub fn read_v1(src: &mut Reader<'_>) -> Result<Self> {
        let mut packet = Self::default();
        packet.title = read_text(src)?;
        packet.subtitle = read_text(src)?;
        packet.winner = src.read_nullable(|s| s.read_string())?;
        packet.duration = read_duration(src)?;
        packet.is_ready_vote_available = src.read_bool()?;
        packet.is_resume_available = src.read_bool()?;
        packet.read_outcome(src)?;
        Ok(packet)
    }

    /// `GameOverPacket.V2.fromPacketBuf`. `is_resume_available` is left `false`.
    pub fn read_v2(src: &mut Reader<'_>) -> Result<Self> {
        let mut packet = Self::default();
        packet.title = read_text(src)?;
        packet.subtitle = read_text(src)?;
        packet.winner = src.read_nullable(|s| s.read_string())?;
        packet.duration = read_duration(src)?;
        packet.is_ready_vote_available = src.read_bool()?;
        packet.resume_description = src.read_nullable(|s| read_text(s))?;
        packet.read_outcome(src)?;
        Ok(packet)
    }

    /// The fields both converters write after the resume field, in Kotlin's order.
    fn write_outcome(&self, dest: &mut Writer) -> Result<()> {
        dest.write_bool(self.is_winner);
        dest.write_bool(self.is_update);

        dest.write_nullable(self.win_streak.as_ref(), |d, v| d.write_i64(*v));
        dest.write_nullable(self.best_win_streak.as_ref(), |d, v| d.write_i64(*v));
        dest.write_bool(self.is_best_win_streak);

        dest.write_nullable(self.captured_items.as_ref(), |d, v| d.write_i32(*v));
        dest.write_nullable(self.best_captured_items.as_ref(), |d, v| d.write_i32(*v));
        dest.write_bool(self.is_best_captured_items);

        dest.write_i64(self.ended_at_millis);
        dest.write_bool(self.is_best_time);
        dest.write_nullable(self.prev_best_time.as_ref(), |d, v| write_duration(d, *v));

        dest.write_nullable(self.seed.as_ref(), |d, v| d.write_i64(*v));

        write_scores(dest, &self.scores)?;
        dest.write_string(self.default_tab.name());
        Ok(())
    }

    /// The fields both converters read after the resume field, in Kotlin's order.
    fn read_outcome(&mut self, src: &mut Reader<'_>) -> Result<()> {
        self.is_winner = src.read_bool()?;
        self.is_update = src.read_bool()?;

        self.win_streak = src.read_nullable(|s| s.read_i64())?;
        self.best_win_streak = src.read_nullable(|s| s.read_i64())?;
        self.is_best_win_streak = src.read_bool()?;

        self.captured_items = src.read_nullable(|s| s.read_i32())?;
        self.best_captured_items = src.read_nullable(|s| s.read_i32())?;
        self.is_best_captured_items = src.read_bool()?;

        self.ended_at_millis = src.read_i64()?;
        self.is_best_time = src.read_bool()?;
        self.prev_best_time = src.read_nullable(read_duration)?;

        self.seed = src.read_nullable(|s| s.read_i64())?;

        self.scores = read_scores(src)?;
        self.default_tab = EndScreenTab::from_name(&src.read_string()?);
        Ok(())
    }
}

/// `writeDuration`: `toMillis()` as a long. Kotlin writes 0 when `toMillis` overflows.
fn write_duration(dest: &mut Writer, value: JavaDuration) {
    dest.write_i64(value.to_millis_or_zero());
}

fn read_duration(src: &mut Reader<'_>) -> Result<JavaDuration> {
    Ok(JavaDuration::of_millis(src.read_i64()?))
}

/// `writeNullable(text, writeText)`: a presence flag, then the text when present.
fn write_nullable_text(dest: &mut Writer, value: Option<&Text>) -> Result<()> {
    dest.write_bool(value.is_some());
    match value {
        Some(text) => write_text(dest, text),
        None => Ok(()),
    }
}

/// `writeList`: an int count, then each entry.
fn write_scores(dest: &mut Writer, scores: &[ScoreRanking]) -> Result<()> {
    let count = i32::try_from(scores.len()).expect("more than i32::MAX scores");
    dest.write_i32(count);
    for entry in scores {
        dest.write_i32(entry.index);
        dest.write_string(&entry.key);
        write_text(dest, &entry.name)?;
        entry.score.write(dest);
        dest.write_nullable(entry.duration.as_ref(), |d, text| d.write_string(text));
    }
    Ok(())
}

/// `readList`: an int count, then that many entries. A negative count is an error (Kotlin's `List(n)` throws).
fn read_scores(src: &mut Reader<'_>) -> Result<Vec<ScoreRanking>> {
    let count = src.read_i32()?;
    let count = usize::try_from(count).map_err(|_| DecodeError::NegativeLength(count))?;
    (0..count).map(|_| read_ranking(src)).collect()
}

fn read_ranking(src: &mut Reader<'_>) -> Result<ScoreRanking> {
    Ok(ScoreRanking {
        index: src.read_i32()?,
        key: src.read_string()?,
        name: read_text(src)?,
        score: TeamScore::read(src)?,
        duration: src.read_nullable(|s| s.read_string())?,
    })
}

//! Ready-timer update packet, both converters, and the `VoteOption` converter it embeds.
//!
//! Mirrors common/src/main/java/me/jfenn/bingo/common/ready/ReadyUpdatePacket.kt and VoteOption.kt.
//! Field order follows the Kotlin `toPacketBuf` of each version.
//!
//! Wire conventions used (IPacketBuf.kt):
//! - `writeNullable(x, dest::writeText)`: a presence boolean, then the text when present. An absent
//!   title or subtitle is therefore a single `false` byte, NOT an empty component.
//! - `writeList`: an INT count, then each element with the element converter (no channel id).
//! - `writeNullable(votePending, writeUUID)`: a presence boolean, then the two longs.
//! - `writeDuration`: milliseconds as a long. Kotlin's `Duration` is carried as that millisecond count.
//! - `writeEnum`: the ordinal as an INT. `readEnum` falls back to the first value (see `GameStatus::from_ordinal`).

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::game::{GameState, GameStatus};
use crate::text::{read_text, write_text, Text};

/// `VoteOption` (V1 converter): an option's id, its text title, its vote count and whether it is selected.
/// The Kotlin `clientSelected` var is local client state and is not on the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct VoteOption {
    /// `java.util.UUID`, carried as its 128 bits (most significant half first on the wire).
    pub id: u128,
    pub title: Text,
    pub votes: i32,
    pub is_selected: bool,
}

impl VoteOption {
    /// Channel id of the `VoteOption.V1` converter. It is embedded in a list, so no packet writes it.
    pub const ID: &'static str = "yet-another-minecraft-bingo:vote_option";

    /// `VoteOption.V1.toPacketBuf`: UUID, text, votes, selected flag.
    pub fn write(&self, dest: &mut Writer) -> Result<()> {
        dest.write_uuid(self.id);
        write_text(dest, &self.title)?;
        dest.write_i32(self.votes);
        dest.write_bool(self.is_selected);
        Ok(())
    }

    /// `VoteOption.V1.fromPacketBuf`.
    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            id: src.read_uuid()?,
            title: read_text(src)?,
            votes: src.read_i32()?,
            is_selected: src.read_bool()?,
        })
    }
}

/// `ReadyUpdatePacket`: the ready screen's timer, player counts, optional title and subtitle, and (V2 only)
/// the vote options and the pending vote.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadyUpdatePacket {
    pub is_running: bool,
    pub is_ready: bool,
    pub state: GameState,
    /// Kotlin `Duration` values, as the millisecond counts they are written as.
    pub remaining_millis: i64,
    pub total_millis: i64,
    pub ready_players: i32,
    pub total_players: i32,
    pub title: Option<Text>,
    pub subtitle: Option<Text>,
    pub can_send_ready: bool,
    /// V2 only. Always empty after reading V1, which has no vote options on the wire.
    pub vote_options: Vec<VoteOption>,
    /// V2 only. Always `None` after reading V1.
    pub vote_pending: Option<u128>,
}

impl ReadyUpdatePacket {
    /// `ReadyUpdatePacket.V1.id`.
    pub const ID_V1: &'static str = "yet-another-minecraft-bingo:ready_timer_update";
    /// `ReadyUpdatePacket.V2.id`.
    pub const ID_V2: &'static str = "yet-another-minecraft-bingo:ready_timer_update_v2";

    /// `ReadyUpdatePacket.V1.toPacketBuf`. The identifier goes out upper-cased. The status, the vote
    /// options and the pending vote are not written.
    pub fn write_v1(&self, dest: &mut Writer) -> Result<()> {
        dest.write_bool(self.is_running);
        dest.write_bool(self.is_ready);
        dest.write_string(&self.state.identifier.to_uppercase());
        dest.write_i64(self.remaining_millis);
        dest.write_i64(self.total_millis);
        dest.write_i32(self.ready_players);
        dest.write_i32(self.total_players);
        write_nullable_text(dest, self.title.as_ref())?;
        write_nullable_text(dest, self.subtitle.as_ref())?;
        dest.write_bool(self.can_send_ready);
        Ok(())
    }

    /// `ReadyUpdatePacket.V1.fromPacketBuf`. The identifier comes back lower-cased, the status is `PREGAME`,
    /// and the vote fields are empty.
    pub fn read_v1(src: &mut Reader<'_>) -> Result<Self> {
        let is_running = src.read_bool()?;
        let is_ready = src.read_bool()?;
        let identifier = src.read_string()?.to_lowercase();
        let remaining_millis = src.read_i64()?;
        let total_millis = src.read_i64()?;
        let ready_players = src.read_i32()?;
        let total_players = src.read_i32()?;
        let title = read_nullable_text(src)?;
        let subtitle = read_nullable_text(src)?;
        let can_send_ready = src.read_bool()?;
        Ok(Self {
            is_running,
            is_ready,
            state: GameState { identifier, status: GameStatus::Pregame },
            remaining_millis,
            total_millis,
            ready_players,
            total_players,
            title,
            subtitle,
            can_send_ready,
            vote_options: Vec::new(),
            vote_pending: None,
        })
    }

    /// `ReadyUpdatePacket.V2.toPacketBuf`: the identifier as is, then the status as an INT ordinal.
    pub fn write_v2(&self, dest: &mut Writer) -> Result<()> {
        dest.write_bool(self.is_running);
        dest.write_bool(self.is_ready);
        dest.write_string(&self.state.identifier);
        dest.write_enum_ordinal(self.state.status.ordinal());
        dest.write_i64(self.remaining_millis);
        dest.write_i64(self.total_millis);
        dest.write_i32(self.ready_players);
        dest.write_i32(self.total_players);
        write_nullable_text(dest, self.title.as_ref())?;
        write_nullable_text(dest, self.subtitle.as_ref())?;
        dest.write_bool(self.can_send_ready);
        write_list(dest, &self.vote_options, VoteOption::write)?;
        dest.write_nullable(self.vote_pending.as_ref(), |dest, id| dest.write_uuid(*id));
        Ok(())
    }

    /// `ReadyUpdatePacket.V2.fromPacketBuf`.
    pub fn read_v2(src: &mut Reader<'_>) -> Result<Self> {
        let is_running = src.read_bool()?;
        let is_ready = src.read_bool()?;
        let identifier = src.read_string()?;
        let status = GameStatus::from_ordinal(src.read_i32()?);
        let remaining_millis = src.read_i64()?;
        let total_millis = src.read_i64()?;
        let ready_players = src.read_i32()?;
        let total_players = src.read_i32()?;
        let title = read_nullable_text(src)?;
        let subtitle = read_nullable_text(src)?;
        let can_send_ready = src.read_bool()?;
        let vote_options = read_list(src, VoteOption::read)?;
        let vote_pending = src.read_nullable(|src| src.read_uuid())?;
        Ok(Self {
            is_running,
            is_ready,
            state: GameState { identifier, status },
            remaining_millis,
            total_millis,
            ready_players,
            total_players,
            title,
            subtitle,
            can_send_ready,
            vote_options,
            vote_pending,
        })
    }
}

/// `writeNullable(text, writeText)`: a presence boolean, then the text when present.
fn write_nullable_text(dest: &mut Writer, text: Option<&Text>) -> Result<()> {
    dest.write_bool(text.is_some());
    if let Some(text) = text {
        write_text(dest, text)?;
    }
    Ok(())
}

/// `readNullable(readText)`.
fn read_nullable_text(src: &mut Reader<'_>) -> Result<Option<Text>> {
    src.read_nullable(|src| read_text(src))
}

/// `writeList`: an INT count, then each element.
fn write_list<T>(
    dest: &mut Writer,
    items: &[T],
    write: impl Fn(&T, &mut Writer) -> Result<()>,
) -> Result<()> {
    let count = i32::try_from(items.len()).expect("list longer than i32::MAX");
    dest.write_i32(count);
    for item in items {
        write(item, dest)?;
    }
    Ok(())
}

/// `readList`: an INT count, then that many elements. A negative count is an error (Kotlin's `List(n)` throws).
fn read_list<T>(
    src: &mut Reader<'_>,
    read: impl Fn(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let count = src.read_i32()?;
    let count = usize::try_from(count).map_err(|_| DecodeError::NegativeLength(count))?;
    let mut items = Vec::new();
    for _ in 0..count {
        items.push(read(src)?);
    }
    Ok(items)
}

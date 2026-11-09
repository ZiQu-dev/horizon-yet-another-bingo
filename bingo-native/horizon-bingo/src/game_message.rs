//! `GameMessagePacket.V1` (channel `yet-another-minecraft-bingo:game_message`). Mirrors
//! bingo-src/common/src/main/java/me/jfenn/bingo/common/scoring/GameMessagePacket.kt.
//!
//! Field order on the wire: UUID id, Duration as milliseconds (a long), team key (string), the single `image`
//! (`CardTileImage.V1`, no channel id), the `imageList` (INT count, then each `CardTileImage.V1`), the message type
//! (`writeEnum`: INT ordinal), the message text (`writeText`), then the `isUpdate` boolean.

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::card_tile_image::{read_enum, CardTileImage};
use crate::game_status::JavaDuration;
use crate::text::{read_text, write_text, Text};

/// Channel id of `GameMessagePacket.V1`.
pub const ID: &str = "yet-another-minecraft-bingo:game_message";

/// `GameMessagePacket.MessageType`, written as its ordinal with `writeEnum`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageType {
    #[default]
    LeadingTeam = 0,
    LineScored = 1,
    ItemScored = 2,
    CardCompleted = 3,
}

impl MessageType {
    pub const VALUES: [Self; 4] = [Self::LeadingTeam, Self::LineScored, Self::ItemScored, Self::CardCompleted];
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameMessagePacket {
    /// `UuidAsString`: most significant 64 bits first.
    pub id: u128,
    /// `timeElapsed`, as `writeDuration` writes it (`toMillis()`, or 0 when that overflows).
    pub time_elapsed: JavaDuration,
    /// `BingoTeamKey.id`.
    pub team: String,
    pub image: CardTileImage,
    pub image_list: Vec<CardTileImage>,
    pub message_type: MessageType,
    pub message: Text,
    pub is_update: bool,
}

impl GameMessagePacket {
    pub fn write(&self, dest: &mut Writer) -> Result<()> {
        dest.write_uuid(self.id);
        dest.write_i64(self.time_elapsed.to_millis_or_zero());
        dest.write_string(&self.team);

        self.image.write(dest)?;
        let count = i32::try_from(self.image_list.len()).expect("more than i32::MAX images");
        dest.write_i32(count);
        for image in &self.image_list {
            image.write(dest)?;
        }

        dest.write_i32(self.message_type as i32);
        write_text(dest, &self.message)?;
        dest.write_bool(self.is_update);
        Ok(())
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        let id = src.read_uuid()?;
        let time_elapsed = JavaDuration::of_millis(src.read_i64()?);
        let team = src.read_string()?;

        let image = CardTileImage::read(src)?;
        // `readList`: `List(readInt())`, so a negative count is an error.
        let count = src.read_i32()?;
        let count = usize::try_from(count).map_err(|_| DecodeError::NegativeLength(count))?;
        let image_list = (0..count).map(|_| CardTileImage::read(src)).collect::<Result<Vec<_>>>()?;

        let message_type = read_enum(src, &MessageType::VALUES)?;
        let message = read_text(src)?;
        let is_update = src.read_bool()?;

        Ok(Self { id, time_elapsed, team, image, image_list, message_type, message, is_update })
    }
}

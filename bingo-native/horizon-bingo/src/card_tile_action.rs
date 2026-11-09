//! `CardTileAction.V1` (channel `card_tile_action`). Mirrors
//! common/src/main/java/me/jfenn/bingo/common/map/CardTileAction.kt: a `Type` ordinal, then the payload.

use crate::card_tile_image::{read_enum, read_stack};
use crate::buf::{Reader, Result, Writer};
use crate::item::{write_item_stack, ItemStack};

/// Channel id of the `CardTileAction.V1` converter.
pub const ID: &str = "yet-another-minecraft-bingo:card_tile_action";

#[derive(Debug, Clone, PartialEq)]
pub enum CardTileAction {
    Advancement(String),
    Item(ItemStack),
    None,
}

/// `CardTileAction.Type`, written as its ordinal.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionType {
    None = 0,
    Advancement = 1,
    Item = 2,
}

impl ActionType {
    const VALUES: [Self; 3] = [Self::None, Self::Advancement, Self::Item];
}

impl CardTileAction {
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(match read_enum(r, &ActionType::VALUES)? {
            ActionType::Advancement => Self::Advancement(r.read_string()?),
            ActionType::Item => Self::Item(read_stack(r)?),
            ActionType::None => Self::None,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        match self {
            Self::Advancement(id) => {
                w.write_i32(ActionType::Advancement as i32);
                w.write_string(id);
            }
            Self::Item(stack) => {
                w.write_i32(ActionType::Item as i32);
                write_item_stack(w, Some(stack))?;
            }
            Self::None => w.write_i32(ActionType::None as i32),
        }
        Ok(())
    }
}

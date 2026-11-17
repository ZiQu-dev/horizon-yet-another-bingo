//! `CardTileImage.V1` (channel `card_tile_image`): one picture of a tile. Mirrors
//! common/src/main/java/me/jfenn/bingo/common/map/CardTileImage.kt. Kept in its own module because `game_message`
//! reuses it. `mapTexture` is not on the wire (the V1 converter never writes it), so it is not modelled.

use crate::buf::{DecodeError, Reader, Result, Writer};
use crate::item::{read_item_stack, write_item_stack, ItemStack};

/// Channel id of the `CardTileImage.V1` converter. Packets that embed it write its fields without the id.
pub const ID: &str = "yet-another-minecraft-bingo:card_tile_image";

/// `ItemStyle` (Kotlin enum, written as its ordinal with `writeEnum`).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemStyle {
    #[default]
    Normal = 0,
    OneOf = 1,
    AllOf = 2,
}

impl ItemStyle {
    pub const VALUES: [Self; 3] = [Self::Normal, Self::OneOf, Self::AllOf];
}

/// `Decoration` (Kotlin enum, written as its ordinal with `writeEnum`).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Decoration {
    #[default]
    None = 0,
    Advancement = 1,
    MultiItem = 2,
    Forbidden = 3,
    FreeSpace = 4,
}

impl Decoration {
    pub const VALUES: [Self; 5] = [Self::None, Self::Advancement, Self::MultiItem, Self::Forbidden, Self::FreeSpace];
}

/// `readEnum<T>()`: reads the INT ordinal; an ordinal out of range is the FIRST value (Kotlin `getOrElse`).
pub fn read_enum<T: Copy>(r: &mut Reader<'_>, values: &[T]) -> Result<T> {
    let ordinal = r.read_i32()?;
    let found = usize::try_from(ordinal).ok().and_then(|i| values.get(i)).copied();
    Ok(found.unwrap_or(values[0]))
}

/// `readItemStack()` for a non-null stack: the mod writes a present stack with count >= 1 (an empty stack is `None`
/// in `read_item_stack`). A present but empty stack is not modelled, so it is an error rather than a silent change.
pub fn read_stack(r: &mut Reader<'_>) -> Result<ItemStack> {
    read_item_stack(r)?.ok_or(DecodeError::Unsupported("a present item stack with count 0"))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CardTileImage {
    pub item: Option<ItemStack>,
    pub texture: Option<String>,
    pub item_style: ItemStyle,
    pub decoration: Decoration,
    pub decoration_texture: Option<String>,
    pub is_achieved: bool,
}

impl CardTileImage {
    /// `readNullable(readItemStack)`, `readNullable(readString)`, `readEnum`, `readEnum`, `readNullable(readString)`, `readBoolean`.
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            item: r.read_nullable(read_stack)?,
            texture: r.read_nullable(|r| r.read_string())?,
            item_style: read_enum(r, &ItemStyle::VALUES)?,
            decoration: read_enum(r, &Decoration::VALUES)?,
            decoration_texture: r.read_nullable(|r| r.read_string())?,
            is_achieved: r.read_bool()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.write_bool(self.item.is_some());
        if let Some(stack) = &self.item {
            write_item_stack(w, Some(stack))?;
        }
        w.write_nullable(self.texture.as_ref(), |w, s| w.write_string(s));
        w.write_i32(self.item_style as i32);
        w.write_i32(self.decoration as i32);
        w.write_nullable(self.decoration_texture.as_ref(), |w, s| w.write_string(s));
        w.write_bool(self.is_achieved);
        Ok(())
    }
}

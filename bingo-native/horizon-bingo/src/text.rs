//! A text component as the mod sends it (`IPacketBuf.writeText`): vanilla's `ComponentSerialization.STREAM_CODEC`,
//! which is the component's JSON-equivalent tree as network NBT. A plain literal is a bare String tag; anything with
//! a style, siblings (`extra`), a translation or an event is a Compound. This layer keeps the tree as lossless
//! [`Nbt`]: turning it into Horizon's own text type is the host's job and belongs to the mod, not to the wire.
//!
//! `writeText(null)` in the mod writes an EMPTY COMPONENT (a String tag with no characters), not "absent": use
//! [`write_text_or_empty`] for a nullable field of the Kotlin code.

use crate::buf::{Reader, Result, Writer};
use crate::nbt::{read_nbt, write_nbt, Nbt};

#[derive(Debug, Clone, PartialEq)]
pub struct Text(pub Nbt);

impl Text {
    pub fn empty() -> Self {
        Text::plain("")
    }

    pub fn plain(value: &str) -> Self {
        Text(Nbt::string(value))
    }

    /// The characters when this is an unstyled literal.
    pub fn as_plain(&self) -> Option<&str> {
        match &self.0 {
            Nbt::String(s) => Some(s),
            _ => None,
        }
    }
}

pub fn write_text(w: &mut Writer, text: &Text) -> Result<()> {
    write_nbt(w, &text.0)
}

/// `writeText(text: IText?)`: a missing text becomes the empty component, as in the mod.
pub fn write_text_or_empty(w: &mut Writer, text: Option<&Text>) -> Result<()> {
    match text {
        Some(t) => write_text(w, t),
        None => write_text(w, &Text::empty()),
    }
}

pub fn read_text(r: &mut Reader<'_>) -> Result<Text> {
    read_nbt(r).map(Text)
}

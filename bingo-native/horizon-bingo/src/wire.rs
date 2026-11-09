//! Kotlin's `IPacketBuf.readList` / `writeList` for the nested converters: an INT length (not a VarInt), then the items.

use crate::buf::{DecodeError, Reader, Result, Writer};

/// The INT count a list starts with. A negative count is an error, as `List(readInt())` throws on one.
pub fn read_count(r: &mut Reader<'_>) -> Result<usize> {
    let count = r.read_i32()?;
    usize::try_from(count).map_err(|_| DecodeError::NegativeLength(count))
}

pub fn read_list<T>(
    r: &mut Reader<'_>,
    mut item: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let count = read_count(r)?;
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(item(r)?);
    }
    Ok(out)
}

/// `writeList`: the INT length, then each item.
pub fn write_list<T>(
    w: &mut Writer,
    items: &[T],
    mut item: impl FnMut(&mut Writer, &T) -> Result<()>,
) -> Result<()> {
    w.write_i32(i32::try_from(items.len()).expect("list longer than i32::MAX"));
    for entry in items {
        item(w, entry)?;
    }
    Ok(())
}

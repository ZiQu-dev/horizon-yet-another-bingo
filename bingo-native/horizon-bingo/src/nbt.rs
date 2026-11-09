//! Network NBT as Minecraft 1.20.2+ sends a tag over the wire (`FriendlyByteBuf.writeNbt`): the tag type byte,
//! then the payload, with NO name on the root. Text components, custom item data and item component values travel
//! this way. The model is lossless: a compound keeps its entries in wire order and a list keeps its element type,
//! so decode then encode gives the same bytes. Strings are Java's *modified* UTF-8 (a u16 byte length; NUL as
//! `c0 80`; characters above U+FFFF as two 3-byte surrogates). Verified against the real mod's text and item
//! encoders (tests/oracle_w2.rs).

use crate::buf::{DecodeError, Reader, Result, Writer};

/// Vanilla's `NbtAccounter` depth limit.
const MAX_DEPTH: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    /// `elem` is the element tag id as written (0 for an empty list that was written as End).
    List { elem: u8, items: Vec<Nbt> },
    Compound(Vec<(String, Nbt)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Nbt {
    pub fn string(value: &str) -> Self {
        Nbt::String(value.to_owned())
    }

    pub fn compound(entries: Vec<(&str, Nbt)>) -> Self {
        Nbt::Compound(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// A list whose element tag id is taken from its first element (End when empty).
    pub fn list(items: Vec<Nbt>) -> Self {
        let elem = items.first().map_or(0, Nbt::tag_id);
        Nbt::List { elem, items }
    }

    pub fn tag_id(&self) -> u8 {
        match self {
            Nbt::Byte(_) => 1,
            Nbt::Short(_) => 2,
            Nbt::Int(_) => 3,
            Nbt::Long(_) => 4,
            Nbt::Float(_) => 5,
            Nbt::Double(_) => 6,
            Nbt::ByteArray(_) => 7,
            Nbt::String(_) => 8,
            Nbt::List { .. } => 9,
            Nbt::Compound(_) => 10,
            Nbt::IntArray(_) => 11,
            Nbt::LongArray(_) => 12,
        }
    }

    /// The entry of a compound with this name.
    pub fn get(&self, key: &str) -> Option<&Nbt> {
        match self {
            Nbt::Compound(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Java's `DataOutputStream.writeUTF` body (without the length): modified UTF-8.
pub fn modified_utf8(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for unit in value.encode_utf16() {
        match unit {
            0x0001..=0x007f => out.push(unit as u8),
            0x0000 | 0x0080..=0x07ff => {
                out.push(0xc0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3f) as u8);
            }
            _ => {
                out.push(0xe0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3f) as u8);
                out.push(0x80 | (unit & 0x3f) as u8);
            }
        }
    }
    out
}

/// The inverse of [`modified_utf8`]. A lone surrogate cannot be a Rust `String` and is an error.
pub fn from_modified_utf8(bytes: &[u8]) -> Result<String> {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let cont = |j: usize| -> Result<u16> {
            match bytes.get(j) {
                Some(c) if c & 0xc0 == 0x80 => Ok(u16::from(c & 0x3f)),
                _ => Err(DecodeError::InvalidModifiedUtf8),
            }
        };
        match b {
            0x00..=0x7f => {
                units.push(u16::from(b));
                i += 1;
            }
            0xc0..=0xdf => {
                units.push((u16::from(b & 0x1f) << 6) | cont(i + 1)?);
                i += 2;
            }
            0xe0..=0xef => {
                units.push((u16::from(b & 0x0f) << 12) | (cont(i + 1)? << 6) | cont(i + 2)?);
                i += 3;
            }
            _ => return Err(DecodeError::InvalidModifiedUtf8),
        }
    }
    String::from_utf16(&units).map_err(|_| DecodeError::InvalidModifiedUtf8)
}

fn write_name(w: &mut Writer, value: &str) -> Result<()> {
    let bytes = modified_utf8(value);
    let len = u16::try_from(bytes.len()).map_err(|_| DecodeError::InvalidModifiedUtf8)?;
    w.write_u16(len);
    w.write_raw(&bytes);
    Ok(())
}

fn read_name(r: &mut Reader<'_>) -> Result<String> {
    let len = usize::from(r.read_u16()?);
    from_modified_utf8(r.read_raw(len)?)
}

fn write_payload(w: &mut Writer, nbt: &Nbt) -> Result<()> {
    match nbt {
        Nbt::Byte(v) => w.write_u8(*v as u8),
        Nbt::Short(v) => w.write_i16(*v),
        Nbt::Int(v) => w.write_i32(*v),
        Nbt::Long(v) => w.write_i64(*v),
        Nbt::Float(v) => w.write_f32(*v),
        Nbt::Double(v) => w.write_f64(*v),
        Nbt::ByteArray(v) => {
            w.write_i32(i32::try_from(v.len()).map_err(|_| DecodeError::NbtTooDeep)?);
            w.write_raw(v);
        }
        Nbt::String(v) => write_name(w, v)?,
        Nbt::List { elem, items } => {
            w.write_u8(if items.is_empty() { *elem } else { items[0].tag_id() });
            w.write_i32(items.len() as i32);
            for item in items {
                write_payload(w, item)?;
            }
        }
        Nbt::Compound(entries) => {
            for (key, value) in entries {
                w.write_u8(value.tag_id());
                write_name(w, key)?;
                write_payload(w, value)?;
            }
            w.write_u8(0);
        }
        Nbt::IntArray(v) => {
            w.write_i32(v.len() as i32);
            for x in v {
                w.write_i32(*x);
            }
        }
        Nbt::LongArray(v) => {
            w.write_i32(v.len() as i32);
            for x in v {
                w.write_i64(*x);
            }
        }
    }
    Ok(())
}

fn read_payload(r: &mut Reader<'_>, tag: u8, depth: usize) -> Result<Nbt> {
    if depth > MAX_DEPTH {
        return Err(DecodeError::NbtTooDeep);
    }
    // A negative or absurd array length would allocate before failing: every element takes at least one byte.
    let length = |r: &mut Reader<'_>, element_bytes: usize| -> Result<usize> {
        let len = r.read_i32()?;
        let len = usize::try_from(len).unwrap_or(0);
        if len.saturating_mul(element_bytes) > r.remaining() {
            return Err(DecodeError::UnexpectedEnd { needed: len * element_bytes, remaining: r.remaining() });
        }
        Ok(len)
    };
    Ok(match tag {
        1 => Nbt::Byte(r.read_u8()? as i8),
        2 => Nbt::Short(r.read_i16()?),
        3 => Nbt::Int(r.read_i32()?),
        4 => Nbt::Long(r.read_i64()?),
        5 => Nbt::Float(r.read_f32()?),
        6 => Nbt::Double(r.read_f64()?),
        7 => {
            let len = length(r, 1)?;
            Nbt::ByteArray(r.read_raw(len)?.to_vec())
        }
        8 => Nbt::String(read_name(r)?),
        9 => {
            let elem = r.read_u8()?;
            let len = length(r, 1)?;
            if elem == 0 && len > 0 {
                return Err(DecodeError::InvalidNbtTag(0));
            }
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(read_payload(r, elem, depth + 1)?);
            }
            Nbt::List { elem, items }
        }
        10 => {
            let mut entries = Vec::new();
            loop {
                let t = r.read_u8()?;
                if t == 0 {
                    break;
                }
                let key = read_name(r)?;
                entries.push((key, read_payload(r, t, depth + 1)?));
            }
            Nbt::Compound(entries)
        }
        11 => {
            let len = length(r, 4)?;
            Nbt::IntArray((0..len).map(|_| r.read_i32()).collect::<Result<_>>()?)
        }
        12 => {
            let len = length(r, 8)?;
            Nbt::LongArray((0..len).map(|_| r.read_i64()).collect::<Result<_>>()?)
        }
        other => return Err(DecodeError::InvalidNbtTag(other)),
    })
}

/// `FriendlyByteBuf.writeNbt` for a present tag: type byte, then the payload, no root name.
pub fn write_nbt(w: &mut Writer, nbt: &Nbt) -> Result<()> {
    w.write_u8(nbt.tag_id());
    write_payload(w, nbt)
}

/// Reads a present root tag; an End tag (nothing there) is an error here.
pub fn read_nbt(r: &mut Reader<'_>) -> Result<Nbt> {
    let tag = r.read_u8()?;
    if tag == 0 {
        return Err(DecodeError::InvalidNbtTag(0));
    }
    read_payload(r, tag, 0)
}

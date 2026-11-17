//! Big-endian packet buffer. Mirrors the helpers of `IPacketBuf`
//! (platform/src/main/java/me/jfenn/bingo/platform/IPacketBuf.kt) with Minecraft's
//! `PacketByteBuf` wire conventions. The concrete `IPacketBuf` implementations are not in the
//! source set, so the conventions are checked by the oracle rows in `tests/` against the mod's real bytes.

use std::fmt;

/// Why a payload could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// An NBT tag id that does not exist (or an End tag where a value was needed).
    InvalidNbtTag(u8),
    /// NBT nested deeper than vanilla's limit of 512.
    NbtTooDeep,
    /// An NBT string was not valid modified UTF-8, or does not fit a u16 length.
    InvalidModifiedUtf8,
    /// A data component type this crate cannot read (its value codec is unknown, so the stack cannot be skipped).
    UnsupportedComponent(i32),
    /// A JSON payload (PlayerSettings) did not parse.
    InvalidJson(&'static str),
    /// A JSON payload (stats packets) that this crate cannot read.
    Json(String),
    /// A UUID string was not in the canonical 8-4-4-4-12 hex form.
    InvalidUuid(String),
    /// The buffer ended before `needed` bytes could be read; `remaining` were left.
    UnexpectedEnd { needed: usize, remaining: usize },
    /// A VarInt continued past its fifth byte.
    VarIntTooLong,
    /// A length prefix was negative.
    NegativeLength(i32),
    /// A string's bytes were not valid UTF-8.
    InvalidUtf8,
    /// The payload had this many bytes left over after the packet was read.
    TrailingBytes(usize),
    /// The payload holds a field this crate does not model yet. Carries the Kotlin field name.
    Unsupported(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNbtTag(tag) => write!(f, "invalid NBT tag {tag}"),
            Self::NbtTooDeep => write!(f, "NBT nested deeper than 512"),
            Self::InvalidModifiedUtf8 => write!(f, "invalid modified UTF-8 in NBT"),
            Self::UnsupportedComponent(id) => write!(f, "data component type {id} is not supported"),
            Self::InvalidJson(why) => write!(f, "invalid JSON: {why}"),
            Self::Json(what) => write!(f, "bad JSON payload: {what}"),
            Self::InvalidUuid(text) => write!(f, "{text:?} is not a canonical UUID"),
            Self::UnexpectedEnd { needed, remaining } => {
                write!(f, "needed {needed} bytes, {remaining} remaining")
            }
            Self::VarIntTooLong => write!(f, "VarInt longer than 5 bytes"),
            Self::NegativeLength(len) => write!(f, "negative length {len}"),
            Self::InvalidUtf8 => write!(f, "string is not valid UTF-8"),
            Self::TrailingBytes(left) => write!(f, "{left} trailing bytes"),
            Self::Unsupported(field) => write!(f, "{field} is not supported by this client yet"),
        }
    }
}

pub type Result<T> = std::result::Result<T, DecodeError>;

/// Writes packet fields in order.
#[derive(Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn write_i16(&mut self, value: i16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_f64(&mut self, value: f64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Bytes as they are, with no length.
    pub fn write_raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    /// `writeBoolean`: one byte, 0 or 1.
    pub fn write_bool(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    /// `writeInt`: four bytes, big-endian.
    pub fn write_i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// `writeLong`: eight bytes, big-endian.
    pub fn write_i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Minecraft's VarInt: seven bits per byte, least significant group first, high bit set
    /// when another byte follows.
    pub fn write_varint(&mut self, value: i32) {
        let mut rest = value as u32;
        loop {
            if (rest & !0x7F) == 0 {
                self.bytes.push(rest as u8);
                return;
            }
            self.bytes.push((rest & 0x7F) as u8 | 0x80);
            rest >>= 7;
        }
    }

    /// `writeString`: VarInt byte length, then the UTF-8 bytes.
    pub fn write_string(&mut self, value: &str) {
        let len = i32::try_from(value.len()).expect("string longer than i32::MAX bytes");
        self.write_varint(len);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    /// `writeUUID`: most significant 64 bits, then least significant 64 bits.
    pub fn write_uuid(&mut self, value: u128) {
        self.write_i64((value >> 64) as u64 as i64);
        self.write_i64(value as u64 as i64);
    }

    /// `writeByteArray`: VarInt byte length, then the bytes (`FriendlyByteBuf.writeByteArray`).
    pub fn write_byte_array(&mut self, value: &[u8]) {
        let len = i32::try_from(value.len()).expect("byte array longer than i32::MAX");
        self.write_varint(len);
        self.bytes.extend_from_slice(value);
    }

    /// `writeEnum`: the ordinal as an i32.
    pub fn write_enum_ordinal(&mut self, ordinal: i32) {
        self.write_i32(ordinal);
    }

    /// `writeNullable`: a presence flag, then the value when present.
    pub fn write_nullable<T>(&mut self, value: Option<&T>, write: impl FnOnce(&mut Self, &T)) {
        self.write_bool(value.is_some());
        if let Some(value) = value {
            write(self, value);
        }
    }
}

/// Reads packet fields in order from a byte slice.
#[derive(Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let data = self.data;
        let remaining = data.len() - self.pos;
        if len > remaining {
            return Err(DecodeError::UnexpectedEnd { needed: len, remaining });
        }
        let slice = &data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn read_i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.array()?))
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(self.array()?))
    }

    pub fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.array()?))
    }

    /// `len` bytes as they are.
    pub fn read_raw(&mut self, len: usize) -> Result<&'a [u8]> {
        self.take(len)
    }

    /// Bytes not read yet.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// `readBoolean`: any non-zero byte is true.
    pub fn read_bool(&mut self) -> Result<bool> {
        Ok(self.take(1)?[0] != 0)
    }

    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    pub fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    pub fn read_varint(&mut self) -> Result<i32> {
        let mut value: u32 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.take(1)?[0];
            value |= u32::from(byte & 0x7F) << shift;
            if (byte & 0x80) == 0 {
                return Ok(value as i32);
            }
            shift += 7;
            if shift >= 35 {
                return Err(DecodeError::VarIntTooLong);
            }
        }
    }

    /// `readString`: VarInt byte length, then UTF-8 bytes.
    pub fn read_string(&mut self) -> Result<String> {
        let len = self.read_varint()?;
        let len = usize::try_from(len).map_err(|_| DecodeError::NegativeLength(len))?;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError::InvalidUtf8)
    }

    /// `readUUID`: most significant 64 bits, then least significant 64 bits.
    pub fn read_uuid(&mut self) -> Result<u128> {
        let most = self.read_i64()? as u64;
        let least = self.read_i64()? as u64;
        Ok((u128::from(most) << 64) | u128::from(least))
    }

    /// `readByteArray`: VarInt byte length, then that many bytes.
    pub fn read_byte_array(&mut self) -> Result<Vec<u8>> {
        let len = self.read_varint()?;
        let len = usize::try_from(len).map_err(|_| DecodeError::NegativeLength(len))?;
        Ok(self.take(len)?.to_vec())
    }

    /// `readNullable`: a presence flag, then the value when present.
    pub fn read_nullable<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<Option<T>> {
        if self.read_bool()? {
            Ok(Some(read(self)?))
        } else {
            Ok(None)
        }
    }

    /// Fails unless every byte has been read.
    pub fn finish(self) -> Result<()> {
        match self.data.len() - self.pos {
            0 => Ok(()),
            left => Err(DecodeError::TrailingBytes(left)),
        }
    }
}

/// Encodes one packet payload.
pub fn encode(write: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut writer = Writer::default();
    write(&mut writer);
    writer.into_bytes()
}

/// Decodes one packet payload. Rust-side choice: bytes left over after the packet is an error.
pub fn decode<T>(bytes: &[u8], read: impl FnOnce(&mut Reader<'_>) -> Result<T>) -> Result<T> {
    let mut reader = Reader::new(bytes);
    let value = read(&mut reader)?;
    reader.finish()?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_matches_the_minecraft_encoding() {
        let cases: [(i32, &[u8]); 7] = [
            (0, &[0x00]),
            (1, &[0x01]),
            (127, &[0x7F]),
            (128, &[0x80, 0x01]),
            (300, &[0xAC, 0x02]),
            (-1, &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]),
            (i32::MAX, &[0xFF, 0xFF, 0xFF, 0xFF, 0x07]),
        ];
        for (value, bytes) in cases {
            assert_eq!(encode(|w| w.write_varint(value)), bytes, "writing {value}");
            assert_eq!(decode(bytes, |r| r.read_varint()), Ok(value), "reading {value}");
        }
    }

    #[test]
    fn varint_longer_than_five_bytes_is_rejected() {
        assert_eq!(
            decode(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00], |r| r.read_varint()),
            Err(DecodeError::VarIntTooLong)
        );
    }

    #[test]
    fn ints_and_longs_are_big_endian() {
        assert_eq!(encode(|w| w.write_i32(0x0102_0304)), [1, 2, 3, 4]);
        assert_eq!(encode(|w| w.write_i64(0x0102_0304_0506_0708)), [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn strings_are_varint_length_then_utf8() {
        assert_eq!(encode(|w| w.write_string("")), [0x00]);
        // 'é' is two bytes in UTF-8, so "hé" has a byte length of 3.
        assert_eq!(encode(|w| w.write_string("hé")), [0x03, 0x68, 0xC3, 0xA9]);
        assert_eq!(
            decode(&[0x03, 0x68, 0xC3, 0xA9], |r| r.read_string()),
            Ok("hé".to_string())
        );
    }

    #[test]
    fn negative_string_length_and_invalid_utf8_are_rejected() {
        assert_eq!(
            decode(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F], |r| r.read_string()),
            Err(DecodeError::NegativeLength(-1))
        );
        assert_eq!(decode(&[0x01, 0xFF], |r| r.read_string()), Err(DecodeError::InvalidUtf8));
    }

    #[test]
    fn short_input_and_trailing_bytes_are_rejected() {
        assert_eq!(
            decode(&[0x00, 0x00, 0x01], |r| r.read_i32()),
            Err(DecodeError::UnexpectedEnd { needed: 4, remaining: 3 })
        );
        assert_eq!(decode(&[0x01, 0x02], |r| r.read_bool()), Err(DecodeError::TrailingBytes(1)));
    }

    #[test]
    fn any_non_zero_byte_reads_as_true() {
        // As Minecraft's PacketByteBuf does (readUnsignedByte() != 0).
        assert_eq!(decode(&[0x02], |r| r.read_bool()), Ok(true));
        assert_eq!(decode(&[0x00], |r| r.read_bool()), Ok(false));
    }

    #[test]
    fn uuid_is_most_significant_half_first() {
        let uuid: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;
        let bytes = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, //
            0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10,
        ];
        assert_eq!(encode(|w| w.write_uuid(uuid)), bytes);
        assert_eq!(decode(&bytes, |r| r.read_uuid()), Ok(uuid));
    }
}

//! Card image packet. Mirrors common/src/main/java/me/jfenn/bingo/common/map/CardImagePacket.kt.

use crate::buf::{Reader, Result, Writer};

/// `CardImagePacket` (V1): a card id, then the card's image bytes when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardImagePacket {
    pub id: String,
    pub image: Option<Vec<u8>>,
}

impl CardImagePacket {
    pub const ID: &'static str = "yet-another-minecraft-bingo:card_image";

    pub fn write(&self, dest: &mut Writer) {
        dest.write_string(&self.id);
        dest.write_nullable(self.image.as_ref(), |dest, image| dest.write_byte_array(image));
    }

    pub fn read(src: &mut Reader<'_>) -> Result<Self> {
        let id = src.read_string()?;
        let image = src.read_nullable(|src| src.read_byte_array())?;
        Ok(Self { id, image })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::{decode, encode};

    #[test]
    fn absent_image_is_a_false_flag_after_the_id() {
        let packet = CardImagePacket { id: String::new(), image: None };
        assert_eq!(encode(|w| packet.write(w)), [0x00, 0x00]);
        assert_eq!(decode(&[0x00, 0x00], CardImagePacket::read), Ok(packet));
    }

    #[test]
    fn image_of_300_bytes_has_a_two_byte_length_prefix() {
        let packet = CardImagePacket { id: "a".to_string(), image: Some(vec![0xEE; 300]) };
        let bytes = encode(|w| packet.write(w));
        // id "a" (VarInt length 1), present flag, image length 300 as VarInt AC 02, then the bytes.
        assert_eq!(&bytes[..5], [0x01, b'a', 0x01, 0xAC, 0x02]);
        assert_eq!(bytes.len(), 5 + 300);
        assert_eq!(decode(&bytes, CardImagePacket::read), Ok(packet));
    }
}

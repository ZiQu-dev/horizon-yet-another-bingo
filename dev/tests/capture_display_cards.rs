//! The real captured payload (206 bytes) from a live server: decoded whole, every byte consumed, and written
//! back to the identical bytes.

use horizon_bingo::display_cards::CardDisplayPacket;
use horizon_bingo::buf::{encode, Reader};

const CAPTURE: &str = include_str!("captures/display_cards.hex");

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect()
}

#[test]
fn the_whole_capture_decodes_and_writes_back_identically() {
    let bytes = unhex(CAPTURE.trim());
    assert_eq!(bytes.len(), 206);

    let mut reader = Reader::new(&bytes);
    let value = CardDisplayPacket::read(&mut reader).expect("capture decodes");
    assert_eq!(reader.remaining(), 0, "every byte is consumed");

    let written = encode(|w| value.write(w).unwrap());
    assert_eq!(written, bytes, "re-encoding gives the identical bytes");

    // What the capture holds.
    assert_eq!(value.display.len(), 1, "entries");
    let (key, card) = &value.display[0];
    assert_eq!(key.as_deref(), Some("bingo_red"));
    assert_eq!(card.team_color.as_deref(), Some("RED"));
    assert_eq!(card.players.len(), 1, "players");
    assert_eq!(card.players[0].uuid, 0xee305221_9b0e_3305_b140_17edb555b9fb);
    assert_eq!(card.players[0].name, "RegTest_control");
    println!("team name text: {:?}", card.team_name);
}

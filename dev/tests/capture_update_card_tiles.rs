//! The real capture of `update_card_tiles` from a live server (25 tiles, team bingo_red), decoded whole.
use horizon_bingo::buf::{decode, encode};
use horizon_bingo::update_card_tiles::CardTilesPacket;

const CAPTURE: &str = include_str!("captures/update_card_tiles.hex");

fn bytes_of(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    assert!(hex.len().is_multiple_of(2), "odd hex length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

#[test]
fn the_whole_capture_decodes_and_writes_back_to_the_same_bytes() {
    let bytes = bytes_of(CAPTURE);
    let packet = decode(&bytes, CardTilesPacket::read).expect("the capture decodes to the end");
    let written = encode(|w| packet.write(w).expect("the packet writes"));
    assert_eq!(written.len(), bytes.len(), "re-encoded length");
    assert_eq!(written, bytes, "re-encoded bytes");
    println!(
        "capture: {} bytes, team {:?}, {} tiles, should_notify {}",
        bytes.len(),
        packet.team_key,
        packet.tiles.len(),
        packet.should_notify
    );
}

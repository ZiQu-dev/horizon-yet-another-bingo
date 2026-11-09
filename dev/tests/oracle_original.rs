//! Compares the Rust codec with bytes produced by the real mod running on Minecraft's real
//! FriendlyByteBuf (exp-oracle/Oracle.java). Expected values come from the Kotlin side.
use horizon_bingo::buf::encode;
use horizon_bingo::game::{GameState, GameStatus};
use horizon_bingo::ready::{ReadyUpdatePacket, SetReadyPacket};
use horizon_bingo::scoring::{CardCompletedPacket, GameMessageClearPacket, ScoredItemLostPacket};
use horizon_bingo::timer::{CountdownPacket, TimerPacket};

const UUID: u128 = 0x0123456789abcdef_fedcba9876543210;

fn oracle(name: &str) -> (String, String) {
    let line = include_str!("oracle.tsv")
        .lines()
        .find(|l| l.split('\t').next() == Some(name))
        .unwrap_or_else(|| panic!("no oracle row {name}"));
    let mut f = line.split('\t');
    f.next();
    (f.next().unwrap().to_string(), f.next().unwrap_or("").to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn check(name: &str, id: &str, bytes: Vec<u8>) {
    let (oid, ohex) = oracle(name);
    assert_eq!(id, oid, "{name}: channel id");
    assert_eq!(hex(&bytes), ohex, "{name}: bytes");
}

fn ready(status: GameStatus, ident: &str, running: bool, is_ready: bool, rem: i64, tot: i64, rp: i32, tp: i32, can: bool, pending: Option<u128>) -> ReadyUpdatePacket {
    ReadyUpdatePacket {
        is_running: running, is_ready, state: GameState { identifier: ident.into(), status },
        remaining_millis: rem, total_millis: tot, ready_players: rp, total_players: tp,
        can_send_ready: can, vote_pending: pending,
    }
}

#[test]
fn simple_packets_match_the_mod() {
    check("timer_90", TimerPacket::ID, encode(|w| TimerPacket { seconds_remaining: 90 }.write(w)));
    check("timer_neg1", TimerPacket::ID, encode(|w| TimerPacket { seconds_remaining: -1 }.write(w)));
    check("countdown_5", CountdownPacket::ID, encode(|w| CountdownPacket { seconds_remaining: 5 }.write(w)));
    check("card_completed_true", CardCompletedPacket::ID, encode(|w| CardCompletedPacket { is_winner: true }.write(w)));
    check("card_completed_false", CardCompletedPacket::ID, encode(|w| CardCompletedPacket { is_winner: false }.write(w)));
    check("scored_item_lost", ScoredItemLostPacket::ID, encode(|w| ScoredItemLostPacket.write(w)));
    check("game_message_clear", GameMessageClearPacket::ID, encode(|w| GameMessageClearPacket.write(w)));
}

#[test]
fn set_ready_matches_the_mod() {
    check("set_ready_v1_true", SetReadyPacket::ID_V1, encode(|w| SetReadyPacket { is_ready: true, option_id: None }.write_v1(w)));
    check("set_ready_v2_true_none", SetReadyPacket::ID_V2, encode(|w| SetReadyPacket { is_ready: true, option_id: None }.write_v2(w)));
    check("set_ready_v2_false_uuid", SetReadyPacket::ID_V2, encode(|w| SetReadyPacket { is_ready: false, option_id: Some(UUID) }.write_v2(w)));
}

#[test]
fn ready_update_matches_the_mod() {
    for (s, n) in [(GameStatus::Pregame, "PREGAME"), (GameStatus::Starting, "STARTING"), (GameStatus::Playing, "PLAYING"), (GameStatus::Postgame, "POSTGAME")] {
        let p = ready(s, "pregame", true, false, 12345, 60000, 2, 5, true, Some(UUID));
        check(&format!("ready_update_v2_{n}"), ReadyUpdatePacket::ID_V2, encode(|w| p.write_v2(w)));
    }
    let d = ready(GameStatus::Playing, "mymod:custom", false, true, 0, 1, 0, 0, false, None);
    check("ready_update_v2_defaults", ReadyUpdatePacket::ID_V2, encode(|w| d.write_v2(w)));
    let v1 = ready(GameStatus::Pregame, "pregame", true, true, 12345, 60000, 2, 5, true, None);
    check("ready_update_v1", ReadyUpdatePacket::ID_V1, encode(|w| v1.write_v1(w)));
}

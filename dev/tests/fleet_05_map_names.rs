//! The names of the players' map items (ledger map-21): a won game's memento is named by the won key, as the original's
//! `createMementoMapItem` picks `team_card_memento_won` when the team finished. The English texts of the keys are the
//! original's `en_us.json` (`Trophy of the Bingo Victor`, `Relic of a Lost Bingo`); the port names them by key.
use horizon_bingo::server::map_items::{item_snbt, Want};

#[test]
fn a_won_memento_is_named_by_the_won_key() {
    let won = Want::Memento { key: "bingo_red".to_string(), won: true };
    let snbt = item_snbt(5, &won);
    assert!(snbt.contains("translate:\"bingo.card.team_card_memento_won\""), "{snbt}");
    assert!(!snbt.contains("team_card_memento_lost"), "{snbt}");
}

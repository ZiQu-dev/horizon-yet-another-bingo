//! The challenge stat squares read their `min` and `max` from the original's objective files: `never_sleep_in_bed`
//! has 0..=0 (sleeping in a bed takes the square away) and `stat_custom_bell_ring` has a minimum of 12.
use horizon_bingo::server::objectives::{objective_of_key, team_achieves, Ctx, Facts, Objective, Runtime, Tags};

const P1: u128 = 1;

/// A player whose sleep counter reads `value` and who holds nothing.
struct Counter(i64);

impl Facts for Counter {
    fn held(&self, _uuid: u128, _item: &str) -> u32 {
        0
    }
    fn stat(&self, _uuid: u128, _stat_type: &str, _stat_name: Option<&str>) -> Option<i64> {
        Some(self.0)
    }
    fn advancement_done(&self, _uuid: u128, _id: &str) -> bool {
        false
    }
}

/// Holds the square at the counter's value, with the same runtime across calls (the baseline is the first value).
fn holds_at(objective: &Objective, runtime: &mut Runtime, value: i64) -> bool {
    let tags = Tags::new();
    let facts = Counter(value);
    let ctx = Ctx { facts: &facts, tags: &tags };
    team_achieves(0, objective, &[P1], &ctx, runtime)
}

#[test]
fn never_sleep_in_bed_is_lost_by_the_first_sleep() {
    let objective = objective_of_key("objective:yet-another-minecraft-bingo:challenge/never_sleep_in_bed")
        .expect("never_sleep_in_bed is in the catalog");
    let mut runtime = Runtime::default();
    // Never slept: the counter stays at its first value, which is the baseline.
    assert!(holds_at(&objective, &mut runtime, 0));
    assert!(holds_at(&objective, &mut runtime, 0));
    // One sleep: 1 - 0 is outside 0..=0.
    assert!(!holds_at(&objective, &mut runtime, 1));
}

#[test]
fn bell_ring_needs_twelve_rings_from_the_baseline() {
    let objective = objective_of_key("objective:yet-another-minecraft-bingo:challenge/stat_custom_bell_ring")
        .expect("stat_custom_bell_ring is in the catalog");
    let mut runtime = Runtime::default();
    assert!(!holds_at(&objective, &mut runtime, 0));
    assert!(!holds_at(&objective, &mut runtime, 11));
    assert!(holds_at(&objective, &mut runtime, 12));
}

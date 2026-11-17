//! Statistics over the history (`history.jsonl`), with the original's definitions: the end screen's win streak, best time
//! and best captured items (`GameOverService.kt`, `WriteStatsService.kt`, `StatsService.kt`, `BestStats.sq`, `Game.sq`),
//! and the `bingo stats` summary (`StatsCommand.kt`). Pure functions over the entries, oldest first. Each function reads
//! the rounds before the one being judged; the end screen passes the history without the round that just ended.
//! Also the per-player capture ledger that counts the items each player found during the round.
use std::collections::BTreeMap;

use serde_json::Value;

use super::options::Options;
use super::store::{self, HistoryEntry};

/// Milliseconds in a day, for the month of a time.
const DAY_MS: i64 = 86_400_000;

/// The rules key of a round: 16 hex digits, FNV-1a over the options as compact JSON. It plays the part of the original's
/// `bingo_options_hash`, so best times and best items are kept per rules.
pub fn rules_key(options: &Options) -> String {
    let text = store::options_to_json(options);
    let compact = serde_json::from_str::<Value>(&text).map_or(text, |value| value.to_string());
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in compact.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The team the player was on in the round, if the player played it.
fn team_of(entry: &HistoryEntry, uuid: u128) -> Option<&str> {
    entry.players.iter().find(|p| p.uuid == uuid).map(|p| p.team.as_str())
}

/// The items the player found in the round, if the player played it.
fn items_of(entry: &HistoryEntry, uuid: u128) -> Option<i32> {
    entry.players.iter().find(|p| p.uuid == uuid).map(|p| p.items)
}

/// A round nobody won with more than one team: a draw.
fn is_draw(entry: &HistoryEntry) -> bool {
    entry.winner.is_none() && entry.teams.len() > 1
}

/// A round with one player.
fn is_single(entry: &HistoryEntry) -> bool {
    entry.players.len() == 1
}

/// The player's team won the round. A forfeit has no winner.
fn won_by(entry: &HistoryEntry, uuid: u128) -> bool {
    entry.winner.is_some() && team_of(entry, uuid) == entry.winner.as_deref()
}

/// The rounds of the player, in order.
fn played<'a>(history: &'a [HistoryEntry], uuid: u128) -> impl Iterator<Item = &'a HistoryEntry> {
    history.iter().filter(move |entry| team_of(entry, uuid).is_some())
}

/// The current win streak after the rounds: the wins since the latest round that was not a forfeit and not won by the
/// player's team. Draws and losses end a streak, a forfeit does not (the original's `latestWinStreakByPlayer`).
pub fn current_streak(history: &[HistoryEntry], uuid: u128) -> i64 {
    let mut streak = 0;
    for entry in played(history, uuid).filter(|entry| !entry.forfeit) {
        streak = if won_by(entry, uuid) { streak + 1 } else { 0 };
    }
    streak
}

/// The best win streak so far: the highest streak the player reached after a round. `None` when it was never above 0
/// (the original keeps a row only for a streak above 0).
pub fn best_streak(history: &[HistoryEntry], uuid: u128) -> Option<i64> {
    let mut streak = 0;
    let mut best = None;
    for entry in played(history, uuid).filter(|entry| !entry.forfeit) {
        streak = if won_by(entry, uuid) { streak + 1 } else { 0 };
        if streak > 0 {
            best = Some(best.map_or(streak, |b: i64| b.max(streak)));
        }
    }
    best
}

/// The best time of the rules, in milliseconds: the shortest round of those rules that was neither a draw nor a forfeit.
/// `single` is the singleplayer query: the original keeps one row per rules, and that row answers it only when the first
/// such round had one player. Without `single` the shortest round answers.
pub fn best_time(history: &[HistoryEntry], rules: &str, single: bool) -> Option<i64> {
    let rows: Vec<&HistoryEntry> = history
        .iter()
        .filter(|entry| entry.rules.as_deref() == Some(rules) && !entry.forfeit && !is_draw(entry))
        .collect();
    let first = rows.first()?;
    if single && !is_single(first) {
        return None;
    }
    rows.iter().map(|entry| entry.duration_ms).min()
}

/// Whether a round set a new best time: no earlier best, or a shorter time, and the round was won (not a forfeit, with a
/// winner). Equal times are not a new best.
pub fn is_best_time(previous: Option<i64>, duration_ms: i64, forfeit: bool, has_winner: bool) -> bool {
    previous.map_or(true, |previous| duration_ms < previous) && !forfeit && has_winner
}

/// The most items the player captured in one round of the rules (the original's `BestCapturedItems`, kept for every round,
/// forfeits too).
pub fn best_items(history: &[HistoryEntry], uuid: u128, rules: &str) -> Option<i32> {
    history
        .iter()
        .filter(|entry| entry.rules.as_deref() == Some(rules))
        .filter_map(|entry| items_of(entry, uuid))
        .max()
}

/// The end screen's numbers of one player.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerEnd {
    pub win_streak: i64,
    pub best_win_streak: Option<i64>,
    pub is_best_win_streak: bool,
    pub captured_items: i32,
    pub best_captured_items: Option<i32>,
    pub is_best_captured_items: bool,
}

/// The end screen's numbers: the game's best time, and the viewer's numbers when the viewer played.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EndStats {
    pub ended_at_ms: i64,
    pub is_best_time: bool,
    pub prev_best_time_ms: Option<i64>,
    /// `None` for a spectator, or a viewer who was on no team.
    pub player: Option<PlayerEnd>,
}

/// The end screen numbers for a round that just ended. `prior` is the history before it, `game` the round itself, and
/// `viewer` the player the screen is for (`None` for the game-wide part only).
pub fn end_stats(prior: &[HistoryEntry], game: &HistoryEntry, viewer: Option<u128>) -> EndStats {
    let single = is_single(game);
    let prev = game.rules.as_deref().and_then(|rules| best_time(prior, rules, single));
    let has_winner = game.winner.is_some();
    let mut end = EndStats {
        ended_at_ms: game.ended_at_ms,
        is_best_time: is_best_time(prev, game.duration_ms, game.forfeit, has_winner),
        prev_best_time_ms: prev,
        player: None,
    };
    let Some(uuid) = viewer else { return end };
    if team_of(game, uuid).is_none() {
        return end;
    }
    let won = won_by(game, uuid);
    let prior_streak = current_streak(prior, uuid);
    let win_streak = if won {
        prior_streak + 1
    } else if game.forfeit || is_draw(game) {
        prior_streak
    } else {
        0
    };
    let best_win = best_streak(prior, uuid);
    let captured = items_of(game, uuid).unwrap_or(0);
    let best_captured = game.rules.as_deref().and_then(|rules| best_items(prior, uuid, rules));
    end.player = Some(PlayerEnd {
        win_streak,
        best_win_streak: best_win,
        is_best_win_streak: won && best_win.is_some_and(|best| win_streak > best),
        captured_items: captured,
        best_captured_items: best_captured,
        is_best_captured_items: best_captured.is_some_and(|best| captured > best),
    });
    end
}

/// The `bingo stats` numbers of one player over some rounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    /// Rounds that were not forfeits.
    pub games: u32,
    /// Rounds the player's team won (not forfeits, not draws).
    pub wins: u32,
    /// Rounds the player's team lost (not forfeits, not draws).
    pub losses: u32,
    /// Items the player found, forfeits included.
    pub items: i64,
    /// Time of the rounds, forfeits included.
    pub playtime_ms: i64,
}

/// `(year, month)` of a time in ms since the epoch, in UTC.
pub fn year_month(ms: i64) -> (i64, u32) {
    let z = ms.div_euclid(DAY_MS) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as u32)
}

/// The totals of the player over the rounds. `month` limits them to the rounds that started in that month (the round
/// started at its end minus its duration).
pub fn totals(history: &[HistoryEntry], uuid: u128, month: Option<(i64, u32)>) -> Totals {
    let mut totals = Totals::default();
    for entry in played(history, uuid) {
        if let Some(month) = month {
            if year_month(entry.ended_at_ms - entry.duration_ms) != month {
                continue;
            }
        }
        totals.items += i64::from(items_of(entry, uuid).unwrap_or(0));
        totals.playtime_ms += entry.duration_ms;
        if entry.forfeit {
            continue;
        }
        totals.games += 1;
        if is_draw(entry) {
            continue;
        }
        if won_by(entry, uuid) {
            totals.wins += 1;
        } else {
            totals.losses += 1;
        }
    }
    totals
}

/// Digits with a comma every three places, as `%,d`.
fn grouped(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// A duration as `3d 2h 01m 32s`: the days and hours only when they are above 0.
pub fn format_duration(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    let (days, hours, minutes, seconds) = (secs / 86_400, (secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    let mut out = String::new();
    if days > 0 {
        out.push_str(&format!("{days}d "));
    }
    if hours > 0 {
        out.push_str(&format!("{hours}h "));
    }
    out.push_str(&format!("{minutes:02}m {seconds:02}s"));
    out
}

/// The `bingo stats` text: a header and the all-time and monthly lines, joined by newlines.
pub fn summary_text(name: &str, all: &Totals, monthly: &Totals) -> String {
    let block = |title: &str, t: &Totals| {
        format!(
            "{title}:\n  Games: {}, Win/Loss: {}/{}, Items: {}\n  Playtime: {}",
            grouped(i64::from(t.games)),
            grouped(i64::from(t.wins)),
            grouped(i64::from(t.losses)),
            grouped(t.items),
            format_duration(t.playtime_ms)
        )
    };
    format!("Bingo stats for {name}:\n{}\n{}", block("All Time", all), block("Monthly", monthly))
}

/// The capture ledger of the round: how many items each player found, and which player found each square of each team.
/// A lost square (Inventory mode) takes the item from the player who found it. The ledger is saved with the game
/// (`game-state.json`), so a restart keeps it.
#[derive(Debug, Clone, Default)]
pub struct Captures {
    /// Team key, then square index, to the player who found it.
    finders: BTreeMap<String, BTreeMap<usize, u128>>,
    /// Items each player found, net of the lost ones.
    items: BTreeMap<u128, i64>,
}

impl Captures {
    /// The squares were found by the player for the team.
    pub fn gained(&mut self, team: &str, player: u128, squares: &[usize]) {
        for &square in squares {
            self.finders.entry(team.to_string()).or_default().insert(square, player);
            *self.items.entry(player).or_default() += 1;
        }
    }

    /// The squares were lost by the team: each one's finder loses it.
    pub fn lost(&mut self, team: &str, squares: &[usize]) {
        for square in squares {
            let finder = self.finders.get_mut(team).and_then(|found| found.remove(square));
            if let Some(player) = finder {
                let count = self.items.entry(player).or_default();
                *count = (*count - 1).max(0);
            }
        }
    }

    /// The items the player found this round.
    pub fn items(&self, player: u128) -> i64 {
        self.items.get(&player).copied().unwrap_or(0)
    }

    /// Empties the ledger for a new round.
    pub fn clear(&mut self) {
        self.finders.clear();
        self.items.clear();
    }

    /// The ledger as JSON: `items` (player uuid hex to count) and `finders` (team, then square index, to the finder's uuid
    /// hex). Keys are sorted.
    pub fn to_json(&self) -> Value {
        let items: BTreeMap<String, i64> = self.items.iter().map(|(&player, &count)| (store::uuid_hex(player), count)).collect();
        let finders: BTreeMap<&str, BTreeMap<String, String>> = self
            .finders
            .iter()
            .map(|(team, found)| {
                let squares = found.iter().map(|(&square, &player)| (square.to_string(), store::uuid_hex(player))).collect();
                (team.as_str(), squares)
            })
            .collect();
        serde_json::json!({ "items": items, "finders": finders })
    }

    /// Reads the ledger written by `to_json`. Broken entries are skipped; anything that is not an object gives an empty
    /// ledger, so a game saved before the ledger existed restores with no per-player counts.
    pub fn from_json(value: &Value) -> Self {
        let mut captures = Self::default();
        if let Some(items) = value.get("items").and_then(Value::as_object) {
            for (key, count) in items {
                if let (Some(player), Some(count)) = (store::uuid_from(key), count.as_i64()) {
                    captures.items.insert(player, count);
                }
            }
        }
        if let Some(finders) = value.get("finders").and_then(Value::as_object) {
            for (team, found) in finders {
                let Some(found) = found.as_object() else { continue };
                let mut squares = BTreeMap::new();
                for (square, player) in found {
                    if let (Ok(square), Some(player)) = (square.parse::<usize>(), player.as_str().and_then(store::uuid_from)) {
                        squares.insert(square, player);
                    }
                }
                captures.finders.insert(team.clone(), squares);
            }
        }
        captures
    }
}

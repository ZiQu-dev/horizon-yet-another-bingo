//! Text formats of the server's saved data: the round options, the history of finished rounds (one JSON object per
//! line) and the per-player settings (a JSON object keyed by player uuid). Pure functions over strings and values:
//! the caller does the file I/O. Reading is lenient: a missing or broken field takes its default, a broken history
//! line or settings entry is skipped.
//! Uuids are written as 32 lowercase hex digits.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Map, Value};

use crate::server::card_gen::DEFAULT_DISTRIBUTION;
use crate::server::hidden_consume::HiddenConsume;
use crate::server::multicard::{StalemateBehavior, WinCondition};
use crate::server::options::{EndWhen, Goal, Mode, Options, DEFAULT_TEAM_COUNT};
use crate::server::teams::PRESETS as TEAM_PRESETS;
use crate::update_settings::PlayerSettings;

/// Round options as pretty JSON. `goal.n` is the count for `lines` and `items`, 25 for `full_card`, and
/// `endWhen.n` is the count for `teams_win`; the other types write 0.
pub fn options_to_json(o: &Options) -> String {
    let (goal_type, goal_n) = match o.goal {
        Goal::Lines(n) => ("lines", n),
        Goal::Items(n) => ("items", n),
        Goal::FullCard => ("full_card", 25),
    };
    let (end_type, end_n) = match o.end_when {
        EndWhen::Never => ("never", 0),
        EndWhen::FirstWin => ("first_win", 0),
        EndWhen::TeamsWin(n) => ("teams_win", n),
        EndWhen::AllWin => ("all_win", 0),
    };
    let mode = match o.mode {
        Mode::Standard => "standard",
        Mode::Inventory => "inventory",
        Mode::Lockout => "lockout",
    };
    let mut value = json!({
        "goal": { "type": goal_type, "n": goal_n },
        "mode": mode,
        "endWhen": { "type": end_type, "n": end_n },
        "pvp": o.pvp,
        "keepInventory": o.keep_inventory,
        "elytra": o.elytra,
        "nightVision": o.night_vision,
        "spawnDistance": o.spawn_distance,
    });
    // The multi-card and stalemate settings are written only when they differ from the defaults, so a round without
    // them keeps the text it always had.
    if o.win_condition != WinCondition::default() {
        value["winCondition"] = o.win_condition.to_json();
    }
    if o.stalemate != StalemateBehavior::default() {
        value["stalemate"] = o.stalemate.to_json();
    }
    // Team chat and the sidebar are written only when they are off (their default is on).
    if !o.team_chat {
        value["teamChat"] = Value::Bool(false);
    }
    if !o.sidebar {
        value["sidebar"] = Value::Bool(false);
    }
    // The card flags are written only when one is on, so a round without them keeps the text it always had.
    if o.hidden_consume != HiddenConsume::default() {
        if let Value::Object(object) = &mut value {
            o.hidden_consume.write_into(object);
        }
    }
    // The difficulty and team settings are written only when they differ from the defaults, as the blocks above are.
    if o.tier_counts != DEFAULT_DISTRIBUTION {
        value["tierCounts"] = json!(o.tier_counts);
    }
    if o.team_auto_shuffle {
        value["teamAutoShuffle"] = json!(true);
    }
    if o.team_count != DEFAULT_TEAM_COUNT {
        value["teamCount"] = json!(o.team_count);
    }
    serde_json::to_string_pretty(&value).expect("a JSON value always serializes")
}

/// Reads round options. Each field that is missing or invalid takes its value from `Options::default()`; text that is
/// not a JSON object gives the defaults. A count of 0 (or none) for `lines`, `items` or `teams_win` makes that whole
/// field the default.
pub fn options_from_json(s: &str) -> Options {
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(s) else {
        return Options::default();
    };
    let d = Options::default();
    Options {
        goal: root.get("goal").and_then(goal_from).unwrap_or(d.goal),
        mode: root.get("mode").and_then(mode_from).unwrap_or(d.mode),
        end_when: root.get("endWhen").and_then(end_when_from).unwrap_or(d.end_when),
        pvp: flag(&root, "pvp", d.pvp),
        keep_inventory: flag(&root, "keepInventory", d.keep_inventory),
        elytra: flag(&root, "elytra", d.elytra),
        night_vision: flag(&root, "nightVision", d.night_vision),
        spawn_distance: root
            .get("spawnDistance")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(d.spawn_distance),
        hidden_consume: HiddenConsume::read_from(&root),
        win_condition: root.get("winCondition").and_then(WinCondition::from_json).unwrap_or(d.win_condition),
        stalemate: root.get("stalemate").and_then(StalemateBehavior::from_json).unwrap_or(d.stalemate),
        tier_counts: root.get("tierCounts").and_then(tier_counts_from).unwrap_or(d.tier_counts),
        team_auto_shuffle: flag(&root, "teamAutoShuffle", d.team_auto_shuffle),
        team_count: root
            .get("teamCount")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=TEAM_PRESETS.len()).contains(n))
            .unwrap_or(d.team_count),
        team_chat: flag(&root, "teamChat", d.team_chat),
        sidebar: flag(&root, "sidebar", d.sidebar),
    }
}

/// Five tier counts (S, A, B, C, D), each from 0 to 25, summing to 25. Anything else is `None`.
fn tier_counts_from(v: &Value) -> Option<[usize; 5]> {
    let items = v.as_array()?;
    if items.len() != 5 {
        return None;
    }
    let mut counts = [0usize; 5];
    for (count, item) in counts.iter_mut().zip(items) {
        let n = usize::try_from(item.as_u64()?).ok()?;
        if n > 25 {
            return None;
        }
        *count = n;
    }
    (counts.iter().sum::<usize>() == 25).then_some(counts)
}

fn flag(obj: &Map<String, Value>, key: &str, default: bool) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// A positive count from the `n` field.
fn count(obj: &Map<String, Value>) -> Option<u32> {
    let n = obj.get("n")?.as_u64()?;
    if n == 0 {
        return None;
    }
    u32::try_from(n).ok()
}

fn goal_from(v: &Value) -> Option<Goal> {
    let obj = v.as_object()?;
    match obj.get("type")?.as_str()? {
        "lines" => Some(Goal::Lines(count(obj)?)),
        "items" => Some(Goal::Items(count(obj)?)),
        "full_card" => Some(Goal::FullCard),
        _ => None,
    }
}

fn mode_from(v: &Value) -> Option<Mode> {
    match v.as_str()? {
        "standard" => Some(Mode::Standard),
        "inventory" => Some(Mode::Inventory),
        "lockout" => Some(Mode::Lockout),
        _ => None,
    }
}

fn end_when_from(v: &Value) -> Option<EndWhen> {
    let obj = v.as_object()?;
    match obj.get("type")?.as_str()? {
        "never" => Some(EndWhen::Never),
        "first_win" => Some(EndWhen::FirstWin),
        "teams_win" => Some(EndWhen::TeamsWin(count(obj)?)),
        "all_win" => Some(EndWhen::AllWin),
        _ => None,
    }
}

/// One finished round, as the history file keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: u128,
    pub ended_at_ms: i64,
    pub duration_ms: i64,
    /// The key of the winning team, `None` when nobody won.
    pub winner: Option<String>,
    pub forfeit: bool,
    pub teams: Vec<HistoryTeam>,
    pub players: Vec<HistoryPlayer>,
    /// The rules key of the round (`stats::rules_key`), written when known. Older lines have none, so they never match
    /// a best time or best items.
    pub rules: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryTeam {
    pub key: String,
    pub name: String,
    pub items: i32,
    pub lines: i32,
    pub won: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPlayer {
    pub uuid: u128,
    pub name: String,
    /// The key of the player's team.
    pub team: String,
    pub items: i32,
}

/// Counts of one player over the history.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerStats {
    /// Rounds the player took part in.
    pub games: u32,
    /// Rounds the player's team won.
    pub wins: u32,
    /// The shortest winning round, in milliseconds.
    pub best_win_ms: Option<i64>,
    /// Items the player found, summed over all rounds.
    pub items: u32,
}

/// A uuid as 32 lowercase hex digits.
pub fn uuid_hex(id: u128) -> String {
    format!("{id:032x}")
}

/// Parses 32 hex digits into a uuid; anything else is `None`.
pub fn uuid_from(s: &str) -> Option<u128> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u128::from_str_radix(s, 16).ok()
}

fn entry_value(e: &HistoryEntry) -> Value {
    let teams: Vec<Value> = e
        .teams
        .iter()
        .map(|t| json!({ "key": t.key, "name": t.name, "items": t.items, "lines": t.lines, "won": t.won }))
        .collect();
    let players: Vec<Value> = e
        .players
        .iter()
        .map(|p| json!({ "uuid": uuid_hex(p.uuid), "name": p.name, "team": p.team, "items": p.items }))
        .collect();
    let mut value = json!({
        "id": uuid_hex(e.id),
        "endedAtMs": e.ended_at_ms,
        "durationMs": e.duration_ms,
        "winner": e.winner,
        "forfeit": e.forfeit,
        "teams": teams,
        "players": players,
    });
    if let (Some(rules), Value::Object(object)) = (&e.rules, &mut value) {
        object.insert("rules".to_string(), json!(rules));
    }
    value
}

/// One history entry as a single line of compact JSON, without a newline.
pub fn entry_to_line(e: &HistoryEntry) -> String {
    serde_json::to_string(&entry_value(e)).expect("a JSON value always serializes")
}

fn i32_field(obj: &Map<String, Value>, key: &str) -> Option<i32> {
    i32::try_from(obj.get(key)?.as_i64()?).ok()
}

fn team_from(v: &Value) -> Option<HistoryTeam> {
    let o = v.as_object()?;
    Some(HistoryTeam {
        key: o.get("key")?.as_str()?.to_owned(),
        name: o.get("name")?.as_str()?.to_owned(),
        items: i32_field(o, "items")?,
        lines: i32_field(o, "lines")?,
        won: o.get("won")?.as_bool()?,
    })
}

fn player_from(v: &Value) -> Option<HistoryPlayer> {
    let o = v.as_object()?;
    Some(HistoryPlayer {
        uuid: uuid_from(o.get("uuid")?.as_str()?)?,
        name: o.get("name")?.as_str()?.to_owned(),
        team: o.get("team")?.as_str()?.to_owned(),
        items: i32_field(o, "items")?,
    })
}

/// A whole entry; any missing or mistyped field makes it `None`.
fn entry_from(v: &Value) -> Option<HistoryEntry> {
    let o = v.as_object()?;
    let teams = o.get("teams")?.as_array()?.iter().map(team_from).collect::<Option<Vec<_>>>()?;
    let players = o.get("players")?.as_array()?.iter().map(player_from).collect::<Option<Vec<_>>>()?;
    Some(HistoryEntry {
        id: uuid_from(o.get("id")?.as_str()?)?,
        ended_at_ms: o.get("endedAtMs")?.as_i64()?,
        duration_ms: o.get("durationMs")?.as_i64()?,
        winner: match o.get("winner")? {
            Value::Null => None,
            other => Some(other.as_str()?.to_owned()),
        },
        forfeit: o.get("forfeit")?.as_bool()?,
        teams,
        players,
        rules: o.get("rules").and_then(Value::as_str).map(str::to_owned),
    })
}

/// Reads the history file: one entry per line, in order. Blank and broken lines are skipped.
pub fn parse_history(text: &str) -> Vec<HistoryEntry> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok().and_then(|v| entry_from(&v)))
        .collect()
}

/// Counts the rounds of `uuid` in the history. A round is a win when the player's team is the winner.
pub fn player_stats(entries: &[HistoryEntry], uuid: u128) -> PlayerStats {
    let mut s = PlayerStats::default();
    for e in entries {
        let mine: Vec<&HistoryPlayer> = e.players.iter().filter(|p| p.uuid == uuid).collect();
        let Some(first) = mine.first() else {
            continue;
        };
        s.games += 1;
        s.items = mine.iter().fold(s.items, |acc, p| acc.saturating_add(p.items.max(0) as u32));
        if e.winner.as_deref() == Some(first.team.as_str()) {
            s.wins += 1;
            s.best_win_ms = Some(s.best_win_ms.map_or(e.duration_ms, |best| best.min(e.duration_ms)));
        }
    }
    s
}

/// One line for chat, e.g. `Alice: 5 games, 2 wins, best win 03:25, 41 items.`
pub fn stats_text(name: &str, s: &PlayerStats) -> String {
    let best = match s.best_win_ms {
        Some(ms) => {
            let secs = ms.max(0) / 1000;
            format!("{:02}:{:02}", secs / 60, secs % 60)
        }
        None => "none".to_owned(),
    };
    format!("{name}: {} games, {} wins, best win {best}, {} items.", s.games, s.wins, s.items)
}

/// The settings of every player as a JSON object: uuid hex -> the settings object of `PlayerSettings::to_json`.
/// Keys are sorted, so the text is the same for the same map.
pub fn settings_to_json(m: &HashMap<u128, PlayerSettings>) -> String {
    let sorted: BTreeMap<String, Value> = m
        .iter()
        .map(|(uuid, s)| {
            let value = serde_json::from_str(&s.to_json()).expect("PlayerSettings::to_json is JSON");
            (uuid_hex(*uuid), value)
        })
        .collect();
    serde_json::to_string_pretty(&sorted).expect("a JSON value always serializes")
}

/// Reads the settings written by `settings_to_json`. Entries with a bad uuid key or broken settings are skipped;
/// unknown keys inside a settings object are ignored.
pub fn settings_from_json(s: &str) -> HashMap<u128, PlayerSettings> {
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(s) else {
        return HashMap::new();
    };
    root.iter()
        .filter_map(|(key, value)| {
            let uuid = uuid_from(key)?;
            let settings = PlayerSettings::from_json(&value.to_string()).ok()?;
            Some((uuid, settings))
        })
        .collect()
}

//! Team model of the server: the eight preset teams, who is on which team, spectators, and the random shuffle that
//! `/bingo shuffleteams <n>` and the automatic shuffle at start use. Pure data; no packets, no game types.
//! Members keep the order in which they joined (a shuffle assigns them in its shuffled order).
use crate::server::card_gen::Rng;

/// A preset team: `key` is the id with its `bingo_` prefix, `color` the vanilla `ChatFormatting` name, `name` the
/// English display name (the original's `bingo.team.<x>` translation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamPreset {
    pub key: &'static str,
    pub color: &'static str,
    pub name: &'static str,
}

/// The eight presets, in the original's order. A shuffle fills them from the first.
pub const PRESETS: [TeamPreset; 8] = [
    TeamPreset { key: "bingo_red", color: "RED", name: "Red" },
    TeamPreset { key: "bingo_blue", color: "BLUE", name: "Blue" },
    TeamPreset { key: "bingo_green", color: "GREEN", name: "Green" },
    TeamPreset { key: "bingo_yellow", color: "YELLOW", name: "Yellow" },
    TeamPreset { key: "bingo_aqua", color: "AQUA", name: "Aqua" },
    TeamPreset { key: "bingo_pink", color: "LIGHT_PURPLE", name: "Pink" },
    TeamPreset { key: "bingo_orange", color: "GOLD", name: "Orange" },
    TeamPreset { key: "bingo_gray", color: "GRAY", name: "Gray" },
];

/// Finds a preset by its key, with or without the `bingo_` prefix, ignoring case.
pub fn preset(key: &str) -> Option<&'static TeamPreset> {
    let key = key.to_ascii_lowercase();
    let bare = key.strip_prefix("bingo_").unwrap_or(&key);
    PRESETS.iter().find(|p| p.key.strip_prefix("bingo_") == Some(bare))
}

/// Players on teams and spectators.
#[derive(Debug, Default, Clone)]
pub struct Teams {
    /// `(player, index into PRESETS)` in join order.
    assigned: Vec<(u128, usize)>,
    /// Spectators in the order they joined the spectators.
    spectators: Vec<u128>,
}

impl Teams {
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts `player` on `team`, leaving the previous team and the spectators. Joining the team they are already on
    /// changes nothing, so they keep their place in its order.
    pub fn join(&mut self, player: u128, team: &str) -> Result<(), String> {
        let index = preset_index(team).ok_or_else(|| format!("Unknown team \"{team}\"."))?;
        if self.index_of(player) == Some(index) {
            return Ok(());
        }
        self.assigned.retain(|&(p, _)| p != player);
        self.spectators.retain(|&p| p != player);
        self.assigned.push((player, index));
        Ok(())
    }

    /// Makes `player` a spectator: leaves their team. A spectator stays one until they join a team.
    pub fn spectate(&mut self, player: u128) {
        self.assigned.retain(|&(p, _)| p != player);
        if !self.spectators.contains(&player) {
            self.spectators.push(player);
        }
    }

    /// Removes `player` from their team and from the spectators. Returns whether anything was removed.
    pub fn leave(&mut self, player: u128) -> bool {
        let before = self.assigned.len() + self.spectators.len();
        self.assigned.retain(|&(p, _)| p != player);
        self.spectators.retain(|&p| p != player);
        self.assigned.len() + self.spectators.len() != before
    }

    /// The key (with prefix) of the player's team.
    pub fn team_of(&self, player: u128) -> Option<&'static str> {
        self.index_of(player).map(|i| PRESETS[i].key)
    }

    pub fn is_spectator(&self, player: u128) -> bool {
        self.spectators.contains(&player)
    }

    /// Members of `team` (key with or without prefix) in join order. Empty for an unknown team.
    pub fn members(&self, team: &str) -> Vec<u128> {
        let Some(index) = preset_index(team) else {
            return Vec::new();
        };
        self.assigned.iter().filter(|&&(_, i)| i == index).map(|&(p, _)| p).collect()
    }

    /// Keys of the teams that have at least one member, in the order of `PRESETS`.
    pub fn teams(&self) -> Vec<&'static str> {
        (0..PRESETS.len())
            .filter(|&i| self.assigned.iter().any(|&(_, t)| t == i))
            .map(|i| PRESETS[i].key)
            .collect()
    }

    /// Clears every team assignment, then deals `players` (spectators skipped, repeats counted once) into `n` teams:
    /// they are shuffled by Fisher-Yates and dealt round-robin to the first `n` presets. Spectators stay spectators.
    pub fn shuffle(&mut self, players: &[u128], n: usize, rng: &mut Rng) -> Result<(), String> {
        if !(1..=PRESETS.len()).contains(&n) {
            return Err(format!("Team count must be between 1 and {}, not {n}.", PRESETS.len()));
        }
        self.assigned.clear();

        let mut order: Vec<u128> = Vec::new();
        for &player in players {
            if !self.is_spectator(player) && !order.contains(&player) {
                order.push(player);
            }
        }
        for i in (1..order.len()).rev() {
            let j = rng.next_below(i + 1);
            order.swap(i, j);
        }
        for (k, player) in order.into_iter().enumerate() {
            self.assigned.push((player, k % n));
        }
        Ok(())
    }

    /// Players in `players` that are on no team and are not spectators, in the order given.
    pub fn unassigned(&self, players: &[u128]) -> Vec<u128> {
        players
            .iter()
            .copied()
            .filter(|&p| self.index_of(p).is_none() && !self.is_spectator(p))
            .collect()
    }

    /// Clears all teams. Spectators are kept.
    pub fn clear(&mut self) {
        self.assigned.clear();
    }

    /// The members as (player, team key) in join order, and the spectators in the order they joined: what
    /// [`Teams::from_parts`] takes back.
    pub fn parts(&self) -> (Vec<(u128, &'static str)>, Vec<u128>) {
        let members = self.assigned.iter().map(|&(player, index)| (player, PRESETS[index].key)).collect();
        (members, self.spectators.clone())
    }

    /// Rebuilds the teams from [`Teams::parts`]. An unknown team, or a player listed twice, is an error.
    pub fn from_parts(members: &[(u128, &str)], spectators: &[u128]) -> Result<Self, String> {
        let mut teams = Self::new();
        for &(player, team) in members {
            let index = preset_index(team).ok_or_else(|| format!("Unknown team \"{team}\"."))?;
            if teams.index_of(player).is_some() || teams.is_spectator(player) {
                return Err(format!("Player {player:032x} is listed twice."));
            }
            teams.assigned.push((player, index));
        }
        for &player in spectators {
            if teams.index_of(player).is_some() || teams.is_spectator(player) {
                return Err(format!("Player {player:032x} is listed twice."));
            }
            teams.spectators.push(player);
        }
        Ok(teams)
    }

    fn index_of(&self, player: u128) -> Option<usize> {
        self.assigned.iter().find(|&&(p, _)| p == player).map(|&(_, i)| i)
    }
}

fn preset_index(team: &str) -> Option<usize> {
    let found = preset(team)?;
    PRESETS.iter().position(|p| p == found)
}

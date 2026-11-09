//! The 26 plugin channels of the mod (`yet-another-minecraft-bingo:<path>`), from
//! `ServerPacketEvents.kt:23-55`. This table is the swarm's work list and its progress meter:
//! a channel is `Done` only when its codec has a row in `tests/oracle.tsv` and passes.

pub const NAMESPACE: &str = "yet-another-minecraft-bingo";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    /// Server to client.
    S2c,
    /// Client to server.
    C2s,
    Both,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Codec ported and equal to the mod's bytes.
    Done,
    /// Needs only primitives (ints, strings, UUIDs, lists, byte arrays, nested primitive structs): a Haiku task.
    Todo,
    /// Carries a text component or an item stack (own or in a nested struct): needs Horizon's codecs first (task W2).
    NeedsText,
}

pub struct Channel {
    pub path: &'static str,
    pub dir: Dir,
    /// Kotlin class and file the codec comes from, relative to `common/src/main/java/me/jfenn/bingo/common/`.
    pub source: &'static str,
    pub status: Status,
}

use Dir::*;
use Status::*;

pub const CHANNELS: [Channel; 26] = [
    Channel { path: "timer", dir: S2c, source: "timer/TimerPacket.kt", status: Done },
    Channel { path: "countdown", dir: S2c, source: "timer/CountdownPacket.kt", status: Done },
    Channel { path: "card_completed", dir: S2c, source: "scoring/CardCompletedPacket.kt", status: Done },
    Channel { path: "scored_item_lost", dir: S2c, source: "scoring/ScoredItemLostPacket.kt", status: Done },
    Channel { path: "game_message_clear", dir: S2c, source: "scoring/GameMessageClearPacket.kt", status: Done },
    Channel { path: "set_ready", dir: C2s, source: "ready/SetReadyPacket.kt (V1)", status: Done },
    Channel { path: "set_ready_v2", dir: C2s, source: "ready/SetReadyPacket.kt (V2)", status: Done },
    Channel { path: "ready_timer_update", dir: S2c, source: "ready/ReadyUpdatePacket.kt (V1)", status: Done },
    Channel { path: "ready_timer_update_v2", dir: S2c, source: "ready/ReadyUpdatePacket.kt (V2)", status: Done },
    Channel { path: "card_reset", dir: S2c, source: "map/CardResetPacket.kt", status: Done },
    Channel { path: "display_cards", dir: S2c, source: "map/CardDisplayPacket.kt", status: Done },
    Channel { path: "update_card_tiles", dir: S2c, source: "map/CardTilesPacket.kt", status: Done },
    Channel { path: "card_image", dir: S2c, source: "map/CardImagePacket.kt", status: Done },
    Channel { path: "open_card", dir: S2c, source: "map/CardOpenPacket.kt", status: Done },
    Channel { path: "card_shuffled", dir: S2c, source: "map/CardShuffledPacket.kt", status: Done },
    Channel { path: "game_over_v2", dir: S2c, source: "game/GameOverPacket.kt (V2)", status: Done },
    Channel { path: "game_over", dir: S2c, source: "game/GameOverPacket.kt (V1)", status: Done },
    Channel { path: "score_update", dir: S2c, source: "scoring/ScoredItemPacket.kt", status: Done },
    Channel { path: "game_status", dir: S2c, source: "game/GameStatusPacket.kt", status: Done },
    Channel { path: "game_message", dir: S2c, source: "scoring/GameMessagePacket.kt", status: Done },
    Channel { path: "tooltip_v2", dir: S2c, source: "menu/tooltips/TooltipPacket.kt", status: Done },
    Channel { path: "game_reset", dir: S2c, source: "game/GameResetPacket.kt", status: Done },
    Channel { path: "update_settings", dir: Both, source: "config/PlayerSettings.kt (V2)", status: Done },
    Channel { path: "stats_check", dir: Both, source: "stats/packets/StatsCheckPacket.kt", status: Done },
    Channel { path: "stats_game", dir: Both, source: "stats/packets/StatsGamePacket.kt", status: Done },
    Channel { path: "stats_index", dir: Both, source: "stats/packets/StatsIndexPacket.kt", status: Done },
];

pub fn id(path: &str) -> String {
    format!("{NAMESPACE}:{path}")
}

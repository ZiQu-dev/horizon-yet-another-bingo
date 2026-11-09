//! Game state types carried by the ready packets. Mirrors `BingoGameStatus` in
//! api/src/main/java/me/jfenn/bingo/api/data/IBingoGame.kt and `GameState` in
//! api/src/main/java/me/jfenn/bingo/api/state/GameStates.kt.

/// `BingoGameStatus`. On the wire: the Kotlin ordinal as an i32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    Pregame,
    Starting,
    Playing,
    Postgame,
}

impl GameStatus {
    /// Declaration order, which is the ordinal order.
    pub const ALL: [GameStatus; 4] = [Self::Pregame, Self::Starting, Self::Playing, Self::Postgame];

    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// Mirrors `readEnum` in IPacketBuf.kt: an ordinal outside the enum falls back to the first value.
    pub fn from_ordinal(ordinal: i32) -> Self {
        usize::try_from(ordinal)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(Self::ALL[0])
    }
}

/// `GameState`. Kotlin compares it by `identifier` only; this struct compares both fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    pub identifier: String,
    pub status: GameStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_follow_kotlin_declaration_order() {
        // IBingoGame.kt: PREGAME, STARTING, PLAYING, POSTGAME.
        assert_eq!(GameStatus::Pregame.ordinal(), 0);
        assert_eq!(GameStatus::Starting.ordinal(), 1);
        assert_eq!(GameStatus::Playing.ordinal(), 2);
        assert_eq!(GameStatus::Postgame.ordinal(), 3);
    }

    #[test]
    fn out_of_range_ordinal_falls_back_to_the_first_value() {
        assert_eq!(GameStatus::from_ordinal(3), GameStatus::Postgame);
        assert_eq!(GameStatus::from_ordinal(4), GameStatus::Pregame);
        assert_eq!(GameStatus::from_ordinal(-1), GameStatus::Pregame);
    }
}

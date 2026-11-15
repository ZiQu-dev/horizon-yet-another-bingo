//! Card difficulty: how many squares of each tier a card has. The five presets are the original's
//! `TierLabel.DIFFICULTY_PRESETS` (`BingoConfig.difficultyPresets`), and `bingo difficulty` takes either a preset name or
//! five counts, S A B C D, that add up to the card's 25 squares. Pure: the caller stores the counts in the round options.

use super::types::CARD_SQUARES;

/// Tier names in the order the counts are written: S, A, B, C, D.
pub const TIER_NAMES: [&str; 5] = ["S", "A", "B", "C", "D"];

/// The presets, in the original's order. `easy` is the default card. The original's menu shows the first four; here all
/// five are commands.
pub const PRESETS: [(&str, [usize; 5]); 5] = [
    ("easy", [0, 0, 0, 9, 16]),
    ("medium", [0, 0, 3, 10, 12]),
    ("hard", [0, 1, 5, 10, 9]),
    ("extreme", [0, 5, 7, 7, 6]),
    ("impossible", [5, 7, 6, 5, 2]),
];

/// The most squares one tier may have (`integer(tier.name, 0, 25)` in the original).
const MAX_TIER_COUNT: usize = CARD_SQUARES;

const USAGE: &str = "Usage: difficulty easy|medium|hard|extreme|impossible, or difficulty <S> <A> <B> <C> <D>";

/// The counts of a preset, by name (any case), or `None` for an unknown name.
pub fn preset(name: &str) -> Option<[usize; 5]> {
    PRESETS.iter().find(|(preset, _)| preset.eq_ignore_ascii_case(name)).map(|&(_, counts)| counts)
}

/// Parses the arguments of `difficulty` (the words after it): one preset name, or five whole numbers from 0 to 25 whose
/// sum is 25. The original also takes sums below 25 and fills the rest with free spaces; the native card has no free
/// space, so those sums are an error here.
pub fn parse_args(args: &[&str]) -> Result<[usize; 5], String> {
    match args {
        [] => Err(USAGE.to_string()),
        [name] => preset(name).ok_or_else(|| format!("Unknown difficulty '{name}'. {USAGE}")),
        [_, _, _, _, _] => {
            let mut counts = [0usize; 5];
            for (index, value) in args.iter().enumerate() {
                counts[index] = match value.parse::<usize>() {
                    Ok(count) if count <= MAX_TIER_COUNT => count,
                    _ => {
                        return Err(format!(
                            "difficulty {} needs a whole number from 0 to {MAX_TIER_COUNT}, got '{value}'",
                            TIER_NAMES[index]
                        ))
                    }
                };
            }
            let total: usize = counts.iter().sum();
            if total != CARD_SQUARES {
                return Err(format!("difficulty counts must add up to {CARD_SQUARES} (the squares of a card), got {total}"));
            }
            Ok(counts)
        }
        _ => Err(format!("difficulty needs one preset name or five counts, got {} arguments. {USAGE}", args.len())),
    }
}

/// How a card's difficulty reads in the options text: the preset name when the counts are one, else the five counts.
pub fn describe(counts: [usize; 5]) -> String {
    match PRESETS.iter().find(|(_, preset)| *preset == counts) {
        Some((name, _)) => name.to_string(),
        None => format!("custom S {} A {} B {} C {} D {}", counts[0], counts[1], counts[2], counts[3], counts[4]),
    }
}

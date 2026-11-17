//! Multi-card play and stalemates of the server: the win condition (`play_to`: `cards n`, `infinite_cards`,
//! `replace_goals`), the progress of each team across the cards, when a team has finished and when the round ends,
//! and the stalemate detector with what the stalemate behaviour does. Pure logic, as the rest of `server`: no game
//! types, no packets. The lead composes these from `session.rs`.
//!
//! Sources in orig/common/src/main/java/me/jfenn/bingo/common/ (`scoring/ScoredItemCheck.kt` calls most of it):
//! - win condition: api/.../options/BingoWinCondition.kt, default `Cards(1)` (options/BingoOptions.kt); the
//!   `play_to` commands (commands/BingoOptionsCommands.kt) take a count of at least 1.
//! - card completed: `onCardCompleted`. The completion is recorded once per card, a team finishes when
//!   `countCards() >= requiredCards`, and a card completed within 3 ticks ends the game (`ticks <= 3`).
//! - goal reached: `updateScores` completes the first team on the card whose score meets the goal (then `break`),
//!   and skips the other teams on the card when `endGameWhen` allows: FirstWin always, TeamsWin(n) once n teams have
//!   completed the card, AllWin and Never not.
//! - round end: `postTickCards`, `endGameWhen` over the finished teams. Pending teams get their next card, or a new one.
//! - stalemate: scoring/ScoreService.kt `isStalemate`, `getLeading` and `StalemateReason`, applied by `updateScores`.
//! - stalemate behaviour: api/.../options/StalemateBehavior.kt (`END_GAME`, `REROLL_CARD`, `NOTHING`), default END_GAME.
//!
//! Comparisons are the original's raw `>=` on the goal's numbers: a goal of 0 is reached at once. `scoring.rs` treats
//! such a goal as no goal, so the two differ only for `Lines(0)` and `Items(0)`, which the options never produce.
use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::{json, Value};

use super::options::{EndWhen, Goal, Mode, Options};
use super::scoring::count_lines;
use super::types::{Card, TeamKey, TeamProgress, CARD_SQUARES};
use crate::score_update::TeamScore;

/// `BingoWinCondition` (`play_to`): a team completes `Cards(n)` cards to win, `Infinite` never finishes by cards,
/// `ReplaceGoals` finishes on the first card (its goals are replaced as they are scored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinCondition {
    Cards(u32),
    Infinite,
    ReplaceGoals,
}

impl Default for WinCondition {
    fn default() -> Self {
        WinCondition::Cards(1)
    }
}

impl WinCondition {
    /// `requiredCards`: `Some(n)` for `Cards(n)`, `Some(1)` for `ReplaceGoals`, `None` for `Infinite`.
    pub fn required_cards(self) -> Option<u32> {
        match self {
            WinCondition::Cards(n) => Some(n),
            WinCondition::Infinite => None,
            WinCondition::ReplaceGoals => Some(1),
        }
    }

    /// `{"type":"cards","n":N}`, `{"type":"infinite","n":0}` or `{"type":"replace_goals","n":0}`, as the round
    /// options write `goal` and `endWhen` (`store.rs`).
    pub fn to_json(self) -> Value {
        match self {
            WinCondition::Cards(n) => json!({ "type": "cards", "n": n }),
            WinCondition::Infinite => json!({ "type": "infinite", "n": 0 }),
            WinCondition::ReplaceGoals => json!({ "type": "replace_goals", "n": 0 }),
        }
    }

    /// Reads `to_json`'s form. `None` for an unknown type, or for `cards` whose `n` is missing, not a number, or below 1.
    pub fn from_json(value: &Value) -> Option<WinCondition> {
        let obj = value.as_object()?;
        match obj.get("type")?.as_str()? {
            "cards" => {
                let n = u32::try_from(obj.get("n")?.as_u64()?).ok()?;
                (n >= 1).then_some(WinCondition::Cards(n))
            }
            "infinite" => Some(WinCondition::Infinite),
            "replace_goals" => Some(WinCondition::ReplaceGoals),
            _ => None,
        }
    }
}

/// `StalemateBehavior`: what happens when a card is in a stalemate. `EndGame` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StalemateBehavior {
    EndGame,
    RerollCard,
    Nothing,
}

impl Default for StalemateBehavior {
    fn default() -> Self {
        StalemateBehavior::EndGame
    }
}

impl StalemateBehavior {
    /// The original's enum names in lower case: `"end_game"`, `"reroll_card"`, `"nothing"`.
    pub fn to_json(self) -> Value {
        Value::from(match self {
            StalemateBehavior::EndGame => "end_game",
            StalemateBehavior::RerollCard => "reroll_card",
            StalemateBehavior::Nothing => "nothing",
        })
    }

    pub fn from_json(value: &Value) -> Option<StalemateBehavior> {
        match value.as_str()? {
            "end_game" => Some(StalemateBehavior::EndGame),
            "reroll_card" => Some(StalemateBehavior::RerollCard),
            "nothing" => Some(StalemateBehavior::Nothing),
            _ => None,
        }
    }
}

/// The rules that decide a card's outcome: the round's goal, mode and end rule (`Options`) and the win condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    pub goal: Goal,
    pub mode: Mode,
    pub end_when: EndWhen,
    pub win: WinCondition,
}

impl Rules {
    pub fn new(options: &Options, win: WinCondition) -> Self {
        Rules { goal: options.goal, mode: options.mode, end_when: options.end_when, win }
    }
}

/// Where a team is (`TeamState`): playing the card at this index of the round's card list, between cards
/// (`PendingCard`: the next card when the card chain names one, else a card is made when the team is settled), or
/// finished (`Finished`, with the time it finished).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Playing(usize),
    Pending(Option<usize>),
    Finished { completed_at: u64 },
}

/// One card a team completed or was moved on from (`TeamCompletedCard`). `is_winner` means it counts towards the win
/// condition; a stalemate win carries its reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardCompletion {
    pub card_index: usize,
    pub is_winner: bool,
    pub stalemate: Option<StalemateReason>,
    pub completed_at: u64,
    pub score: TeamScore,
}

/// A team's progress across the round: its state, every card it completed (`completedCards`) and the squares found on
/// the card it plays now (`progress`, empty again once it is assigned a new card).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamRound {
    pub key: TeamKey,
    pub state: CardState,
    pub completed: Vec<CardCompletion>,
    pub progress: TeamProgress,
}

impl TeamRound {
    pub fn new(key: &str, state: CardState) -> Self {
        Self { key: key.to_string(), state, completed: Vec::new(), progress: TeamProgress::default() }
    }

    /// `countCards()`: the completions that count as a win.
    pub fn count_cards(&self) -> u32 {
        self.completed.iter().filter(|c| c.is_winner).count() as u32
    }

    /// The card index the team plays now, if it plays one.
    pub fn current_card(&self) -> Option<usize> {
        match self.state {
            CardState::Playing(card) => Some(card),
            _ => None,
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, CardState::Finished { .. })
    }

    /// The completion record for `card_index`, if the team has one.
    pub fn completion_for(&self, card_index: usize) -> Option<&CardCompletion> {
        self.completed.iter().find(|c| c.card_index == card_index)
    }
}

/// `StalemateReason`. The two `Win` reasons give the card to `team`; the others are a stalemate with no winner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalemateReason {
    /// The leader of the round wins the card: no other team can tie or beat it (`WinByStalemate`).
    WinByStalemate(TeamKey),
    /// Lockout with the whole card claimed, and the leader holds the card (`WinByLockout`).
    WinByLockout(TeamKey),
    /// No team can tie or beat the leader, and nobody leads the round (`Stalemate`).
    Stalemate,
    /// Lockout with the whole card claimed and no leader (`Lockout`).
    Lockout,
    /// No team can reach the goal the round ends on (`ImpossibleGoal`).
    ImpossibleGoal,
}

impl StalemateReason {
    /// The team that wins the card, for the two win reasons.
    pub fn winner(&self) -> Option<&TeamKey> {
        match self {
            StalemateReason::WinByStalemate(team) | StalemateReason::WinByLockout(team) => Some(team),
            _ => None,
        }
    }
}

/// Why the game ends on a stalemate (`GameEndReason.Stalemate`, `.Lockout`, `.ImpossibleGoal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndCause {
    Stalemate,
    Lockout,
    ImpossibleGoal,
}

/// What `stalemate_action` says to do with a stalemate reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalemateAction {
    /// Nothing: there is no stalemate, or the behaviour is `Nothing` for a stalemate with no winner.
    Nothing,
    /// Complete the card for `team` as a win carrying `reason`. The original does not skip the other teams here.
    WinCard { team: TeamKey, reason: StalemateReason },
    /// End the game with this cause (`END_GAME`).
    EndGame(EndCause),
    /// Every team on the card moves on without a win (`REROLL_CARD`), see `reroll_card`.
    RerollCard,
}

/// The behaviour the reroll notice names. The original always says the `END_GAME` text there, whatever the
/// behaviour is, so the notice must use this value and not the setting.
pub const REROLL_NOTICE_BEHAVIOR: StalemateBehavior = StalemateBehavior::EndGame;

/// One completion of a card, as `onCardCompleted` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub card_index: usize,
    pub is_winner: bool,
    pub stalemate: Option<StalemateReason>,
    pub completed_at: u64,
    pub score: TeamScore,
}

/// What `complete_card` leaves the round to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// The team has its win condition's cards. Its state is `Finished`.
    Finished,
    /// The team moves on to the next card (`Pending(next)`).
    Pending(Option<usize>),
    /// The card was completed within 3 ticks: the game ends with an error and is forfeit. The team state is unchanged.
    ImmediateEnd,
}

/// Records a completion of a card and decides the team's next state (`onCardCompleted`). The completion is kept once
/// per card: a second one for the same card (a skip after a win) changes nothing in the record. `card_ticks` is the
/// card's tick count, `next_card` the card the card chain names after this one.
pub fn complete_card(
    team: &mut TeamRound,
    rules: &Rules,
    card_ticks: u64,
    next_card: Option<usize>,
    completion: Completion,
) -> CompletionOutcome {
    if team.completion_for(completion.card_index).is_none() {
        team.completed.push(CardCompletion {
            card_index: completion.card_index,
            is_winner: completion.is_winner,
            stalemate: completion.stalemate,
            completed_at: completion.completed_at,
            score: completion.score,
        });
    }

    if let Some(required) = rules.win.required_cards() {
        if u64::from(team.count_cards()) >= u64::from(required) {
            team.state = CardState::Finished { completed_at: completion.completed_at };
            return CompletionOutcome::Finished;
        }
    }

    if card_ticks <= 3 {
        return CompletionOutcome::ImmediateEnd;
    }
    team.state = CardState::Pending(next_card);
    CompletionOutcome::Pending(next_card)
}

/// The score a team has for `goal` with `cards` cards (`BingoCard.getTeamScore`): the items found, and the lines only
/// for a `Lines` goal (an `Items` goal, `FullCard` included, counts no lines).
pub fn team_score(goal: Goal, progress: &TeamProgress, cards: u32) -> TeamScore {
    let mut have = [false; CARD_SQUARES];
    for &square in &progress.found {
        if square < CARD_SQUARES {
            have[square] = true;
        }
    }
    let items = have.iter().filter(|&&found| found).count() as i32;
    let lines = match goal {
        Goal::Lines(_) => count_lines(&progress.found),
        Goal::Items(_) | Goal::FullCard => 0,
    };
    TeamScore { items, lines, cards: cards as i32 }
}

/// Whether the team's score meets `goal`: `lines >= n` for `Lines(n)`, `items >= n` for `Items(n)`, and 25 items for
/// `FullCard` (`ScoredItemCheck.updateScores`, `team.score.lines >= goal.lines` and `items >= goal.items`).
pub fn reached_goal(goal: Goal, progress: &TeamProgress) -> bool {
    let score = team_score(goal, progress, 0);
    match goal {
        Goal::Lines(n) => i64::from(score.lines) >= i64::from(n),
        Goal::Items(n) => i64::from(score.items) >= i64::from(n),
        Goal::FullCard => i64::from(score.items) >= CARD_SQUARES as i64,
    }
}

/// `BingoGoal.isFullCard`: 25 items or more, or 11 lines or more (`MAX_ITEMS`, `MAX_LINES`).
fn is_full_card(goal: Goal) -> bool {
    match goal {
        Goal::FullCard => true,
        Goal::Items(n) => n >= 25,
        Goal::Lines(n) => n >= 11,
    }
}

/// A team's key in the order of `ScoreService`'s stalemate comparator: lines then items for a `Lines` goal, items
/// alone otherwise.
fn goal_key(goal: Goal, items: i32, lines: i32) -> (i32, i32) {
    match goal {
        Goal::Lines(_) => (lines, items),
        Goal::Items(_) | Goal::FullCard => (items, 0),
    }
}

/// `ScoreService.comparator` (`compareByItems`): finished teams first, the earliest finisher first; then more cards,
/// more lines, more items. The score comes from `team_score`.
pub(super) fn compare_teams(goal: Goal, a: &TeamRound, b: &TeamRound) -> Ordering {
    let finished_at = |t: &TeamRound| match t.state {
        CardState::Finished { completed_at } => completed_at,
        _ => u64::MAX,
    };
    let sa = team_score(goal, &a.progress, a.count_cards());
    let sb = team_score(goal, &b.progress, b.count_cards());
    // Descending by the finish time (`compareByDescending`): an earlier finish compares greater.
    finished_at(b)
        .cmp(&finished_at(a))
        .then(sa.cards.cmp(&sb.cards))
        .then(sa.lines.cmp(&sb.lines))
        .then(sa.items.cmp(&sb.items))
}

/// `ScoreService.getLeading`: the index of the leading team among `teams`, or `None` when the top is tied. With
/// `allow_ties` a tie keeps the last leader instead. Follows the original's loop, including its handling of a tie
/// that is later beaten.
pub fn leading_team(teams: &[TeamRound], goal: Goal, allow_ties: bool) -> Option<usize> {
    let mut ret = None;
    let mut leader: Option<usize> = None;
    for i in 0..teams.len() {
        match leader.map(|l| compare_teams(goal, &teams[i], &teams[l])) {
            None | Some(Ordering::Greater) => {
                ret = Some(i);
                leader = Some(i);
            }
            Some(Ordering::Equal) if !allow_ties => ret = None,
            Some(_) => {}
        }
    }
    ret
}

/// Whether the team completes the card: the first team in `teams` (in the order given) that plays `card_index` and
/// whose score meets the goal. Returns its index.
fn first_at_goal(teams: &[TeamRound], card_index: usize, goal: Goal) -> Option<usize> {
    teams
        .iter()
        .position(|t| t.state == CardState::Playing(card_index) && reached_goal(goal, &t.progress))
}

/// Whether, once a team has completed the card, the other teams still on it move on without a win (the skip in
/// `updateScores`). `completed_teams` is the number of teams with a completion of the card, the winner's included.
pub fn skip_others_on_card(end_when: EndWhen, completed_teams: usize) -> bool {
    match end_when {
        EndWhen::AllWin | EndWhen::Never => false,
        EndWhen::TeamsWin(n) => completed_teams >= n as usize,
        EndWhen::FirstWin => true,
    }
}

/// The result of a team reaching the goal on a card (`updateScores`): the winner's index in `teams`, what its
/// completion did, and the other teams that were moved on without a win.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFinish {
    pub winner: usize,
    pub outcome: CompletionOutcome,
    /// `(team index, outcome)` for each team moved on without a win, in list order.
    pub skipped: Vec<(usize, CompletionOutcome)>,
}

/// `updateScores`, the goal part: completes the first team that plays `card_index` and has reached the goal, then
/// skips the other teams still on the card when `end_when` allows it. The winner's score is the one it had before this
/// card (the original updates the score before completing). `None` when no team has reached the goal.
pub fn finish_card_on_goal(
    teams: &mut [TeamRound],
    rules: &Rules,
    card_index: usize,
    card_ticks: u64,
    next_card: Option<usize>,
    completed_at: u64,
) -> Option<CardFinish> {
    let winner = first_at_goal(teams, card_index, rules.goal)?;
    let winner_score = team_score(rules.goal, &teams[winner].progress, teams[winner].count_cards());
    let outcome = complete_card(
        &mut teams[winner],
        rules,
        card_ticks,
        next_card,
        Completion { card_index, is_winner: true, stalemate: None, completed_at, score: winner_score },
    );

    let completed_teams = teams.iter().filter(|t| t.completion_for(card_index).is_some()).count();
    let mut skipped = Vec::new();
    if skip_others_on_card(rules.end_when, completed_teams) {
        for i in 0..teams.len() {
            if teams[i].state == CardState::Playing(card_index) {
                let score = team_score(rules.goal, &teams[i].progress, teams[i].count_cards());
                let outcome = complete_card(
                    &mut teams[i],
                    rules,
                    card_ticks,
                    next_card,
                    Completion { card_index, is_winner: false, stalemate: None, completed_at, score },
                );
                skipped.push((i, outcome));
            }
        }
    }
    Some(CardFinish { winner, outcome, skipped })
}

/// `REROLL_CARD`: every team on `card_index` is completed without a win and moves on. Returns `(team index, outcome)`
/// for each, in list order.
pub fn reroll_card(
    teams: &mut [TeamRound],
    rules: &Rules,
    card_index: usize,
    card_ticks: u64,
    next_card: Option<usize>,
    completed_at: u64,
) -> Vec<(usize, CompletionOutcome)> {
    let mut outcomes = Vec::new();
    for i in 0..teams.len() {
        if teams[i].state == CardState::Playing(card_index) {
            let score = team_score(rules.goal, &teams[i].progress, teams[i].count_cards());
            let outcome = complete_card(
                &mut teams[i],
                rules,
                card_ticks,
                next_card,
                Completion { card_index, is_winner: false, stalemate: None, completed_at, score },
            );
            outcomes.push((i, outcome));
        }
    }
    outcomes
}

/// `postTickCards`: whether the round ends now, given every registered team. `Never` never ends it; `FirstWin` when a
/// team has finished; `TeamsWin(n)` when `n` teams have; `AllWin` when every team has. `AllWin` with no teams is true,
/// as the original's `teams.all {}` is.
pub fn round_ends(end_when: EndWhen, teams: &[TeamRound]) -> bool {
    let finished = teams.iter().filter(|t| t.is_finished()).count();
    match end_when {
        EndWhen::Never => false,
        EndWhen::FirstWin => finished >= 1,
        EndWhen::TeamsWin(n) => finished >= n as usize,
        EndWhen::AllWin => finished == teams.len(),
    }
}

fn assign(team: &mut TeamRound, card: usize) {
    team.state = CardState::Playing(card);
    team.progress = TeamProgress::default();
}

/// `postTickCards`, the pending teams: a team pending on a card that exists (`next < card_count`) plays it. The others
/// all get one card made by `make_card` (it returns the new card's index), so `make_card` runs only when a team needs
/// one. Returns whether a card was made. Each team that gets a card starts with empty progress.
pub fn settle_pending(teams: &mut [TeamRound], card_count: usize, make_card: impl FnOnce() -> usize) -> bool {
    let mut needs_card = Vec::new();
    for (i, team) in teams.iter_mut().enumerate() {
        if let CardState::Pending(next) = team.state {
            match next {
                Some(card) if card < card_count => assign(team, card),
                _ => needs_card.push(i),
            }
        }
    }
    if needs_card.is_empty() {
        return false;
    }
    let card = make_card();
    for i in needs_card {
        assign(&mut teams[i], card);
    }
    true
}

/// Whether a square is possible for a team (`isPossible`): it has the square, or the square is open. In lockout mode
/// (outside inventory mode, which lets a team lose squares) a square any team has claimed is no longer possible for
/// the others. `owners` are the lockout owners of the card.
fn possible(team: &TeamRound, square: usize, rules: &Rules, claimed: impl Fn(usize) -> bool) -> bool {
    if team.progress.found.contains(&square) {
        return true;
    }
    let locked = rules.mode == Mode::Lockout && claimed(square);
    !locked
}

/// `ScoreService.isStalemate` for the card at `card_index`, with its squares `card`. `teams` are every registered team:
/// the round's leader is taken over all of them, as the original's `getLeading(state)` is, and must be on this card.
/// The teams on the card are those in `Playing(card_index)`. `owners` maps each square of this card to the team that
/// holds it in lockout mode, and `playing` is whether the game is running (a stalemate is only checked then).
///
/// Order of the checks, as the original's: no teams on the card, or `ReplaceGoals`: none. Lockout with the whole card
/// claimed: `WinByLockout` for the leader, else `Lockout`. Otherwise a stalemate is when there are other teams and none
/// of them can tie or beat the top team (a single team is never a stalemate). The goal is impossible when too few
/// teams can reach it (a full card in lockout with several teams is not). An impossible goal wins over a stalemate.
/// A stalemate with a leader on the card is `WinByStalemate`, unless the goal is impossible.
pub fn stalemate_reason(
    card: &Card,
    card_index: usize,
    teams: &[TeamRound],
    owners: &HashMap<usize, TeamKey>,
    rules: &Rules,
    playing: bool,
) -> Option<StalemateReason> {
    if !playing || rules.win == WinCondition::ReplaceGoals {
        return None;
    }
    let on_card: Vec<usize> = (0..teams.len()).filter(|&i| teams[i].state == CardState::Playing(card_index)).collect();
    if on_card.is_empty() {
        return None;
    }
    let goal = rules.goal;
    let leader = leading_team(teams, goal, false).filter(|&l| teams[l].state == CardState::Playing(card_index));

    let claimed = |square: usize| {
        owners.contains_key(&square) || on_card.iter().any(|&t| teams[t].progress.found.contains(&square))
    };
    let squares = card.squares.len();
    if rules.mode == Mode::Lockout && (0..squares).all(|s| claimed(s)) {
        return Some(match leader {
            Some(l) => StalemateReason::WinByLockout(teams[l].key.clone()),
            None => StalemateReason::Lockout,
        });
    }

    let current = |t: usize| {
        let s = team_score(goal, &teams[t].progress, 0);
        goal_key(goal, s.items, s.lines)
    };
    let Some(tie_leader) = on_card
        .iter()
        .copied()
        .reduce(|best, t| if current(best) < current(t) { t } else { best })
    else {
        return None;
    };

    let possible_squares = |t: usize| -> Vec<usize> {
        (0..squares).filter(|&sq| possible(&teams[t], sq, rules, |s| claimed(s))).collect()
    };
    let possible_items = |t: usize| possible_squares(t).len() as i32;
    let possible_lines = |t: usize| count_lines(&possible_squares(t));
    let possible_key = |t: usize| goal_key(goal, possible_items(t), possible_lines(t));

    let leader_key = current(tie_leader);
    let others: Vec<usize> = on_card.iter().copied().filter(|&t| t != tie_leader).collect();
    // A team can still beat or tie the leader when its possible score is above its own and not below the leader's.
    let is_stalemate = !others.is_empty()
        && others.iter().all(|&t| {
            let p = possible_key(t);
            !(p > current(t) && p >= leader_key)
        });

    let reaches = |t: usize| match goal {
        Goal::Lines(n) => i64::from(possible_lines(t)) >= i64::from(n),
        Goal::Items(n) => i64::from(possible_items(t)) >= i64::from(n),
        Goal::FullCard => i64::from(possible_items(t)) >= CARD_SQUARES as i64,
    };
    let impossible = if is_full_card(goal) && rules.mode == Mode::Lockout && on_card.len() > 1 {
        false
    } else {
        let required = match rules.end_when {
            EndWhen::Never | EndWhen::FirstWin => 1,
            EndWhen::TeamsWin(n) => n as usize,
            EndWhen::AllWin => on_card.len(),
        };
        on_card.iter().filter(|&&t| reaches(t)).count() < required
    };

    if is_stalemate && !impossible {
        if let Some(l) = leader {
            return Some(StalemateReason::WinByStalemate(teams[l].key.clone()));
        }
    }
    if impossible {
        return Some(StalemateReason::ImpossibleGoal);
    }
    if is_stalemate {
        return Some(StalemateReason::Stalemate);
    }
    None
}

/// What to do with a stalemate reason under a behaviour (`updateScores`, after the card's scores are final). A win
/// reason always completes the card for its team; the other reasons follow the behaviour.
pub fn stalemate_action(reason: Option<&StalemateReason>, behavior: StalemateBehavior) -> StalemateAction {
    let Some(reason) = reason else {
        return StalemateAction::Nothing;
    };
    if let Some(team) = reason.winner() {
        return StalemateAction::WinCard { team: team.clone(), reason: reason.clone() };
    }
    let cause = match reason {
        StalemateReason::Lockout => EndCause::Lockout,
        StalemateReason::ImpossibleGoal => EndCause::ImpossibleGoal,
        _ => EndCause::Stalemate,
    };
    match behavior {
        StalemateBehavior::EndGame => StalemateAction::EndGame(cause),
        StalemateBehavior::RerollCard => StalemateAction::RerollCard,
        StalemateBehavior::Nothing => StalemateAction::Nothing,
    }
}

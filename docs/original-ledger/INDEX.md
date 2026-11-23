# Original ledger: index

One ledger per slice of the original mod (Yet Another Minecraft BINGO 2.15.1). Rows are features; status is checked against this port by grep, not by running it. Counts below are rows by status.

| Ledger | DONE | PARTIAL | MISSING | UNSURE |
|---|---|---|---|---|
| 01 card (A) | 14 | 19 | 25 | 7 |
| 02 card (B) | 12 | 11 | 15 | 2 |
| 03 menu (A) | 1 | 40 | 16 | 2 |
| 04 menu (B) | 3 | 22 | 13 | 1 |
| 05 map | 18 | 12 | 6 | 17 |
| 06 game flow and end | 15 | 21 | 11 | 5 |
| 07 spawn | 1 | 17 | 23 | 0 |
| 08 scoring | 7 | 14 | 8 | 4 |
| 09 stats | 14 | 11 | 12 | 1 |
| 10 config and options | 29 | 42 | 29 | 18 |
| 11 commands and controllers | 8 | 27 | 24 | 2 |
| 12 ready, team, reset | 15 | 30 | 15 | 4 |
| 13 data, state, scoreboard | 23 | 20 | 16 | 6 |
| 14 lobby, timer, misc | 16 | 27 | 21 | 6 |
| 15 client HUD | 36 | 16 | 8 | 3 |
| 16 client screens | 48 | 8 | 3 | 3 |
| 17 client rest | 25 | 7 | 5 | 2 |
| 18 mixins and platform glue | 14 | 46 | 29 | 19 |
| 19 resources | 14 | 9 | 11 | 2 |
| 20 docs and feature list | 36 | 14 | 13 | 8 |

The client (15 to 17) is closest to the original; the in-world lobby menu (03, 04), spawn (07), mixin effects (18), commands (11) and options (10) are the furthest.

## Reported by the player, and where it is

1. The lobby menu looks completely different. The original is a grid of small tiles in five columns (Game Mode, Goal, Difficulty, Shufflificator, Best time and Players); see 03, 04 and the screenshot orig-runA/shots/sceneA/01-panel-closeup.png. Not fixed: needs a rebuild of the panel.
2. The end of the game has not the same effect. See 06 (sounds, chat block, end screen with Next Round, More, Close, DNF, Scores tab) and orig-runB/shots/sceneB. Work in progress on branch fix-endgame.
3. Hovering a card item after a click shows no tooltip. See 16. Branch fix-hover fixes the tooltip's offset and colours; the cause of the missing tooltip after a click is not found.

## Tests written from the original (dev/tests, hand-traced, ignored = real difference)

original_scoring.rs, original_card.rs, original_teams_ready.rs, original_game_flow.rs, original_options_commands.rs. Fix branches: fix-cardfix, fix-teams, fix-options, fix-endgame, fix-hover.

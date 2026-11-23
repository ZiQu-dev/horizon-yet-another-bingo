# Ledger brief (read this fully)

We port the Fabric mod "Yet Another Minecraft BINGO" 2.15.1 (Minecraft 26.3, Kotlin/Java) to Horizon as a native Rust mod.
The original source is read only, at /home/mateusz/bingo-horizon-port/orig (common/src/main/java/me/jfenn/bingo/..., common/src/client/..., mc26.3/, platform/, resources in common/src/main/resources). Our port is in /home/mateusz/horizon-yet-another-bingo (bingo-native/src/horizon-bingo/src/ pure logic, bingo-native/src/*.rs adapter, dev/tests tests). The player says our port looks and behaves differently from the original in places (menu look, game-over effect, hovering an item on the card after clicking), so we are building a LEDGER of everything the original does, then closing the gaps step by step, the way the Horizon project keeps a port ledger.

Your job: read EVERY file of your slice completely (not just names; open them), then write ONE markdown file (path given in your task) with this structure and nothing else:

## Slice: <name>  (files read: N, lines: N)
### Features
A table, one row per observable feature or rule, columns:
| id | what the player sees, hears or can do (concrete: texts, colours, numbers, sounds, timings, keys) | original file:function (line) | port status | port place (file:function or "none") | how to verify |
- id: <slice>-01, -02, ...
- port status: DONE (same behaviour), PARTIAL (say what differs), MISSING, or N/A (platform glue the port needs not copy; say why). Decide by grepping the port repo (rg/grep for the function, key, message text, sound name, constant). Do not guess: if you did not find it, say MISSING; if unsure, say UNSURE and what you checked.
- how to verify: either `unit: <what to assert with independent expected values taken from the original's code>` or `live: <headless steps through horizon-ctl/commands and what to look at>` or `visual: compare screenshot of original vs port of <thing>`.
### Constants and data worth copying exactly
Numbers, strings, colour codes, tables, formulas the port must match (with the original file:line).
### Parity risks
The top things in this slice most likely to look or behave differently in the port, ordered by how visible they are to a player.

Rules: read only; the only file you write is your ledger file (use the shell to write it, e.g. a heredoc). Do not edit code, do not run builds, do not start games, do not use pkill or kill. Keep the ledger under 300 lines, dense, no filler, English, no emoji. If your slice is large, group rows but do not skip files. When finished reply with ONE line: the ledger path and counts (rows, MISSING, PARTIAL).

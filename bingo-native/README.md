# Bingo (native mod for Horizon)

`bingo_native` 0.0.1: the **client half** of Yet Another Minecraft BINGO, for Horizon, as a native mod (`.hnmod`).
It is a port of the upstream client code and its packet formats. Upstream project: https://gitlab.com/horrificdev/bingo
(LGPL-3.0). See [NOTICE.md](NOTICE.md) for the licence position, which is not yet decided.

## What it is, and what it is not

- It does **not** run the game logic. The server does: the bingo lobby, teams, the ready timer, scoring and the
  win condition all run on a server with the **original Fabric mod** installed. This mod only draws what the server
  sends and sends the player's choices back.
- Without a server running the original mod, it shows nothing game-related.
- It works only in Horizon, built by the Horizon launcher. It is not a Fabric mod and does not load the jar.
- The mod ships the original's language files (`assets/bingo/lang/`). Its other upstream assets you generate from your own copy of the original mod (see Install).

### What it does (from the source; not played in a live game for this package)

- **Card HUD**: your team's card under the HUD: 25 tiles with item icons, banners, tile pictures, team name.
- **Card screen** (key `Y` by default): tabs for CARDS and SCORES, card selection, arrow keys and wheel, tile
  tooltips, the message history sidebar.
- **End screen**: the scores tab with the score list and game info, and the win fireworks on the first open of a won
  game.
- **Score messages** on the HUD, with the message history, and the ready line and countdown.
- **Shuffle animation** when the server reshuffles a card.
- **Entity tooltips** from the server's `tooltip_v2` packets (`hud_tooltip.rs`).
- **Player heads** in messages, scores and the game-over text.
- **Sounds**: the eight upstream sounds, played through the `bingo_assets` content mod.
- **Settings screen** (key unbound by default, bind it under Options > Mods): quick start button, HUD on or off, card
  overlap, card scale, alignment and offsets, message duration and scale, which teams' messages show, hide on F3 and
  on chat, and sound volumes with a test button for each sound.
- **HUD toggle** (key unbound by default).
- **Sneak with the card key** readies up when the server allows it (`hud_keys.rs`).
- Settings are sent to the server when you join. The client state is reset on connect and disconnect.

### What it does not do

- **Card position screen does not open.** The settings screen's "card position" button asks for a placement screen,
  but nothing opens it, so drag and resize of the card is not available. Card alignment and offsets in the settings
  screen do work.
- **JEI, REI or EMI recipe click** on the card: not implemented.
- **Item stacks with other data components** are refused: only the 30 components listed in `item.rs` are read. A
  packet carrying another component (a player head's `profile`, for one) is dropped. Not checked per packet.
- **Ready packet with a title, subtitle or vote option** is refused (`ReadyUpdatePacket`).
- **Obfuscated text** is drawn as plain text; the host has no random-glyph draw.
- **Scroll bars** use plain boxes: the vanilla scroll bar sprites are not in the mod's atlas.
- **Long button labels** are drawn from the left and do not scroll as vanilla's do.
- **Server-side features**: no lobby, no `/bingo` or `/join` commands, no reroll, no game rules. Those are the server's.
- **Language** is read once per game start, so a language change needs a restart.
- **Not played**: this package was built and unit-tested, not played in a live game. Looks are checked by eye in the
  development notes, not by tests.

## Requirements

- Horizon built from commit `7b59041a89c3f1bc8e458f43d6b8ec48a0f814d3` (pinned in `horizon-mixin.toml`). The launcher
  refuses a build on another commit unless you pass `--allow-version-mismatch`; that was not tried.
- Linux: `cc`, `pkg-config`, ALSA and libudev development files (on Debian and Ubuntu:
  `sudo apt install build-essential pkg-config libasound2-dev libudev-dev`; see `docs/native-mods.md` in the Horizon
  repository). Rust is not needed: the launcher downloads its own toolchain.
- Python 3 (standard library only) for `prepare_assets.py`.
- The original mod's jar for Minecraft 26.3. Source version: 2.15.1 (`gradle.properties` of the upstream). Download it
  yourself from Modrinth; it is not in this repository.
- To play multiplayer bingo: a server running the original mod. A client without it shows no bingo content.

## Install

### 1. Get the original mod jar

Download "Yet Another Minecraft BINGO" for Minecraft 26.3 from Modrinth yourself. Do not share the jar or its files.
The Modrinth page URL was not checked from this package.

### 2. Prepare the assets

Run the script from the development folder (`exp-skeleton/tools/prepare_assets.py`; it is not in the mod bundle):

```sh
python3 -I prepare_assets.py <jar> <assets dir> \
    --registries <resources>/command-registries.json \
    --hmod <game dir>/horizon-mods/bingo_assets.hmod
```

- `<jar>`: the downloaded jar. It is only read.
- `<assets dir>`: the mod's data folder for Horizon, created by you if missing:
  `<game dir>/horizon-mods/.horizon-mod-data/bingo_native/assets`. The mod reads it from there by default.
- `--registries`: `command-registries.json` from Horizon's prepared resources (the folder the game's `--resources`
  option points at, e.g. `.local/horizon-client/26.3` in a source checkout). The script reads the `items` list from it,
  and takes vanilla's languages from `assets/minecraft/lang/` beside it. **Not verified:** that the file sits in that
  folder for a launcher install; this checkout has no prepared resources.
- `--hmod`: optional, but needed for sounds. Writes the content mod `bingo_assets.hmod`.
- `<game dir>`: the instance folder, the one that holds `options.txt`.

The script writes sprites, `card_colors.json`, the languages, `item_names.txt` and vanilla's languages into the assets
dir. `python3 -I prepare_assets.py --help` lists every option.

### 3. Turn on the sound mod

The content mod `bingo_assets` (from `--hmod`) carries the sounds. Turn it on either in Options > Mods (its switch
applies at once: it only has assets), or by adding this line to `<game dir>/horizon-mods.txt`:

```
mod.bingo_assets:true
```

Without it the mod still works; its sounds are silent (the log says `no such sound event` when `BINGO_H0_LOG` is set).

### 4. Build and install the mod

```sh
horizon-launcher native-build <path to bingo-native> --instance <instance name> --trust-native-code --profile performance
```

- `--trust-native-code` is required. Native code runs with full access to the computer; read the source first.
- `--profile performance` is the player's build. `--profile dev` builds about 6 times faster and runs game logic up to
  about twice as slow: use it while developing only.
- The first build downloads Rust and can take 20 to 30 minutes.
- **Not verified:** whether a build from a folder also copies the mod into the instance's `horizon-native-mods/` and
  turns it on. Check after the build: the instance's **Native build** row on the Mods page should say "Up to date",
  and Options > Mods should list the Bingo mod with its keys.

### 5. Join a server running the mod

Join the server as usual. The mod registers its plugin channels with the server by itself (`minecraft:register` in
the play phase), so no server change is needed on the client side. **Not verified in a live game for this package.**

## Keys

Mod keys are bound per mod on its page in **Options > Mods**, not in Options > Controls (the brief said Controls; the
source shows Mods).

| Key | Default | Action |
|---|---|---|
| Open card | `Y` | Opens the card screen (sneak with it to ready up) |
| Toggle HUD | unbound | Shows or hides the card HUD |
| Settings | unbound | Opens the settings screen |

## Configuration

File: `<game dir>/config/bingo/client.json`. The mod writes it when a setting changes. Comments with `//` are accepted.
Out-of-range numbers are clamped to their range on load, a bad value takes its default, and unknown keys are ignored.

| Field | Default | Values |
|---|---|---|
| `enableHud` | `true` | bool |
| `showQuickStartButton` | `true` | bool |
| `cardPausesGame` | `true` | bool |
| `cardScale` | `1.0` | 0.25 to 4 |
| `cardAlignment` | `TOP_LEFT` | `TOP_LEFT`, `TOP_RIGHT`, `BOTTOM_LEFT`, `BOTTOM_RIGHT` |
| `cardOffsetX` | `0` | 0 or more; for a right-hand alignment it moves the card inwards |
| `cardOffsetY` | `0` | 0 or more; for a bottom alignment it moves the card inwards |
| `cardOverlap` | `ABOVE` | `ABOVE`, `UNDERNEATH` |
| `cardTeamOutlines` | `true` | bool |
| `showMultipleCards` | `false` | bool |
| `hideOnF3` | `true` | bool |
| `hideOnChat` | `false` | bool |
| `messageFromOtherTeams` | `true` | bool |
| `messageDurationSeconds` | `5` | 1 to 30 |
| `messageScale` | `1.0` | 0.25 to 4 |
| `soundVolumes` | `{}` | object: sound name to volume 0 to 1.5; a missing sound plays at 0.8 |

Sound names: `item_scored`, `item_lost`, `opponent_scored`, `game_won`, `game_lost`, `timer_tick`,
`start_countdown`, `start_release`.

The HUD toggle key keeps its own value as a file named `enable_hud` in the mod's data folder, separate from
`enableHud`. How the two interact was not checked.

## The title screen's BINGO! button (feature `world-request`)

The original mod adds a "BINGO!" button to the main menu, which opens the bingo lobby world. This package has it
behind the Cargo feature `world-request` (off by default). The feature needs a Horizon that has
`host.request_world(folder)`, which is **Horizon PR horizon-rust/horizon#17246** (API 646, branch
`native-request-world`, fork ZiQu-dev/horizon). The released Horizon does not have it, so the default build has no
button and the released bundle is unchanged.

- The button sits on the title's main page, right of its Singleplayer button, and says "BINGO!" (translated when the
  original's language files have the key `bingo.title.button`). A press asks Horizon to open `saves/bingo`.
- The lobby world's folder is `LOBBY_FOLDER` in `horizon-bingo/src/title_button.rs` ("bingo"). Create a world with
  that folder name first: a missing one shows a red hint under the button for a few seconds.
- The answers are mapped in `api/src/title_button.rs`; the hint wording is in `horizon-bingo/src/title_button.rs`.
  `NotTitle` and `Busy` show nothing. `NotPermitted` means the manifest lacks `"world_control": true`.
- Pure logic (the hints, the folder rule, the button's place, the hit test) is in `dev/tests/title_button.rs`.

Build it: the manifest is the one thing the feature changes. `world_control` is declared in `horizon-native.json`, the
manifest of an unpacked `.hnmod`, not in `horizon-mixin.toml`, so the feature build is laid out with the tool:

```sh
python3 -I tools/pack_hnmod.py --horizon <commit with request_world> --out <folder> --world-request
horizon-launcher native-build <folder> --instance <instance> --trust-native-code
```

`tools/pack_hnmod.py` writes `horizon-native.json` (`"hooks": 646` and `"world_control": true`) and a copy of this
folder under `mixins/bingo_native/` whose `horizon_revision` is the commit, with `world-request` on by default in the
copy's `api/Cargo.toml` (the launcher passes no features of the mod's own). The repository files stay as they are.
Use `--without-hooks` only for a Horizon commit that is not on GitHub: the launcher checks `"hooks"` there, and a
local test build cannot pass that check. The Horizon used must contain the helper: a launcher build against a
Horizon without it fails to compile the `world-request` code.

The live check of this route is in `phase4/live-title-button/REPORT.md` of the development notes, not in the repository.

## Troubleshooting

- **A line `assets: <reason>` in the top-left corner.** The assets did not load. The reason names the file: no
  `sprites` folder (run `prepare_assets.py` again), a missing or invalid `card_colors.json`, `item_names.txt` or
  `lang/en_us.json`. Run the script with the same assets dir.
- **Language is English.** Check the `lang:` line in `options.txt`. Language files must exist as
  `<code>.json` in `vanilla_lang/` or `lang/`. The language is read once when the game starts: restart after a change.
- **No sounds.** Turn on `bingo_assets` (Install, step 3).
- **A key does nothing.** Check the key on the mod's page under Options > Mods. The toggle and settings keys are unbound
  until you bind them.
- **Card position button does nothing.** Known; see "What it does not do".
- **The build stops with a version mismatch.** The Horizon you build on is not the pinned commit. Use a build from
  `7b59041a89c3f1bc8e458f43d6b8ec48a0f814d3`, or try `--allow-version-mismatch` (not tried).
- **Build fails on Linux with a missing library.** Install the packages named under Requirements.

## Environment variables (debugging)

Set them in the game's environment. None is needed to play.

- `BINGO_ASSETS`: a folder to read the assets from, instead of the mod's data folder. It must hold the same layout
  `prepare_assets.py` writes.
- `BINGO_H0_LOG`: a file to append debug lines to, for example `assets from ...`, the card key, and each sound
  (`played` or `no such sound event`). Off unless set.
- `BINGO_H0_DUMP`: a folder to write the first three payloads of each server channel as hex, to `<channel>.<n>.hex`.
  The name `H0` is the internal name of the debug hook. Off unless set; it writes files, so use a scratch folder.

## Server

The bundle's server half (`api/src/server.rs`) runs wherever the mod is installed: install the same bundle on the server.

- **Item names.** The server needs `item_names.txt` in the mod's assets folder, `<game dir>/horizon-mods/.horizon-mod-data/bingo_native/assets/`, or in the folder named by `BINGO_ASSETS`. `tools/prepare_assets.py` writes it from your own copy of the original mod jar (Install, step 2). Without it the server still runs, but cards have no item icons. The server log then gets one WARNING line, at the first tick, with the path it searched, and `bingo status` and `bingo start` add a note saying so.
- **Launcher.** Launcher version 0.2.6x or newer.
- **Clients.** The original Fabric mod 2.15.1 clients work too: a real Fabric client with the original mod joined this server and played a full game (card with icons, score messages, end screen) in a live test.
- **Commands.** Players and the console type `bingo <command>`. `join <team>`, `ready` and `spectator` also work without `bingo`. Commands that change a game need operator level 2: `start`, `end`, `reset`, `shuffleteams`, `ready cancel`, `timelimit`, `goal`, `mode`, `endwhen`, `pvp`, `keepinventory`, `elytra`, `nightvision`, `spawndistance`, `playto`, `stalemate` and `resume`. `bingo help` lists the usages:

  ```
  start [ignore_warnings] | end | reset | shuffleteams <n> | join <team> | spectator | ready [cancel] | timelimit <minutes|off> | card | status | goal lines <n>|items <n>|full_card | mode standard|inventory|lockout|hidden_items on|off|consume_items on|off | endwhen never|first_win|all_win|teams_win <n> | pvp on|off | keepinventory on|off | elytra on|off | nightvision on|off | spawndistance <chunks> | playto cards <n>|infinite|replace_goals | stalemate end_game|reroll_card|do_nothing | options | vote next|keep|lobby | resume | stats
  ```

- **Votes.** When a game ends (Postgame), `bingo vote next|keep|lobby` votes. A plain chat line `vote next`, `vote keep` or `vote lobby` (no `bingo`, any letter case) is the same vote and is not shown to the other players. The vote is carried out once every online player who plays has voted.
- **Files.** In the mod's data folder, `<game dir>/horizon-mods/.horizon-mod-data/bingo_native/`:
  - `options.json`: the round options, written on each change.
  - `player-settings.json`: the players' settings, written when they change.
  - `history.jsonl`: one line per finished game, appended when the game ends.
  - `game-state.json`: the game in progress, rewritten as it goes on. It is emptied when a game ends. A server that restarts during a game restores it; a game in Postgame is not restored.

## Tests

The full suite lives in the development folder `exp-skeleton/`, which holds the oracle rows and the recorded
captures. From there:

```sh
cd exp-skeleton
cargo test --locked --offline
```

At the time of writing this passed: 599 tests in 53 test binaries (run with `CARGO_TARGET_DIR` set outside the
repository). The bundle's own copy of the crate (`horizon-bingo/`) has only its unit tests in `src/` (41 tests, not
run from the bundle). Do not run `cargo` inside the bundle: it writes a `Cargo.lock` into it.

## Layout

- `horizon-mixin.toml`: the bundle: id, version, pinned Horizon commit, and the trait-mod package (`api/`).
- `tools/pack_hnmod.py` (repository root): lays out the unpacked `.hnmod` with its manifest, for the feature build.
- `api/src/lib.rs`: the wiring to Horizon's hooks, environment variables, asset and language loading.
- `horizon-bingo/src/`: the client logic, wire codecs, drawing and screens. Nothing here imports Horizon; the
  host adapter is `api/src/lib.rs`.

## Licence

Copyright (C) 2026 Mateusz Korniak (ZiQu-dev). Written with the help of an AI assistant (Claude, Anthropic).
LGPL-3.0-or-later, as a port of the upstream mod (see `LICENSE`, `COPYING` and `NOTICE.md`). No upstream assets are included, except the language files in `assets/bingo/lang/`.

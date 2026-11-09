# Yet Another Bingo for Horizon (native mod)

`bingo_native` 0.0.1: the **client half** of Yet Another Minecraft BINGO, for Horizon, as a native mod (`.hnmod`).
It is a port of the upstream client code and its packet formats. Upstream project: https://gitlab.com/horrificdev/bingo
(LGPL-3.0). Licensed LGPL-3.0-or-later; see [NOTICE.md](NOTICE.md).

## What it is, and what it is not

- It does **not** run the game logic. The server does: the bingo lobby, teams, the ready timer, scoring and the
  win condition all run on a server with the **original Fabric mod** installed. This mod only draws what the server
  sends and sends the player's choices back.
- Without a server running the original mod, it shows nothing game-related.
- It works only in Horizon, built by the Horizon launcher. It is not a Fabric mod and does not load the jar.
- The mod ships no upstream assets. You generate them from your own copy of the original mod (see Install).

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

Run the script from the development folder (`tools/prepare_assets.py` in this repository; it is not in the mod bundle):

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
horizon-launcher native-build <path to the bingo-native folder of this repository> --instance <instance name> --trust-native-code --profile performance
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

### macOS

Reported from a MacBook (Apple Silicon, Horizon.app in `/Applications`). **Not verified: nobody has built this on macOS yet.**

- Game folder: `~/Library/Application Support/horizon` (holds `options.txt`, `horizon-mods.txt`, `horizon-mods/`,
  `horizon-native-mods/`).
- Launcher data: `~/Library/Application Support/horizon-launcher`. Find the resources file with
  `REG=$(find "$HOME/Library/Application Support/horizon-launcher" -name command-registries.json | head -1)`.
- Native builds need Apple's Command Line Tools: `xcode-select --install`. The launcher asks for them too.
- The launcher is inside the app: `/Applications/Horizon.app/Contents/MacOS/`. Check its file name with `ls`.

```sh
GAME="$HOME/Library/Application Support/horizon"
LAUNCHER="/Applications/Horizon.app/Contents/MacOS/<launcher name from ls>"

# 2. Prepare the assets (run from the repository's tools folder)
cd ~/horizon-yet-another-bingo/tools
python3 -I prepare_assets.py ~/Downloads/bingo-2.15.1+mc26.3.jar "$GAME/horizon-mods/.horizon-mod-data/bingo_native/assets" \
    --registries "$REG" --hmod "$GAME/horizon-mods/bingo_assets.hmod"

# 3. Turn on the sound mod (append, do not overwrite)
echo "mod.bingo_assets:true" >> "$GAME/horizon-mods.txt"

# 4. Build and install the mod
"$LAUNCHER" native-build ~/horizon-yet-another-bingo/bingo-native --instance horizon --trust-native-code --profile performance
```

The steps and options are the same as above. Check the build as in step 4.

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

## Tests

The integration tests (579 at the time of writing) and the recorded captures and oracle rows live in `dev/`. From the
repository root:

```sh
cd dev
cargo test --locked --offline    # offline needs the crates (png, serde_json) already in the cargo cache
```

Run it with `CARGO_TARGET_DIR` set outside the repository. The crate itself also has unit tests in `src/`. Do not run
`cargo` inside `bingo-native/`: it writes a `Cargo.lock` into the bundle. `dev/DESIGN.md` is the design and status
document.

## Layout

- `bingo-native/`: the mod bundle you build. `horizon-mixin.toml` (id, version, pinned Horizon commit, trait-mod
  package `api/`), `api/src/lib.rs` (the wiring to Horizon's hooks, environment variables, asset and language loading)
  and `horizon-bingo/src/` (the client logic, wire codecs, drawing and screens; nothing there imports Horizon).
- `tools/prepare_assets.py`: makes the assets from your own copy of the original mod jar.
- `dev/`: the tests, their recorded data and the design notes.

## Licence

Copyright (C) 2026 Mateusz Korniak (ZiQu-dev). Written with the help of an AI assistant (Claude, Anthropic).
LGPL-3.0-or-later, as a port of the upstream mod (see `LICENSE`, `COPYING` and `NOTICE.md`). No upstream assets are included.

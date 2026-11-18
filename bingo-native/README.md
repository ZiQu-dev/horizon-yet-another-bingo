# Bingo (native mod for Horizon)

`bingo` 0.0.1: Yet Another Minecraft BINGO for Horizon, as a native mod (`.hnmod`). It is a port of the upstream client
code, its packet formats and its server logic. Upstream project: https://gitlab.com/horrificdev/bingo (LGPL-3.0).
See [NOTICE.md](NOTICE.md) for the licence position, which is not yet decided.

## What it is, and what it is not

- On a client, it does **not** run the game logic. A server does: the bingo lobby, teams, the ready timer, scoring and
  the win condition run on a server that has either this bundle's server half (`src/server.rs`, see Server) or the
  **original Fabric mod** installed. The client half only draws what the server sends and sends the player's choices back.
- Without a server running this bundle or the original mod, it shows nothing game-related.
- It works only in Horizon, built by the Horizon launcher. It is not a Fabric mod and does not load the jar.
- The mod ships the original's language files (`assets/bingo/lang/`). Its other upstream assets you generate from your
  own copy of the original mod (see Install).

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
- **Settings screen** (key unbound by default, bind it under Options > Controls): quick start button, HUD on or off, card
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
- **Server-side features** are not in the client half: no lobby, no `/bingo` or `/join` commands, no reroll, no game
  rules. Those run on the server (see Server).
- **Language** is read once per game start, so a language change needs a restart.
- **Not played**: this package was built and unit-tested, not played in a live game. Looks are checked by eye in the
  development notes, not by tests.

## Not in this port yet

- **Map-item card pictures.** The API has no call that makes a map id, so the card and score maps are not made. Cards
  and the rest work without them (`server.rs`, `new_map`).
- **Titles and subtitles** become the action bar (over the hotbar) and chat, as the API has no title call.
- **Player faces** on the card and the scoreboard: the new client API gives no skin pixels.
- **The title-screen BINGO! button** needs Horizon PR horizon-rust/horizon#17246. It is compiled out (see its section).
- **Permissions**: the API has no permission nodes, so `bingo.op` is read as operator level 2 (`server.rs`, `is_op`).
- **`preventLobbyChaos`** is not implemented.
- **Physical lobby world** is not implemented: the lobby is logical only.
- **Per-player sidebar**: Horizon has no per-player sidebar, so every player gets the spectators' version. The
  differences are the `PORT-GAP` comments in `horizon-bingo/src/server/sidebar.rs`.
- **Known engine race at startup**: on Horizon commit `c07bfa76e7`, a mod that ships many assets can panic with
  "world resources joined before entry" when a world starts. Relaunch the game.

## Requirements

- Horizon at commit `c07bfa76e71e8db9e84a7e61832a8fb898718e81` (pinned in `horizon-mod.toml`, `[native]`). A native
  build needs the SDK of that exact commit. Horizon has not published SDKs yet, so you build one locally (Install,
  step 4).
- Linux: `cc`, `pkg-config`, ALSA and libudev development files (on Debian and Ubuntu:
  `sudo apt install build-essential pkg-config libasound2-dev libudev-dev`, the packages Horizon's own build needs).
  For a local SDK build, the launcher's pinned Rust toolchain must be on `PATH` (Install, step 4). Otherwise the
  launcher downloads and uses its own Rust for native builds.
- Python 3 (standard library only) for `prepare_assets.py`.
- The original mod's jar for Minecraft 26.3. Source version: 2.15.1 (`gradle.properties` of the upstream). Download it
  yourself from Modrinth; it is not in this repository.
- To play multiplayer bingo: a server running this bundle's server half or the original mod. A client without the mod
  shows no bingo content.

## Install

### 1. Get the original mod jar

Download "Yet Another Minecraft BINGO" for Minecraft 26.3 from Modrinth yourself. Do not share the jar or its files.
The Modrinth page URL was not checked from this package.

### 2. Prepare the assets

Run the script from the repository root (`tools/prepare_assets.py`; it is not in the mod bundle):

```sh
python3 -I tools/prepare_assets.py <jar> <game dir>/horizon-mod-data/bingo/assets \
    --registries <resources>/command-registries.json \
    --hmod <game dir>/horizon-mods/bingo_assets.hmod
```

- `<jar>`: the downloaded jar. It is only read.
- `<game dir>/horizon-mod-data/bingo/assets`: the mod's data folder for Horizon, created by the script if missing. The mod
  reads it from there by default; `BINGO_ASSETS` names another folder.
- `--registries`: `command-registries.json` from Horizon's prepared resources (the folder the game's `--resources`
  option points at, e.g. `.local/horizon-client/26.3` in a source checkout). The script reads the `items` list from it,
  and takes vanilla's languages from `assets/minecraft/lang/` beside it. **Not verified:** that the file sits in that
  folder for a launcher install; this checkout has no prepared resources.
- `--hmod`: optional, but needed for sounds. Writes the content mod `bingo_assets.hmod`.
- `<game dir>`: the instance folder, the one that holds `options.txt`.

The script writes sprites, `card_colors.json`, the languages, `item_names.txt` and vanilla's languages into the assets
dir. `python3 -I tools/prepare_assets.py --help` lists every option.

### 3. Turn on the sound mod

The content mod `bingo_assets` (from `--hmod`) carries the sounds. Turn it on either in Options > Mods (its switch
applies at once: it only has assets), or by adding this line to `<game dir>/horizon-mods.txt`:

```
mod.bingo_assets:true
```

Without it the mod still works; its sounds are silent (the log says `no such sound event` when `BINGO_H0_LOG` is set).

### 4. Build and install the mod (local developer build)

Horizon has not published SDKs for its commits yet, so the mod is built against an SDK made from a local Horizon
checkout at the pinned commit. Horizon's own guide for this is `docs/modding/src/how-to/native-mods/07-local-developer-builds.md`
in the Horizon repository. The steps:

```sh
# a. In the Horizon checkout at c07bfa76e71e8db9e84a7e61832a8fb898718e81, with tracked changes committed:
#    build the game and its companion programs (horizon-ctl, horizon-prepare, horizon-server), and make
#    horizon-wasm-std.wasm and exporters/ beside it. The required files are listed in that guide's table.
cargo run --locked -p cargo-horizon-mod -- wasm-std CHECKOUT/target/debug/horizon-wasm-std.wasm
horizon-launcher developer-install CHECKOUT --program CHECKOUT/target/debug/horizon-client --trust-unsigned
# The command prints a developer-... tag. Call it DEVELOPER_TAG below.

# b. The SDK of that tag, from the same checkout. Build it with the launcher's pinned Rust first on PATH.
python3 tools/prebuilt_game.py build SDK_FOLDER

# c. Prepare the tag's resources, then build the mod against its SDK.
horizon-launcher prepare --version DEVELOPER_TAG
HORIZON_LAUNCHER_DEVELOPER_SDK=SDK_FOLDER horizon-launcher native-build <path to bingo-native> --version DEVELOPER_TAG --instance <instance name> --trust-native-code
```

- The SDK must be that commit's build for this computer, the same side and the launcher's Rust. The launcher refuses
  any other SDK.
- `--trust-native-code` is required. Native code runs with full access to the computer; read the source first.
- `Cargo.lock` in `bingo-native/` was made with `horizon-launcher native-lock`, which pins the newest registry versions,
  so it is already aligned to the game's dependency graph. Its `Cargo.toml` takes `horizon-mod-api` from
  `.horizon/sdk/`, which the SDK provides; that folder is not in the repository.
- The first build downloads Rust and can take 20 to 30 minutes.
- **Not verified:** whether the build also turns the mod on in the instance. Check after the build: the instance's
  **Native build** row on the Mods page should say "Up to date", and Options > Mods should list the Bingo mod.

Once upstream publishes SDKs for the pinned commit, install from GitHub instead:

```sh
horizon-launcher mods add-github https://github.com/ZiQu-dev/horizon-yet-another-bingo
```

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

# 2. Prepare the assets (run from the repository root)
cd ~/horizon-yet-another-bingo
python3 -I tools/prepare_assets.py ~/Downloads/bingo-2.15.1+mc26.3.jar "$GAME/horizon-mod-data/bingo/assets" \
    --registries "$REG" --hmod "$GAME/horizon-mods/bingo_assets.hmod"

# 3. Turn on the sound mod (append, do not overwrite)
echo "mod.bingo_assets:true" >> "$GAME/horizon-mods.txt"

# 4. Build and install the mod: the same developer build as step 4 above, with the launcher inside the app
"$LAUNCHER" native-build ~/horizon-yet-another-bingo/bingo-native --version DEVELOPER_TAG --instance horizon --trust-native-code
```

The steps and options are the same as above. Check the build as in step 4.

## Keys

The keys are Controls key mappings (`Registrar::register_key_mapping`), so bind them in **Options > Controls**.

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
- **A key does nothing.** Check the key under Options > Controls. The toggle and settings keys are unbound
  until you bind them.
- **Card position button does nothing.** Known; see "What it does not do".
- **The launcher refuses the SDK, or the build stops on a Horizon mismatch.** The SDK must be the build of commit
  `c07bfa76e71e8db9e84a7e61832a8fb898718e81` for this computer, the client side and the launcher's Rust. Build it again
  from that checkout (Install, step 4).
- **Build fails on Linux with a missing library.** Install the packages named under Requirements.

## Environment variables (debugging)

Set them in the game's environment. None is needed to play.

- `BINGO_ASSETS`: a folder to read the assets from, instead of the mod's data folder. It must hold the same layout
  `prepare_assets.py` writes.
- `BINGO_H0_LOG`: a file to append debug lines to, for example `assets from ...`, the card key, and each sound
  (`played` or `no such sound event`). Off unless set.
- `BINGO_H0_DUMP`: a folder to write the first three payloads of each server channel as hex, to `<channel>.<n>.hex`.
  The name `H0` is the internal name of the debug hook. Off unless set; it writes files, so use a scratch folder.

## The title screen's BINGO! button (compiled out)

The original mod adds a "BINGO!" button to the main menu, which opens the bingo lobby world. The code is in
`src/title_button.rs` and `src/horizon-bingo/src/title_button.rs`, but it is compiled out: each use in `src/lib.rs` is
behind `#[cfg(any())]`, so the built mod has no button. It needs `host.request_world(folder)`, which is **Horizon PR
horizon-rust/horizon#17246** (API 646, branch `native-request-world`, fork ZiQu-dev/horizon). The released Horizon does
not have it.

To switch it on, with a Horizon that has that call:

- Remove the `#[cfg(any())]` lines in `src/lib.rs` (the module and the three hooks: client tick, screen render, and
  the press before the screen).
- The mod then needs the permission to open a world, declared in its manifest. `horizon-mod.toml` does not declare it,
  and the Horizon docs in the checked copy do not name the key. Check the docs of that Horizon before adding it.
- The lobby world's folder is `LOBBY_FOLDER` in `src/horizon-bingo/src/title_button.rs` ("bingo"). Create a world with
  that folder name first: a missing one shows a red hint under the button for a few seconds.
- The rule (the hints, the folder rule, the button's place, the hit test) is pure logic, tested in `dev/tests/title_button.rs`.

## Server

The bundle's server half (`src/server.rs`, with `src/horizon-bingo/src/server/`) runs wherever the mod is installed:
install the same bundle on the server.

- **Item names.** The server needs `item_names.txt` in the mod's assets folder, `<game dir>/horizon-mod-data/bingo/assets/`, or in the folder named by `BINGO_ASSETS`. `tools/prepare_assets.py` writes it from your own copy of the original mod jar (Install, step 2). Without it the server still runs, but cards have no item icons. The server log then gets one WARNING line, at the first tick, with the path it searched, and `bingo status` and `bingo start` add a note saying so.
- **Clients.** The original Fabric mod 2.15.1 clients work too: a real Fabric client with the original mod joined this server and played a full game (card with icons, score messages, end screen) in a live test.
- **Commands.** Players and the console type `bingo <command>`. `join <team>`, `ready` and `spectator` also work without `bingo`. Commands that change a game need operator level 2: `start`, `end`, `reset`, `shuffleteams`, `ready cancel`, `timelimit`, `goal`, `mode`, `endwhen`, `pvp`, `keepinventory`, `elytra`, `nightvision`, `spawndistance`, `playto`, `stalemate` and `resume`. `bingo help` lists the usages:

  ```
  start [ignore_warnings] | end | reset | shuffleteams <n> | join <team> | spectator | ready [cancel] | timelimit <minutes|off> | card | status | goal lines <n>|items <n>|full_card | mode standard|inventory|lockout|hidden_items on|off|consume_items on|off | endwhen never|first_win|all_win|teams_win <n> | pvp on|off | keepinventory on|off | elytra on|off | nightvision on|off | spawndistance <chunks> | playto cards <n>|infinite|replace_goals | stalemate end_game|reroll_card|do_nothing | options | vote next|keep|lobby | resume | stats [player] | difficulty easy|medium|hard|extreme|impossible | difficulty <S> <A> <B> <C> <D> | reroll | teams <n> | autoshuffle on|off | objectives on|off | menu | kit | kit add <item> [count] | kit remove <item> | kit clear
  ```

- **Votes.** When a game ends (Postgame), `bingo vote next|keep|lobby` votes. A plain chat line `vote next`, `vote keep` or `vote lobby` (no `bingo`, any letter case) is the same vote and is not shown to the other players. The vote is carried out once every online player who plays has voted.
- **Files.** In the mod's data folder of the world, `<world>/horizon-mod-data/bingo/`:
  - `options.json`: the round options, written on each change.
  - `player-settings.json`: the players' settings, written when they change.
  - `history.jsonl`: one line per finished game, appended when the game ends.
  - `game-state.json`: the game in progress, rewritten as it goes on. It is emptied when a game ends. A server that restarts during a game restores it; a game in Postgame is not restored.

## Tests

The integration tests live in `dev/`, a separate crate that builds the logic crate in `bingo-native/src/horizon-bingo`.
The recorded captures and oracle rows are there too. From the repository root:

```sh
cd dev
cargo test --locked --offline    # offline needs the crates (png, serde_json) already in the cargo cache
```

Run it with `CARGO_TARGET_DIR` set outside the repository. The logic crate also has unit tests in its `src/` (41 tests, not
run from the bundle). Do not run `cargo` inside `bingo-native/`: the bundle needs the SDK in `.horizon/sdk/`, and a build
there may rewrite its pinned `Cargo.lock`. `dev/DESIGN.md` is the design and status document; its file paths still use
the old layout (`exp-skeleton/`), so read them as `dev/` and `bingo-native/`.

## Layout

- `horizon-mod.toml`: the manifest: id `bingo`, version, api 0.2, and the pinned Horizon commit (`[native]`).
- `Cargo.toml`, `Cargo.lock`: the crate. Its `horizon-mod-api` dependency comes from the SDK in `.horizon/sdk/`.
- `src/lib.rs`: the wiring to Horizon's hooks, environment variables, asset and language loading.
- `src/server.rs`: the server adapter: Horizon's server hooks, for the server logic in `src/horizon-bingo/src/server/`.
- `src/title_button.rs`: the title screen's BINGO! button, compiled out (see its section above).
- `src/horizon-bingo/`: the logic crate: client and server modules, wire codecs, drawing and screens. Nothing there
  imports Horizon.
- `assets/bingo/lang/*.json`: the original's language files (LGPL-3.0).
- `tools/prepare_assets.py` (repository root): makes the assets from your own copy of the original mod jar.
- `dev/`: the tests, their recorded data and the design notes.

## Licence

Copyright (C) 2026 Mateusz Korniak (ZiQu-dev). Written with the help of an AI assistant (Claude, Anthropic).
LGPL-3.0-or-later, as a port of the upstream mod (see `LICENSE`, `COPYING` and `NOTICE.md`). No upstream assets are included, except the language files in `assets/bingo/lang/`.

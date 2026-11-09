# Notice: Bingo (native mod for Horizon)

## Original work

- **Name:** Yet Another Minecraft BINGO
- **Upstream repository:** https://gitlab.com/horrificdev/bingo
- **Version followed:** 2.15.1 for Minecraft 26.3 (`mod_version` in the upstream `gradle.properties`).
- **Upstream commit checked:** `a9e232c6a2b0d70137f31490544cf9531dfcb58b` (HEAD of the local clone at the time of
  writing).
- **Licence:** GNU Lesser General Public License, version 3.0 (LGPL-3.0). The upstream `LICENSE` file is the full
  LGPL-3.0 text, without a copyright line.
- **Copyright holders:** not stated in the upstream `LICENSE` or `README.md`. The local clone's commit history has one
  author name, `fennifith`; the upstream Maven group is `me.jfenn`. These are facts from the repository, not a
  statement of who holds copyright. **The maintainer must confirm the holders with upstream before publication.**
- **Upstream credits** (from its `README.md`): the card HUD and "Bingo!" button textures are by marceles; the
  translations are by the upstream's translators, through Crowdin. None of these files is in this package.

## What this package is

`bingo_native` is the client half of the upstream mod, written in Rust for Horizon's native mod API. It re-implements
the client's behaviour and the packet formats from the upstream source. It is not a copy of the upstream code, and the
upstream textures, languages and sounds are not in it. Its unit tests compare packet bytes with output from the
upstream's own code, run in a test harness. It is treated as a derivative work and licensed accordingly (see "Licence of this port").

## What is not in the mod build

- The mod's bundle holds Rust (`.rs`) and TOML (`.toml`) files only. It contains no upstream code in another form, no
  images, no languages and no sounds.
- The assets are generated on the player's own machine by `tools/prepare_assets.py`, from the player's own copy of the
  upstream jar: card sprites, `card_colors.json`, the upstream languages, the item name list, and the content mod
  `bingo_assets.hmod` with the sounds. Those generated files stay on that machine. Do not redistribute them.
- Horizon's prepared vanilla resources (Minecraft's own files) also come from the player's install, not from this
  package.

## Author of this port

- **Port author and copyright holder of the port:** Copyright (C) 2026 Mateusz Korniak (ZiQu-dev).
- The port was written with the help of an AI assistant (Claude, by Anthropic), working on the author's instructions.
- The port is based on the work of `fennifith` (`me.jfenn`) and its contributors, as described above.

## Licence of this port

This port is published under the **GNU Lesser General Public License, version 3.0 or any later version
(LGPL-3.0-or-later)**. The full text is in `LICENSE` (it is the text the upstream project ships).

The port is treated as a work derived from the upstream mod: it was written by reading the upstream source and
re-implements its client logic, screens and packet formats in Rust. For that reason it carries the upstream's licence
rather than Horizon's MIT licence, and:

- the upstream's authorship is acknowledged above; this port is a **modified, ported version**, not the original;
- the source of this package is public (it is the package itself: a native mod's bundle is its source);
- Horizon (MIT) is only a dependency of this mod through its public mod API; Horizon's own files are not part of
  this package and stay under Horizon's licence;
- LGPL-3.0 incorporates the GNU GPL-3.0 by reference; its text is in `COPYING`, beside `LICENSE`.

Still to confirm with upstream before publication: the exact copyright holders (the upstream files name none; see
above). This is the package author's licensing choice, not legal advice.

## Links

- Upstream: https://gitlab.com/horrificdev/bingo
- LGPL-3.0 text: https://www.gnu.org/licenses/lgpl-3.0.html

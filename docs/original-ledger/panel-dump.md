# Panel dump: lobby menu of Yet Another Minecraft BINGO 2.15.1 (MC 26.3)

Source: the original Fabric mod run live (singleplayer lobby world, root page, day, 1 player, defaults). Every entity was read with `execute as @e[type=!player] run data get entity @s` and the chat lines taken from the game log; `panel-dump.json` holds the exact fields (`_snbt` is the raw string the game printed, `_text_en` the English resolution of the text component, `_group` is `menu` or `team_pad`). 141 entities carry the mod tag `bingo-<uuid>` (117 menu + 24 team pad); the lobby also holds 4 paintings and 1 armor stand that are map scenery, not part of the mod, and are not in the dump. Entity UUIDs and the tag uuid are random per spawn.

Menu POI block = (0, 17, -5), yaw 0 (identity matrix, panel faces south). Panel anchor `A` = POI + (0.5, 2.0, 0.05) = (0.5, 19, -4.95). In the tables below coordinates are **relative to the POI block corner (0, 17, -5)**: add (0, 17, -5) for world positions; subtract (0.5, 2.0, 0.05) to get the code's `position` space. All entities have Rotation [0,0] (fixed billboard, yaw 0 pitch 0).

Counts (menu): 33 block_display (tile backgrounds), 61 text_display (labels, glyphs), 23 interaction (hit boxes). Tile = a block_display whose Pos is the bottom-left corner (x grows east, y up) and which extends +x by scale.x, +y by scale.y, +z by 0.05 (so it spans z -0.001..0.049 rel POI z; text sits at z 0.05). Tile background colour is the concrete block, not a text background: all text_display backgrounds are `0` (ARGB 0x00000000, transparent) in the menu.

Block colours (typical concrete texture colours, not read from the game): white_concrete #CFD5D6, gray_concrete #36393D, lime_concrete #5EA818, red_concrete #8E2121 (not used on the root page). Brightness override `{sky,block}` decides how dark it renders: sky 15 = full, 14, 8 (alt, noticeably darker), 3 (headers, nearly black green-tinted).

Text colours: gray = #AAAAAA, black = #000000, dark_green = #00AA00, green = #55FF55, `#99FF99` = pale green headers, default (no colour) = white #FFFFFF.

## Tiles by column and row (top to bottom)

### Column 1 Game Mode

| tile centre (x, y, z) rel POI | w x h | block | sky | label (text, colour, scale, line_width) | glyph (colour) | interaction (Pos rel POI, w x h) |
|---|---|---|---|---|---|---|
| (-3.55, 1.65, 0.024) | 2.5x0.3 | lime_concrete | 3 | "Game Mode" #99FF99 s0.7 lw200 at (-3.55, 1.55, 0.05) | - | - |
| (-4.2, 1.05, 0.024) | 1.2x0.7 | gray_concrete | 14 | "Lockout" gray s0.4 lw78 at (-4.2, 0.8, 0.05) | "◇" gray s0.7 at (-4.65, 0.95, 0.055)<br>"🔒" gray s1.6 at (-4.2, 0.9, 0.05) | (-4.2, 0.7, -0.55) 1.2x0.7 |
| (-2.9, 1.05, 0.024) | 1.2x0.7 | gray_concrete | 14 | "Inventory" gray s0.4 lw78 at (-2.9, 0.8, 0.05) | "◇" gray s0.7 at (-3.35, 0.95, 0.055)<br>"🎒" gray s1.6 at (-2.9, 0.9, 0.05) | (-2.9, 0.7, -0.55) 1.2x0.7 |
| (-4.2, 0.25, 0.024) | 1.2x0.7 | gray_concrete | 14 | "Hidden Items" gray s0.4 lw78 at (-4.2, 0, 0.05) | "◇" gray s0.7 at (-4.65, 0.15, 0.055)<br>"❓" gray s1.6 at (-4.2, 0.1, 0.05) | (-4.2, -0.1, -0.55) 1.2x0.7 |
| (-2.9, 0.25, 0.024) | 1.2x0.7 | gray_concrete | 14 | "Consume Items" gray s0.4 lw78 at (-2.9, 0, 0.05) | "◇" gray s0.7 at (-3.35, 0.15, 0.055)<br>"⬚" gray s1.6 at (-2.9, 0.1, 0.05) | (-2.9, -0.1, -0.55) 1.2x0.7 |
| (-3.55, -0.4, 0.024) | 2.5x0.4 | gray_concrete | 8 | "Features" gray s0.7 lw107 at (-3.55, -0.5, 0.05) | "⋯" gray s0.7 at (-4.65, -0.5, 0.055) | (-3.55, -0.6, -1.2) 2.5x0.4 |

### Column 2 Goal / Time Limit

| tile centre (x, y, z) rel POI | w x h | block | sky | label (text, colour, scale, line_width) | glyph (colour) | interaction (Pos rel POI, w x h) |
|---|---|---|---|---|---|---|
| (-1.2, 1.65, 0.024) | 2x0.3 | lime_concrete | 3 | "Goal" #99FF99 s0.7 lw200 at (-1.2, 1.55, 0.05) | - | - |
| (-2.05, 1.25, 0.024) | 0.3x0.3 | white_concrete | 15 | "-" black s0.7 lw12 at (-2.05, 1.15, 0.05) | - | (-2.05, 1.1, -0.1) 0.3x0.3 |
| (-1.2, 1.25, 0.024) | 1.2x0.3 | gray_concrete | 14 | "1 line" white(default) s0.7 lw200 at (-1.2, 1.15, 0.05) | - | - |
| (-0.35, 1.25, 0.024) | 0.3x0.3 | white_concrete | 15 | "+" black s0.7 lw12 at (-0.35, 1.15, 0.05) | - | (-0.35, 1.1, -0.1) 0.3x0.3 |
| (-1.2, 0.85, 0.024) | 2x0.3 | gray_concrete | 14 | "Lines" gray s0.7 lw85 at (-1.2, 0.75, 0.05) | "⇄" gray s0.7 at (-2.05, 0.75, 0.055) | (-1.2, 0.7, -0.95) 2x0.3 |
| (-1.2, 0.45, 0.024) | 2x0.3 | lime_concrete | 3 | "Time Limit" #99FF99 s0.7 lw200 at (-1.2, 0.35, 0.05) | - | - |
| (-2.05, 0.05, 0.024) | 0.3x0.3 | white_concrete | 15 | "-" black s0.7 lw12 at (-2.05, -0.05, 0.05) | - | (-2.05, -0.1, -0.1) 0.3x0.3 |
| (-1.2, 0.05, 0.024) | 1.2x0.3 | gray_concrete | 14 | "Off" white(default) s0.7 lw200 at (-1.2, -0.05, 0.05) | - | - |
| (-0.35, 0.05, 0.024) | 0.3x0.3 | white_concrete | 15 | "+" black s0.7 lw12 at (-0.35, -0.05, 0.05) | - | (-0.35, -0.1, -0.1) 0.3x0.3 |
| (-1.2, -0.4, 0.024) | 2x0.4 | gray_concrete | 8 | "More..." gray s0.7 lw85 at (-1.2, -0.5, 0.05) | "⋯" gray s0.7 at (-2.05, -0.5, 0.055) | (-1.2, -0.6, -0.95) 2x0.4 |

### Column 3 Difficulty

| tile centre (x, y, z) rel POI | w x h | block | sky | label (text, colour, scale, line_width) | glyph (colour) | interaction (Pos rel POI, w x h) |
|---|---|---|---|---|---|---|
| (0.7, 1.65, 0.024) | 1.6x0.3 | lime_concrete | 3 | "Difficulty" #99FF99 s0.7 lw200 at (0.7, 1.55, 0.05) | - | - |
| (0.7, 1.25, 0.024) | 1.6x0.3 | white_concrete | 15 | "Easy" black s0.7 lw68 at (0.7, 1.15, 0.05) | "◇" black s0.7 at (0.05, 1.15, 0.055)<br>"◆" dark_green s0.7 at (0.05, 1.15, 0.05) | (0.7, 1.1, -0.75) 1.6x0.3 |
| (0.7, 0.85, 0.024) | 1.6x0.3 | gray_concrete | 14 | "Medium" gray s0.7 lw68 at (0.7, 0.75, 0.05) | "◇" gray s0.7 at (0.05, 0.75, 0.055) | (0.7, 0.7, -0.75) 1.6x0.3 |
| (0.7, 0.45, 0.024) | 1.6x0.3 | gray_concrete | 14 | "Hard" gray s0.7 lw68 at (0.7, 0.35, 0.05) | "◇" gray s0.7 at (0.05, 0.35, 0.055) | (0.7, 0.3, -0.75) 1.6x0.3 |
| (0.7, 0.05, 0.024) | 1.6x0.3 | gray_concrete | 14 | "Extreme" gray s0.7 lw68 at (0.7, -0.05, 0.05) | "◇" gray s0.7 at (0.05, -0.05, 0.055) | (0.7, -0.1, -0.75) 1.6x0.3 |
| (0.7, -0.4, 0.024) | 1.6x0.4 | gray_concrete | 8 | "Customize" gray s0.7 lw68 at (0.7, -0.5, 0.05) | "⋯" gray s0.7 at (0.05, -0.5, 0.055) | (0.7, -0.6, -0.75) 1.6x0.4 |

### Column 4 Shufflificator

| tile centre (x, y, z) rel POI | w x h | block | sky | label (text, colour, scale, line_width) | glyph (colour) | interaction (Pos rel POI, w x h) |
|---|---|---|---|---|---|---|
| (2.6, 1.65, 0.024) | 2x0.3 | lime_concrete | 3 | "Shufflificator" #99FF99 s0.7 lw200 at (2.6, 1.55, 0.05) | - | - |
| (2.6, 1.25, 0.024) | 2x0.3 | gray_concrete | 14 | "Auto Shuffle" gray s0.7 lw85 at (2.6, 1.15, 0.05) | "◇" gray s0.7 at (1.75, 1.15, 0.055) | (2.6, 1.1, -0.95) 2x0.3 |
| (1.75, 0.85, 0.024) | 0.3x0.3 | white_concrete | 15 | "-" black s0.7 lw12 at (1.75, 0.75, 0.05) | - | (1.75, 0.7, -0.1) 0.3x0.3 |
| (2.6, 0.85, 0.024) | 1.2x0.3 | gray_concrete | 14 | "3 teams" white(default) s0.7 lw200 at (2.6, 0.75, 0.05) | - | - |
| (3.45, 0.85, 0.024) | 0.3x0.3 | white_concrete | 15 | "+" black s0.7 lw12 at (3.45, 0.75, 0.05) | - | (3.45, 0.7, -0.1) 0.3x0.3 |
| (2.6, 0.45, 0.024) | 2x0.3 | gray_concrete | 14 | "Shuffle!" gray s0.7 lw85 at (2.6, 0.35, 0.05) | - | (2.6, 0.3, -0.95) 2x0.3 |
| (2.6, -0.4, 0.024) | 2x0.4 | gray_concrete | 8 | "Re-roll Card" gray s0.7 lw85 at (2.6, -0.5, 0.05) | "⚄" gray s0.7 at (1.75, -0.5, 0.055) | (2.6, -0.6, -0.95) 2x0.4 |

### Column 5 Best time / Players / START

| tile centre (x, y, z) rel POI | w x h | block | sky | label (text, colour, scale, line_width) | glyph (colour) | interaction (Pos rel POI, w x h) |
|---|---|---|---|---|---|---|
| (4.75, 1.65, 0.024) | 2.1x0.3 | lime_concrete | 3 | "Best time: [Not Set]" #99FF99 s0.7 lw200 at (4.75, 1.55, 0.05) | - | - |
| (4.75, 1.25, 0.024) | 2.1x0.3 | lime_concrete | 3 | "Players: 1 / 8" #99FF99 s0.7 lw200 at (4.75, 1.15, 0.05) | - | - |
| (4.75, 0.45, 0.024) | 2.1x1.1 | gray_concrete | 8 | "START" gray s0.7 lw90 at (4.75, 0.35, 0.05) | - | (4.75, -0.1, -1) 2.1x1.1 |
| (4.75, -0.4, 0.024) | 2.1x0.4 | gray_concrete | 8 | "Reset Defaults" gray s0.7 lw90 at (4.75, -0.5, 0.05) | "⟳" gray s0.7 at (3.85, -0.5, 0.055) | (4.75, -0.6, -1) 2.1x0.4 |

Notes: two text displays per toggle tile hold the same spot: a `◇` (z +0.005) in the text colour, and a `◆` in dark_green (z +0.0) that shows only when the tile is active, otherwise empty text. Active tile (here "Easy"): white_concrete, sky 15, black text. Inactive: gray_concrete, sky 14, gray text. `+`/`-` squares are always white_concrete sky 15 with black text, size 0.3x0.3 (the middle tile between them is gray_concrete 1.2 wide in a Goal/Time/Teams row). Large two-line tiles (Game Mode 1.2x0.7) hold an icon text at scale 1.6 and a caption at scale 0.4 (lw 78). Interaction entities are shifted by -width/2 in z (a quirk of the original) and sit at the tile's x centre and y bottom.

Text anchor: a label text_display sits at x = tile centre, y = tile bottom + h/2 - scale/7 (scale 0.7 gives +h/2 - 0.1), alignment center, so the line is centred on the tile; the background colour 0 means no box behind the text.

## Team pad labels (outside the panel)

Each of the 8 team lodestone pads gets two text_display and one interaction, at the pad block centre `(bx+0.5, by, bz+0.5)`: name label at +1.5 y, "Click to join!" at +1.0 y, interaction at -1.0 y. Billboard `vertical`, alignment center, scale 1 (default transformation), line_width 200, view_range 1.0. The name label has `background: 0` and `shadow: 1b`; "Click to join!" has `shadow: 0b` and `background: 1073741824` (ARGB 0x40000000, the vanilla default shade). The name label component is `chat.square_brackets` wrapping `[colour, extra: [symbol, " ", bingo.team.name(bingo.team.<colour>)]]`, text colour the team colour.

| team | label text (English) | colour | pad block (x, y, z) | label Pos (absolute) | "Click to join!" Pos | interaction Pos, w x h |
|---|---|---|---|---|---|---|
| gold | [🍄 Orange Team] | gold | (-17, 24, -4) | [-16.5, 25.5, -3.5] | [-16.5, 25.0, -3.5] | [-16.5, 23.0, -3.5] 2.0x4.0 |
| red | [🔥 Red Team] | red | (-17, 23, 14) | [-16.5, 24.5, 14.5] | [-16.5, 24.0, 14.5] | [-16.5, 22.0, 14.5] 2.0x4.0 |
| green | [🍂 Green Team] | green | (-15, 28, -20) | [-14.5, 29.5, -19.5] | [-14.5, 29.0, -19.5] | [-14.5, 27.0, -19.5] 2.0x4.0 |
| gray | [🪨 Gray Team] | gray | (-2, 21, 18) | [-1.5, 22.5, 18.5] | [-1.5, 22.0, 18.5] | [-1.5, 20.0, 18.5] 2.0x4.0 |
| light_purple | [🌺 Pink Team] | light_purple | (5, 25, -16) | [5.5, 26.5, -15.5] | [5.5, 26.0, -15.5] | [5.5, 24.0, -15.5] 2.0x4.0 |
| aqua | [🍄 Aqua Team] | aqua | (16, 25, 17) | [16.5, 26.5, 17.5] | [16.5, 26.0, 17.5] | [16.5, 24.0, 17.5] 2.0x4.0 |
| blue | [🌊 Blue Team] | blue | (16, 23, -9) | [16.5, 24.5, -8.5] | [16.5, 24.0, -8.5] | [16.5, 22.0, -8.5] 2.0x4.0 |
| yellow | [🐝 Yellow Team] | yellow | (18, 23, 6) | [18.5, 24.5, 6.5] | [18.5, 24.0, 6.5] | [18.5, 22.0, 6.5] 2.0x4.0 |

## Other lobby entities

- No item_display entities exist: every icon is a text_display with a Unicode glyph (🔒 🎒 ❓ ⬚ ⇄ ⋯ ⚄ ⟳ ◇ ◆), the "icon item" column is therefore always a glyph.
- No panel backdrop entity: the dark panel behind the tiles is the lobby map's own blocks (the panel anchor only places tiles).
- Not from the mod (no `bingo-` tag, not in the dump): 4 paintings (`minecraft:sea` at block (9,17,5) and (10,17,0), `minecraft:sunset` at (9,17,8) and (9,17,-5)) and one armor stand (tags `as_uuid_locked`, `as_modified`, diamond sword, custom pose). They are part of the map.
- Hover tooltips are not entities: `registerButtonInteraction` stores the tooltip lines in `TooltipState` keyed by the interaction entity UUID and sends them to the client mod as a packet (`TooltipController`/`TooltipPacket`). Nothing about hover is visible in the entity data.
- Not obtained: pages other than the root page (Features, Goal, Items, warning/reset prompt) were not dumped; the data is the root page in its default state with one player. Values that depend on state (labels, active tile, player count, best time) change with the state as described below.

## Formulas

Source: `common/src/main/java/me/jfenn/bingo/common/menu/` (`Common.kt`, `Button.kt`, `MenuInstance.kt`, `MenuPage.kt`, `MenuController.kt`, `TitlePanel.kt`, `NumberInput.kt`, `PlayerCount.kt`, `Teams.kt`, `GameModes.kt`, `Goal.kt`, `CardDifficulty.kt`).

Constants (`Common.kt`): `MENU_TEXT_SCALE = 0.7`, `MENU_LINE_HEIGHT = 0.3`, `MENU_LINE_PADDING = 0.1`; materials lime_concrete (title), white_concrete (button on), gray_concrete (button off), red_concrete (danger); brightness `{block 0, sky}`: ON sky 15, OFF sky 14, ALT sky 8; title panels use sky 3; the red danger button on the prompt page uses sky 3.

Placement (`MenuInstance`, `MenuController`):
- `matrix` = identity translated by the POI block position (0, 17, -5) (`translate(pos)`, then `rotateY(yaw)`; yaw 0 here). No 0.5 offset comes from the POI; it comes from the menu translation `(0.5, 2.0, 0.05)`. So anchor A = (0.5, 19, -4.95), and every entity's final Pos = POI + (0.5, 2.0, 0.05) + the position computed by the page code (code position space has x east, y up, z towards the viewer).
- Each entity is built at origin with yaw 0, `init` sets its fields, then `transformPos` applies the matrix to Pos and sets yaw = init yaw + round(degrees(-angleX - angleY)); with yaw 0 the result is Rotation [0, 0] (pitch 0 is set explicitly for text). Special-cases exist for the 180 degrees (translation -1, -1) and 90 degrees (yaw negated) matrices; not used here.
- The entities carry the one tag `bingo-<random uuid>` (`commandTags = setOf(instanceTag)`); every entity is spawned with its own random UUID.
- `MenuEntityHandle` entities are re-initialised on every `markDirty` (option change, player join/leave): fields are rewritten in place, so state changes only change `block_state`, `text`, `brightness`, never Pos.

Layout of the root page (`createRootPage`): cursor `pos` starts at `A + (-5.5 + 2*0.1, 0, 0)`, so the first column's left edge is x = 0.5 - 5.3 = -4.8 (rel POI x -4.8). For each column: centre = cursor + width/2, then cursor += width + 0.1. Widths: Game Modes 1.2 + 0.1 + 1.2 = 2.5, Goal 2.0, Difficulty 1.6, Shufflificator 2.0; the last column takes the rest: `startWidth = (position.x + 5.5 - 2*0.1) - cursor.x` with `position.x` = A.x = 0.5 (page position, code space) and cursor.x = 3.7 at that point (code space, where A.x maps to 0.5 after the +0.5 translation is already inside `position`): (0.5 + 5.5 - 0.2) - 3.7 = 2.1, matching the 2.1 wide tiles in the dump. The panel runs x -4.8 to 5.8 rel POI, centred on 0.5. Column left edges rel POI: -4.8, -2.2, -0.1, 1.6, 3.7; centres: -3.55, -1.2, 0.7, 2.6, 4.75.

Vertical rows: the `offset` vectors in the register functions are mutable and `offset.sub(...)` both returns and accumulates, so the y values are running sums (this is why rows look irregular). Resulting tile bottoms rel POI (and rel A): headers 1.5 (-0.5); row 1.1 (-0.9); 0.7 (-1.3); 0.3 (-1.7); -0.1 (-2.1); bottom row (mode `⋯` tiles, height 0.4) -0.6 (-2.6). Game-mode tiles are 0.7 high at 0.7 and -0.1 (`offsetY = 0.6`, `TILE_HEIGHT = 0.7`, `TILE_MARGIN = 0.1`); the START tile is `startButtonHeight = 1.9 - MENU_PLAYER_COUNT_HEIGHT + 2*0.1 = 1.9 - 1.0 + 0.2 = 1.1` with `MENU_PLAYER_COUNT_HEIGHT = 2*0.3 + 4*0.1 = 1.0`, bottom at -0.1; Reset Defaults is 0.4 high at -0.6; Re-roll Card (`registerCardReroll`, width 2.0, height 0.4) sits at `-0.5 - 3*0.3 - 4*0.1 - 0.8 = -2.6` rel A = -0.6 rel POI.

Tile (`registerTileButton`, defaults width 1.0, height 0.3, text scale 0.7), `position` = bottom centre of the tile:
- block_display: Pos = position + (-w/2, 0, -0.051), `transformation = scale(w, h, 0.05)` (no translation, no rotation), so it extends +x by w, +y by h, +z by 0.05 from z-0.051 (final world z = POI z + 0.05 - 0.051 = -0.001 rel POI) up to 0.049; `block_state` = `blockIdentifier` ?: (white_concrete if active else gray_concrete); if the label is empty (text given but blank) the block is `minecraft:air`. `brightness` = explicit parameter ?: (sky 15 if active else 14). Tiles built with `brightness = MENU_BRIGHTNESS_ALT` (Features, More, Customize, Re-roll, Start, Reset, back button) are always sky 8.
- label text_display: Pos = position + (0, h/2 - scale/7, 0), z = 0 (so 0.05 rel POI, 0.001 in front of the block face); `scale(textScale)`; `lineWidth = (w * 30 / textScale).toInt()` truncating: w 1.6 gives 68, 2.0 gives 85, 2.1 gives 90, 2.5 gives 107 (checks: 1.6*30/0.7 = 68.57, 2.0*30/0.7 = 85.71, 2.1*30/0.7 = 90, 2.5*30/0.7 = 107.14); `+`/`-` (0.3 wide) give 12; text colour via `.formatted()`: BLACK if active else GRAY (written as the component's `color`); billboard fixed, alignment center, background 0.
- `◇` glyph (when `icon` is given or the tile is a toggle): Pos = position + (-w/2 + 0.15, h/2 - 0.1, 0.005), scale 0.7, same black/gray colour, glyph is `icon ?: "◇"`.
- `◆` indicator (toggle without icon): Pos = position + (-w/2 + 0.15, h/2 - 0.1, 0.0), text "◆" if active else "", colour DARK_GREEN.
- interaction (`registerButtonInteraction`): Pos = position + (0, 0, -w/2) (the offset is applied on z, not x: hence interaction z = -w/2 + 0.05 rel POI and a hit box that is w wide in x and z, h tall from the tile bottom), width = w, height = h. Clicks need the `CONFIGURE_GAME` permission; on click it plays the wooden-button on/off sound by the active state and re-emits OptionsChangedEvent (redraw).
- Selected/active versus hover: there is no hover or pressed state in the entities. "Selected" (isActive) is the only visual state: white_concrete + sky 15 + black text (+ dark_green ◆ for text-less icon-less toggles) versus gray_concrete + sky 14 + gray text. Tiles with a constant active flag (the +/- squares, `isActiveProp = ConstantProperty(true)`) are always white. Tiles not created with an `isActiveProp` (Lines, Shuffle!, Features, More, ...) are always inactive (gray text, gray block).

Header (`registerTitlePanel`, width = column width, height 0.3): text_display Pos = position + (0, 0.05, 0) (the dump shows 1.55 for header blocks whose bottom is 1.5), scale 0.7, colour `#99FF99` (0x99ff99), background 0; block_display lime_concrete at position + (-w/2, 0, -0.051), scale (w, 0.3, 0.05), brightness sky 3 (air when the title is empty). The Best time and Players headers use `registerTitlePanel` with their computed texts; `Players: %s` argument is `N / M` in `green` (`1 / 8`), `Best time: %s` argument is `[Not Set]` in gray, or the formatted time in green.

Number input (`registerNumberInput`, width 2.0, height 0.3): `+` at position + (w/2 - h/2, 0, 0) and `-` at position + (-w/2 + h/2, 0, 0), each a tile of 0.3 x 0.3 with text `+`/`-`, always white; the value background is a gray_concrete block of width `w - 2h - 0.2` = 1.2, height 0.3, sky 14 (OFF) at position + (-1.2/2, 0, -0.051); the value text_display is at position + (0, h/2 - 0.1, 0), scale 0.7, alignment center, **shadow true**, line_width default 200, colour default white. Dump centres: -2.05 / -1.2 / -0.35 for Goal and Time Limit.

Icon button (`registerIconButton`, Game Mode tiles, width 1.2, height 0.7): a tile without label/glyph plus an icon text at position + (0, h/2 - 0.15, 0), scale 1.6, and a caption at position + (0, h/2 - 0.25, 0), scale 0.4, `lineWidth = (w * 65).toInt()` = 78. Both use black if active, gray otherwise. The `◇`/`◆` pair from the tile sits at x = -w/2 + 0.15 as above.

Row heights of the Difficulty and other lists: Difficulty tiles are at position.y - (0.5 + (i+1)*(0.1 + 0.3)), i = 0..3 (so bottoms at rel A -0.9, -1.3, -1.7, -2.1), "Customize" at -(0.6 + 4*0.3 + 4*0.1 + 0.4) = -2.6.

Team pads (`MenuController.spawnTeamEntity`, run once per `MenuPoi.Team` found in the lobby map): name label at (bx + 0.5, by + 1.5, bz + 0.5): `value = BingoTeam.fromPreset(...).getName(symbol = true)` which is `[symbol Name Team]` inside `chat.square_brackets` coloured with the team colour; billboard VERTICAL; alignment CENTER; `background = 0`; `shadow = true`. "Click to join!" label at +1.0 y: the translatable `bingo.lobby.click_to_join`; billboard VERTICAL; alignment CENTER; no `background` is set in code but the stored value is `1073741824` (0x40000000: the entity default background is written out); shadow false. Interaction at +(0, -1.0, 0): width 2, height 4 (spans y -1 to +3 around the pad position, i.e. from one block below the pad block's base to above the label); the click spawns 10 dust particles of the team colour, plays the team-changed sound and joins the team. The pad block itself (a lodestone in the map) is replaced with air (`setBlockState(blockPos, air)`), as are the menu POI cells (y -1..1) and the spawn POI block. These entities share the menu's `bingo-` tag.

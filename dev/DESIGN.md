# Bingo for Horizon: phase 2 design (client half)

Checked against the mod's source (mc26.3, 2.15.1), Horizon's docs and `net.rs`, and the Fabric server's own classes.
What is still a guess is marked **guess**. Status of the code is in section 8; the list of what is not done is
section 9.

## 1. What the player needs, and the size of it

Scope A: Horizon as a **client** on a Java server that runs the mod, with the mod's client features.

| Part | Kotlin lines | Notes |
|---|---|---|
| Client code, `common/src/client` | 4 942 | HUD, screens, card renderers, stats sync, settings |
| Client platform glue, `mc26.3/src/client` | 1 736 | draw, screens, networking: replaced by `Host` |
| Packets and shared data used by the client | about 1 500 (**guess**) | 26 channels, card and score models |
| Server logic (`common/` rest, `mc26.3` server mixins) | about 33 000 | **Not needed for A.** Scope B only |

## 2. The handshake: a play-phase `minecraft:register`, no core change (tested)

A first reading of Fabric's networking code said the handshake needed a change in `net.rs`: the configuration-phase
`c:version` and `c:register` payloads. Test C1 showed that is not needed:

- Control, no registration: after `/bingo start` the bot got **no** packet of the mod.
- A client that sends the legacy **`minecraft:register` in the play phase**, channel names joined by NUL
  (`yet-another-minecraft-bingo:timer\0...`), gets at once `display_cards`, `update_card_tiles`, `stats_check` and
  `game_status` about every 2 s. The mod resyncs a player when the channels are registered.

So `client.rs` registers with `minecraft:register` from its tick (`REGISTER_CHANNEL`, `register_payload`), sends
`update_settings` once the registration is in, and registers again when the server announces its channels on a new
join. Not tested: a join where the mod sends its first state before the registration arrives (the resync above
covers it).

## 3. What Horizon already gives (native mod API)

| Need | Hook | Status |
|---|---|---|
| Hear the server's packets, custom payloads included | `native_client_packet` (408) | used |
| Send a custom payload to the server | `native_send_packet` (409) | used, play phase only |
| Draw HUD around vanilla layers | `native_gui_layer_post`, `native_gui_render_post` (410) | used |
| A screen of the mod's own, with clicks and keys | `native_mod_screen_open` (111) + screen hooks | used |
| Client tick | `native_client_tick_post` | used |
| Read player, dimension, world id | `native_player`, `native_client_state` | used |
| Sounds | native sound helper | used (`sounds.rs`) |
| Picture upload | `mod_image` | used (one atlas, `card_images.rs`) |
| Text component and item stack codecs | the mod's own `buf.rs`, `text.rs`, `item.rs` | written and checked against the mod's bytes |

## 4. Where it lives (decision D1)

**Native mod** (`.hnmod`), chosen. It needs a launcher build per player and runs native code with full rights. The
handshake is a play-phase payload the mod can send (section 2), so no change to Horizon's core is needed. The mod
is `c1mod/bingo-native` (pinned to a nightly's commit); it embeds this crate as a path dependency.

## 5. Crate layout (`exp-skeleton/`)

```
wire      buf.rs (big-endian, VarInt strings, UUID, nullable), nbt.rs, text.rs, item.rs (stacks, 30 components)
packets   timer.rs ready.rs ... one file per packet; each is checked against oracle rows (section 7)
channels  channels.rs      the 26 channels: direction, Kotlin source, status
state     state.rs         what the client remembers; changed only by HudState::on_packet
client    client.rs        registration, settings, tick clock, faces, the screen's input; drives the rest
host      host.rs          the ONLY seam to Horizon (trait): send, draw, sound, tick, images, tooltips
drawing   card_draw.rs, card_tile_image.rs, message_draw.rs, ready_draw.rs, tooltip_draw.rs, shuffle_draw.rs,
          fireworks.rs, card_images.rs, atlas.rs, assets.rs, text_plain.rs, ui_draw.rs
screens   screen_main.rs (tabs, buttons, rules), screen_cards.rs (card row, history), screen_end.rs and
          scores_draw.rs (scores tab), history_ctl.rs (message history scroll), screen_controller.rs (the screen as
          one object), screen_placement.rs (card placement, not reachable yet, section 9)
```

Rule: nothing outside `host.rs` imports Horizon. Everything else runs in `cargo test` without a window. Layouts are
pure functions: they return draw commands, hit regions and new scroll state; the host draws them.

## 6. Channel status

All **26 channels are Done**: each has its codec, rows in `tests/oracle.tsv` and `tests/oracle/`, and a test that
compares its bytes with the mod's. `tests/table.rs` checks that the table's 26 paths and directions equal the
registrations in the mod's `ServerPacketEvents.kt`, path by path (the list is written into the test; it is not read
from the Kotlin at test time).

The mod's text packets (`update_card_tiles`, `display_cards`, `tooltip_v2`, `ready_timer_update` V1 and V2,
`game_over` V1 and V2, `game_message`) carry text components. In the mod `writeText` is the component codec, and a
`null` text is written as an **empty component**, not as absent. `ReadyUpdatePacket` reads and writes that the same way.

## 7. The oracle contract

The oracle programs (`exp-oracle/Oracle.java` and `exp-w2/oracle/OracleW2.java`, not in this copy of the tree) run
the mod's real converters over Minecraft's `FriendlyByteBuf` (the registry-aware one needs bootstrapped built-in
registries; dynamic registries are bound by hand) and print `name<TAB>channel<TAB>hex`. Every packet must have rows for: zero or empty values, a negative number,
every enum value, a nullable present and absent. A packet without rows is not done. Facts found by the oracle:
hex colours are written in upper case (`#FF8800`).

The captures in `tests/captures/live/` were taken from a real server by the native mod itself. They decode and
encode back byte for byte, and the client accepts all of them (`tests/live.rs`).

## 8. What is implemented

Each item was checked in the code, and in a real game where the earlier notes say so.

- **Card screen** (`Y` opens it, Close and Escape close it with the original's animation): the tab strip with the
  CARDS and SCORES tabs, the tab frames, the card row with the entrance slide from the HUD and the card fade-in
  (`screen_controller.rs`, `run_faded`), card selection with several cards, arrow keys moving the selection, the
  wheel and scroll bar, hover outlines and tile tooltips (drawn with the vanilla tooltip).
- **Card**: the team's card with its ornate frame, team name, 25 tiles with item icons, banners, and tile pictures from
  `card_image` packets (`card_images.rs`, packed into the one atlas with a size and pixel budget).
- **Shuffle animation**: a shuffled card eases its tiles to the new permutation over 280 ms, with the 280 ms
  debounce (`shuffle_draw.rs`, `state.rs`).
- **Scores tab**: the end screen's score list and game info panel (`screen_end.rs`, `scores_draw.rs`), with its
  scroll bar.
- **Message history sidebar**: the past messages, newest first, with scroll and wheel (`history_ctl.rs`,
  `screen_cards::history`).
- **Score messages** on the HUD under the card, with fade in and out (`message_draw.rs`).
- **Ready line** and countdown on the HUD (`ready_draw.rs`).
- **Winner label**: the winner's card is marked with a turned label in the win colour; the row starts scrolled to it.
  Turned text and squares are drawn through the host's `draw_text_xf` and `fill_square_xf`, which Horizon's adapter
  transforms (`c1mod/bingo-native/api/src/lib.rs`).
- **Win fireworks**: over the HUD card on the win, and on the end screen's first open of a won game (`fireworks.rs`,
  `client.rs`).
- **Coloured text runs**: a text component is flattened to runs with their colour (`text_plain.rs`); used by score
  messages, the history, the end scores and the game over subtitle.
- **Player heads**: the heads of players in messages, the scores and the game over are fetched from the host and go
  into the atlas, with a retry for a skin that is not downloaded yet (`client.rs` faces, `card_images::face_sprite`).
- **Tooltips on tiles**: hover over a tile shows its name and lore, wrapped at 200 px.
- **Settings sent on join**: `update_settings` is sent once the channels are registered (`client.rs`).
- **Items**: 32 data components are read (`item.rs`), each checked against the real game's bytes
  (`tests/oracle/item_components.tsv`, `tests/oracle_item_components.rs`): `custom_data`, `max_stack_size`, `max_damage`,
  `damage`, `unbreakable`, `custom_name`, `item_name`, `item_model`, `lore`, `rarity`, `enchantments`, `attribute_modifiers`,
  `custom_model_data`, `tooltip_display`, `repair_cost`, `stored_enchantments`, `dyed_color`, `map_id`,
  `charged_projectiles`, `bundle_contents`, `potion_contents`, `trim`, `instrument`, `ominous_bottle_amplifier`,
  `jukebox_playable`, `firework_explosion`, `fireworks`, `banner_patterns`, `base_color`, `container`, `profile`,
  `written_book_content`. The other components are a reported `UnsupportedComponent`, never skipped.

**Tests**: section 10.

## 9. Not done

Each of these is either absent from the code or marked `TODO(integration)` where it would go.

- **Settings screen.** The mod's yacl config screen is not ported. The card screen's `onClose` callbacks for it are
  not either. `screen_controller.rs` says so.
- **Card position config.** `screen_placement.rs` (drag and resize of the card, save and cancel) is pure code that
  nothing calls: it is reachable only through the settings screen, which does not exist yet.
- **Entity tooltips** (`tooltip_v2` over an entity, after 20 ticks). `tooltip_draw.rs` holds the rules, but nothing
  draws it. The packet is read into `tooltips_by_entity`.
- **JEI click.** No code.
- **F3 and Tab hiding** of the HUD. No code.
- **Sneak and Y to ready.** No code. `Y` opens the card screen only.
- **Reset on server change.** Nothing clears the state on a new join; the client only registers again. The state
  is reset by `card_reset` and by a display with no teams. `HudState::reset_all` exists, but nothing in the client
  calls it.
- **Clipping and the history's scroll bar sprites.** The message history is not clipped to its area (`UiDraw` has
  no clip), and the scroll bar's vanilla sprites are not in the mod's atlas, so the history's scroll bar is drawn
  plain (`history_ctl.rs`).
- **Long button labels.** Vanilla scrolls a label wider than its button; here it is drawn from the left, without
  scrolling (`screen_main.rs`).
- **Item components beyond the 13 above.** A stack with another component cannot be read, so the packet that
  carries it is refused.
- **Title, subtitle and vote options of the ready packet.** `ReadyUpdatePacket` refuses a present title, subtitle
  or vote option with `Unsupported`, and the packet changes nothing.

## 10. Tests

`cargo test --offline` in the crate: 484 tests across `src/` and `tests/` (48 test binaries), all passing when this was
written. The number changes as tests are added, so count with
`grep -rc '#\[test\]' tests src | awk -F: '{s+=$2} END{print s}'` (same as `cargo test`'s total).

The oracle tests compare bytes with the mod's; the live tests compare with payloads recorded from a real server;
the rest test the rules with independent expected values. Looks are not checked by any test: they are checked by
eye in a real game, and the notes in section 8 say where that was done.

## 11. Risks

1. **The mod API is a draft** and hooks change between nightlies. The mod is pinned to one nightly's commit.
2. **Looks cannot be checked by an oracle.** HUD and screens need screenshots against the real mod, by eye.
3. **Licence**: the mod is LGPL-3. Port from behaviour, not line by line. Do not copy its textures, lang files or
   sounds into the repo. `tools/check_public_tree.py` also rejects Mojang's files. The assets are read from the
   player's own copy of the mod by `tools/prepare_assets.py`.
4. **Version drift**: the mod ships a build per Minecraft version (26.1, 26.2, 26.3); Horizon is 26.3 only.
5. **Text codec and the registry**: the item and text codecs are checked against the oracle with bootstrapped
   built-in registries only. Dynamic registries (banner patterns, for one) are checked by real captures, not by the
   oracle.

## 12. Decisions for you

- **D1** built-in feature or native mod (section 4). **Native mod, chosen.**
- **D2** scope A only, or A and B (server logic for singleplayer and LAN, 16 behaviour mixins). Still open; scope A
  is what the code does.
- **D3** who opens the issue and claims it (`/claim`): the repo's rules want that before work starts. Still open.

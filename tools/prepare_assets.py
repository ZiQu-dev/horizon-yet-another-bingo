#!/usr/bin/env python3
"""Prepares the folder the bingo client reads its assets from, out of the player's own copy of the original mod.

    prepare_assets.py <mod.jar> <out dir> --registries <horizon-client/26.3/command-registries.json>

Nothing is downloaded and nothing from the mod is kept anywhere but the player's folder, as Horizon does with a
Minecraft install. The item list is the registry order of the player's prepared Horizon resources (the packets carry
item ids as numbers).
"""
import argparse, json, pathlib, sys, zipfile

SPRITES = "assets/minecraft/textures/gui/sprites/bingo/"
# The original's lobby: a saved world ("Bingo Lobby/...") inside the jar. The BINGO! button makes saves/bingo from it.
LOBBY_ZIP = "yet-another-minecraft-bingo/lobby_world.zip"
LOBBY_ROOT = "Bingo Lobby/"
COLORS = "assets/yet-another-minecraft-bingo/gui/card_colors.json"
LANG = "assets/yet-another-minecraft-bingo/lang/en_us.json"
LANG_DIR = "assets/yet-another-minecraft-bingo/lang/"
NS_DIR = "assets/yet-another-minecraft-bingo/"
# Vanilla item tags the card pool expands (#minecraft:music_discs, #c:...). Read from the jar if it has them, else from
# Horizon's prepared data folder beside --registries.
TAG_NAMESPACES = ("minecraft", "c")
# The map card's pictures: item pictures under images/ (16x16, made from the vanilla textures by the mod's build) and
# the overlays at the jar root (the mod's own resources). Written to item_icons/ and map/.
ICON_DIR = "yet-another-minecraft-bingo/images/"
MAP_DIR = "yet-another-minecraft-bingo/"
MAP_FILES = (
    "image_fallback.png", "image_frame_advancement.png", "image_frame_forbidden.png", "image_hidden.png",
    "image_hidden_s.png", "image_hidden_a.png", "image_hidden_b.png", "image_hidden_c.png", "image_hidden_d.png",
    "numbers.png",
)


MANIFEST = """id = "bingo_assets"
name = "Bingo assets"
version = "0.1.0"
description = "Sounds of Yet Another Minecraft BINGO, taken from the player's own copy of the mod for the bingo client."
authors = ["horrificdev (the original mod)"]
api = "0.2"
side = "client"
capabilities = ["registries"]
namespaces = ["yet-another-minecraft-bingo"]
"""


def build_hmod(z: zipfile.ZipFile, names: set, dest: pathlib.Path) -> None:
    """A content mod with the original's sounds.json and .ogg files: the player's machine, the player's copy."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as out:
        out.writestr("horizon-mod.toml", MANIFEST)
        for n in sorted(names):
            if n == NS_DIR + "sounds.json" or (n.startswith(NS_DIR + "sounds/") and n.endswith(".ogg")):
                out.writestr(n, z.read(n))
    print(f"content mod -> {dest}")


def collect_item_tags(z: zipfile.ZipFile, names: set, data_dir: pathlib.Path, out: pathlib.Path) -> int:
    """Writes out/tags/item/<namespace>/<path>.json for each vanilla item tag: from the jar, else from data_dir."""
    written = 0
    for ns in TAG_NAMESPACES:
        prefix = f"data/{ns}/tags/item/"
        for n in sorted(names):
            if n.startswith(prefix) and n.endswith(".json"):
                dest = out / "tags" / "item" / ns / n[len(prefix):]
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(z.read(n))
                written += 1
        if not any(n.startswith(prefix) for n in names):
            folder = data_dir / ns / "tags" / "item"
            if folder.is_dir():
                for f in sorted(folder.rglob("*.json")):
                    dest = out / "tags" / "item" / ns / f.relative_to(folder)
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    dest.write_bytes(f.read_bytes())
                    written += 1
    return written


def extract_lobby(z, names, out):
    """The lobby world of the jar, unpacked to `out/lobby_world/` (its `Bingo Lobby/` folder's contents). 0 when the jar has none."""
    import io
    if LOBBY_ZIP not in names:
        return 0
    count = 0
    root = out / "lobby_world"
    with zipfile.ZipFile(io.BytesIO(z.read(LOBBY_ZIP))) as lobby:
        for n in lobby.namelist():
            if n.endswith("/") or not n.startswith(LOBBY_ROOT):
                continue
            rel = pathlib.PurePosixPath(n[len(LOBBY_ROOT):])
            if rel.is_absolute() or ".." in rel.parts:
                continue
            dest = root.joinpath(*rel.parts)
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(lobby.read(n))
            count += 1
    allow_commands(root / "level.dat")
    return count


def allow_commands(level_dat):
    """Sets `allowCommands` in the lobby's level.dat: a world made from it has cheats on, so its player is an operator.

    The mod's `bingo start`, the START tile and the settings need operator rights (the original's `CONFIGURE_GAME`
    permission), and a single-player world has no operators without cheats. The byte is patched in place, no NBT
    library is needed: a byte tag is `01`, a two-byte name length, the name, the value."""
    import gzip
    if not level_dat.is_file():
        return
    data = bytearray(gzip.decompress(level_dat.read_bytes()))
    tag = b"\x01\x00\x0dallowCommands"
    at = data.find(tag)
    if at < 0 or data[at + len(tag)] == 1:
        return
    data[at + len(tag)] = 1
    level_dat.write_bytes(gzip.compress(bytes(data)))


def main() -> int:
    ap = argparse.ArgumentParser(
        description=(
            "Prepares the asset folder of the Bingo client (Yet Another Minecraft BINGO, the client half for Horizon) "
            "from your own copy of the original mod's jar and from Horizon's prepared resources. Nothing is "
            "downloaded, and the jar is only read."
        ),
        epilog=(
            "Writes into <out dir> (existing files with the same names are overwritten, nothing is deleted):\n"
            "  sprites/bingo/*.png    the mod's card and tile textures\n"
            "  card_colors.json       team colours (gui/card_colors.json)\n"
            "  lang/*.json            the mod's languages; en_us.json is required\n"
            "  sounds.json            the mod's sound events, when the jar has them (informational: the client\n"
            "                         plays sounds from the --hmod content mod)\n"
            "  vanilla_lang/*.json    Minecraft's languages, from assets/minecraft/lang/ beside --registries\n"
            "  item_names.txt         item ids in registry order, from the \"items\" list of --registries\n"
            "  tags/item/<ns>/*.json  vanilla item tags (minecraft, c) the card pool expands; from the jar, else from\n"
            "                         the data folder beside --registries\n"
            "  item_icons/**/*.png    the card's item pictures (yet-another-minecraft-bingo/images/, when the jar has them)\n"
            "  map/*.png              the map card's overlays (image_*.png and numbers.png), when the jar has them\n"
            "\n"
            "Example:\n"
            "  python3 -I prepare_assets.py yet-another-minecraft-bingo.jar <out dir> \\\n"
            "      --registries <resources>/command-registries.json \\\n"
            "      --hmod <game dir>/horizon-mods/bingo_assets.hmod\n"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("jar", help="the original mod's jar file, as downloaded from Modrinth (read only)")
    ap.add_argument("out", help="the folder to write the assets into; created if missing")
    ap.add_argument(
        "--registries",
        required=True,
        help=(
            "command-registries.json of Horizon's prepared resources (the --resources folder). Its \"items\" list "
            "names the items; its folder's assets/minecraft/lang/ gives vanilla's languages"
        ),
    )
    ap.add_argument(
        "--hmod",
        help=(
            "also write a content mod (.hmod) with the mod's sounds (sounds.json and the .ogg files) to this path; "
            "put it in the game folder's horizon-mods/ folder and turn it on"
        ),
    )
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    with zipfile.ZipFile(a.jar) as z:
        names = set(z.namelist())
        for need in (COLORS, LANG):
            if need not in names:
                print(f"{a.jar}: no {need}", file=sys.stderr)
                return 1
        sprites = [n for n in names if n.startswith(SPRITES) and n.endswith(".png")]
        if not sprites:
            print(f"{a.jar}: no sprites under {SPRITES}", file=sys.stderr)
            return 1
        for n in sprites:
            dest = out / "sprites" / "bingo" / n[len(SPRITES):]
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(z.read(n))
        (out / "card_colors.json").write_bytes(z.read(COLORS))
        # Map card pictures: optional, so a jar without them still prepares the rest.
        icons = 0
        for n in names:
            if n.startswith(ICON_DIR) and n.endswith(".png"):
                dest = out / "item_icons" / n[len(ICON_DIR):]
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(z.read(n))
                icons += 1
        maps = 0
        for name in MAP_FILES:
            if MAP_DIR + name in names:
                (out / "map").mkdir(parents=True, exist_ok=True)
                (out / "map" / name).write_bytes(z.read(MAP_DIR + name))
                maps += 1
        print(f"{icons} item pictures, {maps} map overlays")
        (out / "lang").mkdir(parents=True, exist_ok=True)
        (out / "lang" / "en_us.json").write_bytes(z.read(LANG))
        langs = 0
        for n in names:
            if n.startswith(LANG_DIR) and n.endswith(".json"):
                (out / "lang" / n[len(LANG_DIR):]).write_bytes(z.read(n))
                langs += 1
        print(f"{extract_lobby(z, names, out)} lobby world files")
        if NS_DIR + "sounds.json" in names:
            (out / "sounds.json").write_bytes(z.read(NS_DIR + "sounds.json"))
        if a.hmod:
            build_hmod(z, names, pathlib.Path(a.hmod))
    # Vanilla's language files from the same prepared resources (item and block names, chat formats the messages use).
    vanilla = pathlib.Path(a.registries).parent / "assets" / "minecraft" / "lang"
    copied = 0
    if vanilla.is_dir():
        (out / "vanilla_lang").mkdir(parents=True, exist_ok=True)
        for f in sorted(vanilla.glob("*.json")):
            (out / "vanilla_lang" / f.name).write_bytes(f.read_bytes())
            copied += 1
    print(f"{copied} vanilla language files")
    items = json.loads(pathlib.Path(a.registries).read_text())["items"]
    (out / "item_names.txt").write_text("\n".join(items) + "\n")
    tags = collect_item_tags(z, names, pathlib.Path(a.registries).parent / "data", out)
    if tags == 0:
        print("warning: no vanilla item tags found (jar or prepared data); the card pool leaves #tags out", file=sys.stderr)
    print(f"{tags} vanilla item tag files")
    print(f"{len(sprites)} sprites, {len(items)} items, {langs} languages -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

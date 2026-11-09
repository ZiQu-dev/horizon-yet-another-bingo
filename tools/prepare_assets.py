#!/usr/bin/env python3
"""Prepares the folder the bingo client reads its assets from, out of the player's own copy of the original mod.

    prepare_assets.py <mod.jar> <out dir> --registries <horizon-client/26.3/command-registries.json>

Nothing is downloaded and nothing from the mod is kept anywhere but the player's folder, as Horizon does with a
Minecraft install. The item list is the registry order of the player's prepared Horizon resources (the packets carry
item ids as numbers).
"""
import argparse, json, pathlib, sys, zipfile

SPRITES = "assets/minecraft/textures/gui/sprites/bingo/"
COLORS = "assets/yet-another-minecraft-bingo/gui/card_colors.json"
LANG = "assets/yet-another-minecraft-bingo/lang/en_us.json"
LANG_DIR = "assets/yet-another-minecraft-bingo/lang/"
NS_DIR = "assets/yet-another-minecraft-bingo/"


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
        (out / "lang").mkdir(parents=True, exist_ok=True)
        (out / "lang" / "en_us.json").write_bytes(z.read(LANG))
        langs = 0
        for n in names:
            if n.startswith(LANG_DIR) and n.endswith(".json"):
                (out / "lang" / n[len(LANG_DIR):]).write_bytes(z.read(n))
                langs += 1
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
    print(f"{len(sprites)} sprites, {len(items)} items, {langs} languages -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

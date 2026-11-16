#!/usr/bin/env python3
"""Lay out an unpacked .hnmod of the native mod, the way `horizon-launcher native-build` reads a bundle.

    python3 -I tools/pack_hnmod.py --horizon <40-hex commit> --out <dir> [--world-request] [--without-hooks]

Writes `<dir>/horizon-native.json` (the manifest the launcher and the game read) and `<dir>/mixins/bingo_native/`, a copy
of `bingo-native/` with `[compatibility] horizon_revision` set to the commit. The manifest is where the permission lives:
`"world_control": true` lets the mod open a world from the title screen, and `"hooks"` names the native hook version
the mod needs. Without `--world-request` the manifest has neither, and the bundle is the released one: no title button.

The feature build also turns `world-request` on in the copy's api package (the launcher passes no features of its own).
Both are copies: the repository files are not changed.

`--without-hooks` leaves `"hooks"` out of the manifest. The launcher checks a declared hook version against the
revision's `native-hooks.json` on GitHub, so a revision that is not pushed (a local test build of an unmerged Horizon)
is refused with it. The permission (`world_control`) is read from the manifest either way.
"""
import argparse
import json
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "bingo-native"
MOD_ID = "bingo_native"
MOD_VERSION = "0.0.1"
GAME = "26.3"
# The horizon-mixins revision the launcher builds the mod with (native_mods.rs MIXINS_REV); the manifest must name it.
MIXINS_REV = "5a64af938ec86b00d53d16a408aabc0038fb3e57"
# The hook version Horizon stamps for `request_world` (native_hooks.rs, API 646 on the title-button branch).
WORLD_REQUEST_HOOKS = 646
SKIP = {".git", "target", "dev"}


def hex40(text: str) -> bool:
    return re.fullmatch(r"[0-9a-f]{40}", text) is not None


def copy_source(dest: Path, horizon: str, world_request: bool) -> None:
    def ignore(folder, names):
        return [n for n in names if n in SKIP or n.endswith(".hnmod")]

    shutil.copytree(SOURCE, dest, ignore=ignore)
    mixin = dest / "horizon-mixin.toml"
    text = mixin.read_text(encoding="utf-8")
    text, count = re.subn(r'(?m)^horizon_revision = "[0-9a-f]{40}"$', f'horizon_revision = "{horizon}"', text)
    if count != 1:
        raise SystemExit("horizon-mixin.toml has no [compatibility] horizon_revision line to set")
    mixin.write_text(text, encoding="utf-8")
    if world_request:
        api = dest / "api" / "Cargo.toml"
        cargo = api.read_text(encoding="utf-8")
        cargo = cargo.replace("[features]\n", '[features]\ndefault = ["world-request"]\n', 1)
        if 'default = ["world-request"]' not in cargo:
            raise SystemExit("api/Cargo.toml has no [features] table to turn world-request on in")
        api.write_text(cargo, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--horizon", required=True, help="the Horizon commit the mod is made for (40 lowercase hex digits)")
    parser.add_argument("--out", required=True, type=Path, help="the folder to write the unpacked .hnmod into (must not exist)")
    parser.add_argument("--world-request", action="store_true", help="the title-screen button (needs a Horizon with request_world)")
    parser.add_argument("--without-hooks", action="store_true", help="omit \"hooks\" (see above: for a Horizon not pushed to GitHub)")
    args = parser.parse_args()
    if not hex40(args.horizon):
        print("--horizon: a Horizon commit is 40 lowercase hex digits", file=sys.stderr)
        return 2
    if args.out.exists():
        print(f"{args.out} exists: choose a new folder", file=sys.stderr)
        return 2
    manifest = {"schema": 1, "id": MOD_ID, "version": MOD_VERSION, "game": GAME, "horizon": args.horizon, "mixins": MIXINS_REV}
    if args.world_request:
        if not args.without_hooks:
            manifest["hooks"] = WORLD_REQUEST_HOOKS
        manifest["world_control"] = True
    args.out.mkdir(parents=True)
    (args.out / "horizon-native.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    copy_source(args.out / "mixins" / MOD_ID, args.horizon, args.world_request)
    print(f"Wrote {args.out} ({'with' if args.world_request else 'without'} the world-request feature)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

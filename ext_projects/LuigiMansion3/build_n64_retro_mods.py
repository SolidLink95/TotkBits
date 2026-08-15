"""Build LM3's six N64/retro DLC costumes with the proven regular builder.

The script is intentionally lightweight. Generated archives are written to
``tmp/ml3/_mods`` by default, never beside this source file in ``res``.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import lm3_costume_mod_builder as builder


HERE = Path(__file__).resolve().parent
DEFAULT_OUTPUT_ROOT = HERE.parent / "_mods"
TARGETS = (27, 28, 29, 30)

# Each source is the first record of the costume's confirmed retro/N64 pair.
N64_COSTUMES = {
    "green_knight_retro": 40,
    "amazing_luigi_retro": 44,
    "paleontoluigist_retro": 48,
    "groovigi_retro": 52,
    "mummigi_retro": 56,
    "capn_weegee_retro": 60,
}


def build(mod_name: str, source_slot: int, output_root: Path) -> None:
    builder.OUTPUT = output_root / mod_name / "romfs"
    builder.TARGET_SOURCE_PAIRS = tuple((target, source_slot) for target in TARGETS)
    builder.TARGETS = TARGETS
    builder.SOURCES = (source_slot,)
    print(f"building {mod_name}: Persistent {source_slot} -> Global {TARGETS}")
    builder.main()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "mods",
        nargs="*",
        choices=tuple(N64_COSTUMES),
        help="costumes to build; omit to build all six",
    )
    parser.add_argument(
        "--output-root",
        type=Path,
        default=DEFAULT_OUTPUT_ROOT,
        help="generated mod directory (default: tmp/ml3/_mods)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    selected = args.mods or list(N64_COSTUMES)
    for mod_name in selected:
        build(mod_name, N64_COSTUMES[mod_name], args.output_root.resolve())


if __name__ == "__main__":
    main()

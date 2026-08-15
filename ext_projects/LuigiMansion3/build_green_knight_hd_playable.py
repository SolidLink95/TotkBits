"""Build the confirmed gameplay-safe Green Knight HD Story Mode mod."""

from pathlib import Path

import lm3_costume_mod_builder_green_knight_working as builder


REPOSITORY = Path(__file__).parents[2]

builder.OUTPUT = (
    REPOSITORY / "tmp" / "ml3" / "_mods" / "green_knight_hd_working" / "romfs"
)
builder.TARGET_SOURCE_PAIRS = tuple((target, 38) for target in (27, 28, 29, 30))
builder.TARGETS = (27, 28, 29, 30)
builder.SOURCES = (38,)
builder.SHARE_OVERSIZED_BY_KIND = False
builder.KEEP_TARGET_KINDS = set()
builder.REDIRECT_COMPLETE_MODEL_PAIRS = {}
builder.SKELETON_GROUP_PAIRS = ((27, 30), (28, 30))
builder.EMPTY_SOURCE_KINDS = {0xB00A, 0xB00B}


if __name__ == "__main__":
    builder.main()

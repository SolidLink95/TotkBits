"""Build the Cap'n Weegee retro Story Mode mod from Persistent slot 60."""

from pathlib import Path

import lm3_costume_mod_builder as builder


builder.OUTPUT = Path(__file__).parents[1] / "capn_weegee_retro" / "romfs"
builder.TARGET_SOURCE_PAIRS = tuple((target, 60) for target in (27, 28, 29, 30))
builder.TARGETS = (27, 28, 29, 30)
builder.SOURCES = (60,)

if __name__ == "__main__":
    builder.main()

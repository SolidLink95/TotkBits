"""Build the Amazing Luigi retro Story Mode mod from Persistent slot 44."""

from pathlib import Path

import lm3_costume_mod_builder as builder


builder.OUTPUT = Path(__file__).parents[1] / "amazing_luigi_retro" / "romfs"
builder.TARGET_SOURCE_PAIRS = tuple((target, 44) for target in (27, 28, 29, 30))
builder.TARGETS = (27, 28, 29, 30)
builder.SOURCES = (44,)

if __name__ == "__main__":
    builder.main()

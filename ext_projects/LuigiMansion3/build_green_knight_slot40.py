"""Build the Green Knight retro Story Mode mod from Persistent slot 40."""

from pathlib import Path

import lm3_costume_mod_builder as builder


builder.OUTPUT = Path(__file__).parents[1] / "green_knight_retro" / "romfs"
builder.TARGET_SOURCE_PAIRS = tuple((target, 40) for target in (27, 28, 29, 30))
builder.TARGETS = (27, 28, 29, 30)
builder.SOURCES = (40,)

if __name__ == "__main__":
    builder.main()

"""Build the high-detail Groovigi Story Mode mod from Persistent slot 50."""

from pathlib import Path

import lm3_costume_mod_builder as builder


builder.OUTPUT = Path(__file__).parents[1] / "groovigi_hd" / "romfs"
builder.TARGET_SOURCE_PAIRS = tuple((target, 50) for target in (27, 28, 29, 30))
builder.TARGETS = (27, 28, 29, 30)
builder.SOURCES = (50,)

if __name__ == "__main__":
    builder.main()

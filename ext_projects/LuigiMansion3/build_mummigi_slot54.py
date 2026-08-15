"""Build the high-detail Mummigi Story Mode mod from Persistent slot 54."""

from pathlib import Path

import lm3_costume_mod_builder as builder


builder.OUTPUT = Path(__file__).parents[1] / "mummigi_hd" / "romfs"
builder.TARGET_SOURCE_PAIRS = tuple((target, 54) for target in (27, 28, 29, 30))
builder.TARGETS = (27, 28, 29, 30)
builder.SOURCES = (54,)

if __name__ == "__main__":
    builder.main()

"""Build the verified Luigi's Mansion 3 N64 Luigi model replacement."""

from pathlib import Path

from lm3_slot_swap import clone_slots

CWD = Path(__file__).parent
DICT_PATH = CWD / "global.dict"
OUTPUT_DIR = CWD / "n64_luigi\romfs")
SOURCE_SLOT = 31
TARGET_SLOTS = [27, 28, 29, 30]


def main() -> None:
    clone_slots(DICT_PATH, OUTPUT_DIR, TARGET_SLOTS, SOURCE_SLOT)


if __name__ == "__main__":
    main()

from __future__ import annotations

import argparse
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path


ENTRY_SIZE = 16
MODEL_START = 0xB006
MODEL_INFO = 0xB002
TEXTURE_HEADER = 0xB501
TEXTURE_DATA = 0xB502


@dataclass
class FileEntry:
    offset: int
    decompressed_size: int
    compressed_size: int
    unknown1: int
    unknown2: int
    unknown3: int


@dataclass
class SubEntry:
    table_offset: int
    kind: int
    flags: int
    size: int
    offset: int


def read_archive(dict_path: Path) -> tuple[bytearray, bytearray, list[FileEntry], int, bool]:
    dict_data = bytearray(dict_path.read_bytes())
    data_path = dict_path.with_suffix(".data")
    data = bytearray(data_path.read_bytes())
    if len(dict_data) < 16:
        raise ValueError("dictionary header is truncated")
    magic, _unknown, compressed, _padding, _largest, file_count, chunk_count, _strings, _pad = struct.unpack_from(
        "<IHBBIBBBB", dict_data, 0
    )
    if magic != 0xA9F32458:
        raise ValueError(f"unexpected dictionary magic 0x{magic:08X}")
    table_offset = 16 + chunk_count * 24
    entries = []
    for index in range(file_count):
        values = struct.unpack_from("<IIIHBB", dict_data, table_offset + index * ENTRY_SIZE)
        entries.append(FileEntry(*values))
    return dict_data, data, entries, table_offset, bool(compressed)


def decompress_entry(data: bytes, entry: FileEntry, compressed: bool) -> bytes:
    raw = data[entry.offset : entry.offset + (entry.compressed_size if compressed else entry.decompressed_size)]
    return zlib.decompress(raw) if compressed else bytes(raw)


def replace_entry(
    dict_data: bytearray,
    data: bytearray,
    entries: list[FileEntry],
    file_table_offset: int,
    index: int,
    payload: bytes,
    compressed: bool,
) -> None:
    entry = entries[index]
    packed = zlib.compress(payload, level=9) if compressed else payload
    capacity = entry.compressed_size if compressed else entry.decompressed_size
    if packed and len(packed) > capacity:
        raise ValueError(
            f"file entry {index} grew beyond its allocation: {len(packed)} > {capacity}"
        )
    data[entry.offset : entry.offset + capacity] = packed + bytes(capacity - len(packed))
    struct.pack_into("<II", dict_data, file_table_offset + index * ENTRY_SIZE + 4, len(payload), len(packed))
    entry.decompressed_size = len(payload)
    entry.compressed_size = len(packed)


def parse_subentries(table: bytes) -> list[SubEntry]:
    pos = 0
    # First table entries are 24 bytes and begin with 0x1301. The final record
    # omits its trailing unknown u32, which is why Toolbox rewinds two bytes.
    while pos + 2 <= len(table) and struct.unpack_from("<H", table, pos)[0] == 0x1301:
        pos += 24
    # The C# reader consumes the failed u16 probe and rewinds it. Our probe
    # does not advance on failure, so `pos` already points at the subtable.
    result = []
    while pos + 12 <= len(table):
        kind, flags, size, offset = struct.unpack_from("<HHII", table, pos)
        result.append(SubEntry(pos, kind, flags, size, offset))
        pos += 12
    return result


def group_models(subentries: list[SubEntry]) -> list[list[SubEntry]]:
    models: list[list[SubEntry]] = []
    current: list[SubEntry] | None = None
    for entry in subentries:
        if entry.kind == MODEL_START:
            current = []
            models.append(current)
        if current is not None:
            current.append(entry)
    return models


def describe(models: list[list[SubEntry]], slots: list[int]) -> None:
    for slot in slots:
        model = models[slot]
        print(f"slot {slot}: {len(model)} chunks")
        for chunk in model:
            print(f"  {chunk.kind:04X} flags={chunk.flags:04X} offset=0x{chunk.offset:X} size=0x{chunk.size:X}")


def swap_slots(dict_path: Path, output_dir: Path, targets: list[int], source: int) -> None:
    dict_data, data, entries, file_table_offset, compressed = read_archive(dict_path)
    table = bytearray(decompress_entry(data, entries[0], compressed))
    subentries = parse_subentries(table)
    models = group_models(subentries)
    if source >= len(models) or any(target >= len(models) for target in targets):
        raise IndexError(f"archive has {len(models)} model slots")

    source_by_kind: dict[int, list[SubEntry]] = {}
    for chunk in models[source]:
        source_by_kind.setdefault(chunk.kind, []).append(chunk)

    for target in targets:
        occurrences: dict[int, int] = {}
        for chunk in models[target]:
            occurrence = occurrences.get(chunk.kind, 0)
            occurrences[chunk.kind] = occurrence + 1
            candidates = source_by_kind.get(chunk.kind, [])
            if occurrence >= len(candidates):
                raise ValueError(
                    f"slot {target} chunk {chunk.kind:04X} occurrence {occurrence} has no slot {source} equivalent"
                )
            replacement = candidates[occurrence]
            struct.pack_into("<II", table, chunk.table_offset + 4, replacement.size, replacement.offset)

    old_entry = entries[0]
    packed = zlib.compress(bytes(table), level=9) if compressed else bytes(table)
    old_capacity = old_entry.compressed_size if compressed else old_entry.decompressed_size
    if len(packed) > old_capacity:
        raise ValueError(f"patched chunk table grew from {old_capacity} to {len(packed)} bytes")
    data[old_entry.offset : old_entry.offset + old_capacity] = packed + bytes(old_capacity - len(packed))
    struct.pack_into("<II", dict_data, file_table_offset + 4, len(table), len(packed))

    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / dict_path.name).write_bytes(dict_data)
    (output_dir / dict_path.with_suffix(".data").name).write_bytes(data)
    patch_path = dict_path.with_suffix(".patch")
    if patch_path.exists():
        (output_dir / patch_path.name).write_bytes(patch_path.read_bytes())

    # Verify the emitted archive and exact redirects.
    _, emitted_data, emitted_entries, _, emitted_compressed = read_archive(output_dir / dict_path.name)
    emitted_table = decompress_entry(emitted_data, emitted_entries[0], emitted_compressed)
    emitted_models = group_models(parse_subentries(emitted_table))
    for target in targets:
        lhs = [(c.kind, c.size, c.offset) for c in emitted_models[target]]
        rhs = [(c.kind, c.size, c.offset) for c in emitted_models[source]]
        if lhs != rhs:
            raise AssertionError(f"slot {target} does not exactly redirect to slot {source}")
    print(f"wrote verified slot swap to {output_dir}")


def clone_slots(dict_path: Path, output_dir: Path, targets: list[int], source: int) -> None:
    """Clone model data while retaining each target's physical chunk offsets.

    LM3 streams model chunks as owned ranges. Pointing several model records at
    one range parses in extraction tools but is not accepted reliably in-game.
    All source chunks fit in the larger target allocations for this costume set.
    """
    dict_data, data, entries, file_table_offset, compressed = read_archive(dict_path)
    decoded = {
        index: bytearray(decompress_entry(data, entries[index], compressed))
        for index in (0, 52, 53, 54)
    }
    subentries = parse_subentries(decoded[0])
    models = group_models(subentries)
    if source >= len(models) or any(target >= len(models) for target in targets):
        raise IndexError(f"archive has {len(models)} model slots")

    # Chunk storage established by both Switch-Toolbox and fmt_lm3.
    file_for_kind = {
        MODEL_START: 52,
        0xB005: 54,
        0xB00C: 52,
        0xB004: 52,
        0xB00A: 52,
        0xB00B: 52,
        0xB003: 52,
        0xB007: 52,
        0xB001: 52,
        MODEL_INFO: 52,
        0xB100: 53,
        0xB008: 53,
        0xB009: 53,
        0xB101: 52,
        0xB102: 52,
        0xB103: 52,
    }
    source_by_kind: dict[int, list[SubEntry]] = {}
    for chunk in models[source]:
        source_by_kind.setdefault(chunk.kind, []).append(chunk)

    expected: dict[tuple[int, int, int], bytes] = {}
    for target in targets:
        occurrences: dict[int, int] = {}
        for chunk in models[target]:
            occurrence = occurrences.get(chunk.kind, 0)
            occurrences[chunk.kind] = occurrence + 1
            candidates = source_by_kind.get(chunk.kind, [])
            if occurrence >= len(candidates):
                raise ValueError(
                    f"slot {target} chunk {chunk.kind:04X} occurrence {occurrence} "
                    f"has no slot {source} equivalent"
                )
            replacement = candidates[occurrence]
            file_index = file_for_kind.get(chunk.kind)
            if file_index is None:
                raise ValueError(f"no storage file mapping for chunk {chunk.kind:04X}")
            if replacement.size > chunk.size:
                raise ValueError(
                    f"slot {target} chunk {chunk.kind:04X} allocation is too small: "
                    f"0x{chunk.size:X} < 0x{replacement.size:X}"
                )
            source_bytes = bytes(
                decoded[file_index][replacement.offset : replacement.offset + replacement.size]
            )
            start = chunk.offset
            decoded[file_index][start : start + replacement.size] = source_bytes
            # Clear the unused tail so stale target data cannot be consumed.
            decoded[file_index][start + replacement.size : start + chunk.size] = bytes(
                chunk.size - replacement.size
            )
            struct.pack_into("<I", decoded[0], chunk.table_offset + 4, replacement.size)
            expected[(target, chunk.kind, occurrence)] = source_bytes

    for index in (52, 53, 54, 0):
        replace_entry(dict_data, data, entries, file_table_offset, index, bytes(decoded[index]), compressed)

    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / dict_path.name).write_bytes(dict_data)
    (output_dir / dict_path.with_suffix(".data").name).write_bytes(data)
    patch_path = dict_path.with_suffix(".patch")
    if patch_path.exists():
        (output_dir / patch_path.name).write_bytes(patch_path.read_bytes())

    # Re-open every changed file and verify target-owned offsets contain exact clones.
    _, emitted_data, emitted_entries, _, emitted_compressed = read_archive(output_dir / dict_path.name)
    emitted = {
        index: decompress_entry(emitted_data, emitted_entries[index], emitted_compressed)
        for index in (0, 52, 53, 54)
    }
    emitted_models = group_models(parse_subentries(emitted[0]))
    for target in targets:
        occurrences: dict[int, int] = {}
        for chunk in emitted_models[target]:
            occurrence = occurrences.get(chunk.kind, 0)
            occurrences[chunk.kind] = occurrence + 1
            file_index = file_for_kind[chunk.kind]
            actual = emitted[file_index][chunk.offset : chunk.offset + chunk.size]
            if actual != expected[(target, chunk.kind, occurrence)]:
                raise AssertionError(f"slot {target} chunk {chunk.kind:04X} clone mismatch")
    print(f"wrote verified in-place slot clones to {output_dir}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Inspect or redirect LM3 global model slots")
    parser.add_argument("dict", type=Path)
    parser.add_argument("--describe", nargs="+", type=int)
    parser.add_argument("--source", type=int)
    parser.add_argument("--targets", nargs="+", type=int)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--clone", action="store_true", help="copy chunks into target-owned ranges")
    args = parser.parse_args()

    _, data, entries, _, compressed = read_archive(args.dict)
    models = group_models(parse_subentries(decompress_entry(data, entries[0], compressed)))
    print(f"found {len(models)} model slots")
    if args.describe:
        describe(models, args.describe)
    if args.source is not None:
        if not args.targets or args.output is None:
            parser.error("--source requires --targets and --output")
        if args.clone:
            clone_slots(args.dict, args.output, args.targets, args.source)
        else:
            swap_slots(args.dict, args.output, args.targets, args.source)


if __name__ == "__main__":
    main()

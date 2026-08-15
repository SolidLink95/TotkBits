"""Redirect Luigi's four Story head meshes to slot 34's complete head mesh."""

from __future__ import annotations

import struct
import sys
from pathlib import Path


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / "src-tauri" / "misc"))
from lm3_slot_swap import (
    decompress_entry,
    group_models,
    parse_subentries,
    read_archive,
    replace_entry,
)

CLEAN = ROOT / "tmp" / "ml3" / "clean" / "global.dict"
OUTPUT = ROOT / "tmp" / "ml3" / "_mods" / "slot34_head_transplant" / "romfs"
SOURCE_SLOT = 34
SOURCE_MESH_INDEX = 1
TARGET_MESHES = {27: 7, 28: 2, 29: 3, 30: 0}


def mesh_records(files: dict[int, bytes | bytearray], model: list, mesh_index: int):
    b003 = next(record for record in model if record.kind == 0xB003)
    b004 = next(record for record in model if record.kind == 0xB004)
    b005 = next(record for record in model if record.kind == 0xB005)
    if not 0 <= mesh_index < b003.size // 0x40:
        raise IndexError(f"mesh index {mesh_index} is outside model")
    b004_cursor = b004.offset
    for index in range(mesh_index):
        descriptor = b003.offset + index * 0x40
        skinned = struct.unpack_from("<I", files[52], descriptor + 0x28)[0] != 0xFFFFFFFF
        b004_cursor += 16 if skinned else 12
    descriptor = b003.offset + mesh_index * 0x40
    skinned = struct.unpack_from("<I", files[52], descriptor + 0x28)[0] != 0xFFFFFFFF
    return descriptor, b004_cursor, 16 if skinned else 12, b005


def main() -> None:
    dictionary, data, entries, table_offset, compressed = read_archive(CLEAN)
    files = {
        index: bytearray(decompress_entry(data, entries[index], compressed))
        for index in (0, 52, 54)
    }
    models = group_models(parse_subentries(files[0]))
    source_descriptor, source_b004, source_b004_size, source_b005 = mesh_records(
        files, models[SOURCE_SLOT], SOURCE_MESH_INDEX
    )
    if source_b004_size != 16:
        raise ValueError("slot 34 head mesh is unexpectedly unskinned")
    source_hash, source_index_offset, source_flags, source_vertices = struct.unpack_from(
        "<IIII", files[52], source_descriptor
    )
    source_pointers = struct.unpack_from("<IIII", files[52], source_b004)

    for target_slot, target_mesh_index in TARGET_MESHES.items():
        target_descriptor, target_b004, target_b004_size, target_b005 = mesh_records(
            files, models[target_slot], target_mesh_index
        )
        if target_b004_size != source_b004_size:
            raise ValueError(f"slot {target_slot} head skinning layout differs from slot 34")
        target_hash = struct.unpack_from("<I", files[52], target_descriptor)[0]
        # Preserve the target identity/material binding hash, but adopt all
        # geometry counts and flags from the complete slot-34 head descriptor.
        files[52][target_descriptor : target_descriptor + 0x40] = files[52][
            source_descriptor : source_descriptor + 0x40
        ]
        struct.pack_into("<I", files[52], target_descriptor, target_hash)
        delta = source_b005.offset - target_b005.offset
        struct.pack_into("<I", files[52], target_descriptor + 4, source_index_offset + delta)
        struct.pack_into(
            "<IIII", files[52], target_b004,
            *(pointer + delta for pointer in source_pointers),
        )
        print(
            f"slot {target_slot} mesh {target_mesh_index}: {target_hash:08X} -> "
            f"slot 34 head {source_hash:08X}, {source_vertices} vertices, "
            f"{source_flags & 0xFFFFFF} indices"
        )

    replace_entry(dictionary, data, entries, table_offset, 52, bytes(files[52]), compressed)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    (OUTPUT / "global.dict").write_bytes(dictionary)
    (OUTPUT / "global.data").write_bytes(data)
    (OUTPUT / "global.patch").write_bytes(CLEAN.with_suffix(".patch").read_bytes())

    # Verify the changed metadata entry survives compression and reopening.
    _, emitted_data, emitted_entries, _, emitted_compressed = read_archive(
        OUTPUT / "global.dict"
    )
    emitted52 = decompress_entry(emitted_data, emitted_entries[52], emitted_compressed)
    if emitted52 != bytes(files[52]):
        raise AssertionError("emitted mesh metadata did not round-trip")
    if len(data) != len(CLEAN.with_suffix(".data").read_bytes()):
        raise AssertionError("global.data physical layout changed")
    print(f"wrote verified slot-34 head transplant to {OUTPUT}")


if __name__ == "__main__":
    main()

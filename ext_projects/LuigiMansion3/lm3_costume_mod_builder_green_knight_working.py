from __future__ import annotations

import struct
import sys
from collections import Counter
from pathlib import Path

sys.path[:0] = [
    r"W:\coding\TotkBits\src-tauri\misc",
    r"W:\coding\TotkBits\tmp\ml3",
]

from lm3_import_mummigi import decode_archive, texture_records
from lm3_slot_swap import (
    decompress_entry,
    group_models,
    parse_subentries,
    read_archive,
    replace_entry,
)

GLOBAL = Path(r"W:\coding\TotkBits\tmp\ml3\clean\global.dict")
PERSISTENT = Path(r"E:\Yuzu\dumps\LM3\romfs\Scarescraper\Persistent.dict")
OUTPUT = Path(r"W:\coding\TotkBits\tmp\ml3\mummigi_story_mod\romfs")
# Use one complete high-detail costume model for every Story Luigi record.
# Persistent slot 50 is the highest-vertex candidate whose complete set of 16
# chunks fits the owned allocations of every Global Story slot.
TARGET_SOURCE_PAIRS = ((27, 50), (28, 50), (29, 50), (30, 50))
TARGETS = tuple(target for target, _source in TARGET_SOURCE_PAIRS)
SOURCES = tuple(source for _target, source in TARGET_SOURCE_PAIRS)

FILE_FOR_KIND = {
    0xB006: 52, 0xB005: 54, 0xB00C: 52, 0xB004: 52,
    0xB00A: 52, 0xB00B: 52, 0xB003: 52, 0xB007: 52,
    0xB001: 52, 0xB002: 52, 0xB100: 53, 0xB008: 53,
    0xB009: 53, 0xB101: 52, 0xB102: 52, 0xB103: 52,
}

# Story Mode drives Luigi's animated face/eye atlas independently of the
# costume's static B006 materials. Keep these small target-owned runtime records
# so the imported model uses Story's facial state metadata instead of resolving
# the costume metadata against an unrelated Global atlas.
KEEP_TARGET_KINDS = {0xB008, 0xB009}
PRESERVE_GLOBAL_TEXTURES = {0xAE8737B7}  # Luigi's animated blue eye
SHARE_OVERSIZED_BY_KIND = False
REDIRECT_COMPLETE_MODEL_PAIRS = {}
# Optional (Global skeleton group, Persistent skeleton group) payload clones.
# Skeleton secondary chunks 7101-7106 all live in file 53.
SKELETON_GROUP_PAIRS = ()
# Source chunks that should remain present in the table but contain no payload.
# This is useful for optional runtime deformation data that a smaller target
# allocation cannot safely relocate.
EMPTY_SOURCE_KINDS = set()


def align(value: int, alignment: int = 16) -> int:
    return (value + alignment - 1) & -alignment


def mesh_layout(files, model):
    b003 = next(record for record in model if record.kind == 0xB003)
    b004 = next(record for record in model if record.kind == 0xB004)
    b003_size = struct.unpack_from("<I", files[0], b003.table_offset + 4)[0]
    result = {}
    b004_position = b004.offset
    for index in range(b003_size // 0x40):
        descriptor = b003.offset + index * 0x40
        mesh_hash, index_offset, index_flags, vertex_count = struct.unpack_from(
            "<IIII", files[52], descriptor
        )
        skinned = struct.unpack_from("<I", files[52], descriptor + 0x28)[0] != 0xFFFFFFFF
        b004_size = 16 if skinned else 12
        skin_offset = None
        if skinned:
            skin_offset = struct.unpack_from("<I", files[52], b004_position)[0]
            vertex_offset = struct.unpack_from("<I", files[52], b004_position + 4)[0]
        else:
            vertex_offset = struct.unpack_from("<I", files[52], b004_position)[0]
        result[mesh_hash] = {
            "descriptor": descriptor,
            "b004": b004_position,
            "b004_size": b004_size,
            "index_offset": index_offset,
            "index_size": (index_flags & 0xFFFFFF) * (1 if index_flags >> 24 == 0x80 else 2),
            "vertex_count": vertex_count,
            "vertex_offset": vertex_offset,
            "skin_offset": skin_offset,
        }
        b004_position += b004_size
    return result


def material_layout(files, model):
    b003 = next(record for record in model if record.kind == 0xB003)
    b006 = next(record for record in model if record.kind == 0xB006)
    b007 = next(record for record in model if record.kind == 0xB007)
    b003_size = struct.unpack_from("<I", files[0], b003.table_offset + 4)[0]
    b006_size = struct.unpack_from("<I", files[0], b006.table_offset + 4)[0]
    mesh_hashes = [
        struct.unpack_from("<I", files[52], b003.offset + index * 0x40)[0]
        for index in range(b003_size // 0x40)
    ]
    material_flag = bytes.fromhex(
        "FFFFFFFFFFFFFFFFFFFFFFFF00000000FFFFFFFFFFFFFFFF00000000"
    )
    bindings = []
    position = b007.offset
    while len(bindings) < len(mesh_hashes) and position + 28 <= b007.offset + b007.size:
        if files[52][position : position + 28] == material_flag:
            relative = struct.unpack_from("<I", files[52], position - 4)[0] - 8
            bindings.append(relative)
        position += 4
    if len(bindings) != len(mesh_hashes):
        raise ValueError("could not resolve every mesh material binding")
    boundaries = sorted(set(bindings) | {b006_size})
    return {
        mesh_hash: {
            "offset": b006.offset + relative,
            "size": min(boundary for boundary in boundaries if boundary > relative) - relative,
        }
        for mesh_hash, relative in zip(mesh_hashes, bindings)
    }


def transplant_story_eyes(changed, original, model):
    """Use Story's eye geometry/UVs with the imported costume model."""
    target_b005 = next(record for record in model if record.kind == 0xB005)
    current_size = struct.unpack_from("<I", changed[0], target_b005.table_offset + 4)[0]
    cursor = align(current_size)
    story = mesh_layout(original, model)
    costume = mesh_layout(changed, model)
    story_materials = material_layout(original, model)
    costume_materials = material_layout(changed, model)

    for mesh_hash in EYE_MESH_HASHES:
        source_eye = story[mesh_hash]
        target_eye = costume[mesh_hash]
        if source_eye["b004_size"] != target_eye["b004_size"]:
            raise ValueError(f"eye mesh {mesh_hash:08X} changed skinning layout")
        source_material = story_materials[mesh_hash]
        target_material = costume_materials[mesh_hash]
        if source_material["size"] != target_material["size"]:
            raise ValueError(f"eye mesh {mesh_hash:08X} changed material layout")
        changed[52][
            target_material["offset"] : target_material["offset"] + target_material["size"]
        ] = original[52][
            source_material["offset"] : source_material["offset"] + source_material["size"]
        ]

        # Retain the costume's mesh ordering, but use the complete Story eye
        # descriptor and its UV-bearing buffers at new target-owned offsets.
        descriptor = bytes(original[52][
            source_eye["descriptor"] : source_eye["descriptor"] + 0x40
        ])
        changed[52][target_eye["descriptor"] : target_eye["descriptor"] + 0x40] = descriptor
        b004 = bytes(original[52][
            source_eye["b004"] : source_eye["b004"] + source_eye["b004_size"]
        ])
        changed[52][
            target_eye["b004"] : target_eye["b004"] + target_eye["b004_size"]
        ] = b004

        index_data = bytes(original[54][
            target_b005.offset + source_eye["index_offset"] :
            target_b005.offset + source_eye["index_offset"] + source_eye["index_size"]
        ])
        index_offset = cursor
        changed[54][target_b005.offset + cursor : target_b005.offset + cursor + len(index_data)] = index_data
        cursor = align(cursor + len(index_data))

        skin_offset = None
        if source_eye["skin_offset"] is not None:
            skin_size = source_eye["vertex_count"] * 0x14
            skin_data = bytes(original[54][
                target_b005.offset + source_eye["skin_offset"] :
                target_b005.offset + source_eye["skin_offset"] + skin_size
            ])
            skin_offset = cursor
            changed[54][target_b005.offset + cursor : target_b005.offset + cursor + len(skin_data)] = skin_data
            cursor = align(cursor + len(skin_data))

        vertex_size = source_eye["vertex_count"] * 0x30
        vertex_data = bytes(original[54][
            target_b005.offset + source_eye["vertex_offset"] :
            target_b005.offset + source_eye["vertex_offset"] + vertex_size
        ])
        vertex_offset = cursor
        changed[54][target_b005.offset + cursor : target_b005.offset + cursor + len(vertex_data)] = vertex_data
        cursor = align(cursor + len(vertex_data))

        struct.pack_into("<I", changed[52], target_eye["descriptor"] + 4, index_offset)
        if skin_offset is not None:
            struct.pack_into("<II", changed[52], target_eye["b004"], skin_offset, vertex_offset)
        else:
            struct.pack_into("<I", changed[52], target_eye["b004"], vertex_offset)

    if cursor > target_b005.size:
        raise ValueError(
            f"Story eye buffers exceed slot B005 allocation: {cursor:#x} > {target_b005.size:#x}"
        )
    struct.pack_into("<I", changed[0], target_b005.table_offset + 4, cursor)
    return cursor

def model_texture_refs(models, files, textures):
    result = []
    for model in models:
        material = next(record for record in model if record.kind == 0xB006)
        payload = files[52][material.offset : material.offset + material.size]
        result.append({
            struct.unpack_from("<I", payload, offset)[0]
            for offset in range(0, len(payload) - 3, 4)
            if struct.unpack_from("<I", payload, offset)[0] in textures
        })
    return result


def replace_u32(payload: bytes, replacements: dict[int, int]) -> bytes:
    output = bytearray(payload)
    for offset in range(0, len(output) - 3, 4):
        value = struct.unpack_from("<I", output, offset)[0]
        if value in replacements:
            struct.pack_into("<I", output, offset, replacements[value])
    return bytes(output)


def group_skeletons(subentries):
    groups = []
    current = None
    for record in subentries:
        if record.kind == 0x7101:
            current = []
            groups.append(current)
        if current is not None and 0x7101 <= record.kind <= 0x7106:
            current.append(record)
    return groups


def main():
    dictionary, data, entries, table_offset, compressed = read_archive(GLOBAL)
    changed = {
        index: bytearray(decompress_entry(data, entries[index], compressed))
        for index in (0, 52, 53, 54, 63, 65)
    }
    original = {index: bytes(changed[index]) for index in (0, 52, 54, 63, 65)}
    _, _, _, _, _, persistent = decode_archive(PERSISTENT)
    global_subentries = parse_subentries(changed[0])
    source_subentries = parse_subentries(persistent[0])
    global_models = group_models(global_subentries)
    source_models = group_models(source_subentries)
    global_textures = texture_records(changed[0], changed[63])
    source_textures = texture_records(persistent[0], persistent[63])
    global_refs = model_texture_refs(global_models, changed, global_textures)
    source_refs = model_texture_refs(source_models, persistent, source_textures)

    counts = Counter(texture for refs in global_refs for texture in refs)
    owned_pool = {
        texture
        for slot in (27, 28, 29, 30)
        for texture in global_refs[slot]
        if texture not in PRESERVE_GLOBAL_TEXTURES
        if counts[texture] == sum(
            texture in global_refs[index] for index in (27, 28, 29, 30)
        )
    }
    available = sorted(
        (global_textures[texture][1].size, texture) for texture in owned_pool
    )
    mapping = {}
    needed_textures = set().union(*(source_refs[source] for source in SOURCES))
    for source_hash in sorted(
        needed_textures,
        key=lambda value: source_textures[value][1].size,
        reverse=True,
    ):
        size = source_textures[source_hash][1].size
        fits = [(capacity, texture) for capacity, texture in available if capacity >= size]
        if not fits:
            raise ValueError(f"no owned texture allocation fits {source_hash:08X}")
        capacity, target_hash = min(fits)
        available.remove((capacity, target_hash))
        mapping[source_hash] = target_hash

    expected_model = {}
    plans = []
    for target, source in TARGET_SOURCE_PAIRS:
        source_by_kind = {record.kind: record for record in source_models[source]}
        for target_record in global_models[target]:
            if target_record.kind in KEEP_TARGET_KINDS:
                plans.append({
                    "target": target, "record": target_record, "payload": None,
                    "file": FILE_FOR_KIND[target_record.kind],
                })
                continue
            source_record = source_by_kind[target_record.kind]
            file_index = FILE_FOR_KIND[target_record.kind]
            if target_record.kind in EMPTY_SOURCE_KINDS:
                payload = b""
            else:
                payload = bytes(persistent[file_index][
                    source_record.offset : source_record.offset + source_record.size
                ])
            if target_record.kind == 0xB006:
                payload = replace_u32(payload, mapping)
            plans.append({
                "target": target, "record": target_record,
                "payload": payload, "file": file_index,
            })
            expected_model[target, target_record.kind] = payload

    # First fill every chunk that fits its original target-owned range. Ranges
    # belonging to oversized chunks remain available as relocation storage.
    allocations = []
    oversized = []
    for plan in plans:
        record = plan["record"]
        payload = plan["payload"]
        if payload is None:
            cursor = record.size
        elif len(payload) <= record.size:
            start = record.offset
            changed[plan["file"]][start : start + len(payload)] = payload
            changed[plan["file"]][start + len(payload) : start + record.size] = bytes(
                record.size - len(payload)
            )
            struct.pack_into("<II", changed[0], record.table_offset + 4, len(payload), start)
            cursor = align(len(payload))
        else:
            oversized.append(plan)
            cursor = 0
        allocations.append({
            "file": plan["file"], "offset": record.offset,
            "capacity": record.size, "cursor": cursor,
            "owner": (plan["target"], record.kind),
        })

    # Oversized metadata is placed at a distinct aligned offset in an unused
    # tail of another target-owned range from the same storage file. No archive
    # entry grows and no two model records share payload bytes.
    for plan in sorted(oversized, key=lambda item: len(item["payload"]), reverse=True):
        payload = plan["payload"]
        if SHARE_OVERSIZED_BY_KIND:
            donors = [
                candidate for candidate in plans
                if candidate["record"].kind == plan["record"].kind
                and candidate["payload"] == payload
                and len(payload) <= candidate["record"].size
            ]
            if donors:
                preferred_target = 29 if plan["target"] == 27 else 30
                donor = next(
                    (candidate for candidate in donors if candidate["target"] == preferred_target),
                    donors[0],
                )
                record = plan["record"]
                struct.pack_into(
                    "<II", changed[0], record.table_offset + 4,
                    len(payload), donor["record"].offset,
                )
                print(
                    f"redirected slot {plan['target']} chunk {record.kind:04X} "
                    f"to typed slot {donor['target']} allocation"
                )
                continue
        candidates = []
        for allocation in allocations:
            if allocation["file"] != plan["file"]:
                continue
            start = align(allocation["cursor"])
            remaining = allocation["capacity"] - start
            if remaining >= len(payload):
                candidates.append((remaining - len(payload), allocation, start))
        if not candidates:
            record = plan["record"]
            raise ValueError(
                f"no target-owned relocation range fits slot {plan['target']} "
                f"chunk {record.kind:04X} ({len(payload):#x} bytes)"
            )
        _waste, allocation, relative = min(candidates, key=lambda item: item[0])
        start = allocation["offset"] + relative
        changed[plan["file"]][start : start + len(payload)] = payload
        allocation["cursor"] = relative + len(payload)
        record = plan["record"]
        struct.pack_into("<II", changed[0], record.table_offset + 4, len(payload), start)
        print(
            f"relocated slot {plan['target']} chunk {record.kind:04X} "
            f"to {allocation['owner']}+{relative:#x}"
        )

    # Some model metadata contains cross-chunk location relationships. For
    # oversized HD costumes, redirect a small target as a complete unit to a
    # coherent copy stored in one of the large Story allocations.
    for target, donor in REDIRECT_COMPLETE_MODEL_PAIRS.items():
        donor_by_kind = {record.kind: record for record in global_models[donor]}
        for record in global_models[target]:
            donor_record = donor_by_kind[record.kind]
            size, offset = struct.unpack_from(
                "<II", changed[0], donor_record.table_offset + 4
            )
            struct.pack_into("<II", changed[0], record.table_offset + 4, size, offset)
        print(f"redirected complete model slot {target} to coherent slot {donor} copy")

    expected_skeletons = {}
    if SKELETON_GROUP_PAIRS:
        global_skeletons = group_skeletons(global_subentries)
        source_skeletons = group_skeletons(source_subentries)
        for target_group, source_group in SKELETON_GROUP_PAIRS:
            target_by_kind = {
                record.kind: record for record in global_skeletons[target_group]
            }
            source_by_kind = {
                record.kind: record for record in source_skeletons[source_group]
            }
            if target_by_kind.keys() != source_by_kind.keys():
                raise ValueError(
                    f"skeleton group {target_group} chunk layout differs from {source_group}"
                )
            for kind, target_record in target_by_kind.items():
                source_record = source_by_kind[kind]
                if source_record.size > target_record.size:
                    raise ValueError(
                        f"skeleton group {target_group} chunk {kind:04X} is too small: "
                        f"{target_record.size:#x} < {source_record.size:#x}"
                    )
                payload = bytes(persistent[53][
                    source_record.offset : source_record.offset + source_record.size
                ])
                changed[53][
                    target_record.offset : target_record.offset + source_record.size
                ] = payload
                changed[53][
                    target_record.offset + source_record.size :
                    target_record.offset + target_record.size
                ] = bytes(target_record.size - source_record.size)
                struct.pack_into(
                    "<I", changed[0], target_record.table_offset + 4, source_record.size
                )
                expected_skeletons[target_group, kind] = payload
            print(
                f"cloned Persistent skeleton group {source_group} "
                f"into Global group {target_group}"
            )

    for source_hash, target_hash in mapping.items():
        source_header, source_image = source_textures[source_hash]
        target_header, target_image = global_textures[target_hash]
        header = bytes(persistent[63][
            source_header.offset : source_header.offset + source_header.size
        ])
        header = replace_u32(header, {source_hash: target_hash})
        image = bytes(persistent[65][
            source_image.offset : source_image.offset + source_image.size
        ])
        changed[63][target_header.offset : target_header.offset + len(header)] = header
        changed[65][target_image.offset : target_image.offset + len(image)] = image
        changed[65][target_image.offset + len(image) : target_image.offset + target_image.size] = bytes(
            target_image.size - len(image)
        )
        struct.pack_into("<I", changed[0], target_image.table_offset + 4, len(image))

    for index in (52, 53, 54, 63, 65, 0):
        replace_entry(
            dictionary, data, entries, table_offset, index,
            bytes(changed[index]), compressed,
        )

    OUTPUT.mkdir(parents=True, exist_ok=True)
    (OUTPUT / "global.dict").write_bytes(dictionary)
    (OUTPUT / "global.data").write_bytes(data)
    (OUTPUT / "global.patch").write_bytes(GLOBAL.with_suffix(".patch").read_bytes())
    if len(data) != len(GLOBAL.with_suffix(".data").read_bytes()):
        raise AssertionError("global.data physical layout changed")

    # Reopen the output and verify all four runtime-selectable Story Luigi slots.
    _, emitted_data, emitted_entries, _, emitted_compressed = read_archive(
        OUTPUT / "global.dict"
    )
    emitted = {
        index: decompress_entry(emitted_data, emitted_entries[index], emitted_compressed)
        for index in (0, 52, 53, 54, 63, 65)
    }
    emitted_models = group_models(parse_subentries(emitted[0]))
    emitted_skeletons = group_skeletons(parse_subentries(emitted[0]))
    emitted_textures = texture_records(emitted[0], emitted[63])
    for texture_hash in PRESERVE_GLOBAL_TEXTURES:
        original_header, original_image = global_textures[texture_hash]
        emitted_header, emitted_image = emitted_textures[texture_hash]
        if original[63][original_header.offset : original_header.offset + original_header.size] != emitted[63][emitted_header.offset : emitted_header.offset + emitted_header.size]:
            raise AssertionError(f"preserved texture {texture_hash:08X} header changed")
        original_pixels = original[65][original_image.offset : original_image.offset + original_image.size]
        emitted_pixels = emitted[65][emitted_image.offset : emitted_image.offset + emitted_image.size]
        if original_pixels != emitted_pixels:
            raise AssertionError(f"preserved texture {texture_hash:08X} image changed")
    for target in TARGETS:
        for record in emitted_models[target]:
            if record.kind in KEEP_TARGET_KINDS:
                continue
            file_index = FILE_FOR_KIND[record.kind]
            actual = emitted[file_index][record.offset : record.offset + record.size]
            if actual != expected_model[target, record.kind]:
                raise AssertionError(
                    f"slot {target} model chunk {record.kind:04X} clone mismatch"
                )
    for (target_group, kind), expected in expected_skeletons.items():
        record = next(
            record for record in emitted_skeletons[target_group]
            if record.kind == kind
        )
        actual = emitted[53][record.offset : record.offset + record.size]
        if actual != expected:
            raise AssertionError(
                f"skeleton group {target_group} chunk {kind:04X} clone mismatch"
            )
    print(
        f"wrote fully in-place Mummigi models {TARGET_SOURCE_PAIRS} "
        f"with {len(mapping)} owned textures"
    )


if __name__ == "__main__":
    main()

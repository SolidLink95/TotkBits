"""Build the supplied Mario FBX/texture replacement for Story Luigi slots."""

from __future__ import annotations

import struct
import subprocess
import sys
import json
from pathlib import Path

from PIL import Image


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path[:0] = [str(ROOT / "src-tauri" / "misc"), str(ROOT / "tmp" / "ml3")]
from lm3_import_mummigi import texture_records
from lm3_slot_swap import (
    decompress_entry, group_models, parse_subentries, read_archive, replace_entry,
)

CLEAN = ROOT / "tmp" / "ml3" / "clean" / "global.dict"
ASSET_ROOT = ROOT / "tmp" / "ml3" / "Mario LM3"
TEXTURES = ASSET_ROOT / "tex"
ASTCENC = ROOT / "tmp" / "ml3" / "tools" / "astcenc-5.7.0" / "bin" / "astcenc-avx2.exe"
TEMP = ROOT / "tmp" / "ml3" / "mario_astc_temp"
OUTPUT = ROOT / "tmp" / "ml3" / "_mods" / "mario_fbx_replacement" / "romfs"
TARGETS = (27, 28, 29, 30)
BLENDER_UP_OFFSET = 0.128029
FBX_SKIN = ASSET_ROOT / "Mario_to_luigi2.skin.json"
FBX_NAME_FOR_SOURCE = {0: "submesh_14"}
SKELETON_GROUP_FOR_SLOT = {27: 27, 28: 27, 29: 28, 30: 28}

# Only FBX submeshes 5, 6, 7, and 14 differ. Their geometry is exactly the
# corresponding slot-34 mesh, so reuse the already resident buffers losslessly.
SOURCE_FOR_TARGET_HASHES = {
    0: {0xA92A6B5E, 0xCE77ED18},                  # FBX submesh 14
}


def mesh_record(file52, model, mesh_index):
    b003 = next(record for record in model if record.kind == 0xB003)
    b004 = next(record for record in model if record.kind == 0xB004)
    b005 = next(record for record in model if record.kind == 0xB005)
    cursor = b004.offset
    for index in range(mesh_index):
        descriptor = b003.offset + index * 0x40
        cursor += 16 if struct.unpack_from("<I", file52, descriptor + 0x28)[0] != 0xFFFFFFFF else 12
    descriptor = b003.offset + mesh_index * 0x40
    size = 16 if struct.unpack_from("<I", file52, descriptor + 0x28)[0] != 0xFFFFFFFF else 12
    return descriptor, cursor, size, b005


def find_mesh(file52, model, hashes):
    b003 = next(record for record in model if record.kind == 0xB003)
    for index in range(b003.size // 0x40):
        value = struct.unpack_from("<I", file52, b003.offset + index * 0x40)[0]
        if value in hashes:
            return index
    raise ValueError(f"model lacks expected mesh hashes {[f'{h:08X}' for h in hashes]}")


def move_source_mesh_up(files, model, mesh_index):
    descriptor, b004, b004_size, b005 = mesh_record(files[52], model, mesh_index)
    if b004_size != 16:
        raise ValueError(f"slot 34 mesh {mesh_index} is unexpectedly unskinned")
    vertex_count = struct.unpack_from("<I", files[52], descriptor + 0x0C)[0]
    vertex_offset = struct.unpack_from("<I", files[52], b004 + 4)[0]
    vertex_base = b005.offset + vertex_offset
    for index in range(vertex_count):
        position = vertex_base + index * 0x30
        z = struct.unpack_from("<f", files[54], position + 8)[0]
        struct.pack_into("<f", files[54], position + 8, z + BLENDER_UP_OFFSET)
    print(
        f"moved slot 34 mesh {mesh_index} ({vertex_count} vertices) "
        f"up +{BLENDER_UP_OFFSET} m on Blender Z"
    )


def skeleton_groups(table):
    groups = []
    current = None
    for record in parse_subentries(table):
        if record.kind == 0x7101:
            current = []
            groups.append(current)
        if current is not None and 0x7101 <= record.kind <= 0x7106:
            current.append(record)
    return groups


def skeleton_id_to_hash(files, group):
    record = next(item for item in group if item.kind == 0x7105)
    result = {}
    for offset in range(record.offset, record.offset + record.size, 8):
        bone_hash, bone_id = struct.unpack_from("<II", files[53], offset)
        result[bone_id] = bone_hash
    return result


def skin_from_fbx(vertex_weights, model, id_to_hash, file52):
    b103 = next(record for record in model if record.kind == 0xB103)
    hashes = [struct.unpack_from("<I", file52, b103.offset + offset)[0]
              for offset in range(0, b103.size, 4)]
    hash_to_b103 = {bone_hash: index for index, bone_hash in enumerate(hashes)}
    output = bytearray()
    for vertex_index, influences in enumerate(vertex_weights):
        resolved = []
        for name, weight in influences:
            if not name.startswith("bone_"):
                continue
            bone_id = int(name[5:])
            if bone_id not in id_to_hash:
                raise ValueError(f"FBX bone {name} is missing from target skeleton")
            bone_hash = id_to_hash[bone_id]
            if bone_hash not in hash_to_b103:
                raise ValueError(
                    f"FBX bone {name}/{bone_hash:08X} is missing from target B103"
                )
            resolved.append((hash_to_b103[bone_hash], float(weight)))
        resolved.sort(key=lambda item: item[1], reverse=True)
        resolved = resolved[:4]
        total = sum(weight for _index, weight in resolved)
        if total <= 1e-8:
            raise ValueError(f"FBX vertex {vertex_index} has no usable skin weight")
        ids = [item[0] for item in resolved] + [0] * (4 - len(resolved))
        weights = [item[1] / total for item in resolved]
        # Quantize deliberately to float32, then make the final influence the
        # exact float32 remainder so the stored four weights sum to 1.0.
        weights = [struct.unpack("<f", struct.pack("<f", value))[0] for value in weights]
        if weights:
            weights[-1] = struct.unpack(
                "<f", struct.pack("<f", 1.0 - sum(weights[:-1]))
            )[0]
        weights += [0.0] * (4 - len(weights))
        output.extend(struct.pack("<BBBBffff", *ids, *weights))
    return bytes(output)


def rigid_skin_from_original(payload, original_vertex_count, new_vertex_count):
    totals = {}
    for vertex in range(original_vertex_count):
        position = vertex * 0x14
        ids = payload[position : position + 4]
        weights = struct.unpack_from("<ffff", payload, position + 4)
        for bone_id, weight in zip(ids, weights):
            if weight > 0.0:
                totals[bone_id] = totals.get(bone_id, 0.0) + weight
    if not totals:
        raise ValueError("original target mesh has no usable skin weights")
    dominant = max(totals, key=totals.get)
    record = struct.pack("<BBBBffff", dominant, 0, 0, 0, 1.0, 0.0, 0.0, 0.0)
    return record * new_vertex_count, dominant


def block_height(width, height):
    result = 8 if width <= 256 or height <= 256 else 16
    result = 4 if width <= 128 or height <= 128 else result
    return 2 if width <= 64 or height <= 64 else result


def block_address(x, y, width_blocks, height):
    width_in_gobs = (width_blocks * 16 + 63) // 64
    gob = ((y // (8 * height)) * 512 * height * width_in_gobs
           + (x * 16 // 64) * 512 * height
           + ((y % (8 * height)) // 8) * 512)
    return (gob + ((x * 16 % 64) // 32) * 256 + ((y % 8) // 2) * 64
            + ((x * 16 % 32) // 16) * 32 + (y % 2) * 16 + (x * 16 % 16))


def tile_astc(linear, width, height):
    width_blocks = (width + 7) // 8
    height_blocks = (height + 4) // 5
    gob_height = block_height(width, height)
    width_gobs = (width_blocks * 16 + 63) // 64
    rows = (height_blocks + 8 * gob_height - 1) // (8 * gob_height)
    output = bytearray(rows * 512 * gob_height * width_gobs)
    for y in range(height_blocks):
        for x in range(width_blocks):
            source = (y * width_blocks + x) * 16
            target = block_address(x, y, width_blocks, gob_height)
            output[target : target + 16] = linear[source : source + 16]
    return bytes(output)


def encode_texture(png: Path):
    TEMP.mkdir(parents=True, exist_ok=True)
    source = Image.open(png).convert("RGBA")
    levels = []
    for level in range(8):
        width = max(1, source.width >> level)
        height = max(1, source.height >> level)
        mip = source.resize((width, height), Image.Resampling.LANCZOS)
        mip_png = TEMP / f"{png.stem}_{level}.png"
        mip_astc = TEMP / f"{png.stem}_{level}.astc"
        mip.save(mip_png)
        subprocess.run(
            [str(ASTCENC), "-cl", str(mip_png), str(mip_astc), "8x5", "-fastest"],
            check=True, stdout=subprocess.DEVNULL,
        )
        encoded = mip_astc.read_bytes()
        if encoded[:4] != bytes.fromhex("13ABA15C"):
            raise ValueError(f"astcenc produced an invalid file for {png.name}")
        levels.append(tile_astc(encoded[16:], width, height))
    return b"".join(levels)


def main():
    dictionary, data, entries, table_offset, compressed = read_archive(CLEAN)
    files = {index: bytearray(decompress_entry(data, entries[index], compressed))
             for index in (0, 52, 53, 54, 63, 65)}
    models = group_models(parse_subentries(files[0]))
    source_model = models[34]
    fbx = json.loads(FBX_SKIN.read_text(encoding="utf-8"))
    skeletons = skeleton_groups(files[0])
    for source_index, name in FBX_NAME_FOR_SOURCE.items():
        descriptor, b004, _size, b005 = mesh_record(files[52], source_model, source_index)
        vertex_count = struct.unpack_from("<I", files[52], descriptor + 0x0C)[0]
        vertex_offset = struct.unpack_from("<I", files[52], b004 + 4)[0]
        positions = fbx[name]["positions"]
        if len(positions) != vertex_count:
            raise ValueError(f"{name} FBX vertex count differs from slot 34")
        for index, position in enumerate(positions):
            struct.pack_into(
                "<fff", files[54], b005.offset + vertex_offset + index * 0x30,
                *position,
            )
        print(f"loaded {vertex_count} exact vertex positions from FBX {name}")
    changed_meshes = 0
    for source_index, hashes in SOURCE_FOR_TARGET_HASHES.items():
        source_descriptor, source_b004, source_size, source_b005 = mesh_record(
            files[52], source_model, source_index
        )
        source_index_offset = struct.unpack_from("<I", files[52], source_descriptor + 4)[0]
        source_pointers = struct.unpack_from("<IIII", files[52], source_b004)
        source_vertex_count = struct.unpack_from("<I", files[52], source_descriptor + 0x0C)[0]
        fbx_name = FBX_NAME_FOR_SOURCE[source_index]
        fbx_weights = fbx[fbx_name]["weights"]
        for slot in TARGETS:
            target_index = find_mesh(files[52], models[slot], hashes)
            target_descriptor, target_b004, target_size, target_b005 = mesh_record(
                files[52], models[slot], target_index
            )
            if source_size != 16 or target_size != 16:
                raise ValueError("Mario replacement expects skinned source and target meshes")
            target_hash = struct.unpack_from("<I", files[52], target_descriptor)[0]
            target_vertex_count = struct.unpack_from("<I", files[52], target_descriptor + 0x0C)[0]
            original_target_skin, original_target_vertex = struct.unpack_from(
                "<II", files[52], target_b004
            )
            remapped_skin = skin_from_fbx(
                fbx_weights,
                models[slot],
                skeleton_id_to_hash(files, skeletons[SKELETON_GROUP_FOR_SLOT[slot]]),
                files[52],
            )
            available = target_vertex_count * 0x30
            if len(remapped_skin) > available:
                raise ValueError(
                    f"slot {slot} mesh {target_index} freed vertex range is too small for skin data"
                )
            skin_start = target_b005.offset + original_target_vertex
            files[54][skin_start : skin_start + len(remapped_skin)] = remapped_skin
            files[52][target_descriptor : target_descriptor + 0x40] = files[52][
                source_descriptor : source_descriptor + 0x40
            ]
            struct.pack_into("<I", files[52], target_descriptor, target_hash)
            delta = source_b005.offset - target_b005.offset
            struct.pack_into("<I", files[52], target_descriptor + 4, source_index_offset + delta)
            struct.pack_into(
                "<IIII", files[52], target_b004,
                original_target_vertex,
                source_pointers[1] + delta,
                source_pointers[2] + delta,
                source_pointers[3] + delta,
            )
            changed_meshes += 1
            print(
                f"slot {slot} submesh {target_index} <- slot 34 mesh {source_index}; "
                f"{source_vertex_count} FBX-rigged vertices"
            )

    textures = texture_records(files[0], files[63])
    for png in sorted(TEXTURES.glob("*.png")):
        texture_hash = int(png.stem, 16)
        _header, image = textures[texture_hash]
        payload = encode_texture(png)
        if len(payload) != image.size:
            raise ValueError(
                f"texture {png.stem} encoded size {len(payload)} != allocation {image.size}"
            )
        files[65][image.offset : image.offset + image.size] = payload
        print(f"replaced texture {png.stem} ({len(payload)} bytes, 8 ASTC mip levels)")

    for index in (52, 54, 65):
        replace_entry(dictionary, data, entries, table_offset, index,
                      bytes(files[index]), compressed)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    (OUTPUT / "global.dict").write_bytes(dictionary)
    (OUTPUT / "global.data").write_bytes(data)
    (OUTPUT / "global.patch").write_bytes(CLEAN.with_suffix(".patch").read_bytes())
    _, emitted_data, emitted_entries, _, emitted_compressed = read_archive(OUTPUT / "global.dict")
    for index in (52, 54, 65):
        actual = decompress_entry(emitted_data, emitted_entries[index], emitted_compressed)
        if actual != bytes(files[index]):
            raise AssertionError(f"entry {index} did not round-trip")
    print(f"wrote Mario replacement ({changed_meshes} mesh redirects) to {OUTPUT}")


if __name__ == "__main__":
    main()

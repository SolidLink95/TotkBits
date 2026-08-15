"""Export LM3 Story meshes to FBX and safely replace their vertex positions.

This first implementation deliberately preserves topology and every non-position
byte. FBX meshes must retain their generated names and exact vertex counts.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import subprocess
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

DEFAULT_BLENDER = Path(r"C:\Program Files\Blender Foundation\Blender 4.0\blender.exe")
DEFAULT_DICT = ROOT / "tmp" / "ml3" / "clean" / "global.dict"
BRIDGE = HERE / "lm3_fbx_blender.py"


def run_blender(blender: Path, *arguments: object) -> None:
    command = [
        str(blender), "--background", "--factory-startup", "--python", str(BRIDGE),
        "--", *(str(argument) for argument in arguments),
    ]
    subprocess.run(command, check=True)


def archive_geometry(dict_path: Path, slots: tuple[int, ...]):
    dictionary, data, entries, table_offset, compressed = read_archive(dict_path)
    files = {
        index: bytearray(decompress_entry(data, entries[index], compressed))
        for index in (0, 52, 54)
    }
    models = group_models(parse_subentries(files[0]))
    meshes = []
    for slot in slots:
        model = models[slot]
        b003 = next(record for record in model if record.kind == 0xB003)
        b004 = next(record for record in model if record.kind == 0xB004)
        b005 = next(record for record in model if record.kind == 0xB005)
        b004_cursor = b004.offset
        for mesh_index in range(b003.size // 0x40):
            descriptor = b003.offset + mesh_index * 0x40
            mesh_hash, index_offset, index_flags, vertex_count = struct.unpack_from(
                "<IIII", files[52], descriptor
            )
            skinned = struct.unpack_from("<I", files[52], descriptor + 0x28)[0] != 0xFFFFFFFF
            if skinned:
                _skin_offset, vertex_offset = struct.unpack_from("<II", files[52], b004_cursor)
                b004_cursor += 16
            else:
                vertex_offset = struct.unpack_from("<I", files[52], b004_cursor)[0]
                b004_cursor += 12
            index_count = index_flags & 0xFFFFFF
            index_width = 1 if index_flags >> 24 == 0x80 else 2
            index_format = "<B" if index_width == 1 else "<H"
            index_base = b005.offset + index_offset
            indices = [
                struct.unpack_from(index_format, files[54], index_base + i * index_width)[0]
                for i in range(index_count)
            ]
            if index_count % 3:
                raise ValueError(f"slot {slot} mesh {mesh_index} index count is not triangular")
            if indices and max(indices) >= vertex_count:
                raise ValueError(f"slot {slot} mesh {mesh_index} has an invalid vertex index")
            vertex_base = b005.offset + vertex_offset
            positions = [
                list(struct.unpack_from("<fff", files[54], vertex_base + i * 0x30))
                for i in range(vertex_count)
            ]
            name = f"slot_{slot}_mesh_{mesh_index:02d}_{mesh_hash:08X}"
            meshes.append({
                "name": name,
                "slot": slot,
                "mesh_index": mesh_index,
                "mesh_hash": mesh_hash,
                "positions": positions,
                "faces": [indices[i : i + 3] for i in range(0, index_count, 3)],
                "vertex_base": vertex_base,
            })
    return dictionary, data, entries, table_offset, compressed, files, meshes


def export_command(args) -> None:
    *_archive, meshes = archive_geometry(args.dict, args.slots)
    manifest = args.fbx.with_suffix(".lm3.json")
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(json.dumps({"meshes": meshes}, separators=(",", ":")), encoding="utf-8")
    run_blender(args.blender, "export", manifest, args.fbx)
    print(f"exported {len(meshes)} meshes to {args.fbx}")


def apply_command(args) -> None:
    dictionary, data, entries, table_offset, compressed, files, meshes = archive_geometry(
        args.dict, args.slots
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    imported_path = args.output.parent / f".{args.output.name}.fbx-positions.json"
    run_blender(args.blender, "import", args.fbx, imported_path)
    imported = json.loads(imported_path.read_text(encoding="utf-8"))
    expected_names = {mesh["name"] for mesh in meshes}
    if set(imported) != expected_names:
        missing = sorted(expected_names - set(imported))
        extra = sorted(set(imported) - expected_names)
        raise ValueError(f"FBX mesh-name mismatch; missing={missing}, extra={extra}")
    changed_vertices = 0
    for mesh in meshes:
        positions = imported[mesh["name"]]
        if len(positions) != len(mesh["positions"]):
            raise ValueError(
                f"{mesh['name']} vertex count changed: {len(mesh['positions'])} -> {len(positions)}"
            )
        for index, position in enumerate(positions):
            if len(position) != 3 or not all(math.isfinite(value) for value in position):
                raise ValueError(f"{mesh['name']} vertex {index} has invalid coordinates")
            old = mesh["positions"][index]
            if any(abs(position[axis] - old[axis]) > 1e-7 for axis in range(3)):
                changed_vertices += 1
            struct.pack_into("<fff", files[54], mesh["vertex_base"] + index * 0x30, *position)
    if not changed_vertices and not args.allow_unchanged:
        raise ValueError("FBX has no changed vertex positions; use --allow-unchanged for a no-op test")
    replace_entry(
        dictionary, data, entries, table_offset, 54, bytes(files[54]), compressed
    )
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "global.dict").write_bytes(dictionary)
    (args.output / "global.data").write_bytes(data)
    (args.output / "global.patch").write_bytes(args.dict.with_suffix(".patch").read_bytes())
    # Reopen the result; replace_entry also guarantees the physical allocation did not grow.
    _, emitted_data, emitted_entries, _, emitted_compressed = read_archive(
        args.output / "global.dict"
    )
    emitted54 = decompress_entry(emitted_data, emitted_entries[54], emitted_compressed)
    if emitted54 != bytes(files[54]):
        raise AssertionError("emitted geometry entry does not round-trip")
    imported_path.unlink(missing_ok=True)
    print(f"replaced {changed_vertices} vertex positions in {args.output}")


def nudge_command(args) -> None:
    run_blender(args.blender, "nudge", args.source, args.output, args.factor)


def cube_heads_command(args) -> None:
    run_blender(args.blender, "cube-heads", args.source, args.output, *args.hashes)


def inspect_command(args) -> None:
    args.output.parent.mkdir(parents=True, exist_ok=True)
    run_blender(args.blender, "import", args.fbx, args.output)
    meshes = json.loads(args.output.read_text(encoding="utf-8"))
    for name, positions in sorted(meshes.items()):
        print(f"{name}: {len(positions)} vertices")


def extract_skin_command(args) -> None:
    args.output.parent.mkdir(parents=True, exist_ok=True)
    run_blender(args.blender, "import-skin", args.fbx, args.output)
    print(f"extracted FBX positions and skin weights to {args.output}")


def export_obj_command(args) -> None:
    *_archive, meshes = archive_geometry(args.dict, args.slots)
    args.obj.parent.mkdir(parents=True, exist_ok=True)
    lines = ["# Extracted directly from an LM3 archive by lm3_fbx_replace.py"]
    vertex_base = 1
    for mesh in meshes:
        lines.append(f"o {mesh['name']}")
        lines.extend(f"v {x:.9g} {y:.9g} {z:.9g}" for x, y, z in mesh["positions"])
        for a, b, c in mesh["faces"]:
            lines.append(f"f {a + vertex_base} {b + vertex_base} {c + vertex_base}")
        vertex_base += len(mesh["positions"])
    args.obj.write_text("\n".join(lines) + "\n", encoding="ascii")
    print(
        f"exported {len(meshes)} meshes and {vertex_base - 1} vertices "
        f"from {args.dict} to {args.obj}"
    )


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--blender", type=Path, default=DEFAULT_BLENDER)
    subparsers = result.add_subparsers(dest="command", required=True)
    export = subparsers.add_parser("export")
    export.add_argument("--dict", type=Path, default=DEFAULT_DICT)
    export.add_argument("--slots", type=int, nargs="+", default=[27, 28, 29, 30])
    export.add_argument("--fbx", type=Path, required=True)
    export.set_defaults(function=export_command)
    nudge = subparsers.add_parser("nudge")
    nudge.add_argument("--source", type=Path, required=True)
    nudge.add_argument("--output", type=Path, required=True)
    nudge.add_argument("--factor", type=float, default=1.001)
    nudge.set_defaults(function=nudge_command)
    cube_heads = subparsers.add_parser("cube-heads")
    cube_heads.add_argument("--source", type=Path, required=True)
    cube_heads.add_argument("--output", type=Path, required=True)
    cube_heads.add_argument(
        "--hashes", nargs="+", default=["40243E90", "E4882C37"],
        help="LM3 mesh hashes to cube-project",
    )
    cube_heads.set_defaults(function=cube_heads_command)
    inspect = subparsers.add_parser("inspect")
    inspect.add_argument("--fbx", type=Path, required=True)
    inspect.add_argument("--output", type=Path, required=True)
    inspect.set_defaults(function=inspect_command)
    extract_skin = subparsers.add_parser("extract-skin")
    extract_skin.add_argument("--fbx", type=Path, required=True)
    extract_skin.add_argument("--output", type=Path, required=True)
    extract_skin.set_defaults(function=extract_skin_command)
    export_obj = subparsers.add_parser("export-obj")
    export_obj.add_argument("--dict", type=Path, default=DEFAULT_DICT)
    export_obj.add_argument("--slots", type=int, nargs="+", default=[27])
    export_obj.add_argument("--obj", type=Path, required=True)
    export_obj.set_defaults(function=export_obj_command)
    apply = subparsers.add_parser("apply")
    apply.add_argument("--dict", type=Path, default=DEFAULT_DICT)
    apply.add_argument("--slots", type=int, nargs="+", default=[27, 28, 29, 30])
    apply.add_argument("--fbx", type=Path, required=True)
    apply.add_argument("--output", type=Path, required=True)
    apply.add_argument("--allow-unchanged", action="store_true")
    apply.set_defaults(function=apply_command)
    return result


def main() -> None:
    args = parser().parse_args()
    args.slots = tuple(args.slots) if hasattr(args, "slots") else ()
    args.function(args)


if __name__ == "__main__":
    main()

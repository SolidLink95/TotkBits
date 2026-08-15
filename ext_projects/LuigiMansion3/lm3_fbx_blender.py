"""Blender-side FBX bridge for lm3_fbx_replace.py."""

import json
import sys
from pathlib import Path

import bpy


def arguments():
    marker = sys.argv.index("--")
    return sys.argv[marker + 1 :]


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def export_fbx(manifest_path: Path, fbx_path: Path):
    reset()
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    for item in manifest["meshes"]:
        mesh = bpy.data.meshes.new(item["name"])
        mesh.from_pydata(item["positions"], [], item["faces"])
        mesh.update()
        obj = bpy.data.objects.new(item["name"], mesh)
        bpy.context.collection.objects.link(obj)
        obj["lm3_slot"] = item["slot"]
        obj["lm3_mesh_index"] = item["mesh_index"]
    bpy.ops.object.select_all(action="SELECT")
    fbx_path.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.fbx(
        filepath=str(fbx_path),
        use_selection=True,
        object_types={"MESH"},
        use_mesh_modifiers=False,
        add_leaf_bones=False,
        bake_anim=False,
        apply_unit_scale=False,
        axis_forward="-Z",
        axis_up="Y",
    )


def import_fbx(fbx_path: Path, output_path: Path):
    reset()
    bpy.ops.import_scene.fbx(filepath=str(fbx_path), axis_forward="-Z", axis_up="Y")
    result = {}
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        if obj.name in result:
            raise RuntimeError(f"duplicate mesh name in FBX: {obj.name}")
        result[obj.name] = [list(vertex.co) for vertex in obj.data.vertices]
    output_path.write_text(json.dumps(result, separators=(",", ":")), encoding="utf-8")


def import_skin(fbx_path: Path, output_path: Path):
    reset()
    bpy.ops.import_scene.fbx(filepath=str(fbx_path), axis_forward="-Z", axis_up="Y")
    result = {}
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        names = [group.name for group in obj.vertex_groups]
        weights = []
        for vertex in obj.data.vertices:
            weights.append([
                [names[item.group], item.weight]
                for item in vertex.groups if item.weight > 1e-6
            ])
        result[obj.name] = {
            "positions": [list(vertex.co) for vertex in obj.data.vertices],
            "weights": weights,
        }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(result, separators=(",", ":")), encoding="utf-8")


def nudge_fbx(source_path: Path, output_path: Path, factor: float):
    reset()
    bpy.ops.import_scene.fbx(filepath=str(source_path), axis_forward="-Z", axis_up="Y")
    changed = 0
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        for vertex in obj.data.vertices:
            vertex.co.x *= factor
            changed += 1
    if not changed:
        raise RuntimeError("FBX contains no mesh vertices")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.fbx(
        filepath=str(output_path),
        use_selection=True,
        object_types={"MESH"},
        use_mesh_modifiers=False,
        add_leaf_bones=False,
        bake_anim=False,
        apply_unit_scale=False,
        axis_forward="-Z",
        axis_up="Y",
    )
    print(f"nudged {changed} FBX vertices by X factor {factor}")


def cube_heads(source_path: Path, output_path: Path, hashes: set[str]):
    """Project selected mesh vertices onto their existing bounding boxes."""
    reset()
    bpy.ops.import_scene.fbx(filepath=str(source_path), axis_forward="-Z", axis_up="Y")
    changed_meshes = []
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH" or obj.name.rsplit("_", 1)[-1].upper() not in hashes:
            continue
        vertices = obj.data.vertices
        minimum = [min(vertex.co[axis] for vertex in vertices) for axis in range(3)]
        maximum = [max(vertex.co[axis] for vertex in vertices) for axis in range(3)]
        center = [(minimum[axis] + maximum[axis]) * 0.5 for axis in range(3)]
        half = [max((maximum[axis] - minimum[axis]) * 0.5, 1e-8) for axis in range(3)]
        for vertex in vertices:
            delta = [vertex.co[axis] - center[axis] for axis in range(3)]
            distance = max(abs(delta[axis]) / half[axis] for axis in range(3))
            if distance > 1e-8:
                for axis in range(3):
                    vertex.co[axis] = center[axis] + delta[axis] / distance
        changed_meshes.append(obj.name)
    if not changed_meshes:
        raise RuntimeError(f"no FBX meshes matched hashes {sorted(hashes)}")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.fbx(
        filepath=str(output_path), use_selection=True, object_types={"MESH"},
        use_mesh_modifiers=False, add_leaf_bones=False, bake_anim=False,
        apply_unit_scale=False, axis_forward="-Z", axis_up="Y",
    )
    print(f"cube-projected head meshes: {', '.join(changed_meshes)}")


def main():
    args = arguments()
    command = args[0]
    if command == "export":
        export_fbx(Path(args[1]), Path(args[2]))
    elif command == "import":
        import_fbx(Path(args[1]), Path(args[2]))
    elif command == "import-skin":
        import_skin(Path(args[1]), Path(args[2]))
    elif command == "nudge":
        nudge_fbx(Path(args[1]), Path(args[2]), float(args[3]))
    elif command == "cube-heads":
        cube_heads(Path(args[1]), Path(args[2]), {value.upper() for value in args[3:]})
    else:
        raise ValueError(f"unknown bridge command: {command}")


if __name__ == "__main__":
    main()

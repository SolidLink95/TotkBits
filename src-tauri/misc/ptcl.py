#!/usr/bin/env python3

from utils import ReadableWriteableStream, RelativeSeekContext, SeekContext

import json
import mmap
import os
from pathlib import Path
import struct
from typing import Dict, List, Tuple, TypedDict

# for use with the const_color values
def pack_float4(c: Tuple[float, float, float, float]) -> bytes:
    return struct.pack("<ffff", c[0], c[1], c[2], c[3])

# for use with anims
def pack_float3(c: Tuple[float, float, float]) -> bytes:
    return struct.pack("<fff", c[0], c[1], c[2])

class AnimKeyFrame(TypedDict):
    value: Tuple[float, float, float]
    keyframe: float

class BinaryData(TypedDict):
    signature: str
    size: int
    subsection_offset: int
    next_section_offset: int
    next_subsection_offset: int
    section_offset: int
    _unk: int
    subsection_count: int

class Emitter(TypedDict):
    const_color0: Tuple[float, float, float, float]
    const_color1: Tuple[float, float, float, float]
    color_anim0: List[AnimKeyFrame]
    color_anim1: List[AnimKeyFrame]
    alpha_anim0: List[AnimKeyFrame]
    alpha_anim1: List[AnimKeyFrame]

def verify_file(stream: ReadableWriteableStream) -> bool:
    with SeekContext(stream):
        if (magic := stream.read(8).decode("utf-8")) != "VFXB    ":
            print(f"Invalid magic: {magic}")
            return False
        stream.read_u8()
        if (version := stream.read_u8()) != 4:
            print(f"Invalid graphics API version: {version}")
            return False
        if (version := stream.read_u16()) != 0x33:
            print(f"Invalid resource version: {version}")
            return False
        if stream.read_u16() != 0xfeff:
            print("Invalid endianness, only little endian is support")
            return False
        return True

# stream should be at the start of the file header
def seek_first_block(stream: ReadableWriteableStream) -> None:
    stream.skip(0x16)
    stream.skip(stream.read_u16() - 0x18)

def read_binary_data(stream: ReadableWriteableStream) -> BinaryData:
    data: Tuple[bytes, int, int, int, int, int, int, int] = struct.unpack("<4sIIIIIII", stream.read(0x20))
    return BinaryData(
        signature = data[0].decode("utf-8"),
        size = data[1],
        subsection_offset = data[2],
        next_section_offset = data[3],
        next_subsection_offset = data[4],
        section_offset = data[5],
        _unk = data[6],
        subsection_count = data[7]
    )

def read_header(stream: ReadableWriteableStream) -> BinaryData:
    with SeekContext(stream):
        return read_binary_data(stream)

def read_float4(stream: ReadableWriteableStream) -> Tuple[float, float, float, float]:
    return struct.unpack("<ffff", stream.read(0x10))

def read_anim(stream: ReadableWriteableStream, count: int) -> List[AnimKeyFrame]:
    frames: List[AnimKeyFrame] = []
    if count > 8:
        print(f"Too many keyframes {count} @ {hex(stream.tell())}") # one of the files has it set to 8 and I'm not sure why?
        count = 7
    with SeekContext(stream):
        for i in range(count + 1): # seems color/alpha anims start at index 1, scale starts at index 4?? unsure what the others are
            frame: Tuple[float, float, float, float] = read_float4(stream)
            frames.append(AnimKeyFrame(
                value = (frame[0], frame[1], frame[2]),
                keyframe = frame[3]
            ))
    stream.skip(0x80)
    return frames

def find_section(stream: ReadableWriteableStream, signature: str) -> int:
    data: BinaryData = read_header(stream)
    if data["signature"] == signature:
        return data["section_offset"]
    while data["next_section_offset"] != 0xffffffff:
        stream.skip(data["next_section_offset"])
        data = read_header(stream)
        if data["signature"] == signature:
            return data["section_offset"]
    return 0xffffffff

def iter_emitters(stream: ReadableWriteableStream) -> Dict[str, Emitter]:
    emitters: Dict[str, Emitter] = {}
    offset: int = 0
    while True:
        stream.skip(offset)
        with SeekContext(stream):
            assert read_header(stream)["signature"] == "EMTR", "Invalid emitter signature"
            emitter: Emitter = Emitter()
            with RelativeSeekContext(stream, read_header(stream)["section_offset"]):
                with RelativeSeekContext(stream, 0x10):
                    emitters[name := stream.read_string(0x60)] = emitter
                with RelativeSeekContext(stream, 0xf48):
                    emitter["const_color0"] = read_float4(stream)
                    emitter["const_color1"] = read_float4(stream)
                counts: Dict[str, int] = {"color0": 0, "color1": 0, "alpha0": 0, "alpha1": 0}
                with RelativeSeekContext(stream, 0x80):
                    counts["color0"] = stream.read_u32()
                    counts["alpha0"] = stream.read_u32()
                    counts["color1"] = stream.read_u32()
                    counts["alpha1"] = stream.read_u32()
                with RelativeSeekContext(stream, 0x680):
                    emitter["color_anim0"] = read_anim(stream, counts["color0"])
                    emitter["alpha_anim0"] = read_anim(stream, counts["alpha0"])
                    emitter["color_anim1"] = read_anim(stream, counts["color1"])
                    emitter["alpha_anim1"] = read_anim(stream, counts["alpha1"])
            print(f"    Emitter found: {name}")
        if (offset := read_header(stream)["next_section_offset"]) == 0xffffffff:
            break
    return emitters

def dump_json(path: str, output_path: str = "") -> Dict[str, Dict[str, Emitter]]:
    file: Dict[str, Dict[str, Emitter]] = {}
    if path.endswith(".zs"):
        print(f"Please decompress {path} first")
        return file
    if not os.path.exists(path):
        print("File does not exist")
        return file
    with open(path, "rb+") as f:
        mm: mmap.mmap = mmap.mmap(f.fileno(), 0)
        stream: ReadableWriteableStream = ReadableWriteableStream(f)
        if (start := mm.find("VFXB    ".encode("utf-8"))) == -1:
            print("Failed to find file magic")
            return file
        stream.seek(start)
        if not verify_file(stream):
            print("File is not valid")
            return file
        seek_first_block(stream)
        if (offset := find_section(stream, "ESTA")) == 0xffffffff:
            print("No emitter sets found")
            return file
        while True:
            stream.skip(offset)
            with SeekContext(stream):
                assert read_header(stream)["signature"] == "ESET", "Invalid emitter set signature"
                with RelativeSeekContext(stream, read_header(stream)["section_offset"] + 0x10):
                    file[eset := stream.read_string(0x60)] = {}
                print(f"Emitter Set found: {eset}")
                emitter_offset: int = read_header(stream)["subsection_offset"]
                if emitter_offset == 0xffffffff:
                    print(f"    No emitters found")
                else:
                    with RelativeSeekContext(stream, emitter_offset):
                        file[eset] = iter_emitters(stream)
            if (offset := read_header(stream)["next_section_offset"]) == 0xffffffff:
                break
    if output_path == "":
        output_path = path + ".json"
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(file, f, indent=2)
    print("Please note that the first keyframe for color and alpha animations is the second in the list and not the first")
    return file

def write_anim(stream: ReadableWriteableStream, frames: List[AnimKeyFrame]) -> None:
    with SeekContext(stream):
        count = min(len(frames), 8)
        if len(frames) > 8:
            print("WARNING: Animation has too many key frames, some will be ignored (max of 8)")
        for i in range(count):
            stream.write(pack_float3(frames[i]["value"]))
            stream.write(struct.pack("<f", frames[i]["keyframe"]))
    stream.skip(0x80)

def apply_emitter_changes(stream: ReadableWriteableStream, changes: Dict[str, Emitter]) -> None:
    offset: int = 0
    while True:
        stream.skip(offset)
        with SeekContext(stream):
            assert read_header(stream)["signature"] == "EMTR", "Invalid emitter signature"
            with RelativeSeekContext(stream, read_header(stream)["section_offset"]):
                with RelativeSeekContext(stream, 0x10):
                    name = stream.read_string(0x60)
                if name in changes:
                    print(f"    Applying changes to {name}")
                    emitter = changes[name]
                    with RelativeSeekContext(stream, 0xf48):
                        stream.write(pack_float4(emitter["const_color0"]))
                        stream.write(pack_float4(emitter["const_color1"]))
                    with RelativeSeekContext(stream, 0x80):
                        stream.write(struct.pack("<I", min(len(emitter["color_anim0"]) - 1, 8)))
                        stream.write(struct.pack("<I", min(len(emitter["alpha_anim0"]) - 1, 8)))
                        stream.write(struct.pack("<I", min(len(emitter["color_anim1"]) - 1, 8)))
                        stream.write(struct.pack("<I", min(len(emitter["alpha_anim1"]) - 1, 8)))
                    with RelativeSeekContext(stream, 0x680):
                        write_anim(stream, emitter["color_anim0"])
                        write_anim(stream, emitter["alpha_anim0"])
                        write_anim(stream, emitter["color_anim1"])
                        write_anim(stream, emitter["alpha_anim1"])
        if (offset := read_header(stream)["next_section_offset"]) == 0xffffffff:
            break
    
def apply_edits(ptcl_path: str, json_path: str) -> bool:
    if not os.path.exists(ptcl_path):
        print("PTCL file does not exist")
        return False
    if ptcl_path.endswith(".zs"):
        print(f"Please decompress {ptcl_path} first")
        return False
    if not os.path.exists(json_path):
        print("JSON file does not exist")
        return False
    changes: Dict[str, Dict[str, Emitter]] = json.loads(Path(json_path).read_text("utf-8"))
    with open(ptcl_path, "rb+") as f:
        mm: mmap.mmap = mmap.mmap(f.fileno(), 0)
        stream: ReadableWriteableStream = ReadableWriteableStream(f)
        if (start := mm.find("VFXB    ".encode("utf-8"))) == -1:
            print("Failed to find file magic")
            return False
        stream.seek(start)
        if not verify_file(stream):
            print("File is not valid")
            return False
        seek_first_block(stream)
        if (offset := find_section(stream, "ESTA")) == 0xffffffff:
            print("No emitter sets found")
            return False
        while True:
            stream.skip(offset)
            with SeekContext(stream):
                assert read_header(stream)["signature"] == "ESET", "Invalid emitter set signature"
                with RelativeSeekContext(stream, read_header(stream)["section_offset"] + 0x10):
                    eset = stream.read_string(0x60)
                if eset in changes:
                    print(f"Applying changes to {eset}")
                    emitter_offset: int = read_header(stream)["subsection_offset"]
                    if emitter_offset == 0xffffffff:
                        print(f"Skipping {eset}, no emitters found")
                    else:
                        with RelativeSeekContext(stream, emitter_offset):
                            apply_emitter_changes(stream, changes[eset])
            if (offset := read_header(stream)["next_section_offset"]) == 0xffffffff:
                break
    return True



def ptcl_apply_edits_lib(ptcl_data: bytes, json_data: bytes) -> str:
    import yaml, tempfile
    changes: Dict[str, Dict[str, Emitter]] = yaml.safe_load(json_data.decode('utf-8'))
    if not isinstance(changes, dict):
        return "Error: Invalid JSON data: " + json_data.decode('utf-8')
    with tempfile.TemporaryFile() as f:
        f.write(ptcl_data)  # Write data to the temporary file
        f.flush()      # Ensure data is written
        f.seek(0)   
    # with open(ptcl_path, "rb+") as f:
        mm: mmap.mmap = mmap.mmap(f.fileno(), 0)
        stream: ReadableWriteableStream = ReadableWriteableStream(f)
        if (start := mm.find("VFXB    ".encode("utf-8"))) == -1:
            return "Error: Failed to find file magic"
        stream.seek(start)
        if not verify_file(stream):
            return "Error: File is not valid"
        seek_first_block(stream)
        if (offset := find_section(stream, "ESTA")) == 0xffffffff:
            return "Error: No emitter sets found"
        while True:
            stream.skip(offset)
            with SeekContext(stream):
                assert read_header(stream)["signature"] == "ESET", "Invalid emitter set signature"
                with RelativeSeekContext(stream, read_header(stream)["section_offset"] + 0x10):
                    eset = stream.read_string(0x60)
                if eset in changes:
                    # print(f"Applying changes to {eset}")
                    emitter_offset: int = read_header(stream)["subsection_offset"]
                    if emitter_offset == 0xffffffff:
                        # print(f"Skipping {eset}, no emitters found")
                        pass
                    else:
                        with RelativeSeekContext(stream, emitter_offset):
                            apply_emitter_changes(stream, changes[eset])
            if (offset := read_header(stream)["next_section_offset"]) == 0xffffffff:
                break
        f.seek(0)
        return f.read()
    # return True


def ptcl_binary_to_text_lib(data:bytes) -> str:
    import yaml, tempfile
    file: Dict[str, Dict[str, Emitter]] = {}
    with tempfile.TemporaryFile() as f:
        f.write(data)  # Write data to the temporary file
        f.flush()      # Ensure data is written
        f.seek(0)   
        # f = io.BytesIO(data)
        # with open(path, "rb+") as f:
        mm: mmap.mmap = mmap.mmap(f.fileno(), 0)
        stream: ReadableWriteableStream = ReadableWriteableStream(f)
        if (start := mm.find("VFXB    ".encode("utf-8"))) == -1:
            return b"Error: Failed to find file magic"
            # print("Failed to find file magic")
            # return file
        stream.seek(start)
        if not verify_file(stream):
            return b"Error: File is not valid"
        seek_first_block(stream)
        if (offset := find_section(stream, "ESTA")) == 0xffffffff:
            return b"Error: No emitter sets found"
            # return file
        while True:
            stream.skip(offset)
            with SeekContext(stream):
                assert read_header(stream)["signature"] == "ESET", "Invalid emitter set signature"
                with RelativeSeekContext(stream, read_header(stream)["section_offset"] + 0x10):
                    file[eset := stream.read_string(0x60)] = {}
                # print(f"Emitter Set found: {eset}")
                emitter_offset: int = read_header(stream)["subsection_offset"]
                if emitter_offset == 0xffffffff:
                    # print(f"    No emitters found")
                    pass
                else:
                    with RelativeSeekContext(stream, emitter_offset):
                        file[eset] = iter_emitters(stream)
            if (offset := read_header(stream)["next_section_offset"]) == 0xffffffff:
                break
    text = yaml.dump(file, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
    return text if isinstance(text, str) else text.decode('utf-8')
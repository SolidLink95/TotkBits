from pathlib import Path
from dataclasses import dataclass
from typing import List
import os, sys

NUMERICS = ["bool","u8", "u16", "u32", "u64", "u128",  "s16", "s32", "s64", "float", "f64"]
SWAPS = {"char": "bytes"}
ENUMS = {"hkxMaterial__Transparency":"u8","hkxMaterial__UVMappingAlgorithm":"u32","hkxIndexBuffer__IndexType":"u8","hkxVertexDescription__DataUsage":"u16","hkxVertexDescription__DataType":"u16","hkaAnimationBinding__BlendHint":"u8","hclBlendSomeVerticesOperator__BlendWeightType":"u8","hclBufferLayout__SlotFlags":"u8","hkaAnimatedReferenceFrame__hkaReferenceFrameTypeEnum":"u8","hclClothData__Platform":"u32","hkaAnimation__AnimationType":"u32","hclStateTransition__TransitionType":"u8","hclBufferLayout__TriangleFormat":"u8","ResFileType":"u8", "hclRuntimeConversionInfo__VectorConversion": "u8"}
BAD_NAME_PREFS = ["ResPhive","hclVirtualCollisionPointsData__TriangleFanSection","hkRefVariant", "hkStringPtr", "ResTagfile", "hkInt", "Ptr", "ResItem", "ResParam", "String", "ResTypeTemplate", 
                  "ResNamedType", "ResTypeBodyDeclaration", "ResTypeHash", 
                  "ResTypeBody", "ResPatch"]
SKIP_ELEMS = ["be"]
IMPORTS = [
    # 'from typing import List, Optional, Type, TypeVar', 
    'from Stream import ReadStream, WriteStream', 
    'from dataclasses import dataclass, field',
    "from BphclEnums import *",
    "from BphclSmallDataclasses import hkRefPtr, hclVirtualCollisionPointsData__BarycentricDictionaryEntry, s16, s32, u8, u16, hclVirtualCollisionPointsData__TriangleFanSection, u32, ResItem, ResPatch, Size, VarUInt, ResTypeTemplate, ResTypeHash",
    # "from bphcl import ResNamedType, ResTypeBodyDeclaration, ResTypeBody",
    "from Havok import hkArray,hkStringPtr,hkRefVariant",
    # "from Cloth import hkRefPtr"
    ]

ARRAYS = ["hkArray", "hkRefPtr"]

@dataclass
class StructField:
    _type: str
    name: str
    list_len: int 
    is_numeric: bool 
    bytes_count: int
    align: int = 0
    
    @classmethod
    def from_line(cls, line:str):
        """Creates a StructField from a line of text."""
        elems = [e for e in line.split() if e.strip() != ""]
        if not elems or len(elems) < 2:
            return None
        if elems[1].endswith(";"):
            elems[1] = elems[1][:-1]
        list_len = "<" in elems[0] 
        _type = elems[0].replace("<", "[").replace(">", "]")
        name = elems[1]
        if "[" in name and "]" in name:
            list_len = True
        is_numeric = _type in NUMERICS
        if _type in SWAPS:
            _type = SWAPS[_type]
        return cls(_type=_type, name=name, list_len=list_len, is_numeric=is_numeric)

class StructParser():
    def __init__(self, struct_str):
        self.struct_str = struct_str
        self.fields: List[StructField] = []
        self.ind = "    "
        self.bad_prefs = ["fn", "//"]
        self.bad_suff = ["<T>",]
        self.bad_text = ["match (","match("]
        self.name = None
        
    def parse(self):
        if "hclVirtualCollisionPointsData__BarycentricDictionaryEntry" in self.struct_str:
            pass
        if any([t in self.struct_str for t in self.bad_text]):
            return False
        if any([self.struct_str.startswith(pref) for pref in self.bad_prefs]):
            return False
        self.lines = self.struct_str.splitlines()
        hdr = [e for e in self.lines[0].split() if e.strip() != ""]
        if ":" in hdr:
            self.parent = hdr[hdr.index(":") + 1]
        else:
            self.parent = None
        if hdr[0] != "struct":
            # print(f"Struct header does not start with 'struct': {(self.struct_str)}")
            return False
        # assert(hdr[0] == "struct"), f"Struct header does not start with 'struct': {(self.struct_str)}"
        self.name = hdr[1]
        if self.name:
            if any([self.name.endswith(suff) for suff in self.bad_suff]):
                return False
            if any([self.name.startswith(pref) for pref in BAD_NAME_PREFS]):
                return False
        fields_lines = self.lines[1:-1]
        for fld_i, field_line in enumerate(fields_lines):
            # if field_line.count("<") > 1 or field_line.count(">") > 1:
            #     return False
            align = 0
            if fld_i > 0 and "AlignTo" in fields_lines[fld_i - 1]:
                align = int(fields_lines[fld_i - 1].split("<")[1].split(">")[0], 16)
            elems = [e for e in field_line.split() if e.strip() != ""]
            elems = [e for e in elems if e not in SKIP_ELEMS]
            if not elems or not field_line or len(elems) < 2:
                continue
            # assert(len(elems) >= 2), f"Field line {repr(field_line)} does not contain enough elements for {self.name}"
            if elems[0] == "};":
                break
            if elems[1].endswith(";"):
                elems[1] = elems[1][:-1]
            list_len = 0
            _type = elems[0].replace("<", "[").replace(">", "]")
            name = elems[1]
            if "[" in name and "]" in name:
                list_len = int(name.split("[")[1].split("]")[0])
            is_numeric = _type in NUMERICS
            if _type in SWAPS:
                _type = SWAPS[_type]
            bytes_count = 0
            if _type == "u8":
                if "[" in name and "]" in name:
                    c = name.split("[")[1].split("]")[0]
                    if c.isnumeric():
                        _type = "bytes"
                        bytes_count = int(c)
            if "[" in name and "]" in name:
                name = name.split("[")[0]
            if name == "magic":
                pass
            field = StructField(_type=_type, name=name, list_len=list_len, is_numeric=is_numeric, bytes_count=bytes_count, align=align)
            self.fields.append(field)
        return True
    
    def generate_py_fields(self):
        if self.parent:
            self.code = f"@dataclass\nclass {self.name}({self.parent}):\n"
        else:
            self.code = f"@dataclass\nclass {self.name}:\n"
        i = 1
        for fld in self.fields:
            _type = fld._type.split("[")[0] if "[" in fld._type else fld._type
            if fld._type.count("[") >= 1 or fld._type.count("]") >= 1:
                # types = [e.replace("]", "") for e in fld._type.split("[")]
                # types[-1] = repr(types[-1])
                # t = "[".join(types) + ("]" * (len(types) - 1))
                self.code += f"{i * self.ind}{fld.name}: {_type}"
            elif fld._type == "bytes":
                self.code += f"{i * self.ind}{fld.name}: {_type} # size: {fld.bytes_count}"
            elif fld.is_numeric:
                self.code += f"{i * self.ind}{fld.name}: int # {fld._type}"
            else:
                self.code += f"{i * self.ind}{fld.name}: {_type}"
            if "[" in fld._type:
                self.code += f" # {fld._type}"
            self.code += "\n"
            
        self.code += "\n"
        
    def generate_py_from_reader(self):
        i = 1
        self.code += f"{i*self.ind}@classmethod\n"
        self.code += f"{i*self.ind}def from_reader(cls, stream: ReadStream) -> \"{self.name}\":\n"
        i+=1
        if self.parent:
            if self.fields and self.fields[0].align > 0:
                self.code += f"{i*self.ind}stream.align_to({hex(self.fields[0].align)})\n"
                self.fields[0].align = 0
            self.code += f"{i*self.ind}base = super().from_reader(stream)\n"
        for fld in self.fields:
            types = [e.replace("]", "") for e in fld._type.split("[")] if "[" in fld._type else []
            if "hkxMaterial" in types:
                pass
            types_count = len(types)
            _type = fld._type.split("[")[0] if "[" in fld._type else fld._type
            if fld.align > 0:
                self.code += f"{i*self.ind}stream.align_to({hex(fld.align)})\n"
            if fld.list_len > 0 and fld._type != "bytes":
                if fld._type in NUMERICS:
                    self.code += f"{i*self.ind}{fld.name} = [stream.read_{fld._type}() for _ in range({fld.list_len})]\n"
                else:
                    self.code += f"{i*self.ind}{fld.name} = [{fld._type}.from_reader(stream) for _ in range({fld.list_len})]\n"
            elif _type in ARRAYS:
                t = fld._type.split("[")[1].split("]")[0]
                if types_count == 3:
                    tmp = f"{i*self.ind}{fld.name} = {types[0]}.from_reader(stream, {types[1]}, {types[2]})\n"
                    self.code += f"{i*self.ind}{fld.name} = {types[0]}.from_reader(stream, {types[1]}, {types[2]})\n"
                elif types_count == 2:
                    self.code += f"{i*self.ind}{fld.name} = {_type}.from_reader(stream, {t})\n"
                elif types_count > 3:
                    raise ValueError(f"Array type {fld._type} has too many types: {self.name}")
            elif fld._type in ENUMS:
                c = ENUMS[fld._type]
                self.code += f"{i*self.ind}{fld.name} = {fld._type}(stream.read_{c}())\n"
            elif fld._type == "bytes":
                self.code += f"{i*self.ind}{fld.name} = stream._read_exact({fld.list_len})\n"
            elif fld.is_numeric:
                self.code += f"{i*self.ind}{fld.name} = stream.read_{fld._type}()\n"
            else:
                self.code += f"{i*self.ind}{fld.name} = {fld._type}.from_reader(stream)\n"
        
        tup = ", ".join([f"{f.name}={f.name}" for f in self.fields])
        if self.parent:
            tup = "**vars(base), " + tup
        self.code += f"{i*self.ind}return {self.name}({tup})\n\n"        
        
    def generate_py_to_binary(self):
        i = 1
        self.code += f"{self.ind*1}def to_binary(self) -> bytes:\n"
        i += 1
        self.code += f"{i*self.ind}stream = WriteStream(b\"\")\n"
        for fld in self.fields:
            if fld._type in ENUMS:
                self.code += f"{i*self.ind}stream.write(self.{fld.name}.value)\n"
            elif fld._type == "bytes":
                self.code += f"{i*self.ind}stream.write(self.{fld.name})\n"
            elif fld.is_numeric:
                self.code += f"{i*self.ind}stream.write_{fld._type}(self.{fld.name})\n"
            else:
                self.code += f"{i*self.ind}stream.write(self.{fld.name}.to_binary())\n"
        self.code += f"{i*self.ind}return stream.getvalue()\n\n"
        
        
    def generate_py_write(self):
        i = 1
        self.code += f"{self.ind*1}def write(self, stream: WriteStream):\n"
        i+=1
        self.code += f"{i*self.ind}stream.write(self.to_binary())\n"
        # for fld in self.fields:
        #     if fld._type in ENUMS:
        #         self.code += f"{i*self.ind}stream.write(self.{fld.name}.value)\n"
        #     elif fld._type == "bytes":
        #         self.code += f"{i*self.ind}stream.write(self.{fld.name})\n"
        #     elif fld.is_numeric:
        #         self.code += f"{i*self.ind}stream.write_{fld._type}(self.{fld.name})\n"
        #     else:
        #         self.code += f"{i*self.ind}stream.write(self.{fld.name}.to_binary())\n"
        self.code += "\n"
        
    def generate_py(self):
        self.generate_py_fields()
        self.generate_py_from_reader()
        self.generate_py_to_binary()
        self.generate_py_write()
        self.code += "\n"
        
        
            

class HexpatParser():
    def __init__(self, file):
        self.file = Path(file)
        self.text = self.file.read_text(encoding='utf-8')
        self.structs = []
        self.imports = IMPORTS
        self.code = ""
        self.banned = ["Curve"]
        self.top_msg = "# Highly Experimental Code\n# Generated by GeneratorPy.py\n"
        
        
    def parse(self):
        assert("struct" in self.text), "No struct found in file"
        v1 = ["struct" + s for s in self.text.split("struct")[1:]]
        v2 = []
        for i, struct in enumerate(v1):
            if "}" not in struct:
                raise ValueError(f"Struct {repr(struct)} is not closed with '}}'")
            v2.append(struct.split("}")[0] + "}")
        for s in v2:
            self.structs.append(StructParser(s))
            
    def to_string(self):
        self.code = "\n".join(self.imports) + "\n\n" + self.top_msg + "\n\n"
        failed = []
        skipped = []
        failed_no_name = 0
        for s in self.structs:
            if s.parse():
                s.generate_py()
                if s.name and s.name in self.banned:
                    skipped.append(s.name)
                    continue
                self.code += s.code
            else:
                if s.name is not None:
                    failed.append(s.name)
                else:
                    failed_no_name += 1
        if failed:
            print(f"Failed to parse {len(failed)} structs: ", "\n".join(failed))
        else:
            print(f"All structs parsed successfully.")
        print(f"{failed_no_name} structs failed to parse due to missing names or other.")
        if skipped:
            print(f"Skipped {len(skipped)} structs: ", "\n".join(skipped))
        self.code = self.code.replace("int # bool", "bool")
        return self.code
    
    def save_to_file(self, file):
        Path(file).write_text(self.to_string(), encoding='utf-8')
        
if __name__== "__main__":
    
    os.system("cls" if os.name == "nt" else "clear")
    file = "bphcl.hexpat"
    hexpat = HexpatParser(file)
    hexpat.parse()
    hexpat.save_to_file("ExperimentalEX.py")
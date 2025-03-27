from dataclasses import dataclass
import io
import struct
from BphclEnums import ResFileType, ResSectionType
from BphclSmallDataclasses import *

class Bphcl():
    def __init__(self):
        self.file:ResPhive = None

    
    
@dataclass
class ResPhive():
    magic: bytes
    reserve0: int
    reserve1: int
    byte_order_mark: int
    file_type: ResFileType
    max_section_capacity: int
    tagfile_offset: int
    param_offset: int
    file_end_offset: int
    tagfile_size: int
    param_size: int
    file_end_size: int
    _size = 36
    
    def validate(self):
        assert self.magic == b'Phive\x00', f"Invalid magic: {self.magic}, expected b'Phive\\x00'"
        assert self.file_type == ResFileType.Cloth, f"Invalid file type: {self.file_type}, expected ResFileType.Cloth"
        
    
    @staticmethod
    def from_reader(stream: io.BytesIO) -> "ResPhive":
        data = stream.read(ResPhive._size)
        if len(data) != ResPhive._size:
            raise ValueError(f"End of file, expected {ResPhive._size} bytes, got {len(data)} bytes")
        
        unpacked = struct.unpack("<6sBBHBBIIIIII", data)
        return ResPhive(
            magic=unpacked[0],
            reserve0=unpacked[1],
            reserve1=unpacked[2],
            byte_order_mark=unpacked[3],
            file_type=ResFileType(unpacked[4]),
            max_section_capacity=unpacked[5],
            tagfile_offset=unpacked[6],
            param_offset=unpacked[7],
            file_end_offset=unpacked[8],
            tagfile_size=unpacked[9],
            param_size=unpacked[10],
            file_end_size=unpacked[11],
        )
        
    
@dataclass
class ResTagFile(ResTagfileSectionHeader):
    data_offset: int    # u128
    data_size: int      # u32
    offset: int        # s32

    @staticmethod
    def from_reader(stream: io.BytesIO) -> "ResTagFile":
        size = Size.from_reader(stream)
        signature = stream.read(4)
        #TODO: finish
    
@dataclass
class ResSection(ResTagfileSectionHeader):
    _type: ResSectionType
    #SDKV
    version: str = None
    #DATA
    data: bytes = None
    #TYPE
    sections: list = None # ResTypeSection
    #INDX
    index: list = None # ResIndexSection
    #TCRF and TCID invalid

    @staticmethod
    def from_reader(stream: io.BytesIO) -> "ResSection":
        size = Size.from_reader(stream)
        signature = stream.read(4)
        _type = ResSectionType(signature)
        match _type:
            case ResSectionType.SDKV:
                version_bytes = stream.read(8)
                try:
                    version = version_bytes.decode("utf-8")
                except Exception as e:
                    print(f"Error: {repr(version_bytes)} is not a valid utf-8 string")
                    raise e
                return ResSection(size=size, signature=signature, _type=_type, version=version)
            case ResSectionType.DATA:
                data = stream.read(size.size - 8)
                return ResSection(size=size, signature=signature, _type=_type, data=data)
            case ResSectionType.TYPE:
                raise NotImplementedError("ResSectionType.TYPE not implemented")
            case ResSectionType.INDX:
                raise NotImplementedError("ResSectionType.INDX not implemented")
            case ResSectionType.TCRF:
                raise ValueError(f"Invalid ResSectionType: TCRF, not supported")
            case ResSectionType.TCID:
                raise ValueError(f"Invalid ResSectionType: TCID, not supported")
            case _:
                raise ValueError(f"Invalid ResSectionType: {_type}")
            

@dataclass 
class ResNamedType():
    index: VarUInt
    template_count: VarUInt
    templates: list[ResTypeTemplate] = None
    name: str = "" # to be filled in later

    @staticmethod
    def from_reader(stream: io.BytesIO) -> "ResNamedType":
        index = VarUInt.from_reader(stream)
        template_count = VarUInt.from_reader(stream)
        templates = []
        for _ in range(template_count):
            templates.append(ResTypeTemplate.from_reader(stream))
        return ResNamedType(index=index, template_count=template_count, templates=templates)
        
    def update_string_names(self, res_type_section: ResTypeSection):
        for i, template in enumerate(self.templates):
            self.templates[i].name = res_type_section.strings[template.index.byte0]
        


@dataclass
class ResTypeSection():
    size: Size
    signature: bytes
    #TPTR | TPAD
    pad: bytes = None
    # (b"TST1" , b"FST1" , b"AST1" , b"TSTR" , b"FSTR" , b"ASTR")
    strings: list[str] = None
    #"TNA1" | "TNAM"
    type_count: VarUInt = None
    types: list[ResNamedType] = None
    #"TBDY" | "TBOD"
    type_bodies = None
    
    
    @staticmethod
    def from_reader(reader: io.BytesIO) -> "ResTypeSection":
        size = Size.from_reader(reader)
        signature = reader.read(4)
        _s = signature
        if _s in (b"TPTR", b"TPAD"):
            pad = reader.read(size.size - 8)
            return ResTypeSection(size=size, signature=signature, pad=pad)
        if _s in (b"TST1" , b"FST1" , b"AST1" , b"TSTR" , b"FSTR" , b"ASTR"):
            data = reader.read(size.size - 8)
            strings_as_bin = data.split(b"\x00")
            strings = []
            for entry in strings_as_bin:
                try:
                    strings.append(entry.decode("utf-8"))
                except Exception as e:
                    print(f"Error: {repr(entry)} is not a valid utf-8 string")
                    raise e
            return ResTypeSection(size=size, signature=signature, strings=strings)
        if _s in (b"TNA1", b"TNAM"):
            type_count = VarUInt.from_reader(reader)
            types = []
            for i in range(type_count._value):
                types.append(ResNamedType.from_reader(reader))
                #remember to update name string fields later
            return ResTypeSection(size=size, signature=signature, type_count=type_count, types=types)
        if _s
        
        return ResTypeSection(size=size, signature=signature)
    



if __name__ == "__main__":
    with open("Armor_225_Head.bphcl", "rb") as f:
        bphcl = Bphcl()
        bphcl.file = ResPhive.from_reader(io.BytesIO(f.read()))
        print(bphcl.file)
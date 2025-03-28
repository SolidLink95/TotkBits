from dataclasses import dataclass
import io
import os
import struct
from BphclEnums import ResFileType, ResSectionType, ResTypeSectionSignature, signature_to_enum
from BphclSmallDataclasses import *
from Stream import ReadStream
import oead

class Bphcl():
    def __init__(self):
        self.file:ResPhive = None
        self.tag_file:ResTagFile = None
        self.cloth_section = None
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "Bphcl":
        bphcl = Bphcl()
        bphcl.file = ResPhive.from_reader(stream)
        bphcl.tag_file = ResTagFile.from_reader(stream)
        aamp_signature = stream.read(4)
        if aamp_signature != b"AAMP":
            raise ValueError(f"Invalid AAMP signature: {aamp_signature}, expected b'AAmp'")
        stream.seek(-4, io.SEEK_CUR)
        bphcl.cloth_section = oead.aamp.ParameterIO.from_binary(stream.read())
        bphcl.tag_file.update_strings()
        return bphcl

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
    def from_reader(stream: ReadStream) -> "ResPhive":
        data = stream.read(ResPhive._size)
        if len(data) != ResPhive._size:
            raise ValueError(f"End of file, expected {ResPhive._size} bytes, got {len(data)} bytes")
        
        unpacked = struct.unpack("<6sBBHBBIIIIII", data)
        res =  ResPhive(
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
        res.validate()
        stream.seek(res.tagfile_offset, io.SEEK_SET)
        return res
        
    

            
        
    
    
@dataclass
class ResIndexSection(ResTagfileSectionHeader):
    items: list[ResItem] = None
    internal_patches: list[ResPatch] = None
    terminator: int = None #u32
    external_patches: list[ResPatch] = None
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResIndexSection":
        hdr = ResTagfileSectionHeader.from_reader(stream)
        res = ResIndexSection(size=hdr.size, signature=hdr.signature)
        _offset = stream.tell() - len(hdr)
        match hdr.signature:
            case b"ITEM":
                res.items = []
                while stream.tell() - _offset <= res.size.size - 1:
                    res.items.append(ResItem.from_reader(stream))
            case b"PTCH":
                res.internal_patches = []
                while stream.tell() - _offset <= res.size.size - 1:
                    poss_null  = stream.read_u32()
                    stream.seek(-4, io.SEEK_CUR)
                    if poss_null == 0:
                        break
                    res.internal_patches.append(ResPatch.from_reader(stream))
                res.terminator = stream.read_u32()
                res.external_patches = []
                while stream.tell() - _offset <= res.size.size - 1:
                    res.external_patches.append(ResPatch.from_reader(stream))
            case _:
                raise ValueError(f"Invalid ResIndexSection signature: {hdr.signature}")
        return res
                    
                
                
        
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
    def from_reader(stream: ReadStream) -> "ResSection":
        size = Size.from_reader(stream)
        signature = stream.read(4)
        _type = ResSectionType(signature)
        res = ResSection(size=size, signature=signature, _type=_type)
        _offset = stream.tell() - 8
        match _type:
            case ResSectionType.SDKV:
                res.version = stream.read_string_w_size(size.size - 8) #
                assert(res.version == '20220100'), f"Invalid version for SDKV: {res.version}, expected '20220100'"
            case ResSectionType.DATA:
                res.data = stream.read(size.size - 8)
            case ResSectionType.TYPE:
                res.sections = []
                while stream.tell() - _offset <= size.size - 1:
                    res.sections.append(ResTypeSection.from_reader(stream))
            case ResSectionType.INDX:
                res.sections = []
                while stream.tell() - _offset <= size.size - 1:
                    res.sections.append(ResIndexSection.from_reader(stream))
            case ResSectionType.TCRF:
                raise ValueError(f"Invalid ResSectionType: TCRF, not supported")
            case ResSectionType.TCID:
                raise ValueError(f"Invalid ResSectionType: TCID, not supported")
            case _:
                raise ValueError(f"Invalid ResSectionType: {_type}")
        return res
    
    def update_strings(self):
        if self._type == ResSectionType.TYPE:
            for section in self.sections:
                if isinstance(section, ResTypeSection):
                    section.update_string_names(section)
        elif self._type == ResSectionType.INDX:
            for section in self.sections:
                if isinstance(section, ResIndexSection):
                    section.update_strings()
        else:
            raise ValueError(f"Invalid ResSectionType: {self._type}")
    
    
@dataclass 
class ResNamedType():
    index: VarUInt
    template_count: VarUInt
    templates: list[ResTypeTemplate] = None
    name: str = "" # to be filled in later

    @staticmethod
    def from_reader(stream: ReadStream) -> "ResNamedType":
        index = VarUInt.from_reader(stream)
        template_count = VarUInt.from_reader(stream)
        templates = None
        if template_count._value:
            templates = []
            for _ in range(template_count._value):
                templates.append(ResTypeTemplate.from_reader(stream))
        return ResNamedType(index=index, template_count=template_count, templates=templates)
        
    def update_string_names(self, res_type_section):
        for i, template in enumerate(self.templates):
            self.templates[i].name = res_type_section.strings[template.index.byte0]
        self.name = res_type_section.strings[self.index.byte0]
        

@dataclass
class ResTypeBodyInterface():
    type_index: VarUInt
    flags: VarUInt
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeBodyInterface":
        type_index = VarUInt.from_reader(stream)
        flags = VarUInt.from_reader(stream)
        return ResTypeBodyInterface(type_index=type_index, flags=flags)


@dataclass
class ResTypeBodyDeclaration():
    name_index: VarUInt
    flags: VarUInt 
    offset: VarUInt 
    type_index: VarUInt 
    reserve: int = None
    name: str = None # to be filled in later
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeBodyDeclaration":
        name_index = VarUInt.from_reader(stream)
        flags = VarUInt.from_reader(stream)
        reserve = None
        if flags._value >> 7 & 1:
            reserve = stream.read(1)[0]
        offset = VarUInt.from_reader(stream)
        type_index = VarUInt.from_reader(stream)
        return ResTypeBodyDeclaration(name_index=name_index, flags=flags, reserve=reserve, offset=offset, type_index=type_index)
    
    def update_string_name(self, res_type_section):
        self.name = res_type_section.strings[self.name_index.byte0] 
    

@dataclass
class ResTypeBody:
    type_index: VarUInt
    parent_type_index: VarUInt = None
    flags: VarUInt = None
    format: VarUInt = None
    subtype_index: VarUInt = None
    version: VarUInt = None
    size: VarUInt = None
    alignment: VarUInt = None
    unknown_flags: VarUInt = None
    decl_count: VarUInt = None
    declarations: list[ResTypeBodyDeclaration] = None
    interface_count: VarUInt = None
    format: list[ResTypeBodyInterface] = None
    attr_index: VarUInt = None
    attr: str = None
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeBody":
        type_index = VarUInt.from_reader(stream)
        res = ResTypeBody(type_index=type_index)
        _val = type_index._value
        if _val != 0:
            res.parent_type_index = VarUInt.from_reader(stream)
            res.flags = VarUInt.from_reader(stream)
            _fl = res.flags._value
            if (_fl >> 0) & 1:
                res.format = VarUInt.from_reader(stream)
            if (_fl >> 1) & 1:
                res.subtype_index = VarUInt.from_reader(stream)
            if (_fl >> 2) & 1:
                res.version = VarUInt.from_reader(stream)
            if (_fl >> 3) & 1:
                res.size = VarUInt.from_reader(stream)
                res.alignment = VarUInt.from_reader(stream)
            if (_fl >> 4) & 1:
                res.unknown_flags = VarUInt.from_reader(stream)
            if (_fl >> 5) & 1:
                res.decl_count = VarUInt.from_reader(stream)
                if res.decl_count._value:
                    res.declarations = []
                    for _ in range(res.decl_count._value):
                        res.declarations.append(ResTypeBodyDeclaration.from_reader(stream))
            if (_fl >> 6) & 1:
                res.interface_count = VarUInt.from_reader(stream)
                if res.interface_count._value:
                    res.format = []
                    for _ in range(res.interface_count._value):
                        res.format.append(ResTypeBodyInterface.from_reader(stream))
            if (_fl >> 7) & 1:
                res.attr_index = VarUInt.from_reader(stream)
                
        return res
                
            
    def update_string_names(self, res_type_section):
        if self.declarations is not None:
            for i, decl in enumerate(self.declarations):
                self.declarations[i].update_string_name(res_type_section)
        if self.attr_index is not None:
            self.attr = res_type_section.strings[self.attr_index.byte0]
        


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
    #THSH
    hash_count: VarUInt = None
    hashes: list[ResTypeHash] = None
    #("TSHA" | "TPRO" | "TPHS" | "TSEQ") and other unsupported types
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeSection":
        size = Size.from_reader(stream)
        signature = stream.read(4)
        _s = signature
        _offset = stream.tell() - 8
        res = ResTypeSection(size=size, signature=signature)
        x = f"{stream.tell():08X}"
        _type = signature_to_enum(signature)
        _offset1 = stream.tell()
        match _type:
            case ResTypeSectionSignature.TPTR:
                res.pad = stream.read(size.size - 8)
            case ResTypeSectionSignature.TST1:
                # data = stream.read(size.size - 8)
                # strings_as_bin = data.split(b"\x00")
                res.strings = []
                while stream.tell() - _offset <= size.size - 1:
                    new_string = stream.read_string()
                    if new_string is None:
                        break
                    res.strings.append(new_string)
                if _s == b"FST1":
                    res.strings.append("FFFFFF40")
            case ResTypeSectionSignature.TNA1:
                res.type_count = VarUInt.from_reader(stream)
                res.types = []
                for i in range(res.type_count._value - 1):
                    res.types.append(ResNamedType.from_reader(stream))
                    #remember to update name string fields later
            case ResTypeSectionSignature.TBDY:
                res.type_bodies = []
                while stream.tell() - _offset <= size.size - 1:
                    res.type_bodies.append(ResTypeBody.from_reader(stream))
            case ResTypeSectionSignature.THSH:
                res.hash_count = VarUInt.from_reader(stream)
                res.hashes = []
                for i in range(res.hash_count._value):
                    res.hashes.append(ResTypeHash.from_reader(stream))
            case ResTypeSectionSignature.UNSUPPORTED:
                raise ValueError(f"Unsupported ResTypeSection signature: {_s}")
            case _:
                raise ValueError(f"Invalid ResTypeSection signature: {_s}")
        _offset2 = stream.tell() - _offset1 + 8
        if _offset2 < size.size:
            stream.read(size.size - _offset2)
        return res

    def update_string_names(self, res_type_section):
        match signature_to_enum(self.signature):
            case ResTypeSectionSignature.TNA1:
                if self.types:
                    for i, type in enumerate(self.types):
                        self.types[i].update_string_names(res_type_section)
            case ResTypeSectionSignature.TBDY:
                if self.type_bodies:
                    for i, type_body in enumerate(self.type_bodies):
                        self.type_bodies[i].update_string_names(res_type_section)
            case _:
                pass
        
        
@dataclass
class ResTagFile(ResTagfileSectionHeader):
    sdkv_section: ResSection = None
    data_section: ResSection = None
    type_section: ResSection = None
    indx_section: ResSection = None
    

    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTagFile":
        size = Size.from_reader(stream)
        signature = stream.read(4)
        if signature != b"TAG0":
            raise ValueError(f"Invalid ResTagFile signature: {signature}, expected b'TAG0' at {stream.tell():08X}")
        res = ResTagFile(size=size, signature=signature)
        main_offset = stream.tell() - 8
        while stream.tell() - main_offset <= size.size - 1:
            _offset = stream.tell()
            res_section = ResSection.from_reader(stream)
            match res_section.signature:
                case b"SDKV":
                    res.sdkv_section = ResSection.from_reader(stream)
                case b"DATA":
                    res.data_section = ResSection.from_reader(stream)
                case b"TYPE":
                    res.type_section = ResSection.from_reader(stream)
                case b"INDX":
                    res.indx_section = ResSection.from_reader(stream)
                case _:
                    pass # error handling already done
            if res.data_section is not None and res.type_section is not None and res.indx_section is not None:
                if stream.tell() - main_offset <= size.size - 1:
                    raise ValueError(f"Invalid ResTagFile structure: DATA, TYPE and INDX sections found but not all of data has been read")
        return res

    def update_strings(self):
        tst1_section: ResTypeSection = next((s for s in self.type_section.sections if signature_to_enum(s.signature)==ResTypeSectionSignature.TST1), None)
        if tst1_section is None:
            raise ValueError("TST1 section not found")
        if tst1_section.strings is None:
            raise ValueError("TST1 section strings are None")
        for i, section in enumerate(self.type_section.sections):
            if not isinstance(section, ResTypeSection):
                raise ValueError(f"Section {i} inside TYPE is not a ResTypeSection")
            self.type_section.sections[i].update_string_names(tst1_section)
        
        
        
        

if __name__ == "__main__":
    os.system("cls")
    with open("Armor_225_Head.bphcl", "rb") as f:
        bphcl = Bphcl()
        bphcl.from_reader(ReadStream(f.read()))
        print(bphcl.file)
        print(bphcl.tag_file)
        print(oead.aamp.ParameterIO.to_text(bphcl.cloth_section))
    pass
from dataclasses import dataclass
import hashlib
import io
import os
from pathlib import Path
import struct
import sys
from BphclEnums import ResFileType, ResSectionType, ResTypeSectionSignature
from BphclSmallDataclasses import *
from Experimental import hclClothContainer, hkMemoryResourceContainer, hkaAnimationContainer
from Havok import hkRootLevelContainer
from Stream import ReadStream, WriteStream
import oead
import yaml
import json
from aamp_totk_hashes import AAMP_TOTK_HASHES
from util import DataConverter, _hex


class Bphcl():
    def __init__(self):
        self.file:ResPhive = None
        self.tag_file:ResTagFile = None
        self.cloth_section = None
        self.aamp_hashes = AAMP_TOTK_HASHES
    
    # def remove_even_more_entries
    
    def cloth_section_to_dict(self) -> dict:
        """Very dirty approach, will fix later"""
        yaml_str = oead.aamp.ParameterIO.to_text(self.cloth_section)
        bad_tags = ["!io", "!list", "!obj"]
        for tag in bad_tags:
            yaml_str = yaml_str.replace(tag, "")
        for crc_hash, name in self.aamp_hashes.items():
            yaml_str = yaml_str.replace(crc_hash, name)
        yaml_dict = yaml.safe_load(yaml_str)
        return yaml_dict
    
    def to_dict(self) -> dict:
        self.validate()
        header = DataConverter.dataclass_to_dict(self.file)
        tag_file = DataConverter.dataclass_to_dict(self.tag_file)
        cloth_section = self.cloth_section_to_dict()
        result = {
            "header": header,
            "tag_file": tag_file,
            "cloth_section": cloth_section,
        }
        return result
    
    def to_yaml(self) -> str:
        data = self.to_dict()
        yaml_str = yaml.dump(data, default_flow_style=False, sort_keys=False)
        return yaml_str
    
    def to_json(self) -> str:
        data = self.to_dict()
        json_str = json.dumps(data, indent=4, default=str)
        return json_str
    
    def to_json_file(self, file_path: str) -> None:
        data = self.to_dict()
        with open(file_path, "w") as f:
            json.dump(data, f, indent=4, default=str)
    
    def to_yaml_file(self, file_path: str) -> None:
        data = self.to_dict()
        with open(file_path, "w") as f:
            yaml.dump(data, f, default_flow_style=False, sort_keys=False)
    
    def validate(self):
        if self.file is None:
            raise ValueError("ResPhive section not found or not read yet")
        if self.tag_file is None:
            raise ValueError("TAG0 section not found or not read yet")
        if self.cloth_section is None:
            raise ValueError("Cloth section not found or not read yet")
        self.file.validate()
        self.tag_file.validate()
    
    @staticmethod
    def from_file(file_path: str) -> "Bphcl":
        with open(file_path, "rb") as f:
            data = f.read()
        return Bphcl.from_binary(data)
    
    @staticmethod
    def from_binary(data: bytes) -> "Bphcl":
        stream = ReadStream(data)
        return Bphcl.from_reader(stream)
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "Bphcl":
        bphcl = Bphcl()
        bphcl.file = ResPhive.from_reader(stream)
        bphcl.tag_file = ResTagFile.from_reader(stream)
        aamp_signature = stream._read_exact(4)
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
        data = stream._read_exact(ResPhive._size)
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
    padding: bytes = None
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResIndexSection":
        _offset = stream.tell()
        aamp_pos = stream.find_next_occ(b"AAMP") # assuming this is the last tagfile part before aamp part
        hdr = ResTagfileSectionHeader.from_reader(stream)
        res = ResIndexSection(size=hdr.size, signature=hdr.signature)
        # _offset = stream.tell() - len(hdr)
        match hdr.signature:
            case b"ITEM":
                _size = res.size.size - 8
                assert _size > 0 and _size % 12 == 0, f"Invalid size for ITEM: {res.size.size}"
                item_count = _size // 12
                res.items = [ResItem.from_reader(stream) for _ in range(item_count)]
            case b"PTCH":
                res.internal_patches = []
                while True:
                    if stream.tell() >= aamp_pos:
                        break
                        # raise ValueError(f"Invalid ResIndexSection: PTCH, internal patches not terminated by 0! Entered AAMP section at {stream.tell():08X}")
                    type_index = stream.read_u32()
                    stream.seek(-4, io.SEEK_CUR)
                    if type_index == 0:
                        break
                    res.internal_patches.append(ResPatch.from_reader(stream))
                if stream.tell() < aamp_pos:
                    res.terminator = stream.read_u32()
                if stream.tell() < aamp_pos:
                    if aamp_pos - stream.tell() > ResPatch.minimal_size():
                        res.external_patches = []
                        while stream.tell() < aamp_pos:
                            res.external_patches.append(ResPatch.from_reader(stream))
                if stream.tell() < aamp_pos:
                    res.padding = stream._read_exact(aamp_pos - stream.tell()) #probably not needed
            case _:
                raise ValueError(f"Invalid ResIndexSection signature: {hdr.signature}")
        return res
                    
                
                
        
@dataclass
class ResSection(ResTagfileSectionHeader):
    _type: ResSectionType
    size: Size
    signature: str
    #SDKV
    version: str = None
    #DATA
    data: bytes = None
    data_offset: int = None # u32
    data_fixed: bytes = None
    # root_container: hkRootLevelContainer = None
    #TYPE
    sections: list = None # ResTypeSection
    #INDX
    index: list = None # ResIndexSection
    #TCRF and TCID invalid

    @staticmethod
    def from_reader(stream: ReadStream) -> "ResSection":
        size = Size.from_reader(stream)
        signature = stream._read_exact(4)
        _type = ResSectionType(signature)
        res = ResSection(_type=_type, size=size, signature=signature.decode(stream.encoding))
        _offset = stream.tell() - 8
        match _type:
            case ResSectionType.SDKV:
                res.version = stream.read_string_w_size(size.size - 8) #
            case ResSectionType.DATA:
                res.data_offset = stream.tell()
                # current_pos = stream.tell()
                # stream.data_offset = current_pos
                res.data = stream._read_exact(size.size - 8)
                # new_pos = stream.tell()
                # stream.seek(current_pos, io.SEEK_SET)
                # res.root_container = hkRootLevelContainer.from_reader(stream)
                # stream.seek(new_pos, io.SEEK_SET)
            case ResSectionType.TYPE:
                res.sections = []
                while stream.tell() - _offset <= size.size - 1:
                    res.sections.append(ResTypeSection.from_reader(stream))
            case ResSectionType.INDX:
                res.sections = []
                while stream.tell() - _offset <= size.size - 1:
                    res.sections.append(ResIndexSection.from_reader(stream))
            case ResSectionType.TCRF:
                raise ValueError(f"Unsupported ResSectionType: TCRF")
            case ResSectionType.TCID:
                raise ValueError(f"Unsupported ResSectionType: TCID")
            case _:
                raise ValueError(f"Invalid ResSectionType: {_type}")
        return res
    
    def update_strings(self):
        
        if self._type == ResSectionType.TYPE or self.sections is not None:
            for section in self.sections:
                if isinstance(section, ResTypeSection):
                    section.update_string_names(section)
        # elif self._type == ResSectionType.INDX:
        #     for i, section in enumerate(self.sections):
        #         if isinstance(section, ResIndexSection):
        #             self.sections[i].update_strings()
        # else:
        #     raise ValueError(f"Invalid ResSectionType: {self._type}")
    
    
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
        # if template_count._value:
        templates = []
        for _ in range(template_count._value):
            templates.append(ResTypeTemplate.from_reader(stream))
        return ResNamedType(index=index, template_count=template_count, templates=templates)
        
    def update_string_names(self, res_type_section):
        if self.templates is not None:
            for i, template in enumerate(self.templates):
                self.templates[i].name = res_type_section.strings[template.index._byte0]
        self.name = res_type_section.strings[self.index._byte0]
        

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
    name: str = "" # to be filled in later
    
    def __repr__(self):
        _n = f"UNKNOWN" if not self.name else f"{self.name}"
        return f"ResTypeBodyDeclaration({_n})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeBodyDeclaration":
        name_index = VarUInt.from_reader(stream)
        flags = VarUInt.from_reader(stream)
        reserve = None
        if flags._value >> 7 & 1:
            reserve = stream._read_exact(1)[0]
        offset = VarUInt.from_reader(stream)
        type_index = VarUInt.from_reader(stream)
        return ResTypeBodyDeclaration(name_index=name_index, flags=flags, reserve=reserve, offset=offset, type_index=type_index)
    
    def update_string_name(self, res_type_section):
        self.name = res_type_section.strings[self.name_index._byte0] 
    

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
    
    # def __repr__(self):
    #     n = "UNKNOWN" if self.
    #     return f"ResTypeBody()"
    
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
            self.attr = res_type_section.strings[self.attr_index._byte0]
        


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
    padding: str = None
    
    @staticmethod
    def signature_to_enum(signature: bytes) -> ResSectionType:
        """Convert a signature to the corresponding enum value."""
        if len(signature) != 4:
            raise ValueError(f"Invalid signature length: {len(signature)} expected 4")
        for entry in ResTypeSectionSignature:
            if signature in entry.value:
                return entry
        raise ValueError(f"Unknown signature: {signature}")
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeSection":
        size = Size.from_reader(stream)
        signature = stream._read_exact(4)
        _offset = stream.tell() - 8
        res = ResTypeSection(size=size, signature=signature)
        x = f"{stream.tell():08X}"
        _type = ResTypeSection.signature_to_enum(signature)
        _offset1 = stream.tell()
        match _type:
            case ResTypeSectionSignature.TPTR:
                res.pad = stream._read_exact(size.size - 8)
                res.pad = len(res.pad)
            case ResTypeSectionSignature.TST1:
                res.strings = []
                while stream.tell() - _offset <= size.size - 1:
                    new_string = stream.read_string()
                    if new_string is None:
                        break
                    res.strings.append(new_string)
            case ResTypeSectionSignature.TNA1:
                res.type_count = VarUInt.from_reader(stream)
                res.types = []
                for i in range(res.type_count._value - 1):
                    res.types.append(ResNamedType.from_reader(stream))
                    #remember to update name string fields later
            case ResTypeSectionSignature.TBDY:
                res.type_bodies = []
                while stream.tell() - _offset < size.size - 1:
                    res.type_bodies.append(ResTypeBody.from_reader(stream))
            case ResTypeSectionSignature.THSH:
                res.hash_count = VarUInt.from_reader(stream)
                res.hashes = []
                for i in range(res.hash_count._value):
                    res.hashes.append(ResTypeHash.from_reader(stream))
            case ResTypeSectionSignature.UNSUPPORTED:
                raise ValueError(f"Unsupported ResTypeSection signature: {signature}")
            case _:
                raise ValueError(f"Invalid ResTypeSection signature: {signature}")
        _offset2 = stream.tell() - _offset1 + 8
        if _offset2 < size.size:
            res.padding = stream._read_exact(size.size - _offset2).hex().upper()
        return res

    def update_string_names(self, res_type_section):
        match ResTypeSection.signature_to_enum(self.signature):
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
    root_container: hkRootLevelContainer = None
    #Cloth sections
    cloth_container: hclClothContainer = None
    mem_resource_container: hkMemoryResourceContainer = None
    animation_container: hkaAnimationContainer = None
    #helper fields
    type_strings: list[str] = None
    field_strings: list[str] = None
    
    def validate(self):
        if self.data_section is None:
            raise ValueError("DATA section not found")
        if self.type_section is None:
            raise ValueError("TYPE section not found")
        if self.indx_section is None:
            raise ValueError("INDX section not found")
        if self.sdkv_section.version is not None:
            assert(self.sdkv_section.version == '20220100'), f"Invalid version for SDKV: {self.sdkv_section.version}, expected '20220100'"
        
        
    
    def read_havok(self, stream: ReadStream):
        reader = ReadStream(self.data_section.data_fixed) #if stream is None else stream
        writer = WriteStream(self.data_section.data_fixed) #if stream is None else stream
        # current_pos = stream.tell()
        # stream.seek(0, io.SEEK_SET)
        # data_offset = stream.find_next_occ(b"DATA")
        # if data_offset == -1:
        #     raise ValueError("DATA section not found, very odd")
        # stream._read_exact(4) # skip signature
        # stream.seek(data_offset, io.SEEK_SET)
        # with open("DATA.bin", "wb") as f:
        #     f.write(self.data_section.data_fixed)
        reader.data_offset = self.data_section.data_offset
        stream = ReadStream(self.data_section.data_fixed) #if stream is None else stream
        self.root_container = hkRootLevelContainer.from_reader(reader)
        reader.seek(self.root_container.cloth_offset - self.data_section.data_offset, io.SEEK_SET)
        self.cloth_container = hclClothContainer.from_reader(reader)
        reader.seek(self.root_container.resource_offset- self.data_section.data_offset, io.SEEK_SET)
        self.mem_resource_container = hkMemoryResourceContainer.from_reader(reader, is_root=True)
        reader.seek(self.root_container.animation_offset- self.data_section.data_offset, io.SEEK_SET)
        self.animation_container = hkaAnimationContainer.from_reader(reader)
        return
        
        
    def update_fields(self, stream: ReadStream):
        self.validate()
        self.get_helper_strings()
        self.update_strings()
        self.handle_relocations()
        self.read_havok(stream)

    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTagFile":
        size = Size.from_reader(stream)
        signature = stream._read_exact(4)
        if signature != b"TAG0":
            raise ValueError(f"Invalid ResTagFile signature: {signature}, expected b'TAG0' at {stream.tell():08X}")
        res = ResTagFile(size=size, signature=signature)
        main_offset = stream.tell() - 8
        while stream.tell() - main_offset <= size.size - 1:
            _offset = stream.tell()
            res_section = ResSection.from_reader(stream)
            match res_section.signature:
                case "SDKV":
                    res.sdkv_section = res_section
                case "DATA":
                    res.data_section = res_section
                case "TYPE":
                    res.type_section = res_section
                case "INDX":
                    res.indx_section = res_section
                case _:
                    pass # error handling already done
            if res.data_section is not None and res.type_section is not None and res.indx_section is not None:
                if stream.tell() - main_offset <= size.size - 1:
                    raise ValueError(f"Invalid ResTagFile structure: DATA, TYPE and INDX sections found but not all of data has been read")
        res.update_fields(stream)
        return res

    # Getters
    def get_type_section(self, section_type: ResTypeSectionSignature|bytes) -> tuple[int, ResTypeSection]:
        for i, section in enumerate(self.type_section.sections):
            if isinstance(section, ResTypeSection):
                if isinstance(section_type, ResTypeSectionSignature):
                    _check = ResTypeSection.signature_to_enum(section.signature) == section_type
                else:
                    _check = section.signature == section_type
                if _check:
                    return (i, section)
        raise ValueError(f"Section {section_type} not found in TYPE section")
    
    def get_tst1_section(self):
        return self.get_type_section(b"TST1")
    
    def get_fst1_section(self):
        return self.get_type_section(b"FST1")
        
    def get_helper_strings(self):
        _, tst1_section = self.get_tst1_section()
        self.type_strings = tst1_section.strings
        _, fst1_section = self.get_fst1_section()
        self.field_strings = fst1_section.strings
    
    def get_indx_section_by_signature(self, signature: bytes) -> tuple[int, ResIndexSection]:
        for i, section in enumerate(self.indx_section.sections):
            if isinstance(section, ResIndexSection) and section.signature == signature:
                return (i, section)
        raise ValueError(f"Subsection {signature} not found in TYPE section")
    
    def get_item_section(self):
        return self.get_indx_section_by_signature(b"ITEM")

    def get_ptch_section(self):
        return self.get_indx_section_by_signature(b"PTCH")
    
    def update_strings(self):
        # tst1_section: ResTypeSection = next((s for s in self.type_section.sections if ResTypeSection.signature_to_enum(s.signature)==ResTypeSectionSignature.TST1), None)
        _, tst1_section = self.get_tst1_section()
        for i, section in enumerate(self.type_section.sections):
            if not isinstance(section, ResTypeSection):
                raise ValueError(f"Section {i} inside TYPE is not a ResTypeSection")
            self.type_section.sections[i].update_string_names(tst1_section)
    

        
    
    def handle_relocations(self):
        # try:
        ptch_section_index, ptch_section = self.get_ptch_section()
        # except ValueError:
        #     return # exit gracefully if no PTCH section is found
        type_section_index = self.type_section
        type_name_index, type_name_section = self.get_type_section(b"TNA1")
        index_section_index = index_section = self.indx_section
        item_section_index, item_section = self.get_item_section()
        _data_offset = self.data_section.data_offset
        data_stream_reader = ReadStream(self.data_section.data)
        data_stream_writer = WriteStream(self.data_section.data)
        data_stream_reader.seek(0, io.SEEK_SET)
        data_stream_writer.seek(0, io.SEEK_SET)
        # MISSING = {}
        # writer= WriteStream()
        # special_patches = ['T*']
        def inplace_fixup(offset: int, type_index: int): #u32, u32
            global MISSING, _log
            data_stream_reader.seek(offset, io.SEEK_SET)
            index = data_stream_reader.read_u32()
            size, _name, _type_index = None, None, int(type_index)
            if index >= len(item_section.items):
                data_stream_writer.seek(offset)
                data_stream_writer.write_u64(0)
                return
            
            if _type_index > 0:
                _named_type = type_name_section.types[_type_index-1]
                _name = _named_type.name
                _item = item_section.items[index]
                if not _name:
                    raise ValueError(f"Update strings fields for type_name_section: {repr(_name)}")
                if _name.startswith(("hkArray",)): # too very specific, but matches hexpat logic
                    assert _named_type.index._byte0 == 1, f"Invalid type name: {_name}"
                    data_stream_writer.seek(offset + 8, io.SEEK_SET)
                    data_stream_writer.write_s32(_item.count)
                # if _named_type.index._byte0 == 8:
                #     assert _name in ("T*", "hkStringPtr"), f"Invalid type name: {_name}"
                #     data_stream_writer.seek(offset + 16, io.SEEK_SET)
                #     data_stream_writer.write_s32(_item.count)
                    #
            if _named_type.index._byte0 in (12, 16):
                pass
            # too very specific again, but matches hexpat logic
            _item = item_section.items[index]
            addr = _item.data_offset - offset # u32 as u64
            data_stream_writer.seek(offset, io.SEEK_SET)
            data_stream_reader.seek(offset, io.SEEK_SET)
            ptr = data_stream_reader.read_u64()
            # addr = addr & 0xFFFFFFFFFFFFFFFF if addr < 0 else addr
            data_stream_writer.write_64bit_int(addr)
        
        int_patches_count = len(ptch_section.internal_patches)
        for i in range(int_patches_count):
            # offs_count = len(ptch_section.internal_patches[i].offsets)
            offs_count = ptch_section.internal_patches[i].count
            _type_index = ptch_section.internal_patches[i].type_index
            for j in range(offs_count):
                _offset = ptch_section.internal_patches[i].offsets[j]
                inplace_fixup(_offset, _type_index)
        data_stream_writer.seek(0, io.SEEK_SET)
        self.data_section.data_fixed = data_stream_writer.read()
        
        
        

if __name__ == "__main__":
    os.system("cls")
    # with open("Armor_225_Head.bphcl", "rb") as f:
    bphcl = Bphcl().from_file("Armor_999_Head.bphcl")
    bphcl.to_yaml_file("Armor_999_Head.yaml")
    bphcl.to_json_file("Armor_999_Head.json")
    pass
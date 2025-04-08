from dataclasses import dataclass
import io
from pathlib import Path
import struct
from BphclEnums import ResFileType, ResSectionType, ResTypeSectionSignature
from BphclSmallDataclasses import *
from Experimental import hclClothContainer, hkMemoryResourceContainer, hkaAnimationContainer
from Havok import hkRootLevelContainer
from Stream import ReadStream, WriteStream
from aamp_totk_hashes import AAMP_TOTK_HASHES
from util import  FilteredInternalPath, _hex,AssetsFromRange, is_offset_in_ranges_list

"""Main classes that require specific parsing"""


@dataclass
class ResPhive(BphclBaseObject):
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
    
    def to_binary(self) -> bytes:
        writer = WriteStream()
        writer.write(self.magic)
        writer.write_u8(self.reserve0)
        writer.write_u8(self.reserve1)
        writer.write_u16(self.byte_order_mark)
        writer.write_u8(self.file_type.value)
        writer.write_u8(self.max_section_capacity)
        writer.write_u32(self.tagfile_offset)
        writer.write_u32(self.param_offset)
        writer.write_u32(self.file_end_offset)
        writer.write_u32(self.tagfile_size)
        writer.write_u32(self.param_size)
        writer.write_u32(self.file_end_size)
        pad_size = 16 - (writer.tell() % 16)
        if pad_size > 0 and pad_size != 16:
            writer.write(pad_size * b"\x00")
        # writer.write((self.tagfile_offset-self._size) * b"\x00")
        result = writer.getvalue()
        tmp = result.hex().upper()
        return result
        

@dataclass
class ResIndexSection(ResTagfileSectionHeader):
    items: List[ResItem] = None
    internal_patches: List[ResPatch] = None
    terminator: int = None #u32
    external_patches: List[ResPatch] = None
    padding: bytes = None
    
    def to_binary(self) -> bytes:
        result = super().to_binary()
        if self.items:
            for item in self.items:
                result += item.to_binary()
        if self.internal_patches is not None:
            for patch in self.internal_patches:
                result += patch.to_binary()
        if self.terminator is not None:
            result += struct.pack("<I", self.terminator)
        if self.external_patches is not None:
            for patch in self.external_patches:
                result += patch.to_binary()
        if self.padding is not None:
            result += self.padding
        return result
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "ResIndexSection":
        _offset = stream.tell()
        aamp_pos = stream.find_next_occ(b"AAMP") # assuming this is the last tagfile part before aamp part
        base = super().from_reader(stream)
        # hdr = ResTagfileSectionHeader.from_reader(stream)
        res = cls(**vars(base))
        # _offset = stream.tell() - len(hdr)
        match res.signature:
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
                raise ValueError(f"Invalid ResIndexSection signature: {res.signature}")
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
    sections: List["ResTypeSection"] = None 
    #INDX
    index: List[ResIndexSection] = None # ResIndexSection
    #TCRF and TCID invalid
    _encoding: str = None

    def to_binary(self) -> bytes:
        writer = WriteStream()
        # result = self.size.to_binary()
        writer.write_u32(0) # size placeholder
        writer.write(self.signature.encode(self._encoding if self._encoding else "utf-8"))
        res_section_size = 0
        match self._type:
            case ResSectionType.SDKV:
                if self.version is not None:
                    writer.write(self.version.encode())
            case ResSectionType.DATA:
                if self.data is not None:
                    writer.write(self.data)
            case ResSectionType.TYPE:
                if self.sections is not None:
                    for section in self.sections:
                        writer.write(section.to_binary())
            case ResSectionType.INDX:
                if self.sections is not None:
                    for section in self.sections:
                        writer.write(section.to_binary())
                res_section_size -= 8 # TODO: check why
                        # if section.padding:
                        #     _pad_len = len(section.padding)
                        #     assert(_pad_len % 2 == 0)
                        #     res_section_size -= _pad_len
            case _:
                raise ValueError(f"Invalid ResSectionType: {self._type}")
        res_section_size += writer.tell()
        new_size = Size(is_chunk=self.size.is_chunk, size=res_section_size)
        writer.seek(0, io.SEEK_SET)
        writer.write(new_size.to_binary())
        writer.seek(res_section_size, io.SEEK_SET)
        assert(self.size.size == res_section_size), f"Invalid size: {res_section_size}, expected {self.size.size} for {self.signature}"
        return writer.getvalue()
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResSection":
        _encoding = stream.encoding
        size = Size.from_reader(stream)
        signature = stream._read_exact(4)
        _type = ResSectionType(signature)
        res = ResSection(_type=_type, size=size, signature=signature.decode(stream.encoding),_encoding=_encoding)
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
    
    
@dataclass 
class ResNamedType(BphclBaseObject):
    index: VarUInt
    template_count: VarUInt
    templates: List[ResTypeTemplate] = None
    name: str = "" # to be filled in later

    def to_binary(self) -> bytes:
        result = self.index.to_binary()
        result += self.template_count.to_binary()
        if self.templates is not None:
            for template in self.templates:
                result += template.to_binary()
        return result
    
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
        
    def update_string_names(self, res_type_section: "ResTypeSection"):
        if self.templates is not None:
            for i, template in enumerate(self.templates):
                self.templates[i].name = res_type_section.strings[template.index._byte0]
        self.name = res_type_section.strings[self.index._byte0]
        

@dataclass
class ResTypeBodyInterface(BphclBaseObject):
    type_index: VarUInt
    flags: VarUInt
    
    def to_binary(self) -> bytes:
        result = self.type_index.to_binary()
        result += self.flags.to_binary()
        return result
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeBodyInterface":
        type_index = VarUInt.from_reader(stream)
        flags = VarUInt.from_reader(stream)
        return ResTypeBodyInterface(type_index=type_index, flags=flags)


@dataclass
class ResTypeBodyDeclaration(BphclBaseObject):
    name_index: VarUInt
    flags: VarUInt 
    offset: VarUInt 
    type_index: VarUInt 
    reserve: int = None
    name: str = "" # to be filled in later
    
    def to_binary(self) -> bytes:
        result = self.name_index.to_binary()
        result += self.flags.to_binary()
        if self.reserve is not None:
            result += bytes([self.reserve])
        result += self.offset.to_binary()
        result += self.type_index.to_binary()
        return result
    
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
class ResTypeBody(BphclBaseObject):
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
    declarations: List[ResTypeBodyDeclaration] = None
    interface_count: VarUInt = None
    interfaces: List[ResTypeBodyInterface] = None
    attr_index: VarUInt = None
    attr: str = None
    
    def to_binary(self):
        writer = WriteStream()
        writer.write(self.type_index.to_binary())
        if self.parent_type_index is not None:
            writer.write(self.parent_type_index.to_binary())
        if self.flags is not None:
            writer.write(self.flags.to_binary())
        if self.format is not None:
            writer.write(self.format.to_binary())
        if self.subtype_index is not None:
            writer.write(self.subtype_index.to_binary())
        if self.version is not None:
            writer.write(self.version.to_binary())
        if self.size is not None:
            writer.write(self.size.to_binary())
        if self.alignment is not None:
            writer.write(self.alignment.to_binary())
        if self.unknown_flags is not None:
            writer.write(self.unknown_flags.to_binary())
        if self.decl_count is not None:
            writer.write(self.decl_count.to_binary())
            if self.declarations is not None:
                for decl in self.declarations:
                    writer.write(decl.to_binary())
        if self.interface_count is not None:
            writer.write(self.interface_count.to_binary())
            if self.interfaces is not None:
                for _int in self.interfaces:
                    writer.write(_int.to_binary())
        if self.attr_index is not None:
            writer.write(self.attr_index.to_binary())
        return writer.getvalue()
    
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
class ResTypeSection(BphclBaseObject):
    size: Size
    signature: bytes
    #TPTR | TPAD
    pad: bytes = None
    # (b"TST1" , b"FST1" , b"AST1" , b"TSTR" , b"FSTR" , b"ASTR")
    strings: List[str] = None
    #"TNA1" | "TNAM"
    type_count: VarUInt = None
    types: List[ResNamedType] = None
    #"TBDY" | "TBOD"
    type_bodies: List["ResTypeBody"] = None
    #THSH
    hash_count: VarUInt = None
    hashes: List[ResTypeHash] = None
    #("TSHA" | "TPRO" | "TPHS" | "TSEQ") and other unsupported types
    padding: str = None
    
    def to_binary(self):
        writer = WriteStream()
        writer.write_u32(0) # size placeholder
        writer.write(self.signature)
        match ResTypeSection.signature_to_enum(self.signature):
            case ResTypeSectionSignature.TPTR:
                if isinstance(self.pad, bytes):
                    writer.write(self.pad)
                elif isinstance(self.pad, int):
                    writer.write(self.pad * b"\x00")
                else:
                    raise ValueError(f"Invalid pad type: {type(self.pad)}")
            case ResTypeSectionSignature.TST1:
                writer.write_list_of_strings(self.strings)
            case ResTypeSectionSignature.TNA1:
                writer.write(self.type_count.to_binary())
                for _type in self.types:
                    writer.write(_type.to_binary())
            case ResTypeSectionSignature.TBDY:
                if self.type_bodies:
                    for type_body in self.type_bodies:
                        writer.write(type_body.to_binary())
            case ResTypeSectionSignature.THSH:
                writer.write(self.hash_count.to_binary())
                for hash in self.hashes:
                    writer.write(hash.to_binary())
            case ResTypeSectionSignature.UNSUPPORTED:
                raise ValueError(f"Unsupported ResTypeSection signature: {self.signature}")
            case _:
                raise ValueError(f"Invalid ResTypeSection signature: {self.signature}")
        if self.padding is not None:
            writer.write(bytes.fromhex(self.padding))
        new_buf_size = writer.tell()
        writer.seek(0, io.SEEK_SET)
        new_size = Size(is_chunk=self.size.is_chunk, size=new_buf_size)
        writer.write(new_size.to_binary())
        return writer.getvalue()
    
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
        
    
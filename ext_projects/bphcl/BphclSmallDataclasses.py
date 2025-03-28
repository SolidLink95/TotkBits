from dataclasses import dataclass
import io
import struct

from Stream import ReadStream


@dataclass
class Size():
    is_chunk: int       # 2-bit
    size: int           # 30-bit
    _endian: str = "big"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "Size":
        data = stream.read(4)
        if len(data) != 4:
            raise ValueError(f"End of file, expected 4 bytes, got {len(data)} bytes")
        is_chunk = data[0] >> 6
        size = int.from_bytes(data[0:4], byteorder=Size._endian)
        size &= 0x3FFFFFFF
        return Size(is_chunk=is_chunk, size=size)


@dataclass
class ResTagfileSectionHeader():
    size: Size
    signature: bytes

    def __len__(self):
        return 8
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTagfileSectionHeader":
        size = Size.from_reader(stream)
        signature = stream.read(4)
        if len(signature) != 4:
            raise ValueError(f"End of file, expected 4 bytes, got {len(signature)} bytes")
        return ResTagfileSectionHeader(size=size, signature=signature)
    
@staticmethod
def from_reader(stream: ReadStream) -> "ResTagfileSectionHeader":
    size = Size.from_reader(stream)
    signature = stream.read(4)
    return ResTagfileSectionHeader(size=size, signature=signature)

@dataclass
class VarUInt():
    byte0: int #u8
    _bytes: bytes = None
    _size: int = None
    _value: int = None

    def __repr__(self):
        return f"VarUInt({self._value})"
    
    @staticmethod
    def from_reader(stream: ReadStream, endian="big") -> "VarUInt":
        byte0 = stream.read(1)[0]
        _bytes, _size, _value, x  = None, None, None, None
        if byte0 & 0x80 != 0:
            x = byte0 >> 3
            if 0x10 <= x <= 0x17:
                _size = 1
            elif 0x18 <= x <= 0x1B:
                _size = 2
            elif x == 0x1C:
                _size = 3
            elif x == 0x1D:
                _size = 4
            elif x == 0x1E:
                _size = 7
            elif x == 0x1F:
                _size = 13
        if _size is not None:
            _bytes = stream.read(_size)
            if len(_bytes) != _size:
                raise ValueError(f"End of file, expected {_size} bytes, got {len(_bytes)} bytes")
        if x is not None and _bytes is not None:
            tmp = bytearray([byte0 & 0x7F])
            tmp.extend(_bytes)
            _value = int.from_bytes(tmp, byteorder=endian)
        else:
            _value = byte0
        return VarUInt(byte0=byte0, _bytes=_bytes, _size=_size, _value=_value)

@dataclass 
class ResTypeTemplate():
    index: VarUInt
    value: VarUInt
    name: str = "" # to be filled in later

    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeTemplate":
        index = VarUInt.from_reader(stream)
        value = VarUInt.from_reader(stream)
        return ResTypeTemplate(index=index, value=value)
        
    
@dataclass
class ResTypeHash():
    type_index: VarUInt
    hash: int # u32
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeHash":
        type_index = VarUInt.from_reader(stream)
        hash = struct.unpack("<I", stream.read(4))[0]
        return ResTypeHash(type_index=type_index, hash=hash)
    
    
@dataclass
class ResItem:
    flags: int # u32
    type_index: int # u32
    data_offset: int # u32
    count: int # u32
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResItem":
        flags = stream.read_u32()
        type_index = stream.read_u32()
        data_offset = stream.read_u32()
        count = stream.read_u32()
        return ResItem(flags=flags, type_index=type_index, data_offset=data_offset, count=count)


@dataclass
class ResPatch:
    type_index: int # u32
    count: int # u32
    offsets: list[int] # list[u32]
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResPatch":
        type_index = stream.read_u32()
        count = stream.read_u32()
        offsets = [stream.read_u32() for _ in range(count)]
        return ResPatch(type_index=type_index, count=count, offsets=offsets)
    
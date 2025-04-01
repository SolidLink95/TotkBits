from dataclasses import dataclass, is_dataclass, fields
import struct
from Stream import ReadStream
from typing import Any

def dataclass_to_clean_dict(obj):
    if not is_dataclass(obj):
        return obj  # base case for recursion (non-dataclass)

    result = {}
    for f in fields(obj):
        if f.name.startswith('_'):
            continue

        value = getattr(obj, f.name)

        if value is None:
            continue

        if is_dataclass(value):
            value = dataclass_to_clean_dict(value)
            if not value:  # skip empty nested dataclasses
                continue

        result[f.name] = value

    return result

@dataclass
class Size():
    is_chunk: int       # 2-bit
    size: int           # 30-bit
    _endian: str = "big"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "Size":
        data = stream._read_exact(4)
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
        signature = stream._read_exact(4)
        if len(signature) != 4:
            raise ValueError(f"End of file, expected 4 bytes, got {len(signature)} bytes")
        return ResTagfileSectionHeader(size=size, signature=signature)
    
@staticmethod
def from_reader(stream: ReadStream) -> "ResTagfileSectionHeader":
    size = Size.from_reader(stream)
    signature = stream._read_exact(4)
    return ResTagfileSectionHeader(size=size, signature=signature)

@dataclass
class VarUInt():
    val: str # representation of the value in hex string format, for json
    _byte0: int #u8
    _bytes: bytes = None
    _size: int = None
    _value: int = None

    def __repr__(self):
        return f"VarUInt({self._value})"
    
    def __len__(self):
        return len(self.val) // 2
    
    
    @staticmethod
    def from_binary(data:bytes|bytearray) -> "VarUInt":
        stream = ReadStream(data)
        return VarUInt.from_reader(stream)
    
    @staticmethod
    def from_reader(stream: ReadStream, endian="big") -> "VarUInt":
        _offset = stream.tell()
        _byte0 = stream._read_exact(1)[0]
        _bytes, _size, _value, x  = None, None, None, None
        if _byte0 & 0x80 != 0:
            x = _byte0 >> 3
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
            _bytes = stream._read_exact(_size)
        if x is not None and _bytes is not None:
            tmp = bytearray([_byte0 & 0x7F])
            tmp.extend(_bytes)
            _value = int.from_bytes(tmp, byteorder=endian)
        else:
            _value = _byte0
        _offset2 = stream.tell()
        stream.seek(_offset)
        val = stream._read_exact(_offset2 - _offset).hex().upper()
        stream.seek(_offset2)
        return VarUInt(_byte0=_byte0, _bytes=_bytes, _size=_size, _value=_value, val=val)

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
        hash = struct.unpack("<I", stream._read_exact(4))[0]
        return ResTypeHash(type_index=type_index, hash=hash)
    
    
@dataclass
class ResItem:
    flags: int # u16
    type_index: int # u16
    data_offset: int # u32
    count: int # u32
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResItem":
        flags = stream.read_u16()
        type_index = stream.read_u16()
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
    
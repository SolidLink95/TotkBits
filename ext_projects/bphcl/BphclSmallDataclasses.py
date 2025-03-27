from dataclasses import dataclass
import io
import struct


@dataclass
class Size():
    is_chunk: int       # 2-bit
    size: int           # 30-bit
    _endian: str = "little"
    
    @staticmethod
    def from_reader(stream: io.BytesIO) -> "Size":
        data = stream.read(4)
        if len(data) != 4:
            raise ValueError(f"End of file, expected 4 bytes, got {len(data)} bytes")
        is_chunk = data[0] & 0b11
        size = int.from_bytes(data[0:4], byteorder=Size._endian)
        size &= 0x3FFFFFFF
        return Size(is_chunk=is_chunk, size=size)


@dataclass
class ResTagfileSectionHeader():
    size: Size
    signature: bytes
    _endian: str = Size._endian
    
@staticmethod
def from_reader(stream: io.BytesIO) -> "ResTagfileSectionHeader":
    size = Size.from_reader(stream)
    signature = stream.read(4)
    return ResTagfileSectionHeader(size=size, signature=signature)

@dataclass
class VarUInt():
    byte0: int #u8
    _bytes: bytes = None
    _size: int = None
    _value: int = None

    @staticmethod
    def from_reader(stream: io.BytesIO, endian="little") -> "VarUInt":
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
            tmp = bytearray([x])
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
    def from_reader(stream: io.BytesIO) -> "ResTypeTemplate":
        index = VarUInt.from_reader(stream)
        value = VarUInt.from_reader(stream)
        return ResTypeTemplate(index=index, value=value)
        
    

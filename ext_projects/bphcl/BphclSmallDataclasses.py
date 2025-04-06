from dataclasses import dataclass, is_dataclass, fields
import io
import struct
from Havok import T, Ptr
from Stream import ReadStream, WriteStream
from typing import Any, List, Optional, Type

from util import _hex


@dataclass
class Size():
    is_chunk: int       # 2-bit
    size: int           # 30-bit
    _endian: str = "big"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "Size":
        data = stream._read_exact(4)
        is_chunk = data[0] >> 6
        size = int.from_bytes(data[0:4], byteorder=Size._endian)
        size &= 0x3FFFFFFF
        return Size(is_chunk=is_chunk, size=size)

    def to_binary(self) -> bytes:
        if not (0 <= self.is_chunk < 4):
            raise ValueError("is_chunk must be a 2-bit value (0–3)")
        if not (0 <= self.size < (1 << 30)):
            raise ValueError("size must be a 30-bit value")

        value = (self.is_chunk << 30) | self.size
        return value.to_bytes(4, byteorder=self._endian)
        

@dataclass
class ResTagfileSectionHeader():
    size: Size
    signature: bytes

    def __len__(self):
        return 8
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "ResTagfileSectionHeader":
        size = Size.from_reader(stream)
        signature = stream._read_exact(4)
        if len(signature) != 4:
            raise ValueError(f"End of file, expected 4 bytes, got {len(signature)} bytes")
        return cls(size=size, signature=signature)
    
    def to_binary(self):
        return self.size.to_binary() + self.signature
    
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
    
    def to_binary(self) -> bytes:
        return bytes.fromhex(self.val)
    
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

    def to_binary(self) -> bytes:
        return self.index.to_binary() + self.value.to_binary()
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTypeTemplate":
        index = VarUInt.from_reader(stream)
        value = VarUInt.from_reader(stream)
        return ResTypeTemplate(index=index, value=value)
        
    
@dataclass
class ResTypeHash():
    type_index: VarUInt
    hash: int # u32
    
    def to_binary(self) -> bytes:
        writer = WriteStream()
        writer.write(self.type_index.to_binary())
        writer.write_u32(self.hash)
        return writer.getvalue()
    
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
    
    def to_binary(self) -> bytes:
        writer = WriteStream()
        writer.write_u16(self.flags)
        writer.write_u16(self.type_index)
        writer.write_u32(self.data_offset)
        writer.write_u32(self.count)
        return writer.getvalue()
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResItem":
        # hexpat way
        # flags = stream.read_u32()
        # type_index = flags & 0xffffff
        # Starlight way
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
    
    def to_binary(self) -> bytes:
        writer = WriteStream()
        writer.write_u32(self.type_index)
        writer.write_u32(self.count)
        for offset in self.offsets:
            writer.write_u32(offset)
        return writer.getvalue()
    
    def __repr__(self):
        c = 4
        o = f"{str(self.offsets[:c])[:-1]}...]" if self.count >= c else f"{str(self.offsets[:c])}"
        return f"ResPatch(type_index={self.type_index}, count={self.count}, offsets={o})"
    
    def __len__(self):
        return 8 + len(self.offsets) * 4
    
    @staticmethod
    def minimal_size():
        return 8 #
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "ResPatch":
        type_index = stream.read_u32()
        count = stream.read_u32()
        offsets = [stream.read_u32() for _ in range(count)]
        return ResPatch(type_index=type_index, count=count, offsets=offsets)
    

#CLOTH


@dataclass
class hkObjectBase:
    _vft_reserve: int # u64

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkObjectBase":
        _vft_reserve = stream.read_u64()
        return hkObjectBase(_vft_reserve=_vft_reserve)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u64(self._vft_reserve)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())


@dataclass
class hkReferencedObject(hkObjectBase):
    m_sizeAndFlags: int # u64
    m_refCount: int # u64

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkReferencedObject":
        stream.align_to(0x8)
        base = super().from_reader(stream)
        m_sizeAndFlags = stream.read_u64()
        m_refCount = stream.read_u64()
        return hkReferencedObject(**vars(base), m_sizeAndFlags=m_sizeAndFlags, m_refCount=m_refCount)

    def to_binary(self) -> bytes:
        stream = WriteStream(super().to_binary())
        stream.write_u64(self.m_sizeAndFlags)
        stream.write_u64(self.m_refCount)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())


@dataclass
class hclShape(hkReferencedObject):
    m_type: int # s32
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclShape":
        par = super().from_reader(stream)
        m_type = stream.read_s32()
        return hclShape(par._vft_reserve, par.m_sizeAndFlags, par.m_refCount, m_type)
    
    def to_binary(self):
        writer = WriteStream(super().to_binary())
        writer.write_s32(self.m_type)
        return writer.getvalue()


@dataclass
class hclAction(hkReferencedObject):
    m_active: bool
    m_registeredWithWorldStepping: bool
    
    @staticmethod   
    def from_reader(stream: ReadStream) -> "hclAction":
        par = super().from_reader(stream)
        m_active = stream.read_bool()
        m_registeredWithWorldStepping = stream.read_bool()
        return hclAction(par._vft_reserve, par.m_sizeAndFlags, par.m_refCount, m_active, m_registeredWithWorldStepping)

    def to_binary(self):
        writer = WriteStream(super().to_binary())
        writer.write_bool(self.m_active)
        writer.write_bool(self.m_registeredWithWorldStepping)
        return writer.getvalue()
    


@dataclass
class hclVirtualCollisionPointsData__TriangleFanSection:
    m_oppositeRealParticleIndices: List[int] # list[u16] len=2
    m_barycentricDictionaryIndex: int # u16
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclVirtualCollisionPointsData__TriangleFanSection":
        stream.align_to(0x2)
        m_oppositeRealParticleIndices = [stream.read_u16() for _ in range(2)]
        m_barycentricDictionaryIndex = stream.read_u16()
        return hclVirtualCollisionPointsData__TriangleFanSection(m_oppositeRealParticleIndices, m_barycentricDictionaryIndex)
  

@dataclass
class hkRefPtr:
    _m_data_type: Optional[Type] # Type of the element in the list
    _offset: int #u64
    m_ptr: Ptr
    m_data: Optional[Type] = None
    
    def __repr__(self):
        # if self._m_data_type == hkStringPtr:
        return f"hkRefPtr({(self.m_data)})"
    
    @staticmethod
    def from_reader(stream: ReadStream, _m_data_type: Type[T]) -> 'hkRefPtr':
        """Reads a reference pointer from the stream."""
        _offset = stream.tell()
        stream.align_to(8)
        m_ptr = Ptr.from_reader(stream)
        _new_offset = _offset + m_ptr.value #+ stream.data_offset
        cur_offset = stream.tell()
        stream.seek(_new_offset, io.SEEK_SET)
        m_data = _m_data_type.from_reader(stream) 
        stream.seek(cur_offset, io.SEEK_SET)
        return hkRefPtr(_m_data_type=_m_data_type, _offset=_offset, m_ptr=m_ptr, m_data=m_data)
    
    def to_binary(self) -> bytes:
        """Converts the reference pointer to a binary representation."""
        return self.m_ptr.to_binary()
    
    def write(self, stream: WriteStream):
        """Writes the reference pointer to the stream."""
        stream.write(self.to_binary())

    def __repr__(self):
        return f"hkRefPtr({self._m_data_type.__name__}, 0x{self.m_ptr.value:08X}, 0x{self._offset:08X})"
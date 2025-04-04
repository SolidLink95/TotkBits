from dataclasses import dataclass, is_dataclass, fields
import io
import struct
from Havok import T, Ptr
from Stream import ReadStream, WriteStream
from typing import Any, List, Optional, Type

from util import _hex

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
    _vft_reserve: int #u64
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkObjectBase':
        """Reads an hkObjectBase from the stream."""
        _vft_reserve = stream.read_u64()
        return hkObjectBase(_vft_reserve)
    

@dataclass
class hkReferencedObject(hkObjectBase):
    m_sizeAndFlags: int #u64
    m_refCount: int #u64
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkReferencedObject':
        """Reads an hkReferencedObject from the stream."""
        stream.align_to(8)
        par = super().from_reader(stream)
        m_sizeAndFlags = stream.read_u64()
        m_refCount = stream.read_u64()
        return hkReferencedObject(par._vft_reserve, m_sizeAndFlags, m_refCount)


@dataclass
class hkVector4f:
    m_x: float
    m_y: float
    m_z: float
    m_w: float
    
    def __repr__(self):
        return f"hkVector4f({self.m_x}, {self.m_y}, {self.m_z}, {self.m_w})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hkVector4f":
        m_x = stream.read_float()
        m_y = stream.read_float()
        m_z = stream.read_float()
        m_w = stream.read_float()
        return hkVector4f(m_x, m_y, m_z, m_w)
    
    def to_binary(self) -> bytes:
        return struct.pack("<ffff", self.m_x, self.m_y, self.m_z, self.m_w)
    
    def write(self, stream: WriteStream):
        """Writes the vector to the stream."""
        stream.write(self.to_binary())


@dataclass
class UIntBase:
    val: int
    _size: int = 4  # default, override in subclasses
    _sign: str = ""

    def __repr__(self):
        return f"{self.__class__.__name__}({_hex(self.val)})"
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "UIntBase":
        val = stream._read_and_unpack(cls._sign, cls._size)[0]
        return cls(val=val)
    
    def to_binary(self) -> bytes:
        return struct.pack(self._sign, self.val)
    
    def write(self, stream: WriteStream):
        """Writes the integer to the stream."""
        stream.write(self.to_binary())


@dataclass
class u32(UIntBase):
    _size: int = 4
    _sign: str = "I"


@dataclass
class u16(UIntBase):
    _size: int = 2
    _sign: str = "H"


@dataclass
class u8(UIntBase):
    _size: int = 1
    _sign: str = "B"
    
    
@dataclass
class BOOL:
    val: bool

    def __repr__(self):
        v = "true" if self.val else "false"
        return f"{self.__class__.__name__}({v})"
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "BOOL":
        val = stream.read_bool()
        return cls(val=bool(val))
    
    
@dataclass
class FLOAT:
    val: float
    
    def __repr__(self):
        return f"{self.__class__.__name__}({self.val})"

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "FLOAT":
        val = stream.read_float()
        return cls(val=val)



@dataclass
class s16(UIntBase):
    _size: int = 2
    _sign: str = "h"

@dataclass
class s32(UIntBase):
    _size: int = 4
    _sign: str = "i"

@dataclass
class s64(UIntBase):
    _size: int = 8
    _sign: str = "q"


@dataclass
class hkMatrix4f:
    m_col0: hkVector4f
    m_col1: hkVector4f
    m_col2: hkVector4f
    m_col3: hkVector4f
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hkMatrix4f":
        stream.align_to(0x10)
        m_col0 = hkVector4f.from_reader(stream)
        m_col1 = hkVector4f.from_reader(stream)
        m_col2 = hkVector4f.from_reader(stream)
        m_col3 = hkVector4f.from_reader(stream)
        return hkMatrix4f(m_col0, m_col1, m_col2, m_col3)


@dataclass
class hkQuaternionf:
    m_vec: hkVector4f
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hkQuaternionf":
        stream.align_to(0x10)
        m_vec = hkVector4f.from_reader(stream)
        return hkQuaternionf(m_vec)


@dataclass
class hkRotationf:
    m_col0: hkVector4f
    m_col1: hkVector4f
    m_col2: hkVector4f
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hkRotationf":
        stream.align_to(0x10)
        m_col0 = hkVector4f.from_reader(stream)
        m_col1 = hkVector4f.from_reader(stream)
        m_col2 = hkVector4f.from_reader(stream)
        return hkRotationf(m_col0, m_col1, m_col2)


@dataclass
class hkTransform:
    m_rotation: hkRotationf
    m_translation: hkVector4f
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hkTransform":
        m_rotation = hkRotationf.from_reader(stream)
        m_translation = hkVector4f.from_reader(stream)
        return hkTransform(m_rotation, m_translation)


@dataclass
class hkQsTransformf:
    m_translation: hkVector4f
    m_rotation: hkQuaternionf
    m_scale: hkVector4f
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hkQsTransformf":
        stream.align_to(0x10)
        m_translation = hkVector4f.from_reader(stream)
        m_rotation = hkQuaternionf.from_reader(stream)
        m_scale = hkVector4f.from_reader(stream)
        return hkQsTransformf(m_translation, m_rotation, m_scale)


@dataclass
class hclShape(hkReferencedObject):
    m_type: int # s32
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclShape":
        par = super().from_reader(stream)
        m_type = stream.read_s32()
        return hclShape(par._vft_reserve, par.m_sizeAndFlags, par.m_refCount, m_type)


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


@dataclass
class hclSimClothData__CollidablePinchingData:
    m_pinchDetectionEnabled: bool
    m_pinchDetectionPriority: int # u8
    m_pinchDetectionRadius: float
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclSimClothData__CollidablePinchingData":
        stream.align_to(0x4)
        m_pinchDetectionEnabled = stream.read_bool()
        m_pinchDetectionPriority = stream.read_u8()
        stream.align_to(0x4)
        m_pinchDetectionRadius = stream.read_float()
        return hclSimClothData__CollidablePinchingData(m_pinchDetectionEnabled, m_pinchDetectionPriority, m_pinchDetectionRadius)
    

@dataclass
class hclVirtualCollisionPointsData__Block:
    m_safeDisplacementRadius: float
    m_startingVCPIndex: int # u16
    m_numVCPs: int # u8
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclVirtualCollisionPointsData__Block":
        stream.align_to(0x4)
        m_safeDisplacementRadius = stream.read_float()
        m_startingVCPIndex = stream.read_u16()
        m_numVCPs = stream.read_u8()
        return hclVirtualCollisionPointsData__Block(m_safeDisplacementRadius, m_startingVCPIndex, m_numVCPs)


@dataclass
class hclVirtualCollisionPointsData__BarycentricDictionaryEntry:
    m_startingBarycentricIndex: int # u16
    m_numBarycentrics: int # u8
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclVirtualCollisionPointsData__BarycentricDictionaryEntry":
        stream.align_to(0x2)
        m_startingBarycentricIndex = stream.read_u16()
        m_numBarycentrics = stream.read_u8()
        return hclVirtualCollisionPointsData__BarycentricDictionaryEntry(m_startingBarycentricIndex, m_numBarycentrics)


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
class hclVirtualCollisionPointsData__TriangleFan:
    m_realParticleIndex: int # u16
    m_vcpStartIndex: int # u16
    m_numTriangles: int # u8
    
    @staticmethod
    def from_reader(stream: ReadStream) -> "hclVirtualCollisionPointsData__TriangleFan":
        stream.align_to(0x2)
        m_realParticleIndex = stream.read_u16()
        m_vcpStartIndex = stream.read_u16()
        m_numTriangles = stream.read_u8()
        return hclVirtualCollisionPointsData__TriangleFan(m_realParticleIndex, m_vcpStartIndex, m_numTriangles)


@dataclass
class hclVirtualCollisionPointsData__TriangleFanLandscape:
    m_realParticleIndex: int #u16
    m_triangleStartIndex: int #u16
    m_vcpStartIndex: int #u16
    


@dataclass
class hkRefPtr:
    _m_data_type: Optional[Type] # Type of the element in the list
    _offset: int #u64
    m_ptr: Ptr
    m_data: Optional[Type] = None
    
    @staticmethod
    def from_reader(stream: ReadStream, _m_data_type: Type[T]) -> 'hkRefPtr':
        """Reads a reference pointer from the stream."""
        _offset = stream.tell()
        stream.align_to(8)
        m_ptr = Ptr.from_reader(stream)
        _new_offset = _offset + m_ptr.value
        cur_offset = stream.tell()
        stream.seek(_new_offset, io.SEEK_SET)
        m_data = _m_data_type.from_reader(stream) 
        stream.seek(cur_offset, io.SEEK_SET)
        return hkRefPtr(_m_data_type=_m_data_type, _offset=_offset, m_ptr=m_ptr, m_data=m_data)
    #TODO: check if i have to return to cur_offse
    
    def to_binary(self) -> bytes:
        """Converts the reference pointer to a binary representation."""
        return self.m_ptr.to_binary()
    
    def write(self, stream: WriteStream):
        """Writes the reference pointer to the stream."""
        stream.write(self.to_binary())

    def __repr__(self):
        return f"hkRefPtr({self._m_data_type.__name__}, 0x{self.m_ptr.value:08X}, 0x{self._offset:08X})"
import io
from Stream import ReadStream, WriteStream
from dataclasses import dataclass, field
from BphclEnums import *
from BphclSmallDataclasses import hkRefPtr,  hclVirtualCollisionPointsData__TriangleFanSection, hkReferencedObject 
from Havok import hkArray,hkStringPtr,hkRefVariant
from util import _hex, hexInt
from Numerics import *


@dataclass
class hclSimClothData__OverridableSimulationInfo:
    m_gravity: "hkVector4f"
    m_globalDampingPerSecond: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__OverridableSimulationInfo":
        stream.align_to(0x10)
        m_gravity = hkVector4f.from_reader(stream)
        m_globalDampingPerSecond = stream.read_float()
        stream._read_exact(0xc) #padding
        return hclSimClothData__OverridableSimulationInfo(m_gravity=m_gravity, m_globalDampingPerSecond=m_globalDampingPerSecond)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_gravity.to_binary())
        stream.write_float(self.m_globalDampingPerSecond)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())

    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        stream.write_float(self.m_gravity)
        stream.write_float(self.m_globalDampingPerSecond)
        stream.write(0xc * b"\x00")
        return






    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        stream.write_float(self.m_gravity)
        stream.write_float(self.m_globalDampingPerSecond)
        stream.write(0xc * b"\x00")
        return

@dataclass
class hclSimClothData__OverridableSimulationInfo:
    m_gravity: "hkVector4f"
    m_globalDampingPerSecond: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__OverridableSimulationInfo":
        stream.align_to(0x10)
        m_gravity = hkVector4f.from_reader(stream)
        m_globalDampingPerSecond = stream.read_float()
        stream._read_exact(0xc) #padding
        return hclSimClothData__OverridableSimulationInfo(m_gravity=m_gravity, m_globalDampingPerSecond=m_globalDampingPerSecond)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_gravity.to_binary())
        stream.write_float(self.m_globalDampingPerSecond)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())

    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        self.m_gravity.to_stream(stream)
        stream.write_float(self.m_globalDampingPerSecond)
        stream.write(0xC) # padding
        return


@dataclass
class Vector2f: # %SIMPLE%
    x: int # float
    y: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "Vector2f":
        x = stream.read_float()
        y = stream.read_float()
        return Vector2f(x=x, y=y)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.x)
        stream.write_float(self.y)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.x)
        stream.write_float(self.y)
        return


@dataclass
class Vector2f: # %SIMPLE%
    x: int # float
    y: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "Vector2f":
        x = stream.read_float()
        y = stream.read_float()
        return Vector2f(x=x, y=y)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.x)
        stream.write_float(self.y)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.x)
        stream.write_float(self.y)
        return


@dataclass
class Vector2f: # %SIMPLE%
    x: int # float
    y: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "Vector2f":
        x = stream.read_float()
        y = stream.read_float()
        return Vector2f(x=x, y=y)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.x)
        stream.write_float(self.y)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.x)
        stream.write_float(self.y)
        return


@dataclass
class hclSimClothData__OverridableSimulationInfo:
    m_gravity: "hkVector4f"
    m_globalDampingPerSecond: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__OverridableSimulationInfo":
        stream.align_to(0x10)
        m_gravity = hkVector4f.from_reader(stream)
        m_globalDampingPerSecond = stream.read_float()
        stream._read_exact(0xc) #padding
        return hclSimClothData__OverridableSimulationInfo(m_gravity=m_gravity, m_globalDampingPerSecond=m_globalDampingPerSecond)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_gravity.to_binary())
        stream.write_float(self.m_globalDampingPerSecond)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())

    
    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        self.m_gravity.to_stream(stream)
        stream.write_float(self.m_globalDampingPerSecond)
        stream.write(0xC) # padding
        return


@dataclass
class Quatf:
    a: int # float
    b: int # float
    c: int # float
    d: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "Quatf":
        a = stream.read_float()
        b = stream.read_float()
        c = stream.read_float()
        d = stream.read_float()
        return Quatf(a=a, b=b, c=c, d=d)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.a)
        stream.write_float(self.b)
        stream.write_float(self.c)
        stream.write_float(self.d)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.a)
        stream.write_float(self.b)
        stream.write_float(self.c)
        stream.write_float(self.d)
        return


@dataclass
class hkBitFieldStorage:
    m_words: hkArray # hkArray[u32]
    m_numBits: int # s32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkBitFieldStorage":
        m_words = hkArray.from_reader(stream, u32)
        m_numBits = stream.read_s32()
        return hkBitFieldStorage(m_words=m_words, m_numBits=m_numBits)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_words.to_binary())
        stream.write_s32(self.m_numBits)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_words.to_stream(stream)
        stream.write_s32(self.m_numBits)
        return


@dataclass
class hkBitField:
    m_storage: hkBitFieldStorage

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkBitField":
        m_storage = hkBitFieldStorage.from_reader(stream)
        return hkBitField(m_storage=m_storage)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_storage.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_storage.to_stream(stream)
        return


@dataclass
class hkRootLevelContainer__NamedVariant:
    m_name: hkStringPtr
    m_className: hkStringPtr
    m_variant: hkRefVariant

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkRootLevelContainer__NamedVariant":
        stream.align_to(0x8)
        m_name = hkStringPtr.from_reader(stream)
        m_className = hkStringPtr.from_reader(stream)
        m_variant = hkRefVariant.from_reader(stream)
        return hkRootLevelContainer__NamedVariant(m_name=m_name, m_className=m_className, m_variant=m_variant)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_name.to_binary())
        stream.write(self.m_className.to_binary())
        stream.write(self.m_variant.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x8)
        self.m_name.to_stream(stream)
        self.m_className.to_stream(stream)
        self.m_variant.to_stream(stream)
        return


@dataclass
class hkRootLevelContainer:
    m_namedVariants: hkArray # hkArray[hkRootLevelContainer__NamedVariant]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkRootLevelContainer":
        stream.align_to(0x8)
        m_namedVariants = hkArray.from_reader(stream, hkRootLevelContainer__NamedVariant)
        return hkRootLevelContainer(m_namedVariants=m_namedVariants)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_namedVariants.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())





    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x8)
        self.m_namedVariants.to_stream(stream)
        return


@dataclass
class hkVector4f:
    m_x: int # float
    m_y: int # float
    m_z: int # float
    m_w: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkVector4f":
        m_x = stream.read_float()
        m_y = stream.read_float()
        m_z = stream.read_float()
        m_w = stream.read_float()
        return hkVector4f(m_x=m_x, m_y=m_y, m_z=m_z, m_w=m_w)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.m_x)
        stream.write_float(self.m_y)
        stream.write_float(self.m_z)
        stream.write_float(self.m_w)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.m_x)
        stream.write_float(self.m_y)
        stream.write_float(self.m_z)
        stream.write_float(self.m_w)
        return


@dataclass
class hkQuaternionf:
    m_vec: hkVector4f

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkQuaternionf":
        stream.align_to(0x10)
        m_vec = hkVector4f.from_reader(stream)
        return hkQuaternionf(m_vec=m_vec)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_vec.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        self.m_vec.to_stream(stream)
        return


@dataclass
class hkRotationf:
    m_col0: hkVector4f
    m_col1: hkVector4f
    m_col2: hkVector4f
    
    _alignment = 0x10

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkRotationf":
        stream.align_to(hkRotationf._alignment)
        m_col0 = hkVector4f.from_reader(stream)
        m_col1 = hkVector4f.from_reader(stream)
        m_col2 = hkVector4f.from_reader(stream)
        return hkRotationf(m_col0=m_col0, m_col1=m_col1, m_col2=m_col2)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_col0.to_binary())
        stream.write(self.m_col1.to_binary())
        stream.write(self.m_col2.to_binary())
        return stream.getvalue()

    
    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(hkRotationf._alignment)
        self.m_col0.to_stream(stream)
        self.m_col1.to_stream(stream)
        self.m_col2.to_stream(stream)
        return


@dataclass
class hkMatrix4f:
    m_col0: hkVector4f
    m_col1: hkVector4f
    m_col2: hkVector4f
    m_col3: hkVector4f

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkMatrix4f":
        stream.align_to(0x10)
        m_col0 = hkVector4f.from_reader(stream)
        m_col1 = hkVector4f.from_reader(stream)
        m_col2 = hkVector4f.from_reader(stream)
        m_col3 = hkVector4f.from_reader(stream)
        return hkMatrix4f(m_col0=m_col0, m_col1=m_col1, m_col2=m_col2, m_col3=m_col3)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_col0.to_binary())
        stream.write(self.m_col1.to_binary())
        stream.write(self.m_col2.to_binary())
        stream.write(self.m_col3.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        self.m_col0.to_stream(stream)
        self.m_col1.to_stream(stream)
        self.m_col2.to_stream(stream)
        self.m_col3.to_stream(stream)
        return


@dataclass
class hkTransform:
    m_rotation: hkRotationf
    m_translation: hkVector4f

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkTransform":
        m_rotation = hkRotationf.from_reader(stream)
        m_translation = hkVector4f.from_reader(stream)
        return hkTransform(m_rotation=m_rotation, m_translation=m_translation)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_rotation.to_binary())
        stream.write(self.m_translation.to_binary())
        return stream.getvalue()

    
    
    def to_stream(self, stream: WriteStream):
        self.m_rotation.to_stream(stream)
        self.m_translation.to_stream(stream)
        return


@dataclass
class hkQsTransformf:
    m_translation: hkVector4f
    m_rotation: hkQuaternionf
    m_scale: hkVector4f

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkQsTransformf":
        stream.align_to(0x10)
        m_translation = hkVector4f.from_reader(stream)
        m_rotation = hkQuaternionf.from_reader(stream)
        m_scale = hkVector4f.from_reader(stream)
        return hkQsTransformf(m_translation=m_translation, m_rotation=m_rotation, m_scale=m_scale)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_translation.to_binary())
        stream.write(self.m_rotation.to_binary())
        stream.write(self.m_scale.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x10)
        self.m_translation.to_stream(stream)
        self.m_rotation.to_stream(stream)
        self.m_scale.to_stream(stream)
        return


@dataclass
class hclShape(hkReferencedObject):
    m_type: int # s32

    def __eq__(self, value):
        return isinstance(value, hclShape) and self.m_type == value.m_type
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclShape":
        base = super().from_reader(stream)
        m_type = stream.read_s32()
        return hclShape(**vars(base), m_type=m_type)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write_s32(self.m_type)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        stream.write_s32(self.m_type)
        return


@dataclass
class hclCollidable(hkReferencedObject):
    m_transform: hkTransform
    m_linearVelocity: hkVector4f
    m_angularVelocity: hkVector4f
    m_userData: int # u64
    m_shape: hkRefPtr # hkRefPtr[hclShape]
    m_name: hkStringPtr
    m_pinchDetectionRadius: int # float
    m_pinchDetectionPriority: int # u8
    m_pinchDetectionEnabled: bool
    m_virtualCollisionPointCollisionEnabled: bool
    m_enabled: bool
    
    _offsets_range: list[tuple[hexInt, hexInt]] = field(default_factory=list)

    # def __repr__(self):
    #     return f"hclCollidable({repr(self.m_name._str)})"
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclCollidable":
        _offsets_range = []
        _offset = stream.tell()
        base = super().from_reader(stream)
        m_transform = hkTransform.from_reader(stream)
        m_linearVelocity = hkVector4f.from_reader(stream)
        m_angularVelocity = hkVector4f.from_reader(stream)
        m_userData = hexInt(stream.read_u64())
        m_shape = hkRefPtr.from_reader(stream, hclShape)
        m_name = hkStringPtr.from_reader(stream)
        m_pinchDetectionRadius = stream.read_float()
        m_pinchDetectionPriority = stream.read_u8()
        m_pinchDetectionEnabled = stream.read_bool()
        m_virtualCollisionPointCollisionEnabled = stream.read_bool()
        m_enabled = stream.read_bool()
        _offsets_range.extend(m_shape._offsets_range)
        _offsets_range.extend(m_name._offsets_range)
        _offsets_range.insert(0, (_offset, stream.tell()))
        return hclCollidable(**vars(base), m_transform=m_transform, m_linearVelocity=m_linearVelocity, m_angularVelocity=m_angularVelocity, m_userData=m_userData, m_shape=m_shape, m_name=m_name, m_pinchDetectionRadius=m_pinchDetectionRadius, m_pinchDetectionPriority=m_pinchDetectionPriority, m_pinchDetectionEnabled=m_pinchDetectionEnabled, m_virtualCollisionPointCollisionEnabled=m_virtualCollisionPointCollisionEnabled, m_enabled=m_enabled)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_transform.to_binary())
        stream.write(self.m_linearVelocity.to_binary())
        stream.write(self.m_angularVelocity.to_binary())
        stream.write_u64(self.m_userData)
        stream.write(self.m_shape.to_binary())
        stream.write(self.m_name.to_binary())
        stream.write_float(self.m_pinchDetectionRadius)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_bool(self.m_virtualCollisionPointCollisionEnabled)
        stream.write_bool(self.m_enabled)
        return stream.getvalue()

    
    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self._offsets_range.to_stream(stream)
        self._offset.to_stream(stream)
        self.m_transform.to_stream(stream)
        self.m_linearVelocity.to_stream(stream)
        self.m_angularVelocity.to_stream(stream)
        stream.write_u64(self.m_userData)
        self.m_shape.to_stream(stream)
        self.m_name.to_stream(stream)
        stream.write_float(self.m_pinchDetectionRadius)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_bool(self.m_virtualCollisionPointCollisionEnabled)
        stream.write_bool(self.m_enabled)
        return


@dataclass
class hclCollidable(hkReferencedObject):
    m_transform: hkTransform
    m_linearVelocity: hkVector4f
    m_angularVelocity: hkVector4f
    m_userData: int # u64
    m_shape: hkRefPtr # hkRefPtr[hclShape]
    m_name: hkStringPtr
    m_pinchDetectionRadius: int # float
    m_pinchDetectionPriority: int # u8
    m_pinchDetectionEnabled: bool
    m_virtualCollisionPointCollisionEnabled: bool
    m_enabled: bool
    
    _offsets_range: list[tuple[hexInt, hexInt]] = field(default_factory=list)

    # def __repr__(self):
    #     return f"hclCollidable({repr(self.m_name._str)})"
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclCollidable":
        _offsets_range = []
        _offset = stream.tell()
        base = super().from_reader(stream)
        m_transform = hkTransform.from_reader(stream)
        m_linearVelocity = hkVector4f.from_reader(stream)
        m_angularVelocity = hkVector4f.from_reader(stream)
        m_userData = hexInt(stream.read_u64())
        m_shape = hkRefPtr.from_reader(stream, hclShape)
        m_name = hkStringPtr.from_reader(stream)
        m_pinchDetectionRadius = stream.read_float()
        m_pinchDetectionPriority = stream.read_u8()
        m_pinchDetectionEnabled = stream.read_bool()
        m_virtualCollisionPointCollisionEnabled = stream.read_bool()
        m_enabled = stream.read_bool()
        _offsets_range.extend(m_shape._offsets_range)
        _offsets_range.extend(m_name._offsets_range)
        _offsets_range.insert(0, (_offset, stream.tell()))
        return hclCollidable(**vars(base), m_transform=m_transform, m_linearVelocity=m_linearVelocity, m_angularVelocity=m_angularVelocity, m_userData=m_userData, m_shape=m_shape, m_name=m_name, m_pinchDetectionRadius=m_pinchDetectionRadius, m_pinchDetectionPriority=m_pinchDetectionPriority, m_pinchDetectionEnabled=m_pinchDetectionEnabled, m_virtualCollisionPointCollisionEnabled=m_virtualCollisionPointCollisionEnabled, m_enabled=m_enabled)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_transform.to_binary())
        stream.write(self.m_linearVelocity.to_binary())
        stream.write(self.m_angularVelocity.to_binary())
        stream.write_u64(self.m_userData)
        stream.write(self.m_shape.to_binary())
        stream.write(self.m_name.to_binary())
        stream.write_float(self.m_pinchDetectionRadius)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_bool(self.m_virtualCollisionPointCollisionEnabled)
        stream.write_bool(self.m_enabled)
        return stream.getvalue()

    
    
    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self._offsets_range.to_stream(stream)
        self._offset.to_stream(stream)
        self.m_transform.to_stream(stream)
        self.m_linearVelocity.to_stream(stream)
        self.m_angularVelocity.to_stream(stream)
        stream.write_u64(self.m_userData)
        self.m_shape.to_stream(stream)
        self.m_name.to_stream(stream)
        stream.write_float(self.m_pinchDetectionRadius)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_bool(self.m_virtualCollisionPointCollisionEnabled)
        stream.write_bool(self.m_enabled)
        return


@dataclass
class hclSimClothData__ParticleData:
    m_mass: int # float
    m_invMass: int # float
    m_radius: int # float
    m_friction: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__ParticleData":
        m_mass = stream.read_float()
        m_invMass = stream.read_float()
        m_radius = stream.read_float()
        m_friction = stream.read_float()
        return hclSimClothData__ParticleData(m_mass=m_mass, m_invMass=m_invMass, m_radius=m_radius, m_friction=m_friction)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.m_mass)
        stream.write_float(self.m_invMass)
        stream.write_float(self.m_radius)
        stream.write_float(self.m_friction)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.m_mass)
        stream.write_float(self.m_invMass)
        stream.write_float(self.m_radius)
        stream.write_float(self.m_friction)
        return


@dataclass
class hclSimClothPose(hkReferencedObject):
    m_name: hkStringPtr
    m_positions: hkArray # hkArray[hkVector4f]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothPose":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        # m_positions = hkArray.from_reader(stream, hkVector4f)
        m_positions = hkArray.from_reader(stream, hkVector4f)
        result = hclSimClothPose(**vars(base), m_name=m_name, m_positions=m_positions)
        return result


    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_positions.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())


@dataclass
class hclConstraintSet(hkReferencedObject):
    m_name: hkStringPtr
    m_constraintId: int # u32
    m_type: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclConstraintSet":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_constraintId = stream.read_u32()
        m_type = stream.read_u32()
        return hclConstraintSet(**vars(base), m_name=m_name, m_constraintId=m_constraintId, m_type=m_type)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write_u32(self.m_constraintId)
        stream.write_u32(self.m_type)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        stream.write_u32(self.m_constraintId)
        stream.write_u32(self.m_type)
        return


@dataclass
class hclSimClothData__CollidableTransformMap:
    m_transformSetIndex: int # u32
    m_transformIndices: hkArray # hkArray[u32]
    m_offsets: hkArray # hkArray[hkMatrix4f]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__CollidableTransformMap":
        m_transformSetIndex = stream.read_u32()
        m_transformIndices = hkArray.from_reader(stream, u32)
        m_offsets = hkArray.from_reader(stream, hkMatrix4f)

        # m_transformIndices.from_reader_self(stream)
        # m_offsets.from_reader_self(stream)
        return hclSimClothData__CollidableTransformMap(m_transformSetIndex=m_transformSetIndex, m_transformIndices=m_transformIndices, m_offsets=m_offsets)
    
    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u32(self.m_transformSetIndex)
        stream.write(self.m_transformIndices.to_binary())
        stream.write(self.m_offsets.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_u32(self.m_transformSetIndex)
        self.m_transformIndices.to_stream(stream)
        self.m_offsets.to_stream(stream)
        return


@dataclass
class hclAction(hkReferencedObject):
    m_active: bool
    m_registeredWithWorldStepping: bool

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclAction":
        base = super().from_reader(stream)
        m_active = stream.read_bool()
        m_registeredWithWorldStepping = stream.read_bool()
        return hclAction(**vars(base), m_active=m_active, m_registeredWithWorldStepping=m_registeredWithWorldStepping)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write_bool(self.m_active)
        stream.write_bool(self.m_registeredWithWorldStepping)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        stream.write_bool(self.m_active)
        stream.write_bool(self.m_registeredWithWorldStepping)
        return


@dataclass
class hclSimClothData__TransferMotionData:
    m_transformSetIndex: int # u32
    m_transformIndex: int # u32
    m_transferTranslationMotion: bool
    m_minTranslationSpeed: int # float
    m_maxTranslationSpeed: int # float
    m_minTranslationBlend: int # float
    m_maxTranslationBlend: int # float
    m_transferRotationMotion: bool
    m_minRotationSpeed: int # float
    m_maxRotationSpeed: int # float
    m_minRotationBlend: int # float
    m_maxRotationBlend: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__TransferMotionData":
        stream.align_to(0x4)
        m_transformSetIndex = stream.read_u32()
        m_transformIndex = stream.read_u32()
        m_transferTranslationMotion = stream.read_bool()
        stream.align_to(0x4)
        m_minTranslationSpeed = stream.read_float()
        m_maxTranslationSpeed = stream.read_float()
        m_minTranslationBlend = stream.read_float()
        m_maxTranslationBlend = stream.read_float()
        m_transferRotationMotion = stream.read_bool()
        stream.align_to(0x4)
        m_minRotationSpeed = stream.read_float()
        m_maxRotationSpeed = stream.read_float()
        m_minRotationBlend = stream.read_float()
        m_maxRotationBlend = stream.read_float()
        return hclSimClothData__TransferMotionData(m_transformSetIndex=m_transformSetIndex, m_transformIndex=m_transformIndex, m_transferTranslationMotion=m_transferTranslationMotion, m_minTranslationSpeed=m_minTranslationSpeed, m_maxTranslationSpeed=m_maxTranslationSpeed, m_minTranslationBlend=m_minTranslationBlend, m_maxTranslationBlend=m_maxTranslationBlend, m_transferRotationMotion=m_transferRotationMotion, m_minRotationSpeed=m_minRotationSpeed, m_maxRotationSpeed=m_maxRotationSpeed, m_minRotationBlend=m_minRotationBlend, m_maxRotationBlend=m_maxRotationBlend)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u32(self.m_transformSetIndex)
        stream.write_u32(self.m_transformIndex)
        stream.write_bool(self.m_transferTranslationMotion)
        stream.write_float(self.m_minTranslationSpeed)
        stream.write_float(self.m_maxTranslationSpeed)
        stream.write_float(self.m_minTranslationBlend)
        stream.write_float(self.m_maxTranslationBlend)
        stream.write_bool(self.m_transferRotationMotion)
        stream.write_float(self.m_minRotationSpeed)
        stream.write_float(self.m_maxRotationSpeed)
        stream.write_float(self.m_minRotationBlend)
        stream.write_float(self.m_maxRotationBlend)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x4)
        stream.write_u32(self.m_transformSetIndex)
        stream.write_u32(self.m_transformIndex)
        stream.write_bool(self.m_transferTranslationMotion)
        stream._writer_align_to(0x4)
        stream.write_float(self.m_minTranslationSpeed)
        stream.write_float(self.m_maxTranslationSpeed)
        stream.write_float(self.m_minTranslationBlend)
        stream.write_float(self.m_maxTranslationBlend)
        stream.write_bool(self.m_transferRotationMotion)
        stream._writer_align_to(0x4)
        stream.write_float(self.m_minRotationSpeed)
        stream.write_float(self.m_maxRotationSpeed)
        stream.write_float(self.m_minRotationBlend)
        stream.write_float(self.m_maxRotationBlend)
        return


@dataclass
class hclSimClothData__LandscapeCollisionData:
    m_landscapeRadius: int # float
    m_enableStuckParticleDetection: bool
    m_stuckParticlesStretchFactorSq: int # float
    m_pinchDetectionEnabled: bool
    m_pinchDetectionPriority: int # u8
    m_pinchDetectionRadius: int # float
    m_collisionTolerance: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__LandscapeCollisionData":
        stream.align_to(0x4)
        m_landscapeRadius = stream.read_float()
        m_enableStuckParticleDetection = stream.read_bool()
        stream.align_to(0x4)
        m_stuckParticlesStretchFactorSq = stream.read_float()
        m_pinchDetectionEnabled = stream.read_bool()
        m_pinchDetectionPriority = stream.read_u8()
        stream.align_to(0x4)
        m_pinchDetectionRadius = stream.read_float()
        m_collisionTolerance = stream.read_float()
        return hclSimClothData__LandscapeCollisionData(m_landscapeRadius=m_landscapeRadius, m_enableStuckParticleDetection=m_enableStuckParticleDetection, m_stuckParticlesStretchFactorSq=m_stuckParticlesStretchFactorSq, m_pinchDetectionEnabled=m_pinchDetectionEnabled, m_pinchDetectionPriority=m_pinchDetectionPriority, m_pinchDetectionRadius=m_pinchDetectionRadius, m_collisionTolerance=m_collisionTolerance)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.m_landscapeRadius)
        stream.write_bool(self.m_enableStuckParticleDetection)
        stream.write_float(self.m_stuckParticlesStretchFactorSq)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream.write_float(self.m_pinchDetectionRadius)
        stream.write_float(self.m_collisionTolerance)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x4)
        stream.write_float(self.m_landscapeRadius)
        stream.write_bool(self.m_enableStuckParticleDetection)
        stream._writer_align_to(0x4)
        stream.write_float(self.m_stuckParticlesStretchFactorSq)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream._writer_align_to(0x4)
        stream.write_float(self.m_pinchDetectionRadius)
        stream.write_float(self.m_collisionTolerance)
        return


@dataclass
class hclSimClothData__CollidablePinchingData:
    m_pinchDetectionEnabled: bool
    m_pinchDetectionPriority: int # u8
    m_pinchDetectionRadius: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData__CollidablePinchingData":
        stream.align_to(0x4)
        m_pinchDetectionEnabled = stream.read_bool()
        m_pinchDetectionPriority = stream.read_u8()
        stream.align_to(0x4)
        m_pinchDetectionRadius = stream.read_float()
        return hclSimClothData__CollidablePinchingData(m_pinchDetectionEnabled=m_pinchDetectionEnabled, m_pinchDetectionPriority=m_pinchDetectionPriority, m_pinchDetectionRadius=m_pinchDetectionRadius)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream.write_float(self.m_pinchDetectionRadius)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x4)
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write_u8(self.m_pinchDetectionPriority)
        stream._writer_align_to(0x4)
        stream.write_float(self.m_pinchDetectionRadius)
        return


@dataclass
class hclVirtualCollisionPointsData__Block:
    m_safeDisplacementRadius: int # float
    m_startingVCPIndex: int # u16
    m_numVCPs: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__Block":
        stream.align_to(0x4)
        m_safeDisplacementRadius = stream.read_float()
        m_startingVCPIndex = stream.read_u16()
        m_numVCPs = stream.read_u8()
        return hclVirtualCollisionPointsData__Block(m_safeDisplacementRadius=m_safeDisplacementRadius, m_startingVCPIndex=m_startingVCPIndex, m_numVCPs=m_numVCPs)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.m_safeDisplacementRadius)
        stream.write_u16(self.m_startingVCPIndex)
        stream.write_u8(self.m_numVCPs)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x4)
        stream.write_float(self.m_safeDisplacementRadius)
        stream.write_u16(self.m_startingVCPIndex)
        stream.write_u8(self.m_numVCPs)
        return


@dataclass
class hclVirtualCollisionPointsData__BarycentricDictionaryEntry:
    m_startingBarycentricIndex: int # u16
    m_numBarycentrics: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__BarycentricDictionaryEntry":
        stream.align_to(0x2)
        m_startingBarycentricIndex = stream.read_u16()
        m_numBarycentrics = stream.read_u8()
        return hclVirtualCollisionPointsData__BarycentricDictionaryEntry(m_startingBarycentricIndex=m_startingBarycentricIndex, m_numBarycentrics=m_numBarycentrics)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u16(self.m_startingBarycentricIndex)
        stream.write_u8(self.m_numBarycentrics)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x2)
        stream.write_u16(self.m_startingBarycentricIndex)
        stream.write_u8(self.m_numBarycentrics)
        return


@dataclass
class hclVirtualCollisionPointsData__TriangleFan:
    m_realParticleIndex: int # u16
    m_vcpStartIndex: int # u16
    m_numTriangles: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__TriangleFan":
        stream.align_to(0x2)
        m_realParticleIndex = stream.read_u16()
        m_vcpStartIndex = stream.read_u16()
        m_numTriangles = stream.read_u8()
        return hclVirtualCollisionPointsData__TriangleFan(m_realParticleIndex=m_realParticleIndex, m_vcpStartIndex=m_vcpStartIndex, m_numTriangles=m_numTriangles)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_vcpStartIndex)
        stream.write_u8(self.m_numTriangles)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x2)
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_vcpStartIndex)
        stream.write_u8(self.m_numTriangles)
        return


@dataclass
class hclVirtualCollisionPointsData__TriangleFanLandscape:
    m_realParticleIndex: int # u16
    m_triangleStartIndex: int # u16
    m_vcpStartIndex: int # u16
    m_numTriangles: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__TriangleFanLandscape":
        stream.align_to(0x2)
        m_realParticleIndex = stream.read_u16()
        m_triangleStartIndex = stream.read_u16()
        m_vcpStartIndex = stream.read_u16()
        m_numTriangles = stream.read_u8()
        return hclVirtualCollisionPointsData__TriangleFanLandscape(m_realParticleIndex=m_realParticleIndex, m_triangleStartIndex=m_triangleStartIndex, m_vcpStartIndex=m_vcpStartIndex, m_numTriangles=m_numTriangles)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_triangleStartIndex)
        stream.write_u16(self.m_vcpStartIndex)
        stream.write_u8(self.m_numTriangles)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x2)
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_triangleStartIndex)
        stream.write_u16(self.m_vcpStartIndex)
        stream.write_u8(self.m_numTriangles)
        return


@dataclass
class hclVirtualCollisionPointsData__EdgeFanSection:
    m_oppositeRealParticleIndex: int # u16
    m_barycentricDictionaryIndex: int # u16

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__EdgeFanSection":
        stream.align_to(0x2)
        m_oppositeRealParticleIndex = stream.read_u16()
        m_barycentricDictionaryIndex = stream.read_u16()
        return hclVirtualCollisionPointsData__EdgeFanSection(m_oppositeRealParticleIndex=m_oppositeRealParticleIndex, m_barycentricDictionaryIndex=m_barycentricDictionaryIndex)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u16(self.m_oppositeRealParticleIndex)
        stream.write_u16(self.m_barycentricDictionaryIndex)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x2)
        stream.write_u16(self.m_oppositeRealParticleIndex)
        stream.write_u16(self.m_barycentricDictionaryIndex)
        return


@dataclass
class hclVirtualCollisionPointsData__EdgeFan:
    m_realParticleIndex: int # u16
    m_edgeStartIndex: int # u16
    m_numEdges: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__EdgeFan":
        stream.align_to(0x2)
        m_realParticleIndex = stream.read_u16()
        m_edgeStartIndex = stream.read_u16()
        m_numEdges = stream.read_u8()
        return hclVirtualCollisionPointsData__EdgeFan(m_realParticleIndex=m_realParticleIndex, m_edgeStartIndex=m_edgeStartIndex, m_numEdges=m_numEdges)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_edgeStartIndex)
        stream.write_u8(self.m_numEdges)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x2)
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_edgeStartIndex)
        stream.write_u8(self.m_numEdges)
        return


@dataclass
class hclVirtualCollisionPointsData__EdgeFanLandscape:
    m_realParticleIndex: int # u16
    m_edgeStartIndex: int # u16
    m_vcpStartIndex: int # u16
    m_numEdges: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__EdgeFanLandscape":
        stream.align_to(0x2)
        m_realParticleIndex = stream.read_u16()
        m_edgeStartIndex = stream.read_u16()
        m_vcpStartIndex = stream.read_u16()
        m_numEdges = stream.read_u8()
        return hclVirtualCollisionPointsData__EdgeFanLandscape(m_realParticleIndex=m_realParticleIndex, m_edgeStartIndex=m_edgeStartIndex, m_vcpStartIndex=m_vcpStartIndex, m_numEdges=m_numEdges)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_edgeStartIndex)
        stream.write_u16(self.m_vcpStartIndex)
        stream.write_u8(self.m_numEdges)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x2)
        stream.write_u16(self.m_realParticleIndex)
        stream.write_u16(self.m_edgeStartIndex)
        stream.write_u16(self.m_vcpStartIndex)
        stream.write_u8(self.m_numEdges)
        return


@dataclass
class hclVirtualCollisionPointsData__BarycentricPair:
    m_u: int # float
    m_v: int # float

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData__BarycentricPair":
        stream.align_to(0x4)
        m_u = stream.read_float()
        m_v = stream.read_float()
        return hclVirtualCollisionPointsData__BarycentricPair(m_u=m_u, m_v=m_v)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.m_u)
        stream.write_float(self.m_v)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x4)
        stream.write_float(self.m_u)
        stream.write_float(self.m_v)
        return


@dataclass
class hclVirtualCollisionPointsData:
    m_blocks: hkArray # hkArray[hclVirtualCollisionPointsData__Block]
    m_numVCPoints: int # u16
    m_landscapeParticlesBlockIndex: hkArray # hkArray[u16]
    m_numLandscapeVCPoints: int # u16
    m_edgeBarycentricsDictionary: hkArray # hkArray[float]
    m_edgeDictionaryEntries: hkArray # hkArray[hclVirtualCollisionPointsData__BarycentricDictionaryEntry]
    m_triangleBarycentricsDictionary: hkArray # hkArray[hclVirtualCollisionPointsData__BarycentricPair]
    m_triangleDictionaryEntries: hkArray # hkArray[hclVirtualCollisionPointsData__BarycentricDictionaryEntry]
    m_edges: hkArray # hkArray[hclVirtualCollisionPointsData__EdgeFanSection]
    m_edgeFans: hkArray # hkArray[hclVirtualCollisionPointsData__EdgeFan]
    m_triangles: hkArray # hkArray[hclVirtualCollisionPointsData__TriangleFanSection]
    m_triangleFans: hkArray # hkArray[hclVirtualCollisionPointsData__TriangleFan]
    m_edgesLandscape: hkArray # hkArray[hclVirtualCollisionPointsData__EdgeFanSection]
    m_edgeFansLandscape: hkArray # hkArray[hclVirtualCollisionPointsData__EdgeFanLandscape]
    m_trianglesLandscape: hkArray # hkArray[hclVirtualCollisionPointsData__TriangleFanSection]
    m_triangleFansLandscape: hkArray # hkArray[hclVirtualCollisionPointsData__TriangleFanLandscape]
    m_edgeFanIndices: hkArray # hkArray[u16]
    m_triangleFanIndices: hkArray # hkArray[u16]
    m_edgeFanIndicesLandscape: hkArray # hkArray[u16]
    m_triangleFanIndicesLandscape: hkArray # hkArray[u16]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclVirtualCollisionPointsData":
        m_blocks = hkArray.from_reader(stream, hclVirtualCollisionPointsData__Block)
        m_numVCPoints = stream.read_u16()
        m_landscapeParticlesBlockIndex = hkArray.from_reader(stream, u16)
        m_numLandscapeVCPoints = stream.read_u16()
        m_edgeBarycentricsDictionary = hkArray.from_reader(stream, FLOAT)
        m_edgeDictionaryEntries = hkArray.from_reader(stream, hclVirtualCollisionPointsData__BarycentricDictionaryEntry)
        m_triangleBarycentricsDictionary = hkArray.from_reader(stream, hclVirtualCollisionPointsData__BarycentricPair)
        m_triangleDictionaryEntries = hkArray.from_reader(stream, hclVirtualCollisionPointsData__BarycentricDictionaryEntry)
        m_edges = hkArray.from_reader(stream, hclVirtualCollisionPointsData__EdgeFanSection)    
        m_edgeFans = hkArray.from_reader(stream, hclVirtualCollisionPointsData__EdgeFan)        
        m_triangles = hkArray.from_reader(stream, hclVirtualCollisionPointsData__TriangleFanSection)
        m_triangleFans = hkArray.from_reader(stream, hclVirtualCollisionPointsData__TriangleFan)        
        m_edgesLandscape = hkArray.from_reader(stream, hclVirtualCollisionPointsData__EdgeFanSection)
        m_edgeFansLandscape = hkArray.from_reader(stream, hclVirtualCollisionPointsData__EdgeFanLandscape)
        m_trianglesLandscape = hkArray.from_reader(stream, hclVirtualCollisionPointsData__TriangleFanSection)
        m_triangleFansLandscape = hkArray.from_reader(stream, hclVirtualCollisionPointsData__TriangleFanLandscape)
        m_edgeFanIndices = hkArray.from_reader(stream, u16)
        m_triangleFanIndices = hkArray.from_reader(stream, u16)
        m_edgeFanIndicesLandscape = hkArray.from_reader(stream, u16)
        m_triangleFanIndicesLandscape = hkArray.from_reader(stream, u16)

        result = hclVirtualCollisionPointsData(m_blocks=m_blocks, m_numVCPoints=m_numVCPoints, m_landscapeParticlesBlockIndex=m_landscapeParticlesBlockIndex, m_numLandscapeVCPoints=m_numLandscapeVCPoints, m_edgeBarycentricsDictionary=m_edgeBarycentricsDictionary, m_edgeDictionaryEntries=m_edgeDictionaryEntries, m_triangleBarycentricsDictionary=m_triangleBarycentricsDictionary, m_triangleDictionaryEntries=m_triangleDictionaryEntries, m_edges=m_edges, m_edgeFans=m_edgeFans, m_triangles=m_triangles, m_triangleFans=m_triangleFans, m_edgesLandscape=m_edgesLandscape, m_edgeFansLandscape=m_edgeFansLandscape, m_trianglesLandscape=m_trianglesLandscape, m_triangleFansLandscape=m_triangleFansLandscape, m_edgeFanIndices=m_edgeFanIndices, m_triangleFanIndices=m_triangleFanIndices, m_edgeFanIndicesLandscape=m_edgeFanIndicesLandscape, m_triangleFanIndicesLandscape=m_triangleFanIndicesLandscape)
        # result.from_reader_self(stream)
        return result


    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_blocks.to_binary())
        stream.write_u16(self.m_numVCPoints)
        stream.write(self.m_landscapeParticlesBlockIndex.to_binary())
        stream.write_u16(self.m_numLandscapeVCPoints)
        stream.write(self.m_edgeBarycentricsDictionary.to_binary())
        stream.write(self.m_edgeDictionaryEntries.to_binary())
        stream.write(self.m_triangleBarycentricsDictionary.to_binary())
        stream.write(self.m_triangleDictionaryEntries.to_binary())
        stream.write(self.m_edges.to_binary())
        stream.write(self.m_edgeFans.to_binary())
        stream.write(self.m_triangles.to_binary())
        stream.write(self.m_triangleFans.to_binary())
        stream.write(self.m_edgesLandscape.to_binary())
        stream.write(self.m_edgeFansLandscape.to_binary())
        stream.write(self.m_trianglesLandscape.to_binary())
        stream.write(self.m_triangleFansLandscape.to_binary())
        stream.write(self.m_edgeFanIndices.to_binary())
        stream.write(self.m_triangleFanIndices.to_binary())
        stream.write(self.m_edgeFanIndicesLandscape.to_binary())
        stream.write(self.m_triangleFanIndicesLandscape.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_blocks.to_stream(stream)
        stream.write_u16(self.m_numVCPoints)
        self.m_landscapeParticlesBlockIndex.to_stream(stream)
        stream.write_u16(self.m_numLandscapeVCPoints)
        self.m_edgeBarycentricsDictionary.to_stream(stream)
        self.m_edgeDictionaryEntries.to_stream(stream)
        self.m_triangleBarycentricsDictionary.to_stream(stream)
        self.m_triangleDictionaryEntries.to_stream(stream)
        self.m_edges.to_stream(stream)
        self.m_edgeFans.to_stream(stream)
        self.m_triangles.to_stream(stream)
        self.m_triangleFans.to_stream(stream)
        self.m_edgesLandscape.to_stream(stream)
        self.m_edgeFansLandscape.to_stream(stream)
        self.m_trianglesLandscape.to_stream(stream)
        self.m_triangleFansLandscape.to_stream(stream)
        self.m_edgeFanIndices.to_stream(stream)
        self.m_triangleFanIndices.to_stream(stream)
        self.m_edgeFanIndicesLandscape.to_stream(stream)
        self.m_triangleFanIndicesLandscape.to_stream(stream)
        self.result.to_stream(stream)
        return


@dataclass
class hclSimClothData(hkReferencedObject):
    m_name: hkStringPtr
    m_simulationInfo: hclSimClothData__OverridableSimulationInfo
    m_particleDatas: hkArray # hkArray[hclSimClothData__ParticleData]
    m_fixedParticles: hkArray # hkArray[u16]
    m_doNormals: bool
    m_simOpIds: hkArray # hkArray[u32]
    m_simClothPoses: hkArray # hkArray[hkRefPtr[hclSimClothPose]]
    m_staticConstraintSets: hkArray # hkArray[hkRefPtr[hclConstraintSet]]
    m_antiPinchConstraintSets: hkArray # hkArray[hkRefPtr[hclConstraintSet]]
    m_collidableTransformMap: hclSimClothData__CollidableTransformMap
    m_perInstanceCollidables: hkArray # hkArray[hkRefPtr[hclCollidable]]
    m_maxParticleRadius: int # float
    m_staticCollisionMasks: hkArray # hkArray[u32]
    m_actions: hkArray # hkArray[hkRefPtr[hclAction]]
    m_totalMass: int # float
    m_transferMotionData: hclSimClothData__TransferMotionData
    m_transferMotionEnabled: bool
    m_landscapeCollisionEnabled: bool
    m_landscapeCollisionData: hclSimClothData__LandscapeCollisionData
    m_numLandscapeCollidableParticles: int # u32
    m_triangleIndices: hkArray # hkArray[u16]
    m_triangleFlips: hkArray # hkArray[u8]
    m_pinchDetectionEnabled: bool
    m_perParticlePinchDetectionEnabledFlags: hkArray # hkArray[BOOL]
    m_collidablePinchingDatas: hkArray # hkArray[hclSimClothData__CollidablePinchingData]
    m_minPinchedParticleIndex: int # u16
    m_maxPinchedParticleIndex: int # u16
    m_maxCollisionPairs: int # u32
    m_virtualCollisionPointsData: hclVirtualCollisionPointsData

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclSimClothData":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_simulationInfo = hclSimClothData__OverridableSimulationInfo.from_reader(stream)
        m_particleDatas = hkArray.from_reader(stream, hclSimClothData__ParticleData)
        m_fixedParticles = hkArray.from_reader(stream, u16) 
        m_doNormals = stream.read_bool()
        m_simOpIds = hkArray.from_reader(stream, u32)
        m_simClothPoses = hkArray.from_reader(stream, hkRefPtr, hclSimClothPose) 
        m_staticConstraintSets = hkArray.from_reader(stream, hkRefPtr, hclConstraintSet)        
        m_antiPinchConstraintSets = hkArray.from_reader(stream, hkRefPtr, hclConstraintSet)     
        m_collidableTransformMap = hclSimClothData__CollidableTransformMap.from_reader(stream)        
        m_perInstanceCollidables = hkArray.from_reader(stream, hkRefPtr, hclCollidable)
        m_maxParticleRadius = stream.read_float()
        m_staticCollisionMasks = hkArray.from_reader(stream, u32)
        m_actions = hkArray.from_reader(stream, hkRefPtr, hclAction)
        m_totalMass = stream.read_float()
        m_transferMotionData = hclSimClothData__TransferMotionData.from_reader(stream)
        m_transferMotionEnabled = stream.read_bool()
        m_landscapeCollisionEnabled = stream.read_bool()
        m_landscapeCollisionData = hclSimClothData__LandscapeCollisionData.from_reader(stream)        
        m_numLandscapeCollidableParticles = stream.read_u32()
        m_triangleIndices = hkArray.from_reader(stream, u16)
        m_triangleFlips = hkArray.from_reader(stream, u8)
        m_pinchDetectionEnabled = stream.read_bool()
        m_perParticlePinchDetectionEnabledFlags = hkArray.from_reader(stream, BOOL)
        m_collidablePinchingDatas = hkArray.from_reader(stream, hclSimClothData__CollidablePinchingData)
        m_minPinchedParticleIndex = stream.read_u16()
        m_maxPinchedParticleIndex = stream.read_u16()
        m_maxCollisionPairs = stream.read_u32()
        m_virtualCollisionPointsData = hclVirtualCollisionPointsData.from_reader(stream)

        result =  hclSimClothData(**vars(base), m_name=m_name, m_simulationInfo=m_simulationInfo, m_particleDatas=m_particleDatas, m_fixedParticles=m_fixedParticles, m_doNormals=m_doNormals, m_simOpIds=m_simOpIds, m_simClothPoses=m_simClothPoses, m_staticConstraintSets=m_staticConstraintSets, m_antiPinchConstraintSets=m_antiPinchConstraintSets, m_collidableTransformMap=m_collidableTransformMap, m_perInstanceCollidables=m_perInstanceCollidables, m_maxParticleRadius=m_maxParticleRadius, m_staticCollisionMasks=m_staticCollisionMasks, m_actions=m_actions, m_totalMass=m_totalMass, m_transferMotionData=m_transferMotionData, m_transferMotionEnabled=m_transferMotionEnabled, m_landscapeCollisionEnabled=m_landscapeCollisionEnabled, m_landscapeCollisionData=m_landscapeCollisionData, m_numLandscapeCollidableParticles=m_numLandscapeCollidableParticles, m_triangleIndices=m_triangleIndices, m_triangleFlips=m_triangleFlips, m_pinchDetectionEnabled=m_pinchDetectionEnabled, m_perParticlePinchDetectionEnabledFlags=m_perParticlePinchDetectionEnabledFlags, m_collidablePinchingDatas=m_collidablePinchingDatas, m_minPinchedParticleIndex=m_minPinchedParticleIndex, m_maxPinchedParticleIndex=m_maxPinchedParticleIndex, m_maxCollisionPairs=m_maxCollisionPairs, m_virtualCollisionPointsData=m_virtualCollisionPointsData)

        return result

    
    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_simulationInfo.to_binary())
        stream.write(self.m_particleDatas.to_binary())
        stream.write(self.m_fixedParticles.to_binary())
        stream.write_bool(self.m_doNormals)
        stream.write(self.m_simOpIds.to_binary())
        stream.write(self.m_simClothPoses.to_binary())
        stream.write(self.m_staticConstraintSets.to_binary())
        stream.write(self.m_antiPinchConstraintSets.to_binary())
        stream.write(self.m_collidableTransformMap.to_binary())
        stream.write(self.m_perInstanceCollidables.to_binary())
        stream.write_float(self.m_maxParticleRadius)
        stream.write(self.m_staticCollisionMasks.to_binary())
        stream.write(self.m_actions.to_binary())
        stream.write_float(self.m_totalMass)
        stream.write(self.m_transferMotionData.to_binary())
        stream.write_bool(self.m_transferMotionEnabled)
        stream.write_bool(self.m_landscapeCollisionEnabled)
        stream.write(self.m_landscapeCollisionData.to_binary())
        stream.write_u32(self.m_numLandscapeCollidableParticles)
        stream.write(self.m_triangleIndices.to_binary())
        stream.write(self.m_triangleFlips.to_binary())
        stream.write_bool(self.m_pinchDetectionEnabled)
        stream.write(self.m_perParticlePinchDetectionEnabledFlags.to_binary())
        stream.write(self.m_collidablePinchingDatas.to_binary())
        stream.write_u16(self.m_minPinchedParticleIndex)
        stream.write_u16(self.m_maxPinchedParticleIndex)
        stream.write_u32(self.m_maxCollisionPairs)
        stream.write(self.m_virtualCollisionPointsData.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_simulationInfo.to_stream(stream)
        self.m_particleDatas.to_stream(stream)
        self.m_fixedParticles.to_stream(stream)
        stream.write_bool(self.m_doNormals)
        self.m_simOpIds.to_stream(stream)
        self.m_simClothPoses.to_stream(stream)
        self.m_staticConstraintSets.to_stream(stream)
        self.m_antiPinchConstraintSets.to_stream(stream)
        self.m_collidableTransformMap.to_stream(stream)
        self.m_perInstanceCollidables.to_stream(stream)
        stream.write_float(self.m_maxParticleRadius)
        self.m_staticCollisionMasks.to_stream(stream)
        self.m_actions.to_stream(stream)
        stream.write_float(self.m_totalMass)
        self.m_transferMotionData.to_stream(stream)
        stream.write_bool(self.m_transferMotionEnabled)
        stream.write_bool(self.m_landscapeCollisionEnabled)
        self.m_landscapeCollisionData.to_stream(stream)
        stream.write_u32(self.m_numLandscapeCollidableParticles)
        self.m_triangleIndices.to_stream(stream)
        self.m_triangleFlips.to_stream(stream)
        stream.write_bool(self.m_pinchDetectionEnabled)
        self.m_perParticlePinchDetectionEnabledFlags.to_stream(stream)
        self.m_collidablePinchingDatas.to_stream(stream)
        stream.write_u16(self.m_minPinchedParticleIndex)
        stream.write_u16(self.m_maxPinchedParticleIndex)
        stream.write_u32(self.m_maxCollisionPairs)
        self.m_virtualCollisionPointsData.to_stream(stream)
        self.result.to_stream(stream)
        return


@dataclass
class hclBufferLayout__BufferElement:
    m_vectorConversion: hclRuntimeConversionInfo__VectorConversion
    m_vectorSize: int # u8
    m_slotId: int # u8
    m_slotStart: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclBufferLayout__BufferElement":
        m_vectorConversion = hclRuntimeConversionInfo__VectorConversion(stream.read_u8())
        m_vectorSize = stream.read_u8()
        m_slotId = stream.read_u8()
        m_slotStart = stream.read_u8()
        return hclBufferLayout__BufferElement(m_vectorConversion=m_vectorConversion, m_vectorSize=m_vectorSize, m_slotId=m_slotId, m_slotStart=m_slotStart)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_vectorConversion.value)
        stream.write_u8(self.m_vectorSize)
        stream.write_u8(self.m_slotId)
        stream.write_u8(self.m_slotStart)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_u8(self.m_vectorConversion)
        stream.write_u8(self.m_vectorSize)
        stream.write_u8(self.m_slotId)
        stream.write_u8(self.m_slotStart)
        return


@dataclass
class hclBufferLayout__Slot:
    m_flags: hclBufferLayout__SlotFlags
    m_stride: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclBufferLayout__Slot":
        m_flags = hclBufferLayout__SlotFlags(stream.read_u8())
        m_stride = stream.read_u8()
        return hclBufferLayout__Slot(m_flags=m_flags, m_stride=m_stride)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_flags.value)
        stream.write_u8(self.m_stride)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_u8(self.m_flags)
        stream.write_u8(self.m_stride)
        return


@dataclass
class hclBufferLayout:
    m_elementsLayout: hclBufferLayout__BufferElement
    m_slots: hclBufferLayout__Slot
    m_numSlots: int # u8
    m_triangleFormat: hclBufferLayout__TriangleFormat

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclBufferLayout":
        m_elementsLayout = [hclBufferLayout__BufferElement.from_reader(stream) for _ in range(4)]
        m_slots = [hclBufferLayout__Slot.from_reader(stream) for _ in range(4)]
        m_numSlots = stream.read_u8()
        m_triangleFormat = hclBufferLayout__TriangleFormat(stream.read_u8())
        return hclBufferLayout(m_elementsLayout=m_elementsLayout, m_slots=m_slots, m_numSlots=m_numSlots, m_triangleFormat=m_triangleFormat)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_elementsLayout.to_binary())
        stream.write(self.m_slots.to_binary())
        stream.write_u8(self.m_numSlots)
        stream.write(self.m_triangleFormat.value)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_elementsLayout.to_stream(stream)
        self.m_slots.to_stream(stream)
        stream.write_u8(self.m_numSlots)
        stream.write_u8(self.m_triangleFormat)
        return


@dataclass
class hclBufferDefinition(hkReferencedObject):
    m_meshName: hkStringPtr
    m_bufferName: hkStringPtr
    m_type: int # s32
    m_subType: int # s32
    m_numVertices: int # u32
    m_numTriangles: int # u32
    m_bufferLayout: hclBufferLayout

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclBufferDefinition":
        base = super().from_reader(stream)
        m_meshName = hkStringPtr.from_reader(stream)
        m_bufferName = hkStringPtr.from_reader(stream)
        m_type = stream.read_s32()
        m_subType = stream.read_s32()
        m_numVertices = stream.read_u32()
        m_numTriangles = stream.read_u32()
        m_bufferLayout = hclBufferLayout.from_reader(stream)
        return hclBufferDefinition(**vars(base), m_meshName=m_meshName, m_bufferName=m_bufferName, m_type=m_type, m_subType=m_subType, m_numVertices=m_numVertices, m_numTriangles=m_numTriangles, m_bufferLayout=m_bufferLayout)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_meshName.to_binary())
        stream.write(self.m_bufferName.to_binary())
        stream.write_s32(self.m_type)
        stream.write_s32(self.m_subType)
        stream.write_u32(self.m_numVertices)
        stream.write_u32(self.m_numTriangles)
        stream.write(self.m_bufferLayout.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_meshName.to_stream(stream)
        self.m_bufferName.to_stream(stream)
        stream.write_s32(self.m_type)
        stream.write_s32(self.m_subType)
        stream.write_u32(self.m_numVertices)
        stream.write_u32(self.m_numTriangles)
        self.m_bufferLayout.to_stream(stream)
        return


@dataclass
class hclTransformSetDefinition(hkReferencedObject):
    m_name: hkStringPtr
    m_type: int # s32
    m_numTransforms: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclTransformSetDefinition":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_type = stream.read_s32()
        m_numTransforms = stream.read_u32()
        return hclTransformSetDefinition(**vars(base), m_name=m_name, m_type=m_type, m_numTransforms=m_numTransforms)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write_s32(self.m_type)
        stream.write_u32(self.m_numTransforms)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        stream.write_s32(self.m_type)
        stream.write_u32(self.m_numTransforms)
        return


@dataclass
class hclBufferUsage:
    m_perComponentFlags: bytes # size: 4
    m_trianglesRead: bool

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclBufferUsage":
        m_perComponentFlags = stream._read_exact(4)
        m_trianglesRead = stream.read_bool()
        return hclBufferUsage(m_perComponentFlags=m_perComponentFlags, m_trianglesRead=m_trianglesRead)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_perComponentFlags)
        stream.write_bool(self.m_trianglesRead)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_perComponentFlags.to_stream(stream)
        stream.write_bool(self.m_trianglesRead)
        return


@dataclass
class hclClothState__BufferAccess:
    m_bufferIndex: int # u32
    m_bufferUsage: hclBufferUsage
    m_shadowBufferIndex: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclClothState__BufferAccess":
        stream.align_to(0x4)
        m_bufferIndex = stream.read_u32()
        m_bufferUsage = hclBufferUsage.from_reader(stream)
        stream.align_to(0x4)
        m_shadowBufferIndex = stream.read_u32()
        return hclClothState__BufferAccess(m_bufferIndex=m_bufferIndex, m_bufferUsage=m_bufferUsage, m_shadowBufferIndex=m_shadowBufferIndex)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u32(self.m_bufferIndex)
        stream.write(self.m_bufferUsage.to_binary())
        stream.write_u32(self.m_shadowBufferIndex)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x4)
        stream.write_u32(self.m_bufferIndex)
        self.m_bufferUsage.to_stream(stream)
        stream._writer_align_to(0x4)
        stream.write_u32(self.m_shadowBufferIndex)
        return


@dataclass
class hclTransformSetUsage__TransformTracker:
    m_read: hkBitField
    m_readBeforeWrite: hkBitField
    m_written: hkBitField

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclTransformSetUsage__TransformTracker":
        m_read = hkBitField.from_reader(stream)
        m_readBeforeWrite = hkBitField.from_reader(stream)
        m_written = hkBitField.from_reader(stream)
        return hclTransformSetUsage__TransformTracker(m_read=m_read, m_readBeforeWrite=m_readBeforeWrite, m_written=m_written)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_read.to_binary())
        stream.write(self.m_readBeforeWrite.to_binary())
        stream.write(self.m_written.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_read.to_stream(stream)
        self.m_readBeforeWrite.to_stream(stream)
        self.m_written.to_stream(stream)
        return


@dataclass
class hclTransformSetUsage:
    m_perComponentFlags: bytes # size: 2
    m_perComponentTransformTrackers: hkArray # hkArray[hclTransformSetUsage__TransformTracker]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclTransformSetUsage":
        stream.align_to(0x8)
        m_perComponentFlags = stream._read_exact(2)
        m_perComponentTransformTrackers = hkArray.from_reader(stream, hclTransformSetUsage__TransformTracker)
        return hclTransformSetUsage(m_perComponentFlags=m_perComponentFlags, m_perComponentTransformTrackers=m_perComponentTransformTrackers)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_perComponentFlags)
        stream.write(self.m_perComponentTransformTrackers.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x8)
        self.m_perComponentFlags.to_stream(stream)
        self.m_perComponentTransformTrackers.to_stream(stream)
        return


@dataclass
class hclClothState__TransformSetAccess:
    m_transformSetIndex: int # u32
    m_transformSetUsage: hclTransformSetUsage

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclClothState__TransformSetAccess":
        m_transformSetIndex = stream.read_u32()
        m_transformSetUsage = hclTransformSetUsage.from_reader(stream)
        return hclClothState__TransformSetAccess(m_transformSetIndex=m_transformSetIndex, m_transformSetUsage=m_transformSetUsage)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u32(self.m_transformSetIndex)
        stream.write(self.m_transformSetUsage.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_u32(self.m_transformSetIndex)
        self.m_transformSetUsage.to_stream(stream)
        return


@dataclass
class hclOperator(hkReferencedObject):
    m_name: hkStringPtr
    m_operatorID: int # u32
    m_type: int # u32
    m_usedBuffers: hkArray # hkArray[hclClothState__BufferAccess]
    m_usedTransformSets: hkArray # hkArray[hclClothState__TransformSetAccess]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclOperator":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_operatorID = stream.read_u32()
        m_type = stream.read_u32()
        m_usedBuffers = hkArray.from_reader(stream, hclClothState__BufferAccess)
        m_usedTransformSets = hkArray.from_reader(stream, hclClothState__TransformSetAccess)
        return hclOperator(**vars(base), m_name=m_name, m_operatorID=m_operatorID, m_type=m_type, m_usedBuffers=m_usedBuffers, m_usedTransformSets=m_usedTransformSets)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write_u32(self.m_operatorID)
        stream.write_u32(self.m_type)
        stream.write(self.m_usedBuffers.to_binary())
        stream.write(self.m_usedTransformSets.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        stream.write_u32(self.m_operatorID)
        stream.write_u32(self.m_type)
        self.m_usedBuffers.to_stream(stream)
        self.m_usedTransformSets.to_stream(stream)
        return


@dataclass
class hclStateDependencyGraph__Branch:
    m_branchId: int # s32
    m_stateOperatorIndices: hkArray # hkArray[s32]
    m_parentBranches: hkArray # hkArray[s32]
    m_childBranches: hkArray # hkArray[s32]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclStateDependencyGraph__Branch":
        m_branchId = stream.read_s32()
        m_stateOperatorIndices = hkArray.from_reader(stream, s32)
        m_parentBranches = hkArray.from_reader(stream, s32)
        m_childBranches = hkArray.from_reader(stream, s32)
        return hclStateDependencyGraph__Branch(m_branchId=m_branchId, m_stateOperatorIndices=m_stateOperatorIndices, m_parentBranches=m_parentBranches, m_childBranches=m_childBranches)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_s32(self.m_branchId)
        stream.write(self.m_stateOperatorIndices.to_binary())
        stream.write(self.m_parentBranches.to_binary())
        stream.write(self.m_childBranches.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_s32(self.m_branchId)
        self.m_stateOperatorIndices.to_stream(stream)
        self.m_parentBranches.to_stream(stream)
        self.m_childBranches.to_stream(stream)
        return


@dataclass
class hclStateDependencyGraph(hkReferencedObject):
    m_branches: hkArray # hkArray[hclStateDependencyGraph__Branch]
    m_rootBranchIds: hkArray # hkArray[s32]
    m_children: hkArray # hkArray[hkArray[s32]]
    m_parents: hkArray # hkArray[hkArray[s32]]
    m_multiThreadable: bool

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclStateDependencyGraph":
        base = super().from_reader(stream)
        m_branches = hkArray.from_reader(stream, hclStateDependencyGraph__Branch)
        m_rootBranchIds = hkArray.from_reader(stream, s32)
        m_children = hkArray.from_reader(stream, hkArray, s32)
        m_parents = hkArray.from_reader(stream, hkArray, s32)
        m_multiThreadable = stream.read_bool()
        return hclStateDependencyGraph(**vars(base), m_branches=m_branches, m_rootBranchIds=m_rootBranchIds, m_children=m_children, m_parents=m_parents, m_multiThreadable=m_multiThreadable)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_branches.to_binary())
        stream.write(self.m_rootBranchIds.to_binary())
        stream.write(self.m_children.to_binary())
        stream.write(self.m_parents.to_binary())
        stream.write_bool(self.m_multiThreadable)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_branches.to_stream(stream)
        self.m_rootBranchIds.to_stream(stream)
        self.m_children.to_stream(stream)
        self.m_parents.to_stream(stream)
        stream.write_bool(self.m_multiThreadable)
        return


@dataclass
class hclClothState(hkReferencedObject):
    m_name: hkStringPtr
    m_operators: hkArray # hkArray[u32]
    m_usedBuffers: hkArray # hkArray[hclClothState__BufferAccess]
    m_usedTransformSets: hkArray # hkArray[hclClothState__TransformSetAccess]
    m_usedSimCloths: hkArray # hkArray[u32]
    m_dependencyGraph: hkRefPtr # hkRefPtr[hclStateDependencyGraph]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclClothState":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_operators = hkArray.from_reader(stream, u32)
        m_usedBuffers = hkArray.from_reader(stream, hclClothState__BufferAccess)
        m_usedTransformSets = hkArray.from_reader(stream, hclClothState__TransformSetAccess)
        m_usedSimCloths = hkArray.from_reader(stream, u32)
        m_dependencyGraph = hkRefPtr.from_reader(stream, hclStateDependencyGraph)
        return hclClothState(**vars(base), m_name=m_name, m_operators=m_operators, m_usedBuffers=m_usedBuffers, m_usedTransformSets=m_usedTransformSets, m_usedSimCloths=m_usedSimCloths, m_dependencyGraph=m_dependencyGraph)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_operators.to_binary())
        stream.write(self.m_usedBuffers.to_binary())
        stream.write(self.m_usedTransformSets.to_binary())
        stream.write(self.m_usedSimCloths.to_binary())
        stream.write(self.m_dependencyGraph.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_operators.to_stream(stream)
        self.m_usedBuffers.to_stream(stream)
        self.m_usedTransformSets.to_stream(stream)
        self.m_usedSimCloths.to_stream(stream)
        self.m_dependencyGraph.to_stream(stream)
        return


@dataclass
class hclStateTransition__SimClothTransitionData:
    m_isSimulated: bool
    m_transitionConstraints: hkArray # hkArray[u32]
    m_transitionType: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclStateTransition__SimClothTransitionData":
        m_isSimulated = stream.read_bool()
        m_transitionConstraints = hkArray.from_reader(stream, u32)
        m_transitionType = stream.read_u32()
        return hclStateTransition__SimClothTransitionData(m_isSimulated=m_isSimulated, m_transitionConstraints=m_transitionConstraints, m_transitionType=m_transitionType)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_bool(self.m_isSimulated)
        stream.write(self.m_transitionConstraints.to_binary())
        stream.write_u32(self.m_transitionType)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_bool(self.m_isSimulated)
        self.m_transitionConstraints.to_stream(stream)
        stream.write_u32(self.m_transitionType)
        return


@dataclass
class hclStateTransition__BlendOpTransitionData:
    m_bufferASimCloths: hkArray # hkArray[s32]
    m_bufferBSimCloths: hkArray # hkArray[s32]
    m_transitionType: hclStateTransition__TransitionType
    m_blendWeightType: hclBlendSomeVerticesOperator__BlendWeightType
    m_blendOperatorId: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclStateTransition__BlendOpTransitionData":
        m_bufferASimCloths = hkArray.from_reader(stream, s32)
        m_bufferBSimCloths = hkArray.from_reader(stream, s32)
        m_transitionType = hclStateTransition__TransitionType(stream.read_u8())
        m_blendWeightType = hclBlendSomeVerticesOperator__BlendWeightType(stream.read_u8())
        stream.align_to(0x4)
        m_blendOperatorId = stream.read_u32()
        return hclStateTransition__BlendOpTransitionData(m_bufferASimCloths=m_bufferASimCloths, m_bufferBSimCloths=m_bufferBSimCloths, m_transitionType=m_transitionType, m_blendWeightType=m_blendWeightType, m_blendOperatorId=m_blendOperatorId)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_bufferASimCloths.to_binary())
        stream.write(self.m_bufferBSimCloths.to_binary())
        stream.write(self.m_transitionType.value)
        stream.write(self.m_blendWeightType.value)
        stream.write_u32(self.m_blendOperatorId)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_bufferASimCloths.to_stream(stream)
        self.m_bufferBSimCloths.to_stream(stream)
        stream.write_u8(self.m_transitionType)
        stream.write_u8(self.m_blendWeightType)
        stream._writer_align_to(0x4)
        stream.write_u32(self.m_blendOperatorId)
        return


@dataclass
class hclStateTransition__StateTransitionData:
    m_simClothTransitionData: hkArray # hkArray[hclStateTransition__SimClothTransitionData]
    m_blendOpTransitionData: hkArray # hkArray[hclStateTransition__BlendOpTransitionData]
    m_simulatedState: bool
    m_emptyState: bool

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclStateTransition__StateTransitionData":
        m_simClothTransitionData = hkArray.from_reader(stream, hclStateTransition__SimClothTransitionData)
        m_blendOpTransitionData = hkArray.from_reader(stream, hclStateTransition__BlendOpTransitionData)
        m_simulatedState = stream.read_bool()
        m_emptyState = stream.read_bool()
        return hclStateTransition__StateTransitionData(m_simClothTransitionData=m_simClothTransitionData, m_blendOpTransitionData=m_blendOpTransitionData, m_simulatedState=m_simulatedState, m_emptyState=m_emptyState)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_simClothTransitionData.to_binary())
        stream.write(self.m_blendOpTransitionData.to_binary())
        stream.write_bool(self.m_simulatedState)
        stream.write_bool(self.m_emptyState)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_simClothTransitionData.to_stream(stream)
        self.m_blendOpTransitionData.to_stream(stream)
        stream.write_bool(self.m_simulatedState)
        stream.write_bool(self.m_emptyState)
        return


@dataclass
class hclStateTransition(hkReferencedObject):
    m_name: hkStringPtr
    m_stateIds: hkArray # hkArray[u32]
    m_stateTransitionData: hkArray # hkArray[hclStateTransition__StateTransitionData]
    m_simClothTransitionConstraints: hkArray # hkArray[hkArray[u32]]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclStateTransition":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_stateIds = hkArray.from_reader(stream, u32)
        m_stateTransitionData = hkArray.from_reader(stream, hclStateTransition__StateTransitionData)
        m_simClothTransitionConstraints = hkArray.from_reader(stream, hkArray, u32)
        return hclStateTransition(**vars(base), m_name=m_name, m_stateIds=m_stateIds, m_stateTransitionData=m_stateTransitionData, m_simClothTransitionConstraints=m_simClothTransitionConstraints)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_stateIds.to_binary())
        stream.write(self.m_stateTransitionData.to_binary())
        stream.write(self.m_simClothTransitionConstraints.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_stateIds.to_stream(stream)
        self.m_stateTransitionData.to_stream(stream)
        self.m_simClothTransitionConstraints.to_stream(stream)
        return


@dataclass
class hclClothData(hkReferencedObject):
    m_name: hkStringPtr
    m_simClothDatas: hkArray # hkArray[hkRefPtr[hclSimClothData]]
    m_bufferDefinitions: hkArray # hkArray[hkRefPtr[hclBufferDefinition]]
    m_transformSetDefinitions: hkArray # hkArray[hkRefPtr[hclTransformSetDefinition]]
    m_operators: hkArray # hkArray[hkRefPtr[hclOperator]]
    m_clothStateDatas: hkArray # hkArray[hkRefPtr[hclClothState]]
    m_stateTransitions: hkArray # hkArray[hkRefPtr[hclStateTransition]]
    m_actions: hkArray # hkArray[hkRefPtr[hclAction]]
    m_generatedAtRuntime: bool
    m_targetPlatform: hclClothData__Platform

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclClothData":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_simClothDatas = hkArray.from_reader(stream, hkRefPtr, hclSimClothData)
        m_bufferDefinitions = hkArray.from_reader(stream, hkRefPtr, hclBufferDefinition)
        m_transformSetDefinitions = hkArray.from_reader(stream, hkRefPtr, hclTransformSetDefinition)
        m_operators = hkArray.from_reader(stream, hkRefPtr, hclOperator)
        m_clothStateDatas = hkArray.from_reader(stream, hkRefPtr, hclClothState)
        m_stateTransitions = hkArray.from_reader(stream, hkRefPtr, hclStateTransition)
        m_actions = hkArray.from_reader(stream, hkRefPtr, hclAction)
        
        m_generatedAtRuntime = stream.read_bool()
        stream.align_to(0x4)
        m_targetPlatform = hclClothData__Platform(stream.read_u32())
        result = hclClothData(**vars(base), m_name=m_name, m_simClothDatas=m_simClothDatas, m_bufferDefinitions=m_bufferDefinitions, m_transformSetDefinitions=m_transformSetDefinitions, m_operators=m_operators, m_clothStateDatas=m_clothStateDatas, m_stateTransitions=m_stateTransitions, m_actions=m_actions, m_generatedAtRuntime=m_generatedAtRuntime, m_targetPlatform=m_targetPlatform) 
        
        return result

    
    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_simClothDatas.to_binary())
        stream.write(self.m_bufferDefinitions.to_binary())
        stream.write(self.m_transformSetDefinitions.to_binary())
        stream.write(self.m_operators.to_binary())
        stream.write(self.m_clothStateDatas.to_binary())
        stream.write(self.m_stateTransitions.to_binary())
        stream.write(self.m_actions.to_binary())
        stream.write_bool(self.m_generatedAtRuntime)
        stream.write(self.m_targetPlatform.value)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())

    
    def validate(self):
        assert self.m_targetPlatform == hclClothData__Platform.HCL_PLATFORM_NX, f"Invalid target platform: {self.m_targetPlatform.name}, expected hclClothData__Platform.HCL_PLATFORM_NX"


    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_simClothDatas.to_stream(stream)
        self.m_bufferDefinitions.to_stream(stream)
        self.m_transformSetDefinitions.to_stream(stream)
        self.m_operators.to_stream(stream)
        self.m_clothStateDatas.to_stream(stream)
        self.m_stateTransitions.to_stream(stream)
        self.m_actions.to_stream(stream)
        stream.write_bool(self.m_generatedAtRuntime)
        stream._writer_align_to(0x4)
        stream.write_u32(self.m_targetPlatform)
        self.result.to_stream(stream)
        return


@dataclass
class hclClothContainer(hkReferencedObject):
    m_collidables: hkArray # hkArray[hkRefPtr[hclCollidable]]
    m_clothDatas: hkArray # hkArray[hkRefPtr[hclClothData]]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hclClothContainer":
        # stream.seek(0x110, 0)
        base = super().from_reader(stream)
        m_collidables = hkArray.from_reader(stream, hkRefPtr, hclCollidable)
        m_clothDatas = hkArray.from_reader(stream, hkRefPtr, hclClothData)
        
        return hclClothContainer(**vars(base), m_collidables=m_collidables, m_clothDatas=m_clothDatas)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_collidables.to_binary())
        stream.write(self.m_clothDatas.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_collidables.to_stream(stream)
        self.m_clothDatas.to_stream(stream)
        return


@dataclass
class hkMemoryResourceHandle__ExternalLink:
    m_memberName: hkStringPtr
    m_externalId: hkStringPtr

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkMemoryResourceHandle__ExternalLink":
        m_memberName = hkStringPtr.from_reader(stream)
        m_externalId = hkStringPtr.from_reader(stream)
        return hkMemoryResourceHandle__ExternalLink(m_memberName=m_memberName, m_externalId=m_externalId)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_memberName.to_binary())
        stream.write(self.m_externalId.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_memberName.to_stream(stream)
        self.m_externalId.to_stream(stream)
        return


@dataclass
class hkMemoryResourceHandle(hkReferencedObject):
    m_variant: hkRefVariant
    m_name: hkStringPtr
    m_references: hkArray # hkArray[hkMemoryResourceHandle__ExternalLink]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkMemoryResourceHandle":
        base = super().from_reader(stream)
        m_variant = hkRefVariant.from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_references = hkArray.from_reader(stream, hkMemoryResourceHandle__ExternalLink)
        return hkMemoryResourceHandle(**vars(base), m_variant=m_variant, m_name=m_name, m_references=m_references)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_variant.to_binary())
        stream.write(self.m_name.to_binary())
        stream.write(self.m_references.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_variant.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_references.to_stream(stream)
        return


@dataclass
class hkMemoryResourceContainer(hkReferencedObject):
    m_name: hkStringPtr
    m_parent: hkRefPtr = None # hkRefPtr[hkMemoryResourceContainer]
    m_resourceHandles: hkArray = None # hkArray[hkRefPtr[hkMemoryResourceHandle]]
    m_children: hkArray = None # hkArray[hkRefPtr[hkMemoryResourceContainer]]

    @classmethod
    def from_reader(cls, stream: ReadStream, is_root=False) -> "hkMemoryResourceContainer":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_parent = hkRefPtr.from_reader(stream, hkMemoryResourceContainer) if is_root else None
        m_resourceHandles = hkArray.from_reader(stream, hkRefPtr, hkMemoryResourceHandle) if is_root else None
        m_children = hkArray.from_reader(stream, hkRefPtr, hkMemoryResourceContainer) if is_root else None
        return hkMemoryResourceContainer(**vars(base), m_name=m_name, m_parent=m_parent, m_resourceHandles=m_resourceHandles, m_children=m_children)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_parent.to_binary())
        stream.write(self.m_resourceHandles.to_binary())
        stream.write(self.m_children.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_parent.to_stream(stream)
        self.m_resourceHandles.to_stream(stream)
        self.m_children.to_stream(stream)
        return


@dataclass
class hkaBone:
    m_name: hkStringPtr
    m_lockTranslation: bool

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaBone":
        m_name = hkStringPtr.from_reader(stream)
        m_lockTranslation = stream.read_bool()
        return hkaBone(m_name=m_name, m_lockTranslation=m_lockTranslation)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_name.to_binary())
        stream.write_bool(self.m_lockTranslation)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_name.to_stream(stream)
        stream.write_bool(self.m_lockTranslation)
        return


@dataclass
class hkLocalFrame(hkReferencedObject):

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkLocalFrame":
        base = super().from_reader(stream)
        return hkLocalFrame(**vars(base), )

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        return


@dataclass
class hkaSkeleton__LocalFrameOnBone:
    m_localFrame: hkRefPtr # hkRefPtr[hkLocalFrame]
    m_boneIndex: int # u16

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaSkeleton__LocalFrameOnBone":
        m_localFrame = hkRefPtr.from_reader(stream, hkLocalFrame)
        m_boneIndex = stream.read_u16()
        return hkaSkeleton__LocalFrameOnBone(m_localFrame=m_localFrame, m_boneIndex=m_boneIndex)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_localFrame.to_binary())
        stream.write_u16(self.m_boneIndex)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_localFrame.to_stream(stream)
        stream.write_u16(self.m_boneIndex)
        return


@dataclass
class hkaSkeleton__Partition:
    m_name: hkStringPtr
    m_startBoneIndex: int # u16
    m_numBones: int # u16

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaSkeleton__Partition":
        m_name = hkStringPtr.from_reader(stream)
        m_startBoneIndex = stream.read_u16()
        m_numBones = stream.read_u16()
        return hkaSkeleton__Partition(m_name=m_name, m_startBoneIndex=m_startBoneIndex, m_numBones=m_numBones)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_name.to_binary())
        stream.write_u16(self.m_startBoneIndex)
        stream.write_u16(self.m_numBones)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_name.to_stream(stream)
        stream.write_u16(self.m_startBoneIndex)
        stream.write_u16(self.m_numBones)
        return


@dataclass
class hkaSkeleton(hkReferencedObject):
    m_name: hkStringPtr
    m_parentIndices: hkArray # hkArray[u16]
    m_bones: hkArray # hkArray[hkaBone]
    m_referencePose: hkArray # hkArray[hkQsTransformf]
    m_referenceFloats: hkArray # hkArray[float]
    m_floatSlots: hkArray # hkArray[hkStringPtr]
    m_localFrames: hkArray # hkArray[hkaSkeleton__LocalFrameOnBone]
    m_partitions: hkArray # hkArray[hkaSkeleton__Partition]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaSkeleton":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_parentIndices = hkArray.from_reader(stream, u16)
        m_bones = hkArray.from_reader(stream, hkaBone)
        m_referencePose = hkArray.from_reader(stream, hkQsTransformf)
        m_referenceFloats = hkArray.from_reader(stream, float)
        m_floatSlots = hkArray.from_reader(stream, hkStringPtr)
        m_localFrames = hkArray.from_reader(stream, hkaSkeleton__LocalFrameOnBone)
        m_partitions = hkArray.from_reader(stream, hkaSkeleton__Partition)
        return hkaSkeleton(**vars(base), m_name=m_name, m_parentIndices=m_parentIndices, m_bones=m_bones, m_referencePose=m_referencePose, m_referenceFloats=m_referenceFloats, m_floatSlots=m_floatSlots, m_localFrames=m_localFrames, m_partitions=m_partitions)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_parentIndices.to_binary())
        stream.write(self.m_bones.to_binary())
        stream.write(self.m_referencePose.to_binary())
        stream.write(self.m_referenceFloats.to_binary())
        stream.write(self.m_floatSlots.to_binary())
        stream.write(self.m_localFrames.to_binary())
        stream.write(self.m_partitions.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_parentIndices.to_stream(stream)
        self.m_bones.to_stream(stream)
        self.m_referencePose.to_stream(stream)
        self.m_referenceFloats.to_stream(stream)
        self.m_floatSlots.to_stream(stream)
        self.m_localFrames.to_stream(stream)
        self.m_partitions.to_stream(stream)
        return


@dataclass
class hkaAnimatedReferenceFrame(hkReferencedObject):
    m_frameType: hkaAnimatedReferenceFrame__hkaReferenceFrameTypeEnum

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaAnimatedReferenceFrame":
        base = super().from_reader(stream)
        m_frameType = hkaAnimatedReferenceFrame__hkaReferenceFrameTypeEnum(stream.read_u8())
        return hkaAnimatedReferenceFrame(**vars(base), m_frameType=m_frameType)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_frameType.value)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        stream.write_u8(self.m_frameType)
        return


@dataclass
class hkaAnnotationTrack__Annotation:
    m_time: int # float
    m_text: hkStringPtr

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaAnnotationTrack__Annotation":
        m_time = stream.read_float()
        m_text = hkStringPtr.from_reader(stream)
        return hkaAnnotationTrack__Annotation(m_time=m_time, m_text=m_text)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_float(self.m_time)
        stream.write(self.m_text.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_float(self.m_time)
        self.m_text.to_stream(stream)
        return


@dataclass
class hkaAnnotationTrack:
    m_trackName: hkStringPtr
    m_annotations: hkArray # hkArray[hkaAnnotationTrack__Annotation]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaAnnotationTrack":
        m_trackName = hkStringPtr.from_reader(stream)
        m_annotations = hkArray.from_reader(stream, hkaAnnotationTrack__Annotation)
        return hkaAnnotationTrack(m_trackName=m_trackName, m_annotations=m_annotations)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_trackName.to_binary())
        stream.write(self.m_annotations.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_trackName.to_stream(stream)
        self.m_annotations.to_stream(stream)
        return


@dataclass
class hkaAnimation(hkReferencedObject):
    m_type: hkaAnimation__AnimationType
    m_duration: int # float
    m_numberOfTransformTracks: int # s32
    m_numberOfFloatTracks: int # s32
    m_extractedMotion: hkRefPtr # hkRefPtr[hkaAnimatedReferenceFrame]
    m_annotationTracks: hkArray # hkArray[hkaAnnotationTrack]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaAnimation":
        base = super().from_reader(stream)
        m_type = hkaAnimation__AnimationType(stream.read_u32())
        m_duration = stream.read_float()
        m_numberOfTransformTracks = stream.read_s32()
        m_numberOfFloatTracks = stream.read_s32()
        m_extractedMotion = hkRefPtr.from_reader(stream, hkaAnimatedReferenceFrame)
        m_annotationTracks = hkArray.from_reader(stream, hkaAnnotationTrack)
        return hkaAnimation(**vars(base), m_type=m_type, m_duration=m_duration, m_numberOfTransformTracks=m_numberOfTransformTracks, m_numberOfFloatTracks=m_numberOfFloatTracks, m_extractedMotion=m_extractedMotion, m_annotationTracks=m_annotationTracks)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_type.value)
        stream.write_float(self.m_duration)
        stream.write_s32(self.m_numberOfTransformTracks)
        stream.write_s32(self.m_numberOfFloatTracks)
        stream.write(self.m_extractedMotion.to_binary())
        stream.write(self.m_annotationTracks.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        stream.write_u32(self.m_type)
        stream.write_float(self.m_duration)
        stream.write_s32(self.m_numberOfTransformTracks)
        stream.write_s32(self.m_numberOfFloatTracks)
        self.m_extractedMotion.to_stream(stream)
        self.m_annotationTracks.to_stream(stream)
        return


@dataclass
class hkaAnimationBinding(hkReferencedObject):
    m_originalSkeletonName: hkStringPtr
    m_animation: hkRefPtr # hkRefPtr[hkaAnimation]
    m_transformTrackToBoneIndices: hkArray # hkArray[s16]
    m_floatTrackToFloatSlotIndices: hkArray # hkArray[s16]
    m_partitionIndices: hkArray # hkArray[s16]
    m_blendHint: hkaAnimationBinding__BlendHint

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaAnimationBinding":
        base = super().from_reader(stream)
        m_originalSkeletonName = hkStringPtr.from_reader(stream)
        m_animation = hkRefPtr.from_reader(stream, hkaAnimation)
        m_transformTrackToBoneIndices = hkArray.from_reader(stream, s16)
        m_floatTrackToFloatSlotIndices = hkArray.from_reader(stream, s16)
        m_partitionIndices = hkArray.from_reader(stream, s16)
        m_blendHint = hkaAnimationBinding__BlendHint(stream.read_u8())
        return hkaAnimationBinding(**vars(base), m_originalSkeletonName=m_originalSkeletonName, m_animation=m_animation, m_transformTrackToBoneIndices=m_transformTrackToBoneIndices, m_floatTrackToFloatSlotIndices=m_floatTrackToFloatSlotIndices, m_partitionIndices=m_partitionIndices, m_blendHint=m_blendHint)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_originalSkeletonName.to_binary())
        stream.write(self.m_animation.to_binary())
        stream.write(self.m_transformTrackToBoneIndices.to_binary())
        stream.write(self.m_floatTrackToFloatSlotIndices.to_binary())
        stream.write(self.m_partitionIndices.to_binary())
        stream.write(self.m_blendHint.value)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_originalSkeletonName.to_stream(stream)
        self.m_animation.to_stream(stream)
        self.m_transformTrackToBoneIndices.to_stream(stream)
        self.m_floatTrackToFloatSlotIndices.to_stream(stream)
        self.m_partitionIndices.to_stream(stream)
        stream.write_u8(self.m_blendHint)
        return


@dataclass
class hkaBoneAttachment(hkReferencedObject):
    m_originalSkeletonName: hkStringPtr
    m_boneFromAttachment: hkMatrix4f
    m_attachment: hkRefVariant
    m_name: hkStringPtr
    m_boneIndex: int # u16

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaBoneAttachment":
        base = super().from_reader(stream)
        m_originalSkeletonName = hkStringPtr.from_reader(stream)
        m_boneFromAttachment = hkMatrix4f.from_reader(stream)
        m_attachment = hkRefVariant.from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_boneIndex = stream.read_u16()
        return hkaBoneAttachment(**vars(base), m_originalSkeletonName=m_originalSkeletonName, m_boneFromAttachment=m_boneFromAttachment, m_attachment=m_attachment, m_name=m_name, m_boneIndex=m_boneIndex)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_originalSkeletonName.to_binary())
        stream.write(self.m_boneFromAttachment.to_binary())
        stream.write(self.m_attachment.to_binary())
        stream.write(self.m_name.to_binary())
        stream.write_u16(self.m_boneIndex)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_originalSkeletonName.to_stream(stream)
        self.m_boneFromAttachment.to_stream(stream)
        self.m_attachment.to_stream(stream)
        self.m_name.to_stream(stream)
        stream.write_u16(self.m_boneIndex)
        return


@dataclass
class hkxVertexBuffer__VertexData:
    m_vectorData: hkArray # hkArray[u32]
    m_floatData: hkArray # hkArray[u32]
    m_uint32Data: hkArray # hkArray[u32]
    m_uint16Data: hkArray # hkArray[u16]
    m_uint8Data: hkArray # hkArray[u8]
    m_numVerts: int # u32
    m_vectorStride: int # u32
    m_floatStride: int # u32
    m_uint32Stride: int # u32
    m_uint16Stride: int # u32
    m_uint8Stride: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxVertexBuffer__VertexData":
        m_vectorData = hkArray.from_reader(stream, u32)
        m_floatData = hkArray.from_reader(stream, u32)
        m_uint32Data = hkArray.from_reader(stream, u32)
        m_uint16Data = hkArray.from_reader(stream, u16)
        m_uint8Data = hkArray.from_reader(stream, u8)
        m_numVerts = stream.read_u32()
        m_vectorStride = stream.read_u32()
        m_floatStride = stream.read_u32()
        m_uint32Stride = stream.read_u32()
        m_uint16Stride = stream.read_u32()
        m_uint8Stride = stream.read_u32()
        return hkxVertexBuffer__VertexData(m_vectorData=m_vectorData, m_floatData=m_floatData, m_uint32Data=m_uint32Data, m_uint16Data=m_uint16Data, m_uint8Data=m_uint8Data, m_numVerts=m_numVerts, m_vectorStride=m_vectorStride, m_floatStride=m_floatStride, m_uint32Stride=m_uint32Stride, m_uint16Stride=m_uint16Stride, m_uint8Stride=m_uint8Stride)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_vectorData.to_binary())
        stream.write(self.m_floatData.to_binary())
        stream.write(self.m_uint32Data.to_binary())
        stream.write(self.m_uint16Data.to_binary())
        stream.write(self.m_uint8Data.to_binary())
        stream.write_u32(self.m_numVerts)
        stream.write_u32(self.m_vectorStride)
        stream.write_u32(self.m_floatStride)
        stream.write_u32(self.m_uint32Stride)
        stream.write_u32(self.m_uint16Stride)
        stream.write_u32(self.m_uint8Stride)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_vectorData.to_stream(stream)
        self.m_floatData.to_stream(stream)
        self.m_uint32Data.to_stream(stream)
        self.m_uint16Data.to_stream(stream)
        self.m_uint8Data.to_stream(stream)
        stream.write_u32(self.m_numVerts)
        stream.write_u32(self.m_vectorStride)
        stream.write_u32(self.m_floatStride)
        stream.write_u32(self.m_uint32Stride)
        stream.write_u32(self.m_uint16Stride)
        stream.write_u32(self.m_uint8Stride)
        return


@dataclass
class hkxVertexDescription__ElementDecl:
    m_byteOffset: int # u32
    m_type: hkxVertexDescription__DataType
    m_usage: hkxVertexDescription__DataUsage
    m_byteStride: int # u32
    m_numElements: int # u8
    m_channelID: hkStringPtr

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxVertexDescription__ElementDecl":
        stream.align_to(0x8)
        m_byteOffset = stream.read_u32()
        m_type = hkxVertexDescription__DataType(stream.read_u16())
        m_usage = hkxVertexDescription__DataUsage(stream.read_u16())
        m_byteStride = stream.read_u32()
        m_numElements = stream.read_u8()
        m_channelID = hkStringPtr.from_reader(stream)
        return hkxVertexDescription__ElementDecl(m_byteOffset=m_byteOffset, m_type=m_type, m_usage=m_usage, m_byteStride=m_byteStride, m_numElements=m_numElements, m_channelID=m_channelID)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u32(self.m_byteOffset)
        stream.write(self.m_type.value)
        stream.write(self.m_usage.value)
        stream.write_u32(self.m_byteStride)
        stream.write_u8(self.m_numElements)
        stream.write(self.m_channelID.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream._writer_align_to(0x8)
        stream.write_u32(self.m_byteOffset)
        stream.write_u16(self.m_type)
        stream.write_u16(self.m_usage)
        stream.write_u32(self.m_byteStride)
        stream.write_u8(self.m_numElements)
        self.m_channelID.to_stream(stream)
        return


@dataclass
class hkxVertexDescription:
    m_decls: hkArray # hkArray[hkxVertexDescription__ElementDecl]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxVertexDescription":
        m_decls = hkArray.from_reader(stream, hkxVertexDescription__ElementDecl)
        return hkxVertexDescription(m_decls=m_decls)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_decls.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_decls.to_stream(stream)
        return


@dataclass
class hkxVertexBuffer(hkReferencedObject):
    m_data: hkxVertexBuffer__VertexData
    m_desc: hkxVertexDescription

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxVertexBuffer":
        base = super().from_reader(stream)
        m_data = hkxVertexBuffer__VertexData.from_reader(stream)
        m_desc = hkxVertexDescription.from_reader(stream)
        return hkxVertexBuffer(**vars(base), m_data=m_data, m_desc=m_desc)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_data.to_binary())
        stream.write(self.m_desc.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_data.to_stream(stream)
        self.m_desc.to_stream(stream)
        return


@dataclass
class hkxIndexBuffer(hkReferencedObject):
    m_indexType: hkxIndexBuffer__IndexType
    m_indices16: hkArray # hkArray[u16]
    m_indices32: hkArray # hkArray[u32]
    m_vertexBaseOffset: int # u32
    m_length: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxIndexBuffer":
        base = super().from_reader(stream)
        m_indexType = hkxIndexBuffer__IndexType(stream.read_u8())
        m_indices16 = hkArray.from_reader(stream, u16)
        m_indices32 = hkArray.from_reader(stream, u32)
        m_vertexBaseOffset = stream.read_u32()
        m_length = stream.read_u32()
        return hkxIndexBuffer(**vars(base), m_indexType=m_indexType, m_indices16=m_indices16, m_indices32=m_indices32, m_vertexBaseOffset=m_vertexBaseOffset, m_length=m_length)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_indexType.value)
        stream.write(self.m_indices16.to_binary())
        stream.write(self.m_indices32.to_binary())
        stream.write_u32(self.m_vertexBaseOffset)
        stream.write_u32(self.m_length)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        stream.write_u8(self.m_indexType)
        self.m_indices16.to_stream(stream)
        self.m_indices32.to_stream(stream)
        stream.write_u32(self.m_vertexBaseOffset)
        stream.write_u32(self.m_length)
        return


@dataclass
class hkxAttribute:
    m_name: hkStringPtr
    m_value: hkRefVariant

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxAttribute":
        m_name = hkStringPtr.from_reader(stream)
        m_value = hkRefVariant.from_reader(stream)
        return hkxAttribute(m_name=m_name, m_value=m_value)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_name.to_binary())
        stream.write(self.m_value.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_name.to_stream(stream)
        self.m_value.to_stream(stream)
        return


@dataclass
class hkxAttributeGroup:
    m_name: hkStringPtr
    m_attributes: hkArray # hkArray[hkxAttribute]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxAttributeGroup":
        m_name = hkStringPtr.from_reader(stream)
        m_attributes = hkArray.from_reader(stream, hkxAttribute)
        return hkxAttributeGroup(m_name=m_name, m_attributes=m_attributes)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_name.to_binary())
        stream.write(self.m_attributes.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_name.to_stream(stream)
        self.m_attributes.to_stream(stream)
        return


@dataclass
class hkxAttributeHolder(hkReferencedObject):
    m_attributeGroups: hkArray # hkArray[hkxAttributeGroup]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxAttributeHolder":
        base = super().from_reader(stream)
        m_attributeGroups = hkArray.from_reader(stream, hkxAttributeGroup)
        return hkxAttributeHolder(**vars(base), m_attributeGroups=m_attributeGroups)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_attributeGroups.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_attributeGroups.to_stream(stream)
        return


@dataclass
class hkxMaterial__TextureStage:
    m_texture: hkRefVariant
    m_usageHint: hkxMaterial__TextureType
    m_tcoordChannel: int # s32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMaterial__TextureStage":
        m_texture = hkRefVariant.from_reader(stream)
        m_usageHint = hkxMaterial__TextureType.from_reader(stream)
        m_tcoordChannel = stream.read_s32()
        return hkxMaterial__TextureStage(m_texture=m_texture, m_usageHint=m_usageHint, m_tcoordChannel=m_tcoordChannel)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_texture.to_binary())
        stream.write(self.m_usageHint.to_binary())
        stream.write_s32(self.m_tcoordChannel)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_texture.to_stream(stream)
        self.m_usageHint.to_stream(stream)
        stream.write_s32(self.m_tcoordChannel)
        return


@dataclass
class hkxMaterial__Property:
    m_key: int # u32
    m_value: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMaterial__Property":
        m_key = stream.read_u32()
        m_value = stream.read_u32()
        return hkxMaterial__Property(m_key=m_key, m_value=m_value)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write_u32(self.m_key)
        stream.write_u32(self.m_value)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_u32(self.m_key)
        stream.write_u32(self.m_value)
        return


@dataclass
class hkxMaterial(hkxAttributeHolder):
    m_name: hkStringPtr
    m_stages: hkArray # hkArray[hkxMaterial__TextureStage]
    m_diffuseColor: hkVector4f
    m_ambientColor: hkVector4f
    m_specularColor: hkVector4f
    m_emissiveColor: hkVector4f
    m_subMaterials: hkArray # hkArray[hkRefPtr[hkxMaterial]]
    m_extraData: hkRefVariant
    m_uvMapScale: int # float
    m_uvMapOffset: int # float
    m_uvMapRotation: int # float
    m_uvMapAlgorithm: hkxMaterial__UVMappingAlgorithm
    m_specularMultiplier: int # float
    m_specularExponent: int # float
    m_transparency: hkxMaterial__Transparency
    m_userData: int # u64
    m_properties: hkArray # hkArray[hkxMaterial__Property]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMaterial":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_stages = hkArray.from_reader(stream, hkxMaterial__TextureStage)
        m_diffuseColor = hkVector4f.from_reader(stream)
        m_ambientColor = hkVector4f.from_reader(stream)
        m_specularColor = hkVector4f.from_reader(stream)
        m_emissiveColor = hkVector4f.from_reader(stream)
        m_subMaterials = hkArray.from_reader(stream, hkRefPtr, hkxMaterial)
        m_extraData = hkRefVariant.from_reader(stream)
        m_uvMapScale = [stream.read_float() for _ in range(2)]
        m_uvMapOffset = [stream.read_float() for _ in range(2)]
        m_uvMapRotation = stream.read_float()
        m_uvMapAlgorithm = hkxMaterial__UVMappingAlgorithm(stream.read_u32())
        m_specularMultiplier = stream.read_float()
        m_specularExponent = stream.read_float()
        m_transparency = hkxMaterial__Transparency(stream.read_u8())
        stream.align_to(0x8)
        m_userData = stream.read_u64()
        m_properties = hkArray.from_reader(stream, hkxMaterial__Property)
        return hkxMaterial(**vars(base), m_name=m_name, m_stages=m_stages, m_diffuseColor=m_diffuseColor, m_ambientColor=m_ambientColor, m_specularColor=m_specularColor, m_emissiveColor=m_emissiveColor, m_subMaterials=m_subMaterials, m_extraData=m_extraData, m_uvMapScale=m_uvMapScale, m_uvMapOffset=m_uvMapOffset, m_uvMapRotation=m_uvMapRotation, m_uvMapAlgorithm=m_uvMapAlgorithm, m_specularMultiplier=m_specularMultiplier, m_specularExponent=m_specularExponent, m_transparency=m_transparency, m_userData=m_userData, m_properties=m_properties)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkxAttributeHolder.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_stages.to_binary())
        stream.write(self.m_diffuseColor.to_binary())
        stream.write(self.m_ambientColor.to_binary())
        stream.write(self.m_specularColor.to_binary())
        stream.write(self.m_emissiveColor.to_binary())
        stream.write(self.m_subMaterials.to_binary())
        stream.write(self.m_extraData.to_binary())
        stream.write_float(self.m_uvMapScale)
        stream.write_float(self.m_uvMapOffset)
        stream.write_float(self.m_uvMapRotation)
        stream.write(self.m_uvMapAlgorithm.value)
        stream.write_float(self.m_specularMultiplier)
        stream.write_float(self.m_specularExponent)
        stream.write(self.m_transparency.value)
        stream.write_u64(self.m_userData)
        stream.write(self.m_properties.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkxAttributeHolder.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_stages.to_stream(stream)
        self.m_diffuseColor.to_stream(stream)
        self.m_ambientColor.to_stream(stream)
        self.m_specularColor.to_stream(stream)
        self.m_emissiveColor.to_stream(stream)
        self.m_subMaterials.to_stream(stream)
        self.m_extraData.to_stream(stream)
        stream.write_float(self.m_uvMapScale)
        stream.write_float(self.m_uvMapOffset)
        stream.write_float(self.m_uvMapRotation)
        stream.write_u32(self.m_uvMapAlgorithm)
        stream.write_float(self.m_specularMultiplier)
        stream.write_float(self.m_specularExponent)
        stream.write_u8(self.m_transparency)
        stream._writer_align_to(0x8)
        stream.write_u64(self.m_userData)
        self.m_properties.to_stream(stream)
        return


@dataclass
class hkxVertexAnimation__UsageMap:
    m_use: hkxVertexDescription__DataUsage
    m_useIndexOrig: int # u8
    m_useIndexLocal: int # u8

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxVertexAnimation__UsageMap":
        m_use = hkxVertexDescription__DataUsage(stream.read_u16())
        m_useIndexOrig = stream.read_u8()
        m_useIndexLocal = stream.read_u8()
        return hkxVertexAnimation__UsageMap(m_use=m_use, m_useIndexOrig=m_useIndexOrig, m_useIndexLocal=m_useIndexLocal)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_use.value)
        stream.write_u8(self.m_useIndexOrig)
        stream.write_u8(self.m_useIndexLocal)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        stream.write_u16(self.m_use)
        stream.write_u8(self.m_useIndexOrig)
        stream.write_u8(self.m_useIndexLocal)
        return


@dataclass
class hkxVertexAnimation(hkReferencedObject):
    m_time: int # float
    m_vertData: hkxVertexBuffer
    m_vertexIndexMap: hkArray # hkArray[s32]
    m_componentMap: hkArray # hkArray[hkxVertexAnimation__UsageMap]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxVertexAnimation":
        base = super().from_reader(stream)
        m_time = stream.read_float()
        m_vertData = hkxVertexBuffer.from_reader(stream)
        m_vertexIndexMap = hkArray.from_reader(stream, s32)
        m_componentMap = hkArray.from_reader(stream, hkxVertexAnimation__UsageMap)
        return hkxVertexAnimation(**vars(base), m_time=m_time, m_vertData=m_vertData, m_vertexIndexMap=m_vertexIndexMap, m_componentMap=m_componentMap)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write_float(self.m_time)
        stream.write(self.m_vertData.to_binary())
        stream.write(self.m_vertexIndexMap.to_binary())
        stream.write(self.m_componentMap.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        stream.write_float(self.m_time)
        self.m_vertData.to_stream(stream)
        self.m_vertexIndexMap.to_stream(stream)
        self.m_componentMap.to_stream(stream)
        return


@dataclass
class hkMeshBoneIndexMapping:
    m_mapping: hkArray # hkArray[u16]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkMeshBoneIndexMapping":
        m_mapping = hkArray.from_reader(stream, u16)
        return hkMeshBoneIndexMapping(m_mapping=m_mapping)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(self.m_mapping.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_mapping.to_stream(stream)
        return


@dataclass
class hkxMeshSection__InstanceInfo:
    m_localTransform: hkMatrix4f
    m_vertexBase: int # u32
    m_verticesCount: int # u32
    m_indexOffset: int # u32
    m_indicesCount: int # u32

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMeshSection__InstanceInfo":
        m_localTransform = hkMatrix4f.from_reader(stream)
        m_vertexBase = stream.read_u32()
        m_verticesCount = stream.read_u32()
        m_indexOffset = stream.read_u32()
        m_indicesCount = stream.read_u32()
        return hkxMeshSection__InstanceInfo(m_localTransform=m_localTransform, m_vertexBase=m_vertexBase, m_verticesCount=m_verticesCount, m_indexOffset=m_indexOffset, m_indicesCount=m_indicesCount)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_localTransform.to_binary())
        stream.write_u32(self.m_vertexBase)
        stream.write_u32(self.m_verticesCount)
        stream.write_u32(self.m_indexOffset)
        stream.write_u32(self.m_indicesCount)
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_localTransform.to_stream(stream)
        stream.write_u32(self.m_vertexBase)
        stream.write_u32(self.m_verticesCount)
        stream.write_u32(self.m_indexOffset)
        stream.write_u32(self.m_indicesCount)
        return


@dataclass
class hkxMeshSection(hkReferencedObject):
    m_vertexBuffer: hkRefPtr # hkRefPtr[hkxVertexBuffer]
    m_indexBuffers: hkArray # hkArray[hkRefPtr[hkxIndexBuffer]]
    m_material: hkRefPtr # hkRefPtr[hkxMaterial]
    m_userChannels: hkArray # hkArray[hkRefVariant]
    m_vertexAnimations: hkArray # hkArray[hkRefPtr[hkxVertexAnimation]]
    m_linearKeyFrameHints: hkArray # hkArray[float]
    m_boneMatrixMap: hkArray # hkArray[hkMeshBoneIndexMapping]
    m_instances: hkArray # hkArray[hkxMeshSection__InstanceInfo]
    m_originalBoundingBoxMin: hkVector4f
    m_originalBoundingBoxMax: hkVector4f

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMeshSection":
        base = super().from_reader(stream)
        m_vertexBuffer = hkRefPtr.from_reader(stream, hkxVertexBuffer)
        m_indexBuffers = hkArray.from_reader(stream, hkRefPtr, hkxIndexBuffer)
        m_material = hkRefPtr.from_reader(stream, hkxMaterial)
        m_userChannels = hkArray.from_reader(stream, hkRefVariant)
        m_vertexAnimations = hkArray.from_reader(stream, hkRefPtr, hkxVertexAnimation)
        m_linearKeyFrameHints = hkArray.from_reader(stream, float)
        m_boneMatrixMap = hkArray.from_reader(stream, hkMeshBoneIndexMapping)
        m_instances = hkArray.from_reader(stream, hkxMeshSection__InstanceInfo)
        m_originalBoundingBoxMin = hkVector4f.from_reader(stream)
        m_originalBoundingBoxMax = hkVector4f.from_reader(stream)
        return hkxMeshSection(**vars(base), m_vertexBuffer=m_vertexBuffer, m_indexBuffers=m_indexBuffers, m_material=m_material, m_userChannels=m_userChannels, m_vertexAnimations=m_vertexAnimations, m_linearKeyFrameHints=m_linearKeyFrameHints, m_boneMatrixMap=m_boneMatrixMap, m_instances=m_instances, m_originalBoundingBoxMin=m_originalBoundingBoxMin, m_originalBoundingBoxMax=m_originalBoundingBoxMax)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_vertexBuffer.to_binary())
        stream.write(self.m_indexBuffers.to_binary())
        stream.write(self.m_material.to_binary())
        stream.write(self.m_userChannels.to_binary())
        stream.write(self.m_vertexAnimations.to_binary())
        stream.write(self.m_linearKeyFrameHints.to_binary())
        stream.write(self.m_boneMatrixMap.to_binary())
        stream.write(self.m_instances.to_binary())
        stream.write(self.m_originalBoundingBoxMin.to_binary())
        stream.write(self.m_originalBoundingBoxMax.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_vertexBuffer.to_stream(stream)
        self.m_indexBuffers.to_stream(stream)
        self.m_material.to_stream(stream)
        self.m_userChannels.to_stream(stream)
        self.m_vertexAnimations.to_stream(stream)
        self.m_linearKeyFrameHints.to_stream(stream)
        self.m_boneMatrixMap.to_stream(stream)
        self.m_instances.to_stream(stream)
        self.m_originalBoundingBoxMin.to_stream(stream)
        self.m_originalBoundingBoxMax.to_stream(stream)
        return


@dataclass
class hkxMesh__UserChannelInfo(hkxAttributeHolder):
    m_name: hkStringPtr
    m_className: hkStringPtr

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMesh__UserChannelInfo":
        base = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_className = hkStringPtr.from_reader(stream)
        return hkxMesh__UserChannelInfo(**vars(base), m_name=m_name, m_className=m_className)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkxAttributeHolder.to_binary(self))
        stream.write(self.m_name.to_binary())
        stream.write(self.m_className.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkxAttributeHolder.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_className.to_stream(stream)
        return


@dataclass
class hkxMesh(hkReferencedObject):
    m_sections: hkArray # hkArray[hkRefPtr[hkxMeshSection]]
    m_userChannelInfos: hkArray # hkArray[hkRefPtr[hkxMesh__UserChannelInfo]]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkxMesh":
        base = super().from_reader(stream)
        m_sections = hkArray.from_reader(stream, hkRefPtr, hkxMeshSection)
        m_userChannelInfos = hkArray.from_reader(stream, hkRefPtr, hkxMesh__UserChannelInfo)
        return hkxMesh(**vars(base), m_sections=m_sections, m_userChannelInfos=m_userChannelInfos)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_sections.to_binary())
        stream.write(self.m_userChannelInfos.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_sections.to_stream(stream)
        self.m_userChannelInfos.to_stream(stream)
        return


@dataclass
class hkaMeshBinding__Mapping:
    m_mapping: hkArray # hkArray[s16]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaMeshBinding__Mapping":
        m_mapping = hkArray.from_reader(stream, s16)
        return hkaMeshBinding__Mapping(m_mapping=m_mapping)

    def to_binary(self) -> bytes:
        stream = WriteStream(b"")
        stream.write(self.m_mapping.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        self.m_mapping.to_stream(stream)
        return


@dataclass
class hkaMeshBinding(hkReferencedObject):
    m_mesh: hkRefPtr # hkRefPtr[hkxMesh]
    m_originalSkeletonName: hkStringPtr
    m_name: hkStringPtr
    m_skeleton: hkRefPtr # hkRefPtr[hkaSkeleton]
    m_mappings: hkArray # hkArray[hkaMeshBinding__Mapping]
    m_boneFromSkinMeshTransforms: hkArray # hkArray[hkTransform]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaMeshBinding":
        base = super().from_reader(stream)
        m_mesh = hkRefPtr.from_reader(stream, hkxMesh)
        m_originalSkeletonName = hkStringPtr.from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_skeleton = hkRefPtr.from_reader(stream, hkaSkeleton)
        m_mappings = hkArray.from_reader(stream, hkaMeshBinding__Mapping)
        m_boneFromSkinMeshTransforms = hkArray.from_reader(stream, hkTransform)
        return hkaMeshBinding(**vars(base), m_mesh=m_mesh, m_originalSkeletonName=m_originalSkeletonName, m_name=m_name, m_skeleton=m_skeleton, m_mappings=m_mappings, m_boneFromSkinMeshTransforms=m_boneFromSkinMeshTransforms)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_mesh.to_binary())
        stream.write(self.m_originalSkeletonName.to_binary())
        stream.write(self.m_name.to_binary())
        stream.write(self.m_skeleton.to_binary())
        stream.write(self.m_mappings.to_binary())
        stream.write(self.m_boneFromSkinMeshTransforms.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_mesh.to_stream(stream)
        self.m_originalSkeletonName.to_stream(stream)
        self.m_name.to_stream(stream)
        self.m_skeleton.to_stream(stream)
        self.m_mappings.to_stream(stream)
        self.m_boneFromSkinMeshTransforms.to_stream(stream)
        return


@dataclass
class hkaAnimationContainer(hkReferencedObject):
    m_skeletons: hkArray # hkArray[hkRefPtr[hkaSkeleton]]
    m_animations: hkArray # hkArray[hkRefPtr[hkaAnimation]]
    m_bindings: hkArray # hkArray[hkRefPtr[hkaAnimationBinding]]
    m_attachments: hkArray # hkArray[hkRefPtr[hkaBoneAttachment]]
    m_skins: hkArray # hkArray[hkRefPtr[hkaMeshBinding]]

    @classmethod
    def from_reader(cls, stream: ReadStream) -> "hkaAnimationContainer":
        base = super().from_reader(stream)
        m_skeletons = hkArray.from_reader(stream, hkRefPtr, hkaSkeleton)
        m_animations = hkArray.from_reader(stream, hkRefPtr, hkaAnimation)
        m_bindings = hkArray.from_reader(stream, hkRefPtr, hkaAnimationBinding)
        m_attachments = hkArray.from_reader(stream, hkRefPtr, hkaBoneAttachment)
        m_skins = hkArray.from_reader(stream, hkRefPtr, hkaMeshBinding)
        return hkaAnimationContainer(**vars(base), m_skeletons=m_skeletons, m_animations=m_animations, m_bindings=m_bindings, m_attachments=m_attachments, m_skins=m_skins)

    def to_binary(self) -> bytes:
        stream = WriteStream()
        stream.write(hkReferencedObject.to_binary(self))
        stream.write(self.m_skeletons.to_binary())
        stream.write(self.m_animations.to_binary())
        stream.write(self.m_bindings.to_binary())
        stream.write(self.m_attachments.to_binary())
        stream.write(self.m_skins.to_binary())
        return stream.getvalue()

    def write(self, stream: WriteStream):
        stream.write(self.to_binary())



    
    def to_stream(self, stream: WriteStream):
        hkReferencedObject.to_stream(stream)
        self.m_skeletons.to_stream(stream)
        self.m_animations.to_stream(stream)
        self.m_bindings.to_stream(stream)
        self.m_attachments.to_stream(stream)
        self.m_skins.to_stream(stream)
        return



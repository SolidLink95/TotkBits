from dataclasses import dataclass
from typing import List
from Experimental import hclConstraintSet, hclOperator, hkVector4f
from Havok import hkArray, hkStringPtr
from util import BphclBaseObject
from Numerics import *




@dataclass
class hclBoneSpaceDeformer__Base(BphclBaseObject):
    m_vertexIndices: List[u16]
    m_boneIndices: List[u16]
    _vert_size: int = -1
    _bone_size: int = -1
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_vertexIndices = [u16.from_reader(reader) for _ in range(cls._vert_size)]
        m_boneIndices = [u16.from_reader(reader) for _ in range(cls._bone_size)]
        result = cls(m_vertexIndices, m_boneIndices)
        return result
    
    def to_stream(self, stream: WriteStream):
        for i in range(len(self.m_vertexIndices)):
            self.m_vertexIndices[i].to_stream(stream)
        for i in range(len(self.m_boneIndices)):
            self.m_boneIndices[i].to_stream(stream)

@dataclass
class hclBoneSpaceDeformer__OneBlendEntryBlock(hclBoneSpaceDeformer__Base):
    _vert_size: int = 16
    _bone_size: int = 16
            

@dataclass
class hclBoneSpaceDeformer__TwoBlendEntryBlock(hclBoneSpaceDeformer__Base):
    _vert_size: int = 8
    _bone_size: int = 16
    

@dataclass
class hclBoneSpaceDeformer__ThreeBlendEntryBlock(hclBoneSpaceDeformer__Base):
    _vert_size: int = 5
    _bone_size: int = 15
    m_padding: bytes = b"" # 8
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        base = hclBoneSpaceDeformer__Base.from_reader(reader)
        m_padding = reader._read_exact(8)
        result = cls(**vars(base), m_padding=m_padding)
        return result
    
    def to_stream(self, stream: WriteStream):
        hclBoneSpaceDeformer__Base.to_stream(self, stream)
        stream.write(self.m_padding)
    

@dataclass
class hclBoneSpaceDeformer__FourBlendEntryBlock(hclBoneSpaceDeformer__ThreeBlendEntryBlock):
    _vert_size: int = 4
    _bone_size: int = 16
    m_padding: bytes = b""
    
    
@dataclass
class hclBoneSpaceDeformer__LocalBlockP(BphclBaseObject):
    m_localPosition: List[hkVector4f] # hkVector4 == hkVector4f, size 16
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_localPosition = [hkVector4f.from_reader(reader) for _ in range(16)]
        result = cls(m_localPosition)
        return result
    
    def to_stream(self, stream: WriteStream):
        for i in range(len(self.m_localPosition)):
            self.m_localPosition[i].to_stream(stream)
            

@dataclass
class hclBoneSpaceDeformer__LocalBlockUnpackedP(hclBoneSpaceDeformer__LocalBlockP):
    pass


@dataclass
class hclCompressibleLinkConstraintSet__Link(BphclBaseObject):
    m_particleA: int # u16
    m_particleB: int # u16
    m_restLength: float
    m_compressionLength: float
    m_stiffness: float
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_particleA = reader.read_u16()
        m_particleB = reader.read_u16()
        m_restLength = reader.read_float()
        m_compressionLength = reader.read_float()
        m_stiffness = reader.read_float()
        result = cls(m_particleA, m_particleB, m_restLength, m_compressionLength, m_stiffness)
        return result


@dataclass
class hclCompressibleLinkConstraintSet(hclConstraintSet):
    m_links: hkArray # hkArray<hclCompressibleLinkConstraintSet::Link
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_links = hkArray.from_reader(reader, hclCompressibleLinkConstraintSet__Link)
        result = cls(m_links)
        return result


@dataclass
class hclMeshBoneDeformUtility__BoneAxis(u32):
    pass


@dataclass
class hclMoveParticlesOperator__VertexParticlePair(BphclBaseObject):
    m_vertexIndex: int # u16
    m_particleIndex: int # u16
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_vertexIndex = reader.read_u16()
        m_particleIndex = reader.read_u16()
        result = cls(m_vertexIndex, m_particleIndex)
        return result
    
    def to_stream(self, stream: WriteStream):
        stream.write_u16(self.m_vertexIndex)
        stream.write_u16(self.m_particleIndex)
        

@dataclass
class hclObjectSpaceDeformer__OneBlendEntryBlock(hclBoneSpaceDeformer__Base):
    _vert_size: int = 16
    _bone_size: int = 16
    

@dataclass
class hclObjectSpaceDeformer__TwoBlendEntryBlock(hclBoneSpaceDeformer__Base):
    m_boneWeights: List[int] = None # list[u8]
    _vert_size: int = 16
    _bone_size: int = 32
    _bone_w_size: int = 32
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        base = hclBoneSpaceDeformer__Base.from_reader(reader)
        m_boneWeights = [reader.read_u8() for _ in range(cls._bone_w_size)]
        result = cls(**vars(base), m_boneWeights=m_boneWeights)
        return result
    
    def to_stream(self, stream: WriteStream):
        hclBoneSpaceDeformer__Base.to_stream(self, stream)
        for i in range(len(self.m_boneWeights)):
            stream.write_u8(self.m_boneWeights[i])
            
            
@dataclass
class hclObjectSpaceDeformer__ThreeBlendEntryBlock(hclObjectSpaceDeformer__TwoBlendEntryBlock):
    _vert_size: int = 16
    _bone_size: int = 48
    _bone_w_size: int = 48
    
    
@dataclass
class hclObjectSpaceDeformer__FourBlendEntryBlock(hclObjectSpaceDeformer__ThreeBlendEntryBlock):
    _vert_size: int = 16
    _bone_size: int = 64
    _bone_w_size: int = 64
    

@dataclass
class hclObjectSpaceDeformer__FiveBlendEntryBlock(hclObjectSpaceDeformer__FourBlendEntryBlock):
    _vert_size: int = 16
    _bone_size: int = 80
    _bone_w_size: int = 80
    
    
@dataclass
class hclObjectSpaceDeformer__SixBlendEntryBlock(hclObjectSpaceDeformer__FiveBlendEntryBlock):
    _vert_size: int = 16
    _bone_size: int = 96
    _bone_w_size: int = 96
    

@dataclass
class hclObjectSpaceDeformer__SevenBlendEntryBlock(hclObjectSpaceDeformer__SixBlendEntryBlock):
    _vert_size: int = 16
    _bone_size: int = 112
    _bone_w_size: int = 112


@dataclass
class hclObjectSpaceDeformer__EightBlendEntryBlock(hclObjectSpaceDeformer__SevenBlendEntryBlock):
    _vert_size: int = 16
    _bone_size: int = 128
    _bone_w_size: int = 128
    
    
@dataclass
class hkPackedVector3(BphclBaseObject):
    m_values: List[int] # list[s16]
    _size: int = 4
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_values = [reader.read_s16() for _ in range(cls._size)]
        result = cls(m_values)
        return result
    
    
    def to_stream(self, stream: WriteStream):
        for i in range(len(self.m_values)):
            stream.write_s16(self.m_values[i])


@dataclass
class hclObjectSpaceDeformer__LocalBlockP(BphclBaseObject):
    m_localPosition: List[hkPackedVector3]  # 16
    _size: int  = 16
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_localPosition = [hkPackedVector3.from_reader(reader) for _ in range(cls._size)]
        result = cls(m_localPosition)
        return result
    
    def to_stream(self, stream: WriteStream):
        for i in range(len(self.m_localPosition)):
            self.m_localPosition[i].to_stream(stream)


@dataclass
class hclObjectSpaceDeformer__LocalBlockUnpackedP(BphclBaseObject):
    m_localPosition: List[hkVector4f]  # 16 hkVector4f == hkVector4, size 16
    _size: int  = 16
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_localPosition = [hkVector4f.from_reader(reader) for _ in range(cls._size)]
        result = cls(m_localPosition)
        return result
    
    def to_stream(self, stream: WriteStream):
        for i in range(len(self.m_localPosition)):
            self.m_localPosition[i].to_stream(stream)
            
            
@dataclass
class hclSimpleMeshBoneDeformOperator__TriangleBonePair(BphclBaseObject):
    m_boneOffset: int # u16
    m_triangleOffset: int # u16
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_boneOffset = reader.read_u16()
        m_triangleOffset = reader.read_u16()
        result = cls(m_boneOffset, m_triangleOffset)
        return result
    
    def to_stream(self, stream: WriteStream):
        stream.write_u16(self.m_boneOffset)
        stream.write_u16(self.m_triangleOffset)


@dataclass
class hclSimulateOperator__Config(BphclBaseObject):
    m_name: hkStringPtr
    m_constraintExecution: hkArray # hkArray<hkInt32
    m_instanceCollidablesUsed: hkArray # hkArray<bool
    m_subSteps: int # u8
    m_numberOfSolveIterations: int # u8
    m_useAllInstanceCollidables: bool 
    m_adaptConstraintStiffness: bool 
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        m_name = hkStringPtr.from_reader(reader)
        m_constraintExecution = hkArray.from_reader(reader, s32)
        m_instanceCollidablesUsed = hkArray.from_reader(reader, BOOL)
        m_subSteps = reader.read_u8()
        m_numberOfSolveIterations = reader.read_u8()
        m_useAllInstanceCollidables = reader.read_bool()
        m_adaptConstraintStiffness = reader.read_bool()
        result = cls(m_name, m_constraintExecution, m_instanceCollidablesUsed, m_subSteps, m_numberOfSolveIterations, m_useAllInstanceCollidables, m_adaptConstraintStiffness)
        return result


    def to_stream(self, stream: WriteStream):
        self.m_name.to_stream(stream)
        self.m_constraintExecution.to_stream(stream)
        self.m_instanceCollidablesUsed.to_stream(stream)
        stream.write_u8(self.m_subSteps)
        stream.write_u8(self.m_numberOfSolveIterations)
        stream.write_bool(self.m_useAllInstanceCollidables)
        stream.write_bool(self.m_adaptConstraintStiffness)
    
@dataclass
class hclSimulateOperator(hclOperator):
    m_simClothIndex: int # u32
    m_simulateOpConfigs: hkArray # hkArray<hclSimulateOperator::Config
    
    
    @classmethod
    def from_reader(cls, reader: ReadStream):
        base = hclOperator.from_reader(reader)
        m_simClothIndex = reader.read_u32()
        m_simulateOpConfigs = hkArray.from_reader(reader, hclSimulateOperator__Config)
        result = cls(**vars(base), m_simClothIndex=m_simClothIndex, m_simulateOpConfigs=m_simulateOpConfigs)
        return result
    
    def to_stream(self, stream: WriteStream):
        hclOperator.to_stream(self, stream)
        stream.write_u32(self.m_simClothIndex)
        self.m_simulateOpConfigs.to_stream(stream)
        
        
DERIVED_CLASSES = {
    "hclBoneSpaceDeformer__OneBlendEntryBlock": hclBoneSpaceDeformer__OneBlendEntryBlock,
    "hclBoneSpaceDeformer__TwoBlendEntryBlock": hclBoneSpaceDeformer__TwoBlendEntryBlock,
    "hclBoneSpaceDeformer__ThreeBlendEntryBlock": hclBoneSpaceDeformer__ThreeBlendEntryBlock,
    "hclBoneSpaceDeformer__FourBlendEntryBlock": hclBoneSpaceDeformer__FourBlendEntryBlock,
    "hclBoneSpaceDeformer__LocalBlockP": hclBoneSpaceDeformer__LocalBlockP,
    "hclBoneSpaceDeformer__LocalBlockUnpackedP": hclBoneSpaceDeformer__LocalBlockUnpackedP,
    "hclCompressibleLinkConstraintSet__Link": hclCompressibleLinkConstraintSet__Link,
    "hclCompressibleLinkConstraintSet": hclCompressibleLinkConstraintSet,
    "hclMeshBoneDeformUtility__BoneAxis": hclMeshBoneDeformUtility__BoneAxis,
    "hclMoveParticlesOperator__VertexParticlePair": hclMoveParticlesOperator__VertexParticlePair,
    "hclObjectSpaceDeformer__OneBlendEntryBlock": hclObjectSpaceDeformer__OneBlendEntryBlock,
    "hclObjectSpaceDeformer__TwoBlendEntryBlock": hclObjectSpaceDeformer__TwoBlendEntryBlock,
    "hclObjectSpaceDeformer__ThreeBlendEntryBlock": hclObjectSpaceDeformer__ThreeBlendEntryBlock,
    "hclObjectSpaceDeformer__FourBlendEntryBlock": hclObjectSpaceDeformer__FourBlendEntryBlock,
    "hclObjectSpaceDeformer__FiveBlendEntryBlock": hclObjectSpaceDeformer__FiveBlendEntryBlock,
    "hclObjectSpaceDeformer__SixBlendEntryBlock": hclObjectSpaceDeformer__SixBlendEntryBlock,
    "hclObjectSpaceDeformer__SevenBlendEntryBlock": hclObjectSpaceDeformer__SevenBlendEntryBlock,
    "hclObjectSpaceDeformer__EightBlendEntryBlock": hclObjectSpaceDeformer__EightBlendEntryBlock,
    "hkPackedVector3": hkPackedVector3,
    "hclObjectSpaceDeformer__LocalBlockP": hclObjectSpaceDeformer__LocalBlockP,
    "hclObjectSpaceDeformer__LocalBlockUnpackedP": hclObjectSpaceDeformer__LocalBlockUnpackedP,
    "hclSimpleMeshBoneDeformOperator__TriangleBonePair": hclSimpleMeshBoneDeformOperator__TriangleBonePair,
    "hclSimulateOperator__Config": hclSimulateOperator__Config,
    "hclSimulateOperator": hclSimulateOperator
}
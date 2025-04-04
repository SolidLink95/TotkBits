import io
from typing import List, Optional, Type, TypeVar
from BphclSmallDataclasses import  BOOL, FLOAT, hkRefPtr, u16, u32, u8, hclAction, hclShape, hclSimClothData__CollidablePinchingData, hclVirtualCollisionPointsData__Block, hkMatrix4f, hkTransform, hkVector4f
from Havok import T, Ptr, hkArray, hkStringPtr
from Stream import ReadStream
from dataclasses import dataclass, field
from Experimental import hkReferencedObject, hclVirtualCollisionPointsData__BarycentricDictionaryEntry



    
    
@dataclass
class hclSimClothData__OverridableSimulationInfo:
    m_gravity: hkVector4f
    m_globalDampingPerSecond: float
    padding: bytes # 0xc bytes
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclSimClothData__OverridableSimulationInfo':
        """Reads an hclSimClothData__OverridableSimulationInfo from the stream."""
        stream.align_to(0x10)
        m_gravity = hkVector4f.from_reader(stream)
        m_globalDampingPerSecond = stream.read_float()
        padding = stream.read(0xc)
        return hclSimClothData__OverridableSimulationInfo(m_gravity, m_globalDampingPerSecond, padding)


@dataclass
class hclSimClothData__ParticleData:
    m_mass: float
    m_invMass: float
    m_radius: float
    m_friction: float
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclSimClothData__ParticleData':
        """Reads an hclSimClothData__ParticleData from the stream."""
        m_mass = stream.read_float()
        m_invMass = stream.read_float()
        m_radius = stream.read_float()
        m_friction = stream.read_float()
        return hclSimClothData__ParticleData(m_mass, m_invMass, m_radius, m_friction)
    
    
@dataclass
class hclConstraintSet(hkReferencedObject):
    m_name: hkStringPtr
    m_constraintId: int # u32
    m_type: int # u32
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclConstraintSet':
        """Reads an hclConstraintSet from the stream."""
        par = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_constraintId = stream.read_u32()
        m_type = stream.read_u32()
        return hclConstraintSet(par._vft_reserve, par.m_sizeAndFlags, par.m_refCount, m_name, m_constraintId, m_type)
    
    
@dataclass
class hclSimClothPose(hkReferencedObject):
    m_name: hkStringPtr
    m_positions: hkArray[hkVector4f]
    
    @staticmethod
    def from_reader(stream):
        par = super().from_reader(stream)
        m_name = hkStringPtr.from_reader(stream)
        m_positions = hkArray.from_reader(stream, hkVector4f)
        return hclSimClothPose(par._vft_reserve, par.m_sizeAndFlags, par.m_refCount, m_name, m_positions)
    
    
@dataclass
class hclSimClothData__CollidableTransformMap:
    m_transformSetIndex: int # u32
    m_transformIndices: hkArray[u32] # u32
    m_offsets: hkArray[hkMatrix4f]
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclSimClothData__CollidableTransformMap':
        """Reads an hclSimClothData__CollidableTransformMap from the stream."""
        m_transformSetIndex = stream.read_u32()
        m_transformIndices = hkArray.from_reader(stream, u32)
        m_offsets = hkArray.from_reader(stream, hkMatrix4f)
        return hclSimClothData__CollidableTransformMap(m_transformSetIndex, m_transformIndices, m_offsets)
    

@dataclass
class hclCollidable(hkReferencedObject):
    m_transform: hkTransform
    m_linearVelocity: hkVector4f
    m_angularVelocity: hkVector4f
    m_userData: int # u64
    m_shape: hkRefPtr[hclShape]
    m_name: hkStringPtr
    m_pinchDetectionRadius: float
    m_pinchDetectionPriority: int # u8
    m_pinchDetectionEnabled: bool
    m_virtualCollisionPointCollisionEnabled: bool
    m_enabled: bool
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclCollidable':
        """Reads an hclCollidable from the stream."""
        par = super().from_reader(stream)
        m_transform = hkTransform.from_reader(stream)
        m_linearVelocity = hkVector4f.from_reader(stream)
        m_angularVelocity = hkVector4f.from_reader(stream)
        m_userData = stream.read_u64()
        m_shape = hkRefPtr.from_reader(stream, hclShape)
        m_name = hkStringPtr.from_reader(stream)
        m_pinchDetectionRadius = stream.read_float()
        m_pinchDetectionPriority = stream.read_u8()
        m_pinchDetectionEnabled = stream.read_bool()
        m_virtualCollisionPointCollisionEnabled = stream.read_bool()
        m_enabled = stream.read_bool()
        return hclCollidable(par._vft_reserve, par.m_sizeAndFlags, par.m_refCount, m_transform, m_linearVelocity, m_angularVelocity, m_userData, m_shape, m_name, m_pinchDetectionRadius, m_pinchDetectionPriority, m_pinchDetectionEnabled, m_virtualCollisionPointCollisionEnabled, m_enabled)
    

@dataclass
class hclSimClothData__TransferMotionData:
    m_transformSetIndex: int # u32
    m_transformIndex: int # u32
    m_transferTranslationMotion: bool 
    
    m_minTranslationSpeed: float
    m_maxTranslationSpeed: float
    m_minTranslationBlend: float
    m_maxTranslationBlend: float
    m_transferRotationMotion: bool
    
    m_minRotationSpeed: float
    m_maxRotationSpeed: float
    m_minRotationBlend: float
    m_maxRotationBlend: float
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclSimClothData__TransferMotionData':
        """Reads an hclSimClothData__TransferMotionData from the stream."""
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
        
        return hclSimClothData__TransferMotionData(m_transformSetIndex, m_transformIndex, m_transferTranslationMotion, m_minTranslationSpeed, m_maxTranslationSpeed, m_minTranslationBlend, m_maxTranslationBlend, m_transferRotationMotion, m_minRotationSpeed, m_maxRotationSpeed, m_minRotationBlend, m_maxRotationBlend)


@dataclass
class hclSimClothData__LandscapeCollisionData:
    m_landscapeRadius: float
    m_enableStuckParticleDetection: bool
    
    m_stuckParticlesStretchFactorSq: float
    m_pinchDetectionEnabled: bool
    m_pinchDetectionPriority: int # u8
    
    m_pinchDetectionRadius: float
    m_collisionTolerance: float
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hclSimClothData__LandscapeCollisionData':
        """Reads an hclSimClothData__LandscapeCollisionData from the stream."""
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
        
        return hclSimClothData__LandscapeCollisionData(m_landscapeRadius, m_enableStuckParticleDetection, m_stuckParticlesStretchFactorSq, m_pinchDetectionEnabled, m_pinchDetectionPriority, m_pinchDetectionRadius, m_collisionTolerance)


@dataclass
class hclVirtualCollisionPointsData:
    m_blocks: hkArray[hclVirtualCollisionPointsData__Block]
    m_numVCPoints: int # u16
    m_landscapeParticlesBlockIndex: hkArray[u16] # u16
    m_numLandscapeVCPoints: int # u16
    m_edgeBarycentricsDictionary: hkArray[FLOAT]
    m_edgeDictionaryEntries: hkArray[hclVirtualCollisionPointsData__BarycentricDictionaryEntry] # u16


@dataclass
class hclSimClothData(hkReferencedObject):
    m_name: hkStringPtr
    m_simulationInfo: hclSimClothData__OverridableSimulationInfo
    m_particleDatas: hkArray[hclSimClothData__ParticleData]
    m_fixedParticles: hkArray[u16] # u16
    m_doNormals: bool
    m_simOpIds: hkArray[u32] # u32
    m_simClothPoses: hkArray[hkRefPtr[hclSimClothPose]]
    m_staticConstraintSets: hkArray[hkRefPtr[hclConstraintSet]]
    m_antiPinchConstraintSets: hkArray[hkRefPtr[hclConstraintSet]]
    m_collidableTransformMap: hclSimClothData__CollidableTransformMap
    m_perInstanceCollidables: hkArray[hkRefPtr[hclCollidable]]
    m_maxParticleRadius: float
    m_staticCollisionMasks: hkArray[u32] # u32
    m_actions: hkArray[hkRefPtr[hclAction]]
    m_totalMass: float
    m_transferMotionData: hclSimClothData__TransferMotionData
    m_transferMotionEnabled: bool
    m_landscapeCollisionEnabled: bool
    m_landscapeCollisionData: hclSimClothData__LandscapeCollisionData
    m_numLandscapeCollidableParticles: int # u32
    m_triangleIndices: hkArray[u16] # u16
    m_triangleFlips: hkArray[u8] # u8
    m_pinchDetectionEnabled: bool
    m_perParticlePinchDetectionEnabledFlags: hkArray[BOOL]
    m_collidablePinchingDatas: hkArray[hclSimClothData__CollidablePinchingData]
    m_minPinchedParticleIndex: int # u16
    m_maxPinchedParticleIndex: int # u16
    m_maxCollisionPairs: int # u16
    m_virtualCollisionPointsData: hclVirtualCollisionPointsData
    
    
# @dataclass
# class hclClothData(hkReferencedObject):
#     m_name: hkStringPtr # ...
#     m_simClothDatas: 


@dataclass
class hclClothContainer(hkReferencedObject):
    pass
from enum import Enum, IntEnum


class ResFileType(IntEnum):
    Shape = 0
    NavMesh = 1
    StaticCompound = 2
    Cloth = 3
    
    
class ResSectionType(Enum):
    SDKV = b"SDKV"
    DATA = b"DATA"
    TYPE = b"TYPE"
    INDX = b"INDX"
    TCRF = b"TCRF"
    TCID = b"TCID"


class ResTypeSectionSignature(Enum):
    TPTR = (b"TPTR", b"TPAD")
    TST1 = (b"TST1" , b"FST1" , b"AST1" , b"TSTR" , b"FSTR" , b"ASTR")
    TNA1 = (b"TNA1", b"TNAM")
    TBDY = (b"TBDY" , b"TBOD")
    THSH = (b"THSH",)
    UNSUPPORTED = (b"TSHA" , b"TPRO" , b"TPHS" , b"TSEQ")


class hkxMaterial__TextureType(Enum):
    TEX_UNKNOWN = 0x0
    TEX_DIFFUSE = 0x1
    TEX_REFLECTION = 0x2
    TEX_BUMP = 0x3
    TEX_NORMAL = 0x4
    TEX_DISPLACEMENT = 0x5
    TEX_SPECULAR = 0x6
    TEX_SPECULARANDGLOSS = 0x7
    TEX_OPACITY = 0x8
    TEX_EMISSIVE = 0x9
    TEX_REFRACTION = 0xa
    TEX_GLOSS = 0xb
    TEX_DOMINANTS = 0xc
    TEX_NOTEXPORTED = 0xd
    TEX_NUM_TYPES = 0xe

class hclRuntimeConversionInfo__VectorConversion(Enum):
    VC_FLOAT4 = 0x0
    VC_FLOAT3 = 0x1
    VC_BYTE4 = 0x2
    VC_SHORT3 = 0x3
    VC_HFLOAT3 = 0x4
    VC_SBYTE4 = 0x5
    VC_CUSTOM_A = 0x14
    VC_CUSTOM_B = 0x15
    VC_CUSTOM_C = 0x16
    VC_CUSTOM_D = 0x17
    VC_CUSTOM_E = 0x18
    VC_MAX_NUM = 0x20
    VC_NONE = 0xfa

class hclBufferLayout__TriangleFormat(Enum):
    TF_THREE_INT32S = 0x0
    TF_THREE_INT16S = 0x1
    TF_OTHER = 0x2

class hclStateTransition__TransitionType(Enum):
    TRANSITION_INACTIVE = 0x0
    ACQUIRE_VELOCITY_FROM_ANIMATION = 0x1
    TRANSITION_TO_ANIMATION = 0x2
    TRANSITION_FROM_ANIMATION = 0x4
    BLEND_TO_ANIMATION = 0x8
    BLEND_FROM_ANIMATION = 0x10
    ANIMATION_TRANSITION_TYPE = 0x6
    ANIMATION_BLEND_TYPE = 0x18
    ANIMATION_TYPE = 0x1e
    TO_ANIMATION_TYPE = 0xa
    FROM_ANIMATION_TYPE = 0x14
    TRANSFER_VELOCITY_TO_SLOD = 0x20
    ACQUIRE_VELOCITY_FROM_SLOD = 0x40
    TRANSITION_TO_SLOD = 0x80
    TRANSITION_FROM_SLOD = 0x100
    BLEND_TO_SLOD = 0x200
    BLEND_FROM_SLOD = 0x400
    SLOD_TRANSITION_TYPE = 0x180
    SLOD_BLEND_TYPE = 0x600
    SLOD_TYPE = 0x780
    TO_SLOD_TYPE = 0x280
    FROM_SLOD_TYPE = 0x500
    BLEND_NO_SIM_TRANSITION = 0x800


class hkaAnimation__AnimationType(Enum):
    HK_UNKNOWN_ANIMATION = 0x0
    HK_INTERLEAVED_ANIMATION = 0x1
    HK_MIRRORED_ANIMATION = 0x2
    HK_SPLINE_COMPRESSED_ANIMATION = 0x3
    HK_QUANTIZED_COMPRESSED_ANIMATION = 0x4
    HK_PREDICTIVE_COMPRESSED_ANIMATION = 0x5
    HK_REFERENCE_POSE_ANIMATION = 0x6


class hclClothData__Platform(Enum):
    HCL_PLATFORM_INVALID = 0x0
    HCL_PLATFORM_WIN32 = 0x1
    HCL_PLATFORM_X64 = 0x2
    HCL_PLATFORM_MACPPC = 0x4
    HCL_PLATFORM_IOS = 0x8
    HCL_PLATFORM_MAC386 = 0x10
    HCL_PLATFORM_PS3 = 0x20
    HCL_PLATFORM_XBOX360 = 0x40
    HCL_PLATFORM_WII = 0x80
    HCL_PLATFORM_LRB = 0x100
    HCL_PLATFORM_LINUX = 0x200
    HCL_PLATFORM_PSVITA = 0x400
    HCL_PLATFORM_ANDROID = 0x800
    HCL_PLATFORM_CTR = 0x1000
    HCL_PLATFORM_WIIU = 0x2000
    HCL_PLATFORM_PS4 = 0x4000
    HCL_PLATFORM_XBOXONE = 0x8000
    HCL_PLATFORM_MAC64 = 0x10000
    HCL_PLATFORM_NX = 0x20000
    HCL_PLATFORM_GDK = 0x40000
    HCL_PLATFORM_PS5 = 0x80000
    HCL_PLATFORM_MACARM64 = 0x100000


class hkaAnimatedReferenceFrame__hkaReferenceFrameTypeEnum(Enum):
    REFERENCE_FRAME_UNKNOWN = 0x0
    REFERENCE_FRAME_DEFAULT = 0x1
    REFERENCE_FRAME_PARAMETRIC = 0x2
    

class hclBufferLayout__SlotFlags(Enum):
    SF_NO_ALIGNED_START = 0x0
    SF_16BYTE_ALIGNED_START = 0x1
    SF_64BYTE_ALIGNED_START = 0x3
    
    
class hclBlendSomeVerticesOperator__BlendWeightType(Enum):
    CONSTANT_BLEND = 0x0
    CUSTOM_WEIGHT = 0x1
    BUFFER_A_WEIGHT = 0x2
    BUFFER_B_WEIGHT = 0x3
    BLEND_BUFFER_A_TO_B = 0x4
    BLEND_BUFFER_B_TO_A = 0x5
    BLEND_BUFFER_A_TO_CUSTOM_WEIGHT = 0x6
    BLEND_BUFFER_B_TO_CUSTOM_WEIGHT = 0x7
    BLEND_CUSTOM_WEIGHT_TO_BUFFER_A = 0x8
    BLEND_CUSTOM_WEIGHT_TO_BUFFER_B = 0x9
    

class hkaAnimationBinding__BlendHint(Enum):
    NORMAL = 0x0
    ADDITIVE_PARENT_SPACE = 0x1
    ADDITIVE_CHILD_SPACE = 0x2
    
    
class hkxVertexDescription__DataType(Enum):
    HKX_DT_NONE = 0x0
    HKX_DT_UINT8 = 0x1
    HKX_DT_INT16 = 0x2
    HKX_DT_UINT32 = 0x3
    HKX_DT_FLOAT = 0x4
    
    
class hkxVertexDescription__DataUsage(Enum):
    HKX_DU_NONE = 0x0
    HKX_DU_POSITION = 0x1
    HKX_DU_COLOR = 0x2
    HKX_DU_NORMAL = 0x4
    HKX_DU_TANGENT = 0x8
    HKX_DU_BINORMAL = 0x10
    HKX_DU_TEXCOORD = 0x20
    HKX_DU_BLENDWEIGHTS = 0x40
    HKX_DU_BLENDINDICES = 0x80
    HKX_DU_USERDATA = 0x100
    
    
class hkxIndexBuffer__IndexType(Enum):
    INDEX_TYPE_INVALID = 0x0
    INDEX_TYPE_TRI_LIST = 0x1
    INDEX_TYPE_TRI_STRIP = 0x2
    INDEX_TYPE_TRI_FAN = 0x3
    INDEX_TYPE_MAX_ID = 0x4
    
    
class hkxMaterial__UVMappingAlgorithm(Enum):
    UVMA_SRT = 0x0
    UVMA_TRS = 0x1
    UVMA_3DSMAX_STYLE = 0x2
    UVMA_MAYA_STYLE = 0x3
    
    
class hkxMaterial__Transparency(Enum):
    transp_none = 0x0
    transp_alpha = 0x2
    transp_additive = 0x3
    transp_colorkey = 0x4
    transp_subtractive = 0x9
# def signature_to_enum(signature: bytes) -> ResSectionType:
#     """Convert a signature to the corresponding enum value."""
#     if len(signature) != 4:
#         raise ValueError(f"Invalid signature length: {len(signature)} expected 4")
#     for entry in ResTypeSectionSignature:
#         if signature in entry.value:
#             return entry
#     raise ValueError(f"Unknown signature: {signature}")


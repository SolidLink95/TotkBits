from enum import Enum,IntEnum

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
    

def signature_to_enum(signature: bytes) -> ResSectionType:
    """Convert a signature to the corresponding enum value."""
    if len(signature) != 4:
        raise ValueError(f"Invalid signature length: {len(signature)} expected 4")
    for entry in ResTypeSectionSignature:
        if signature in entry.value:
            return entry
    raise ValueError(f"Unknown signature: {signature}")


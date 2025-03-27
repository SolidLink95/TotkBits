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


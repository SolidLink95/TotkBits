
from dataclasses import dataclass
import struct
from Stream import ReadStream, WriteStream
from util import BphclBaseObject, _hex


@dataclass
class UIntBase(BphclBaseObject):
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
class BOOL(BphclBaseObject):
    val: bool

    def __repr__(self):
        v = "true" if self.val else "false"
        return f"{self.__class__.__name__}({v})"
    
    @classmethod
    def from_reader(cls, stream: ReadStream) -> "BOOL":
        val = stream.read_bool()
        return cls(val=bool(val))
    
    
@dataclass
class FLOAT(BphclBaseObject):
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

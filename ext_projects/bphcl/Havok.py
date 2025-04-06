import io
from typing import List, Optional, Type, TypeVar, get_origin
from Stream import ReadStream, WriteStream
from dataclasses import dataclass, field, fields, is_dataclass

from util import _hex

T = TypeVar('T')  # Generic type variable for element type


@dataclass
class hkInt: #Int
    value: int #s32
    offset: int
    
    def __repr__(self):
        return f"hkInt({self.value}, offset=0x{self.offset:08X})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkInt':
        """Reads an integer from the stream."""
        value = stream.read_s32()
        offset = stream.tell()
        return hkInt(value, offset)
    
    def to_binary(self) -> bytes:
        """Converts the integer to a binary representation."""
        return self.value.to_bytes(4, byteorder='little', signed=True)
    
    def write(self, stream: WriteStream):
        """Writes the integer to the stream."""
        stream.write(self.to_binary())


@dataclass
class Ptr:
    value:int #u64 if > 0 else s64
    offset:int #u64 stream.tell()
    
    def __repr__(self):
        return f"Ptr(0x{self.value:08X}, offset=0x{self.offset:08X})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'Ptr':
        """Reads a pointer from the stream."""
        offset = stream.tell() #- stream.data_offset
        value = stream.read_64bit_int()
        return Ptr(value, offset)
    
    def to_binary(self) -> bytes:
        """Converts the pointer to a binary representation."""
        return self.value.to_bytes(8, byteorder='little', signed=False)
    
    def write(self, stream: WriteStream):
        """Writes the pointer to the stream."""
        stream.write(self.to_binary())

@dataclass
class hkRefVariant:
    m_ptr: Ptr
    _offset: int 
    
    def __repr__(self):
        return f"hkRefVariant(0x{self.m_ptr.value:08X},0x{self.m_ptr.offset:08X})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkRefVariant':
        """Reads a reference variant from the stream."""
        stream.align_to(8)  # std::mem::AlignTo<0x8>
        _offset = stream.tell()
        m_ptr = Ptr.from_reader(stream)
        current_pos = stream.tell()
        
        return hkRefVariant(m_ptr, _offset)


@dataclass
class hkStringPtr: # size 8 bytes
    _offset: int #u64
    m_stringAndFlag: Ptr
    _str: str = ''
    
    def __repr__(self):
        return f"hkStringPtr({repr(self._str)})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkStringPtr':
        """Reads a string pointer from the stream."""
        stream.align_to(8)
        _offset = stream.tell()
        m_stringAndFlag = Ptr.from_reader(stream)
        
        string_address = m_stringAndFlag.value + _offset #+ stream.data_offset

        current_pos = stream.tell()
        stream.seek(string_address)
        string_data = stream.read_string()
        stream.seek(current_pos)
        return hkStringPtr(_offset, m_stringAndFlag, string_data)
    
    def to_binary(self) -> bytes:
        """Converts the string pointer to a binary representation."""
        return self.m_stringAndFlag.to_binary()
    
    def write(self, stream: WriteStream):
        stream.write(self.to_binary())
        # stream.write(self._str.encode('utf-8') + b'\x00')  # Null-terminated string

    """[
    "Cloth Container",
    "Resource Data",
    "Animation Container",
    "hclClothContainer",
    "hkMemoryResourceContainer",
    "hkaAnimationContainer"
]
"""
@dataclass
class hkRootLevelContainer__NamedVariant:
    m_name: hkStringPtr    
    m_className: hkStringPtr    
    m_variant: hkRefVariant
    
    def __repr__(self):
        n = f"m_name={repr(self.m_name._str)}" if self.m_name._str else f"m_name={self.m_name.m_stringAndFlag.value:04X}"
        cn = f"m_className={repr(self.m_className._str)}" if self.m_className._str else f"m_className={self.m_className.m_stringAndFlag.value:04X}"
        v = f"m_variant=0x{self.m_variant.m_ptr.value:04X}"
        return f"<hkvar {n} {cn} {v}>"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkRootLevelContainer__NamedVariant':
        """Reads a named variant from the stream."""
        stream.align_to(8)
        m_name = hkStringPtr.from_reader(stream)
        m_className = hkStringPtr.from_reader(stream)
        m_variant = hkRefVariant.from_reader(stream)
        return hkRootLevelContainer__NamedVariant(m_name, m_className, m_variant)    
    
class hkArrayList(List):
    pass

@dataclass
class hkArray:
    _offset: int #u64
    offset: Ptr # 8 bytes
    m_size: hkInt # 4 bytes
    m_capacityAndFlags: int #s32
    
    _m_data_type: Optional[Type] = None
    _sec_element_type: Optional[Type] = None
    m_data: hkArrayList = field(default_factory=list)
    
    def __repr__(self):
        return f"<hkArray offset=0x{self.offset.value:04X} size={self.m_size.value} capacity={self.m_capacityAndFlags}>"
    
    @staticmethod
    def get_default_array(stream: ReadStream,element_type: Type[T], _sec_element_type: Type[T]=None):
        stream.align_to(8)  # std::mem::AlignTo<0x8>
        _offset = stream.tell()
        offset = Ptr.from_reader(stream)
        m_size = hkInt.from_reader(stream)
        m_capacityAndFlags = stream.read_s32()
        return hkArray(_offset, offset, m_size, m_capacityAndFlags, _m_data_type=element_type,_sec_element_type=_sec_element_type)
        
    
    @staticmethod
    def from_reader(stream: 'ReadStream', element_type: Type[T], _sec_element_type: Type[T]=None) -> 'hkArray':
        """Reads an hkArray<T> from the stream."""
        # stream.align_to(8)  # std::mem::AlignTo<0x8>
        array = hkArray.get_default_array(stream, element_type, _sec_element_type)
        current_pos = stream.tell()
        new_offset = array.offset.offset + array.offset.value
        stream.seek(new_offset)

        i = 0
        while i < array.m_size.value:
            i += 1
            if _sec_element_type is None:
                item = element_type.from_reader(stream)
            else:
                item = element_type.from_reader(stream, _sec_element_type)
            array.m_data.append(item)
        stream.seek(current_pos)
        return array
    
    def to_binary(self) -> bytes:
        """Converts the array to a binary representation."""
        writer = WriteStream()
        writer.write(self.offset.to_binary())
        writer.write(self.m_size.to_binary())
        writer.write_s32(self.m_capacityAndFlags)
        for item in self.m_data:
            writer.write(item.to_binary())
        return writer.getvalue()
    
    def write(self, stream: WriteStream):
        """Writes the array to the stream."""
        stream.write(self.to_binary())
        
    
@dataclass
class hkRootLevelContainer:
    m_namedVariants: hkArray = field(default_factory=list)
    cloth_offset: int = 0 # u128
    resource_offset: int = 0# u128
    animation_offset: int = 0# u128
    
    @staticmethod  
    def from_reader(stream: ReadStream) -> 'hkRootLevelContainer':
        """Reads a root level container from the stream."""
        stream.align_to(8)
        m_namedVariants = hkArray.from_reader(stream, hkRootLevelContainer__NamedVariant)
        
        for hkVar in m_namedVariants.m_data:
            assert(isinstance(hkVar, hkRootLevelContainer__NamedVariant))
            addr = hkVar.m_variant._offset + hkVar.m_variant.m_ptr.value + stream.data_offset
            match hkVar.m_className._str:
                case "hclClothContainer":
                    cloth_offset = addr
                case "hkMemoryResourceContainer":
                    resource_offset = addr
                case "hkaAnimationContainer":
                    animation_offset = addr
                case _:
                    raise ValueError(f"Unknown class name: {hkVar.m_className._str}")
        
        return hkRootLevelContainer(m_namedVariants, cloth_offset, resource_offset, animation_offset)
    


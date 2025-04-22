import copy
import io
from typing import Any, List, Optional, Type, TypeVar, get_origin, get_type_hints
import uuid
from Stream import ReadStream, WriteStream
from dataclasses import dataclass, field, fields, is_dataclass

from util import BphclBaseObject, OffsetInfo, _hex, hexInt

T = TypeVar('T')  # Generic type variable for element type


@dataclass
class hkInt(BphclBaseObject): #Int
    value: int #s32
    offset: int
    
    def __repr__(self):
        return f"hkInt({self.value}, offset={_hex(self.offset)})"
    
    def to_stream(self, stream: WriteStream):
        """Writes the integer to the stream."""
        stream.write_s32(self.value)
    
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
class Ptr(BphclBaseObject):
    value:int #u64 if > 0 else s64
    offset:int #u64 stream.tell()
    
    _offsets_range: list[tuple[hexInt, hexInt]] = field(default_factory=list)
    
    def __repr__(self):
        return f"Ptr(val={_hex(self.value)}, offset={_hex(self.offset)})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'Ptr':
        """Reads a pointer from the stream."""
        offset = stream.tell() #- stream.data_offset
        value = stream.read_64bit_int()
        # if [Ptr(val=0x57D0, offset=0x3F8)]
        # if (offset + value) in list(range(0x7c40, 0x91c1 + 1)) and value >=0:
        #     pass
        _offsets_range = [(hexInt(offset), hexInt(stream.tell()))]
        return Ptr(value, offset, _offsets_range)
    
    def to_binary(self) -> bytes:
        """Converts the pointer to a binary representation."""
        writer = WriteStream()
        writer.write_64bit_int(self.value)
        return writer.getvalue()
    
    def write(self, stream: WriteStream):
        """Writes the pointer to the stream."""
        stream.write(self.to_binary())
    
    def to_stream(self, stream: WriteStream):
        """Writes the pointer to the stream."""
        stream.write_64bit_int(self.value)

@dataclass
class hkRefVariant(BphclBaseObject):
    m_ptr: Ptr
    _offset: int 
    
    _align_val: int = 8
    
    def __repr__(self):
        return f"hkRefVariant(0x{self.m_ptr.value:08X},0x{self.m_ptr.offset:08X})"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkRefVariant':
        """Reads a reference variant from the stream."""
        stream.align_to(hkRefVariant._align_val)  # std::mem::AlignTo<0x8>
        _offset = stream.tell()
        m_ptr = Ptr.from_reader(stream)
        current_pos = stream.tell()
        
        return hkRefVariant(m_ptr, _offset)

    def to_stream(self, stream: WriteStream):
        """Writes the reference variant to the stream."""
        stream._writer_align_to(hkRefVariant._align_val)
        self.m_ptr.to_stream(stream)
        cur_position = stream.tell()
        true_offset = hexInt(self.m_ptr.value + self.m_ptr.offset)
        stream.seek(true_offset, io.SEEK_SET)
        #nothing to write
        stream.seek(cur_position, io.SEEK_SET)
        

@dataclass
class hkStringPtr(BphclBaseObject): # size 8 bytes
    _offset: int #u64
    m_stringAndFlag: Ptr
    _str: str = ''
    
    _offsets_range: list[tuple[hexInt, hexInt]] = field(default_factory=list)
    _align_val: int = 8
    
    def __repr__(self):
        return f"hkStringPtr({repr(self._str)})"
    
    def to_stream(self, stream: WriteStream):
        """Writes the string pointer to the stream."""
        stream._writer_align_to(hkStringPtr._align_val)
        self.m_stringAndFlag.to_stream(stream)
        #experimental: write string, may be redundant or dangerous
        # but assuming all offsets check out
        cur_position = stream.tell()
        true_offset = hexInt(self.m_stringAndFlag.value + self.m_stringAndFlag.offset)
        stream.seek(true_offset, io.SEEK_SET)
        stream.write_string(self._str)
        stream.seek(cur_position, io.SEEK_SET)
    
    @staticmethod
    def get_hkStringPtr_values(instance) -> dict[str, "hkStringPtr"]:
        """Return list of values from the dataclass instance that are of type hkStringPtr."""
        if not is_dataclass(instance):
            raise ValueError("Provided object is not a dataclass instance.")

        cls = type(instance)
        hints = get_type_hints(cls)

        return {
            field.name : getattr(instance, field.name)
            for field in fields(instance)
            if hints.get(field.name) == hkStringPtr
        }
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkStringPtr':
        """Reads a string pointer from the stream."""
        _offsets_range = []
        stream.align_to(hkStringPtr._align_val)
        _offset = stream.tell()
        m_stringAndFlag = Ptr.from_reader(stream)
        string_address = m_stringAndFlag.value + _offset #+ stream.data_offset

        current_pos = stream.tell()
        stream.seek(string_address)
        string_data = stream.read_string()
        _offsets_range.append((string_address, stream.tell()))
        stream.seek(current_pos)
        _offsets_range.extend(m_stringAndFlag._offsets_range)
        _offsets_range.insert(0, (_offset, stream.tell()))
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
class hkRootLevelContainer__NamedVariant(BphclBaseObject):
    m_name: hkStringPtr    
    m_className: hkStringPtr    
    m_variant: hkRefVariant
    
    _align_val: int = 8
    
    def to_stream(self, stream: WriteStream):
        """Writes the named variant to the stream."""
        stream._writer_align_to(hkRootLevelContainer__NamedVariant._align_val)
        self.m_name.to_stream(stream)
        self.m_className.to_stream(stream)
        self.m_variant.to_stream(stream)
    
    def __repr__(self):
        n = f"m_name={repr(self.m_name._str)}" if self.m_name._str else f"m_name={self.m_name.m_stringAndFlag.value:04X}"
        cn = f"m_className={repr(self.m_className._str)}" if self.m_className._str else f"m_className={self.m_className.m_stringAndFlag.value:04X}"
        v = f"m_variant=0x{self.m_variant.m_ptr.value:04X}"
        return f"<hkvar {n} {cn} {v}>"
    
    @staticmethod
    def from_reader(stream: ReadStream) -> 'hkRootLevelContainer__NamedVariant':
        """Reads a named variant from the stream."""
        stream.align_to(hkRootLevelContainer__NamedVariant._align_val)
        m_name = hkStringPtr.from_reader(stream)
        m_className = hkStringPtr.from_reader(stream)
        m_variant = hkRefVariant.from_reader(stream)
        return hkRootLevelContainer__NamedVariant(m_name, m_className, m_variant)    
    
class hkArrayList(List):
    pass

@dataclass
class hkArray(BphclBaseObject):
    _offset: int #u64
    offset: Ptr # 8 bytes
    m_size: hkInt # 4 bytes
    m_capacityAndFlags: int #s32
    
    _m_data_type: Optional[Type] = None
    _sec_element_type: Optional[Type] = None
    m_data: hkArrayList = field(default_factory=list)
    
    _uuid: str = str(uuid.uuid4())
    # offsets in stream where data is stored
    _offsets_range: list[tuple[int, int]] = field(default_factory=list)
    _align_val: int = 8
    
    # def update_item_offsets(self, tag, str_offset, str_offset_old, ptr, true_offset):
    #     """Moved items require updating ITEM and PTCH patches and sections"""
    #     ptch_section_index, ptch_section = tag.get_ptch_section()
    #     item_section_index, item_section = tag.get_item_section()
    #     backup: OffsetInfo = copy.deepcopy(tag._indexes[str_offset_old])
    #     int_offset = int(str_offset)
    #     int_offset_old = int(str_offset_old)
    #     del tag._indexes[str_offset_old]
    #     #item: TBD
    #     index = backup.index
    #     # final_offset = ptr.value + ptr.offset
    #     # final_offset = true_offset + int_offset
    #     final_offset = true_offset
    #     tag.indx_section.sections[item_section_index].items[index].data_offset = final_offset
    #     backup.item.data_offset = final_offset
    #     #internal_patch
    #     assert(int_offset_old in backup.internal_patch.offsets)
    #     backup.internal_patch.offsets.remove(int_offset_old)
    #     backup.internal_patch.offsets.append(int_offset)
    #     backup.internal_patch.offsets.sort()
    #     i = backup.internal_patch_index
    #     tag.indx_section.sections[ptch_section_index].internal_patches[i] = backup.internal_patch
    #     # assign to _indexes
    #     backup.offset = int_offset
    #     tag._indexes[str_offset] = backup
        
        
    
    # def move_items(self, new_offset:int, tag):
    #     if str(type(self.m_data[0])) == "<class 'hkRefPtr'>":
    #         raise ValueError(f"Cannot move items in a list for type {type(self.m_data[0])}")
    #     root_offset = int(self.offset.offset)
    #     true_offset = self.offset.value + self.offset.offset
    #     self.offset.value = new_offset - self.offset.offset # assuming list is moved forward
    #     for i, item in enumerate(self.m_data):
    #         str_offset_old = str(item.m_ptr.offset)
    #         true_offset = item.m_ptr.value + item.m_ptr.offset
    #         item.m_ptr.value = true_offset - new_offset # assuming list is moved forward
    #         item.m_ptr.offset = new_offset # assuming list is moved forward
    #         if  new_offset in tag._indexes:
    #             self.update_item_offsets(tag, str(new_offset), str_offset_old, item.m_ptr, true_offset)
    #         self.m_data[i] = item
    #         assert(item.m_ptr.value + item.m_ptr.offset) == true_offset, f"Item {i} offset mismatch: {item.m_ptr.value + item.m_ptr.offset} != {ind}"
    #         new_offset += 8
    #     # Adjusting tag
    #     # ptch_section_index, ptch_section = tag.get_ptch_section()
    #     # type_name_index, type_name_section = tag.get_type_section(b"TNA1")
    #     item_section_index, item_section = tag.get_item_section()
    #     str_offset = str(root_offset)
    #     #item
    #     index = tag._indexes[str_offset].index
    #     final_offset = self.offset.value + root_offset
    #     tag.indx_section.sections[item_section_index].items[index].data_offset = final_offset
    #     tag._indexes[str_offset].item.data_offset = final_offset
    #     #internal_patch: TBD
    #     # tag.indx_section.sections[item_section_index].sections[index].data_offset = self.offset.value
    #     return
    
    def to_stream(self, stream: WriteStream):
        """Writes the array to the stream."""
        stream._writer_align_to(hkArray._align_val)
        self.offset.to_stream(stream)
        self.m_size.to_stream(stream)
        stream.write_s32(self.m_capacityAndFlags)
        # the items may lay somewhere else in the stream, so we need to write them separately
        cur_position = stream.tell()
        true_offset = hexInt(self.offset.value + self.offset.offset)
        stream.seek(true_offset, io.SEEK_SET)
        for item in self.m_data:
            item.to_stream(stream)
        stream.seek(cur_position, io.SEEK_SET)
        
        
    def __repr__(self):
        # return f"<hkArray offset=0x{self.offset.value:04X} size={self.m_size.value} capacity={self.m_capacityAndFlags}>"
        t1,t2 = None, None
        t1 = self._m_data_type.__name__ if self._m_data_type is not None else None
        t2 = self._sec_element_type.__name__ if self._sec_element_type is not None else None
        t = t2 if t2 else t1
        t3 = f" ({t1})" if t2 else ""
        return f"<hkArray type={t}{t3} size={self.m_size.value} at {_hex(self.offset.value)} >"
    
    def _contains(self, item: T) -> bool:
        return self._get_item_index(item) != -1
    
    def _get_item_index(self, item:T):
        _names = hkArray.get_string_names_from_instance(item)
        for _name in _names:
            for i, _item in enumerate(self.m_data):
                _item_name = hkArray.get_string_name_from_instance(_item)
                if _item_name == _name:
                    return i
        return -1
            
    
    def _append(self, item: T):
        if self._m_data_type is not None and not isinstance(item, self._m_data_type):
            raise TypeError(f"Expected {self._m_data_type}, got {type(item)}")
        self.m_data.append(item)
        self.m_size.value += 1
    
    @staticmethod
    def get_default_array(stream: ReadStream,element_type: Type[T], _sec_element_type: Type[T]=None):
        stream.align_to(hkArray._align_val)  # std::mem::AlignTo<0x8>
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
        array._offsets_range.append((hexInt(array._offset), hexInt(current_pos)))
        new_offset = array.offset.offset + array.offset.value
        stream.seek(new_offset)

        for _ in range(array.m_size.value):
            if _sec_element_type is None:
                item = element_type.from_reader(stream)
            else:
                item = element_type.from_reader(stream, _sec_element_type)
            array.m_data.append(item)
        array._offsets_range.append((hexInt(new_offset), hexInt(stream.tell())))
        stream.seek(current_pos)
        return array
    
    def to_binary(self) -> bytes:
        """Converts the array to a binary representation."""
        writer = WriteStream()
        writer.write(self.offset.to_binary())
        writer.write(self.m_size.to_binary())
        writer.write_s32(self.m_capacityAndFlags)
        # the items may lay somewhere else in the stream, so we need to write them separately
        # for item in self.m_data:
        #     writer.write(item.to_binary())
        return writer.getvalue()
    
        
        
    def write(self, stream: WriteStream):
        """Writes the array to the stream."""
        stream.write(self.to_binary())
    
    def write(self, writer: WriteStream):
        """Writes the array to the stream."""
        cur_position = writer.tell()
        writer.write_u64(0) # Ptr placeholder
        writer.write_s32(len(self.m_data)) # m_size placeholder
        writer.write(self.to_binary())
    
@dataclass
class hkRootLevelContainer(BphclBaseObject):
    m_namedVariants: hkArray = field(default_factory=list)
    cloth_offset: int = 0 # u128
    resource_offset: int = 0# u128
    animation_offset: int = 0# u128
    
    _align_val: int = 8
    
    def to_stream(self, stream: WriteStream):
        """Writes the root level container to the stream."""
        stream._writer_align_to(hkRootLevelContainer._align_val)
        self.m_namedVariants.to_stream(stream)
        stream.seek_end()
        stream._writer_align_to(hkRootLevelContainer._align_val)
        return
    
    @staticmethod  
    def from_reader(stream: ReadStream) -> 'hkRootLevelContainer':
        """Reads a root level container from the stream."""
        stream.align_to(hkRootLevelContainer._align_val)
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
    


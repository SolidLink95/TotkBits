import copy
from dataclasses import dataclass
import io
from pathlib import Path
from BphclEnums import ResTypeSectionSignature
from BphclMainDataclasses import ResIndexSection, ResSection, ResTypeSection
from BphclSmallDataclasses import *
from Experimental import hclClothContainer, hclCollidable, hkMemoryResourceContainer, hkaAnimationContainer
from Havok import hkRootLevelContainer, hkStringPtr
from Stream import ReadStream, WriteStream
from util import  FilteredInternalPath, OffsetInfo, _hex,AssetsFromRange, find_nested_instances, is_offset_in_ranges_list

DATA_START_OFFSET = 0x50
    
@dataclass
class ResTagFile(ResTagfileSectionHeader):
    sdkv_section: ResSection = None
    data_section: ResSection = None
    type_section: ResSection = None
    indx_section: ResSection = None
    root_container: hkRootLevelContainer = None
    #Cloth sections
    cloth_container: hclClothContainer = None
    mem_resource_container: hkMemoryResourceContainer = None
    animation_container: hkaAnimationContainer = None
    #helper fields
    _type_strings: List[str] = None
    _field_strings: List[str] = None
    _indexes = {}
    _data_fixed: bytes = None
    
    
    def update_data_binary(self):
        # stream = WriteStream((self.data_section.size.size - 8) * b"\x00")
        # stream = WriteStream()
        # stream = WriteStream(self.data_section.data)
        stream = WriteStream(self.data_section._data_fixed)
        self.root_container.to_stream(stream)
        stream.seek(self.root_container.cloth_offset, 0)
        self.cloth_container.to_stream(stream)
        stream.seek(self.root_container.resource_offset, 0)
        self.mem_resource_container.to_stream(stream)
        stream.seek(self.root_container.animation_offset, 0)
        self.animation_container.to_stream(stream)
        new_data = stream.getvalue()
        Path("tmp/DATA_new.bin").write_bytes(new_data)
        new_size = Size(1, len(new_data))
        # new_section_data = new_size.to_binary() + b"DATA" + new_data
        self.data_section._data_fixed = new_data
        new_section_data = self.revert_relocations()
        print(new_section_data == self.data_section.data)
        s1, s2 = len(new_section_data), len(self.data_section.data)
        print(s1, s2)
        size = s1 if s1 < s2 else s2
        # print([hexInt(i+0x50) for i in range(size) if new_section_data[i] != self.data_section.data[i]])
        # print([hexInt(new_section_data[i]) for i in range(size) if new_section_data[i] != self.data_section.data[i]])
        Path("tmp/DATA.bin").write_bytes(new_section_data)
        self.data_section.data = new_section_data
    
    def validate(self):
        if self.data_section is None:
            raise ValueError("DATA section not found")
        if self.type_section is None:
            raise ValueError("TYPE section not found")
        if self.indx_section is None:
            raise ValueError("INDX section not found")
        if self.sdkv_section.version is not None:
            assert(self.sdkv_section.version == '20220100'), f"Invalid version for SDKV: {self.sdkv_section.version}, expected '20220100'"
        
    def to_binary(self) -> bytes:
        stream = WriteStream()
        # result = self.size.to_binary()
        stream.write_padding(4) # size
        stream.write(self.signature)
        self.sdkv_section.to_stream(stream)
        self.data_section.to_stream(stream)
        self.type_section.to_stream(stream)
        self.indx_section.to_stream(stream)
        stream.seek_end()
        new_size = stream.tell() - 8
        _size = Size(is_chunk=self.size.is_chunk, size=new_size)
        stream.seek_start()
        stream.write(_size.to_binary())
        stream.seek_start()
        return stream.getvalue()
    
    def read_havok(self, stream: ReadStream):
        reader = ReadStream(self.data_section._data_fixed) #if stream is None else stream
        writer = WriteStream(self.data_section._data_fixed) #if stream is None else stream
        reader.data_offset = self.data_section.data_offset
        stream = ReadStream(self.data_section._data_fixed) #if stream is None else stream
        self.root_container = hkRootLevelContainer.from_reader(reader)
        reader.seek(self.root_container.cloth_offset - self.data_section.data_offset, io.SEEK_SET)
        self.cloth_container = hclClothContainer.from_reader(reader)
        reader.seek(self.root_container.resource_offset- self.data_section.data_offset, io.SEEK_SET)
        self.mem_resource_container = hkMemoryResourceContainer.from_reader(reader, is_root=True)
        reader.seek(self.root_container.animation_offset- self.data_section.data_offset, io.SEEK_SET)
        self.animation_container = hkaAnimationContainer.from_reader(reader)
        return
        
    def update_fields(self, stream: ReadStream):
        self.validate()
        self.get_helper_strings()
        self.update_strings()
        self.handle_relocations()
        self.read_havok(stream)

    @staticmethod
    def from_reader(stream: ReadStream) -> "ResTagFile":
        size = Size.from_reader(stream)
        signature = stream._read_exact(4)
        if signature != b"TAG0":
            raise ValueError(f"Invalid ResTagFile signature: {signature}, expected b'TAG0' at {stream.tell():08X}")
        res = ResTagFile(size=size, signature=signature)
        main_offset = stream.tell() - 8
        while stream.tell() - main_offset <= size.size - 1:
            _offset = stream.tell()
            res_section = ResSection.from_reader(stream)
            match res_section.signature:
                case "SDKV":
                    res.sdkv_section = res_section
                case "DATA":
                    res.data_section = res_section
                case "TYPE":
                    res.type_section = res_section
                case "INDX":
                    res.indx_section = res_section
                case _:
                    pass # error handling already done
            if res.data_section is not None and res.type_section is not None and res.indx_section is not None:
                if stream.tell() - main_offset <= size.size - 1:
                    raise ValueError(f"Invalid ResTagFile structure: DATA, TYPE and INDX sections found but not all of data has been read")
        res.update_fields(stream)
        return res

    
    # Getters
    def get_type_section(self, section_type: ResTypeSectionSignature|bytes) -> tuple[int, ResTypeSection]:
        for i, section in enumerate(self.type_section.sections):
            if isinstance(section, ResTypeSection):
                if isinstance(section_type, ResTypeSectionSignature):
                    _check = ResTypeSection.signature_to_enum(section.signature) == section_type
                else:
                    _check = section.signature == section_type
                if _check:
                    return (i, section)
        raise ValueError(f"Section {section_type} not found in TYPE section")
    
    def get_tst1_section(self):
        return self.get_type_section(b"TST1")
    
    def get_fst1_section(self):
        return self.get_type_section(b"FST1")
        
    def get_helper_strings(self):
        _, tst1_section = self.get_tst1_section()
        self._type_strings = tst1_section.strings
        _, fst1_section = self.get_fst1_section()
        self._field_strings = fst1_section.strings
    
    def get_indx_section_by_signature(self, signature: bytes) -> tuple[int, ResIndexSection]:
        for i, section in enumerate(self.indx_section.sections):
            if isinstance(section, ResIndexSection) and section.signature == signature:
                return (i, section)
        raise ValueError(f"Subsection {signature} not found in TYPE section")
    
    def get_item_section(self):
        return self.get_indx_section_by_signature(b"ITEM")

    def get_ptch_section(self):
        return self.get_indx_section_by_signature(b"PTCH")
    
    def update_strings(self):
        # tst1_section: ResTypeSection = next((s for s in self.type_section.sections if ResTypeSection.signature_to_enum(s.signature)==ResTypeSectionSignature.TST1), None)
        _, tst1_section = self.get_tst1_section()
        for i, section in enumerate(self.type_section.sections):
            if not isinstance(section, ResTypeSection):
                raise ValueError(f"Section {i} inside TYPE is not a ResTypeSection")
            self.type_section.sections[i].update_string_names(tst1_section)
    
    def handle_relocations(self):
        ptch_section_index, ptch_section = self.get_ptch_section()
        type_name_index, type_name_section = self.get_type_section(b"TNA1")
        item_section_index, item_section = self.get_item_section()
        data_stream_reader = ReadStream(self.data_section.data)
        data_stream_writer = WriteStream(self.data_section.data)
        data_stream_writer.seek(0, io.SEEK_SET)
        internal_patch_index = -1
        def inplace_fixup(offset: int, internal_patch: int): #u32, u32=
            # tmp = hexInt(offset + 0x50)
            if offset == 248:
                pass
            
            type_index = internal_patch.type_index
            data_stream_reader.seek(offset, io.SEEK_SET)
            # index_tmp = data_stream_reader.read_u64()
            # data_stream_reader.seek(offset, io.SEEK_SET)
            index = data_stream_reader.read_u32()
            # assert(index == index_tmp, f"Invalid index: {index} != {index_tmp}")
            # self._indexes[str(int(offset))] = index
            _item = item_section.items[index]
            _named_type = None
            
            if type_index > 0:
                _named_type = type_name_section.types[type_index-1]
                _name = _named_type.name
                if not _name:
                    raise ValueError(f"Update strings fields for type_name_section: {repr(_name)}")
                if _name.startswith(("hkArray",)):
                    # assert _named_type.index._byte0 == 1, f"Invalid type name: {_name}"
                    data_stream_writer.seek(offset + 8, io.SEEK_SET)
                    tmp1 = data_stream_writer.read_s32()
                    assert(tmp1 == 0) # always true
                    data_stream_writer.seek(offset + 8, io.SEEK_SET)
                    data_stream_writer.write_s32(_item.count)
            addr = _item.data_offset - offset # u32 as u64
            data_stream_writer.seek(offset, io.SEEK_SET)
            data_stream_writer.write_64bit_int(addr) # ptr
            addr &= 0xFFFFFFFFFFFFFFFF 
            offset_info = OffsetInfo(offset, index, _item, internal_patch_index, internal_patch, _named_type, addr, hexInt(offset + DATA_START_OFFSET))
            self._indexes[str(int(offset))] = offset_info
        
        for i, internal_patch in enumerate(ptch_section.internal_patches):
            internal_patch_index = i
            for j, _offset in enumerate(internal_patch.offsets):
                inplace_fixup(hexInt(_offset), internal_patch)
                # inplace_fixup(_offset, internal_patch.type_index)
        data_stream_writer.seek(0, io.SEEK_SET)
        self.data_section._data_fixed = data_stream_writer.read()
        # self.revert_relocations()        
    
    def revert_relocations(self) -> bytes:
        """Revert relocations to original values"""
        ptch_section_index, ptch_section = self.get_ptch_section()
        stream = WriteStream(self.data_section._data_fixed)
        for str_offset, info in self._indexes.items():
            if str_offset == "248":
                pass
            stream.seek(info.offset)
            stream.write_64bit_int(info.index)
            if info.named_type is not None and info.named_type.name.startswith("hkArray"):
                stream.seek(info.offset + 8)
                stream.write_u64(0)
        stream.seek_start()
        new_data = stream.read()
        return new_data
    
    
    
    
    def test_get_assets_from_offsets_ranges(self, offsets: list[tuple[int, int]]):
        ptch_section_index, ptch_section = self.get_ptch_section()
        type_name_index, type_name_section = self.get_type_section(b"TNA1")
        item_section_index, item_section = self.get_item_section()
        stream = WriteStream(self.data_section.data)
        stream.seek(0, io.SEEK_SET)
        assets = AssetsFromRange()
        for i, internal_patch in enumerate(ptch_section.internal_patches):
            filtered_patch = None
            if any([is_offset_in_ranges_list(offset, offsets) for offset in internal_patch.offsets]):
                filtered_patch = FilteredInternalPath(index=i, internalPatch=internal_patch)
                for j, offset in enumerate(internal_patch.offsets):
                    type_index = internal_patch.type_index
                    stream.seek(offset, io.SEEK_SET)
                    index = stream.read_u32()
                    _item = item_section.items[index]
                    if is_offset_in_ranges_list(offset, offsets):
                        assets.items.append((index, _item))
                        if offset in internal_patch.offsets:
                            filtered_patch.offsets.append(offset)
                    if type_index > 0:
                        _named_type = type_name_section.types[type_index-1]
                        if _named_type.name.startswith(("hkArray",)):
                            stream.seek(offset + 8, io.SEEK_SET)
                            if is_offset_in_ranges_list(offset+8, offsets):
                                assets.named_types.append((type_index-1, _named_type))
                                if (offset+8) in internal_patch.offsets:
                                    filtered_patch.offsets.append(offset+8)
                    addr = _item.data_offset - offset # u32 as u64
                assets.internal_patches.append(filtered_patch)
                # NOT all offsets are in the filtered patch!
                # assert(len(filtered_patch.offsets) == len(internal_patch.offsets)), f"Invalid filtered patch offsets: {filtered_patch.offsets} != {internal_patch.offsets}"
        assets.sort()
        return assets

    
    def test_add_collidable(self, collidables: List[hclCollidable], other_bphcl: "ResTagFile"):
        """test for collidables"""
        collidable = collidables[0]
        m_collidables = self.cloth_container.m_collidables
        m_collidables_names = [e.m_data.m_name._str for e in m_collidables.m_data]
        assert(len(m_collidables_names)==len(list(set(m_collidables_names)))), f"Duplicates collidables found! File corrupted!"
        assert(len(m_collidables._offsets_range) == 2)
        aff_assets = self.get_assets_from_offsets_ranges(m_collidables._offsets_range)
        aff_data_assets = self.get_assets_from_offsets_ranges(m_collidables._offsets_range[1:])
        aff_off_assets = self.get_assets_from_offsets_ranges(m_collidables._offsets_range[0:1])
        stream_f = WriteStream(self.data_section._data_fixed)
        stream = WriteStream(self.data_section.data)
        
        #loop starts here
        stream_f_other = WriteStream(other_bphcl.data_section._data_fixed)
        stream_other = WriteStream(other_bphcl.data_section.data)
        
        coll_assets = other_bphcl.get_assets_from_offsets_ranges(collidable._offsets_range)
        
        if len(coll_assets):
            pass  # do something
        #check if exists in the file
        if collidable.m_name._str in m_collidables_names:
            print(f"Collidable {collidable.m_name._str} already exists in the file")
        new_collidable = copy.deepcopy(collidable)
        # look for hclShape - actually skip that, append the hclShape to buffer
        stream_f.seek_end()
        cur_pos_new_hkrefptr_shape = stream_f.tell()
        stream_f.write_s64(-(0xe0))
        stream_f.write(new_collidable.m_shape.m_data.to_binary())
        stream_f.seek(cur_pos_new_hkrefptr_shape, io.SEEK_SET)
        print(new_collidable)
        new_hkrefptr_shape = hkRefPtr.from_reader(stream_f, hclShape)
        # m_name - assuming it is absent in original file
        stream_f.seek_end()
        stream_f.write_u32(0) # needed, new_strptr fails otherwise TODO: check why
        cur_pos_new_strptr = stream_f.tell()
        stream_f.write_u64(8)
        stream_f.write_string(new_collidable.m_name._str)
        stream_f.seek(cur_pos_new_strptr, io.SEEK_SET)
        stream_f.align_to(8)
        stream_f.seek(cur_pos_new_strptr, io.SEEK_SET)
        new_strptr = hkStringPtr.from_reader(stream_f)
        #assign new fields values
        # new_hkrefptr_shape.m_ptr.offset = -(0xe0) 
        new_collidable.m_shape = new_hkrefptr_shape
        new_collidable.m_name = new_strptr
        stream_f.seek_end()
        # write new collidable to the end of the stream
        # collidable_offset = stream_f.tell()
        # stream_f.write(new_collidable.to_binary())
        #write hkRefPtr of the new collidable
        stream_f.seek_end()
        cur_pos_new_collidable = stream_f.tell()
        stream_f.write_64bit_int(8)
        new_collidable.to_stream(stream_f)
        # stream_f.write(new_collidable.to_binary())
        with open("tmp/tmp.bin", "wb") as f:
            f.write(stream_f.getvalue())
        stream_f.seek(cur_pos_new_collidable, io.SEEK_SET)
        new_coll_hkrefptr = hkRefPtr.from_reader(stream_f, hclCollidable)
        m_collidables.m_data._append(new_coll_hkrefptr)
        #End of collidables loop, now move the hkArray of collidables
        stream_f.seek_end()
        new_hkarray_offset = stream_f.tell()
        relative_offset = new_hkarray_offset - m_collidables.offset.offset
        new_Ptr = Ptr(value=relative_offset, offset=new_hkarray_offset, size=8, type=0x1)
        
        return
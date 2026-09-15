//! Builds a BPHCL (hk2022 tagfile inside a Phive container) from scratch,
//! object by object, using the member layouts of a TYPE section: the
//! converter allocates typed items, writes members by name and links them
//! with relocations, and `finish` assembles header, TAG0 and the AAMP block.
//!
//! Conventions measured on the vanilla corpus: objects referenced by a
//! pointer carry ITEM flags `0x10000000 | type`, array/string storage
//! `0x20000000 | type`; `hkArray` fields store the storage ITEM index with
//! size/capacity zero; every relocation is grouped under the type of the
//! pointer *field* (`hkStringPtr`, `hkArray<T>`, `T*<T>`, `hkRefPtr<T>`,
//! `hkRefVariant`); item #0 is the null item.

use super::{reflect::Reflect, BphclDocument, Item, Patch};
use crate::parser::binary::{BinaryWriter, Endian};
use std::io::{self, ErrorKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemRef(pub usize);

pub struct ObjectWriter<'a> {
    reflect: Reflect<'a>,
    data: Vec<u8>,
    items: Vec<Item>,
    patches: Vec<Patch>,
}

const OBJECT_FLAGS: u32 = 0x1000_0000;
const STORAGE_FLAGS: u32 = 0x2000_0000;

impl<'a> ObjectWriter<'a> {
    /// `types` only supplies the TYPE table (its DATA is never read).
    pub fn new(types: &'a BphclDocument) -> io::Result<Self> {
        let reflect =
            Reflect::new(types).ok_or_else(|| invalid("type donor BPHCL has no DATA section"))?;
        Ok(Self {
            reflect,
            data: Vec::new(),
            items: vec![Item {
                flags: 0,
                type_index: 0,
                data_offset: 0,
                count: 0,
            }],
            patches: Vec::new(),
        })
    }

    pub fn reflect(&self) -> &Reflect<'a> {
        &self.reflect
    }

    pub fn type_index(&self, name: &str) -> io::Result<u32> {
        self.reflect
            .find_type(name)
            .ok_or_else(|| invalid(&format!("type table has no bodied type {name}")))
    }

    pub fn size_of(&self, type_index: u32) -> u32 {
        self.reflect.size_of(type_index)
    }

    pub fn member_offset(&self, type_index: u32, name: &str) -> io::Result<u32> {
        self.reflect.member_offset(type_index, name).ok_or_else(|| {
            invalid(&format!(
                "type {} has no member {name}",
                self.reflect.type_name(type_index)
            ))
        })
    }

    pub fn member_type(&self, type_index: u32, name: &str) -> io::Result<u32> {
        self.reflect
            .member(type_index, name)
            .map(|member| member.type_index)
            .ok_or_else(|| {
                invalid(&format!(
                    "type {} has no member {name}",
                    self.reflect.type_name(type_index)
                ))
            })
    }

    /// Relocation group type for an `hkArray<element>` field.
    pub fn array_patch_type(&self, element_type: u32) -> io::Result<u32> {
        self.reflect.array_type_for(element_type).ok_or_else(|| {
            invalid(&format!(
                "type table has no hkArray<{}>",
                self.reflect.type_name(element_type)
            ))
        })
    }

    /// Relocation group type for a `T*<element>` pointer.
    pub fn pointer_patch_type(&self, element_type: u32) -> io::Result<u32> {
        self.reflect
            .template_type("T*", element_type)
            .ok_or_else(|| {
                invalid(&format!(
                    "type table has no T*<{}>",
                    self.reflect.type_name(element_type)
                ))
            })
    }

    pub fn ref_ptr_patch_type(&self, element_type: u32) -> io::Result<u32> {
        self.reflect
            .template_type("hkRefPtr", element_type)
            .ok_or_else(|| {
                invalid(&format!(
                    "type table has no hkRefPtr<{}>",
                    self.reflect.type_name(element_type)
                ))
            })
    }

    fn align(&mut self, alignment: usize) {
        while self.data.len() % alignment != 0 {
            self.data.push(0);
        }
    }

    fn push_item(&mut self, flags: u32, type_index: u32, count: u32) -> ItemRef {
        let index = self.items.len();
        self.items.push(Item {
            flags: flags | type_index,
            type_index,
            data_offset: self.data.len() as u32,
            count,
        });
        ItemRef(index)
    }

    /// Allocates `count` zeroed elements of `type_index`. Objects reached by a
    /// pointer (`object == true`) and array storage differ only in flags.
    pub fn alloc(&mut self, type_index: u32, count: u32, object: bool) -> io::Result<ItemRef> {
        let size = self.size_of(type_index);
        if size == 0 && count > 0 {
            return Err(invalid(&format!(
                "type {} has no size",
                self.reflect.type_name(type_index)
            )));
        }
        self.align(16);
        let item = self.push_item(
            if object { OBJECT_FLAGS } else { STORAGE_FLAGS },
            type_index,
            count,
        );
        self.data
            .resize(self.data.len() + (size as usize) * (count as usize), 0);
        Ok(item)
    }

    pub fn alloc_named(
        &mut self,
        type_name: &str,
        count: u32,
        object: bool,
    ) -> io::Result<ItemRef> {
        let type_index = self.type_index(type_name)?;
        self.alloc(type_index, count, object)
    }

    /// Allocates a NUL-terminated `char` item.
    pub fn string(&mut self, value: &str) -> io::Result<ItemRef> {
        let type_index = self.type_index("char")?;
        self.align(8);
        let bytes = value.as_bytes();
        let item = self.push_item(STORAGE_FLAGS, type_index, bytes.len() as u32 + 1);
        self.data.extend_from_slice(bytes);
        self.data.push(0);
        Ok(item)
    }

    pub fn item(&self, item: ItemRef) -> &Item {
        &self.items[item.0]
    }

    fn absolute(&self, item: ItemRef, offset: u32) -> usize {
        self.items[item.0].data_offset as usize + offset as usize
    }

    pub fn set_bytes(&mut self, item: ItemRef, offset: u32, bytes: &[u8]) -> io::Result<()> {
        let at = self.absolute(item, offset);
        let slot = self
            .data
            .get_mut(at..at + bytes.len())
            .ok_or_else(|| invalid("member write exceeds the item"))?;
        slot.copy_from_slice(bytes);
        Ok(())
    }

    pub fn set_u8(&mut self, item: ItemRef, offset: u32, value: u8) -> io::Result<()> {
        self.set_bytes(item, offset, &[value])
    }

    pub fn set_u16(&mut self, item: ItemRef, offset: u32, value: u16) -> io::Result<()> {
        self.set_bytes(item, offset, &value.to_le_bytes())
    }

    pub fn set_u32(&mut self, item: ItemRef, offset: u32, value: u32) -> io::Result<()> {
        self.set_bytes(item, offset, &value.to_le_bytes())
    }

    pub fn set_f32(&mut self, item: ItemRef, offset: u32, value: f32) -> io::Result<()> {
        self.set_bytes(item, offset, &value.to_le_bytes())
    }

    pub fn set_vec4(&mut self, item: ItemRef, offset: u32, value: [f32; 4]) -> io::Result<()> {
        for (i, component) in value.iter().enumerate() {
            self.set_f32(item, offset + i as u32 * 4, *component)?;
        }
        Ok(())
    }

    pub fn set_matrix4(&mut self, item: ItemRef, offset: u32, value: [f32; 16]) -> io::Result<()> {
        for (i, component) in value.iter().enumerate() {
            self.set_f32(item, offset + i as u32 * 4, *component)?;
        }
        Ok(())
    }

    fn add_patch(&mut self, patch_type: u32, absolute_offset: u32) {
        match self
            .patches
            .iter_mut()
            .find(|patch| patch.type_index == patch_type)
        {
            Some(group) => group.offsets.push(absolute_offset),
            None => self.patches.push(Patch {
                type_index: patch_type,
                offsets: vec![absolute_offset],
            }),
        }
    }

    /// Writes a relocated pointer (`item index`, upper half zero).
    pub fn set_pointer(
        &mut self,
        item: ItemRef,
        offset: u32,
        target: ItemRef,
        patch_type: u32,
    ) -> io::Result<()> {
        self.set_u32(item, offset, target.0 as u32)?;
        self.set_u32(item, offset + 4, 0)?;
        let absolute = self.absolute(item, offset) as u32;
        self.add_patch(patch_type, absolute);
        Ok(())
    }

    /// Writes an `hkArray` field: relocated storage pointer, size and
    /// capacity zero (the count lives in the ITEM).
    pub fn set_array(
        &mut self,
        item: ItemRef,
        offset: u32,
        storage: ItemRef,
        patch_type: u32,
    ) -> io::Result<()> {
        self.set_pointer(item, offset, storage, patch_type)?;
        self.set_u32(item, offset + 8, 0)?;
        self.set_u32(item, offset + 12, 0)
    }

    /// Writes a `hkStringPtr` member.
    pub fn set_string(&mut self, item: ItemRef, offset: u32, value: &str) -> io::Result<()> {
        let string_type = self.type_index("hkStringPtr")?;
        let storage = self.string(value)?;
        self.set_pointer(item, offset, storage, string_type)
    }

    /// Storage for an array of `T*<element>` (or `hkRefPtr<element>` when
    /// `ref_ptr`): 8 bytes per entry, every entry relocated.
    pub fn pointer_storage(
        &mut self,
        element_type: u32,
        targets: &[ItemRef],
        ref_ptr: bool,
    ) -> io::Result<ItemRef> {
        let entry_type = if ref_ptr {
            self.ref_ptr_patch_type(element_type)?
        } else {
            self.pointer_patch_type(element_type)?
        };
        self.align(16);
        let storage = self.push_item(STORAGE_FLAGS, entry_type, targets.len() as u32);
        self.data.resize(self.data.len() + targets.len() * 8, 0);
        for (k, target) in targets.iter().enumerate() {
            self.set_pointer(storage, k as u32 * 8, *target, entry_type)?;
        }
        Ok(storage)
    }

    /// Convenience: allocate storage for scalar elements and fill it.
    pub fn scalar_storage<T: Copy>(
        &mut self,
        type_name: &str,
        values: &[T],
        encode: impl Fn(T) -> Vec<u8>,
    ) -> io::Result<ItemRef> {
        let type_index = self.type_index(type_name)?;
        let size = self.size_of(type_index);
        self.align(16);
        let item = self.push_item(STORAGE_FLAGS, type_index, values.len() as u32);
        self.data
            .resize(self.data.len() + size as usize * values.len(), 0);
        for (k, value) in values.iter().enumerate() {
            let bytes = encode(*value);
            self.set_bytes(item, k as u32 * size, &bytes[..size as usize])?;
        }
        Ok(item)
    }

    pub fn u16_storage(&mut self, values: &[u16]) -> io::Result<ItemRef> {
        self.scalar_storage("hkUint16", values, |v| v.to_le_bytes().to_vec())
    }

    pub fn i16_storage(&mut self, values: &[i16]) -> io::Result<ItemRef> {
        self.scalar_storage("hkInt16", values, |v| v.to_le_bytes().to_vec())
    }

    pub fn u32_storage(&mut self, values: &[u32]) -> io::Result<ItemRef> {
        self.scalar_storage("hkUint32", values, |v| v.to_le_bytes().to_vec())
    }

    pub fn uint_storage(&mut self, values: &[u32]) -> io::Result<ItemRef> {
        self.scalar_storage("unsigned int", values, |v| v.to_le_bytes().to_vec())
    }

    pub fn i32_storage(&mut self, type_name: &str, values: &[i32]) -> io::Result<ItemRef> {
        self.scalar_storage(type_name, values, |v| v.to_le_bytes().to_vec())
    }

    pub fn u8_storage(&mut self, type_name: &str, values: &[u8]) -> io::Result<ItemRef> {
        self.scalar_storage(type_name, values, |v| vec![v])
    }

    pub fn vec4_storage(&mut self, values: &[[f32; 4]]) -> io::Result<ItemRef> {
        self.scalar_storage("hkVector4", values, |v| {
            v.iter().flat_map(|f| f.to_le_bytes()).collect()
        })
    }

    pub fn matrix4_storage(&mut self, values: &[[f32; 16]]) -> io::Result<ItemRef> {
        self.scalar_storage("hkMatrix4", values, |v| {
            v.iter().flat_map(|f| f.to_le_bytes()).collect()
        })
    }

    /// Serialises header, TAG0 (SDKV, DATA, TYPE, INDX/ITEM/PTCH) and the
    /// AAMP registration block.
    pub fn finish(mut self, type_section: &[u8], aamp: &[u8]) -> io::Result<Vec<u8>> {
        self.align(16);
        assemble_bphcl(&self.data, &self.items, &self.patches, type_section, aamp)
    }
}

/// Assembles a complete BPHCL file from its parts. `type_section` is a whole
/// TYPE section including its own header.
pub fn assemble_bphcl(
    data: &[u8],
    items: &[Item],
    patches: &[Patch],
    type_section: &[u8],
    aamp: &[u8],
) -> io::Result<Vec<u8>> {
    let sdkv = section("SDKV", 1, b"20220100")?;
    let data = section("DATA", 1, data)?;
    let mut item_bytes = BinaryWriter::new();
    for item in items {
        item_bytes.write_u32(item.flags);
        item_bytes.write_u32(item.data_offset);
        item_bytes.write_u32(item.count);
    }
    let mut patch_bytes = BinaryWriter::new();
    let mut groups = patches.to_vec();
    groups.sort_by_key(|group| group.type_index);
    for group in &mut groups {
        group.offsets.sort_unstable();
        patch_bytes.write_u32(group.type_index);
        patch_bytes.write_u32(group.offsets.len() as u32);
        for offset in &group.offsets {
            patch_bytes.write_u32(*offset);
        }
    }
    let item_section = section("ITEM", 1, &item_bytes.into_inner())?;
    let patch_section = section("PTCH", 1, &patch_bytes.into_inner())?;
    let index = section("INDX", 0, &[item_section, patch_section].concat())?;
    let tag = section(
        "TAG0",
        0,
        &[sdkv, data, type_section.to_vec(), index].concat(),
    )?;

    let tag_offset = 48u32;
    let parameter_offset = tag_offset + tag.len() as u32;
    let total = parameter_offset + aamp.len() as u32;
    let mut out = BinaryWriter::with_endian(Endian::Little);
    out.write_bytes(b"Phive ");
    out.write_bytes(&[0x01, 0x00, 0xff, 0xfe, 0x03, 0x03]);
    out.write_u32(tag_offset);
    out.write_u32(parameter_offset);
    out.write_u32(total);
    out.write_u32(tag.len() as u32);
    out.write_u32(aamp.len() as u32);
    out.write_zeros(16);
    out.write_bytes(&tag);
    out.write_bytes(aamp);
    Ok(out.into_inner())
}

/// A document holding nothing but a TYPE section, usable as the reflection
/// donor of an `ObjectWriter`.
pub fn type_donor(type_section: &[u8]) -> io::Result<BphclDocument> {
    let null = Item {
        flags: 0,
        type_index: 0,
        data_offset: 0,
        count: 0,
    };
    let bytes = assemble_bphcl(&[0u8; 16], &[null], &[], type_section, &[])?;
    BphclDocument::parse(&bytes)
}

fn section(signature: &str, kind: u8, payload: &[u8]) -> io::Result<Vec<u8>> {
    let size = payload.len() + 8;
    if size > 0x3fff_ffff {
        return Err(invalid("section exceeds the 30-bit size limit"));
    }
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(((kind as u32) << 30) | size as u32).to_be_bytes());
    out.extend_from_slice(signature.as_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

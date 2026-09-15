//! Read-only reflection over a BPHCL document driven by its TYPE section:
//! member lookup by name, element sizes, type kinds and pointer resolution.
//! It is what the geometry rescaler, the HKCL converter and the structural
//! validators use instead of hard-coded offsets.

use super::{BphclDocument, Item, TypeBody};

/// Kinds decoded from the low byte of a type body `format`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeKind {
    Void,
    Bool,
    String,
    Int,
    Float,
    Pointer,
    Record,
    /// `hkArray<T>` (format 0x8): pointer + size + capacity.
    Array,
    /// Inline `T[N]` (format 0xNN28): `N` elements of the subtype.
    InlineArray(u32),
    Other(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct TypeFormat {
    pub kind: TypeKind,
    pub format: u32,
    pub subtype: u32,
    pub size: u32,
}

/// A member resolved through the parent chain: name, byte offset, type.
#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub offset: u32,
    pub type_index: u32,
}

pub struct Reflect<'a> {
    pub document: &'a BphclDocument,
    /// Absolute offset of the DATA payload inside `document.raw`.
    pub base: usize,
}

impl<'a> Reflect<'a> {
    pub fn new(document: &'a BphclDocument) -> Option<Self> {
        let base = document.tag.find("DATA")?.payload_offset;
        Some(Self { document, base })
    }

    pub fn type_name(&self, type_index: u32) -> String {
        self.document
            .type_table
            .type_name(type_index)
            .unwrap_or("?")
            .to_string()
    }

    pub fn body(&self, type_index: u32) -> Option<&'a TypeBody> {
        self.document
            .type_table
            .bodies
            .iter()
            .find(|body| body.type_index == type_index)
    }

    /// Members including the ones inherited from parent types, in
    /// declaration order.
    pub fn members(&self, type_index: u32) -> Vec<Member> {
        let mut out = Vec::new();
        let mut chain = Vec::new();
        let mut current = type_index;
        while let Some(body) = self.body(current) {
            chain.push(body);
            if body.parent_type_index == 0 || body.parent_type_index == current || chain.len() > 16
            {
                break;
            }
            current = body.parent_type_index;
        }
        for body in chain.into_iter().rev() {
            for member in &body.members {
                out.push(Member {
                    name: self
                        .document
                        .type_table
                        .field_strings
                        .get(member.name_index as usize)
                        .cloned()
                        .unwrap_or_default(),
                    offset: member.offset,
                    type_index: member.type_index,
                });
            }
        }
        out
    }

    pub fn member(&self, type_index: u32, name: &str) -> Option<Member> {
        self.members(type_index)
            .into_iter()
            .find(|member| member.name == name)
    }

    pub fn member_offset(&self, type_index: u32, name: &str) -> Option<u32> {
        self.member(type_index, name).map(|member| member.offset)
    }

    /// Byte size of a type, walking parents until one declares it.
    pub fn size_of(&self, type_index: u32) -> u32 {
        let mut current = type_index;
        for _ in 0..16 {
            let Some(body) = self.body(current) else {
                return 0;
            };
            if let Some(size) = body.size {
                return size;
            }
            if body.parent_type_index == 0 || body.parent_type_index == current {
                return 0;
            }
            current = body.parent_type_index;
        }
        0
    }

    pub fn format_of(&self, type_index: u32) -> TypeFormat {
        let mut current = type_index;
        for _ in 0..16 {
            let Some(body) = self.body(current) else {
                break;
            };
            if let Some(format) = body.format {
                let kind = match format & 0xff {
                    0 => TypeKind::Void,
                    2 => TypeKind::Bool,
                    3 => TypeKind::String,
                    4 => TypeKind::Int,
                    5 => TypeKind::Float,
                    6 => TypeKind::Pointer,
                    7 => TypeKind::Record,
                    8 if format >> 8 == 0 => TypeKind::Array,
                    8 | 0x28 => TypeKind::InlineArray(format >> 8),
                    0x83 => TypeKind::String,
                    other => TypeKind::Other(other),
                };
                return TypeFormat {
                    kind,
                    format,
                    subtype: body.subtype_index.unwrap_or(0),
                    size: body.size.unwrap_or(0),
                };
            }
            if body.parent_type_index == 0 || body.parent_type_index == current {
                break;
            }
            current = body.parent_type_index;
        }
        TypeFormat {
            kind: TypeKind::Void,
            format: 0,
            subtype: 0,
            size: self.size_of(type_index),
        }
    }

    /// Index of a named, bodied type (the first match).
    pub fn find_type(&self, name: &str) -> Option<u32> {
        (1..=self.document.type_table.named_types.len() as u32)
            .find(|index| self.type_name(*index) == name && self.body(*index).is_some())
    }

    /// The `hkArray<element>` type index, needed as the PTCH group of an
    /// array pointer field.
    pub fn array_type_for(&self, element_type: u32) -> Option<u32> {
        self.template_type("hkArray", element_type)
    }

    pub fn template_type(&self, name: &str, argument: u32) -> Option<u32> {
        self.document
            .type_table
            .named_types
            .iter()
            .enumerate()
            .find(|(_, named)| {
                self.document
                    .type_table
                    .type_strings
                    .get(named.string_index as usize)
                    .is_some_and(|value| value == name)
                    && named
                        .templates
                        .iter()
                        .any(|template| template.type_index == argument)
            })
            .map(|(index, _)| index as u32 + 1)
    }

    // ---- raw access (offsets are DATA-relative) ----

    pub fn bytes(&self, offset: u32, len: usize) -> Option<&'a [u8]> {
        let start = self.base.checked_add(offset as usize)?;
        self.document.raw.get(start..start.checked_add(len)?)
    }

    pub fn u8(&self, offset: u32) -> u8 {
        self.bytes(offset, 1).map(|b| b[0]).unwrap_or(0)
    }

    pub fn u16(&self, offset: u32) -> u16 {
        self.bytes(offset, 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .unwrap_or(0)
    }

    pub fn u32(&self, offset: u32) -> u32 {
        self.bytes(offset, 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0)
    }

    pub fn f32(&self, offset: u32) -> f32 {
        f32::from_bits(self.u32(offset))
    }

    /// The ITEM referenced by the pointer field at `offset`, if the field
    /// carries a relocation.
    pub fn referenced(&self, offset: u32) -> Option<usize> {
        if !self
            .document
            .patches
            .iter()
            .any(|patch| patch.offsets.contains(&offset))
        {
            return None;
        }
        let index = self.u32(offset) as usize;
        (index < self.document.items.len()).then_some(index)
    }

    pub fn item(&self, index: usize) -> Option<&'a Item> {
        self.document.items.get(index)
    }

    pub fn string_at(&self, offset: u32) -> Option<String> {
        let item = self.item(self.referenced(offset)?)?;
        let tail = self.bytes(item.data_offset, item.count as usize)?;
        let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
        Some(String::from_utf8_lossy(&tail[..end]).into_owned())
    }

    /// Storage item of an `hkArray` field: `(data offset, count, element size)`.
    pub fn array(&self, field: u32) -> Option<(u32, u32, u32)> {
        let item = self.item(self.referenced(field)?)?;
        Some((item.data_offset, item.count, self.size_of(item.type_index)))
    }

    /// Item indices held by an array of pointers (`T*` / `hkRefPtr`).
    pub fn pointer_array(&self, field: u32) -> Vec<usize> {
        let Some(item) = self.referenced(field).and_then(|index| self.item(index)) else {
            return Vec::new();
        };
        (0..item.count)
            .filter_map(|k| self.referenced(item.data_offset + k * 8))
            .collect()
    }

    pub fn u16_array(&self, field: u32) -> Vec<u16> {
        match self.array(field) {
            Some((offset, count, _)) => (0..count).map(|k| self.u16(offset + k * 2)).collect(),
            None => Vec::new(),
        }
    }

    pub fn u32_array(&self, field: u32) -> Vec<u32> {
        match self.array(field) {
            Some((offset, count, _)) => (0..count).map(|k| self.u32(offset + k * 4)).collect(),
            None => Vec::new(),
        }
    }
}

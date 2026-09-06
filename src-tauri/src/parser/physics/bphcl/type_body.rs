use super::{TypeInterface, TypeMember};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeBody {
    pub type_index: u32,
    pub parent_type_index: u32,
    pub flags: u32,
    pub format: Option<u32>,
    pub subtype_index: Option<u32>,
    pub version: Option<u32>,
    pub size: Option<u32>,
    pub alignment: Option<u32>,
    pub unknown_flags: Option<u32>,
    pub encoded_member_count: u32,
    pub members: Vec<TypeMember>,
    pub interface_count: Option<u32>,
    pub interfaces: Vec<TypeInterface>,
    pub attribute_index: Option<u32>,
}

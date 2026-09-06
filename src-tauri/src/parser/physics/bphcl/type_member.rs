#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeMember {
    pub name_index: u32,
    pub flags: u32,
    pub reserve: Option<u8>,
    pub offset: u32,
    pub type_index: u32,
}

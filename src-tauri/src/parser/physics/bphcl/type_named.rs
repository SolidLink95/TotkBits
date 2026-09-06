use super::TypeTemplate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeNamed {
    pub string_index: u32,
    pub templates: Vec<TypeTemplate>,
}

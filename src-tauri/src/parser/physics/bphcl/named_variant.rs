use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct NamedVariant {
    pub index: usize,
    pub name: String,
    pub class_name: String,
    pub item_index: usize,
    pub object_type: String,
}

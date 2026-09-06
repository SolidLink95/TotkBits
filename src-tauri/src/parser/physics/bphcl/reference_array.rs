use super::Item;

/// Metadata needed to replace an `hkArray<T*>` backing allocation.
#[derive(Clone, Debug)]
pub struct ReferenceArray {
    pub field_offset: u32,
    pub storage_item_index: usize,
    pub storage_item: Item,
    pub entry_patch_type_index: u32,
}

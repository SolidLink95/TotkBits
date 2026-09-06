use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClothSkeletonPair {
    pub index: usize,
    pub cloth_item_index: usize,
    pub skeleton_item_index: usize,
}

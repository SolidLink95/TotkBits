use super::Bone;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Skeleton {
    pub index: usize,
    pub name: String,
    pub item_index: usize,
    pub bones: Vec<Bone>,
}

use super::Vector4;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Bone {
    pub index: usize,
    pub name: String,
    pub parent_index: Option<usize>,
    pub lock_translation: bool,
    pub translation: Vector4,
    pub rotation: Vector4,
}

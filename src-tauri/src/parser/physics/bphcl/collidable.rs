use super::{CollidableShape, Vector4};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct Collidable {
    pub index: usize,
    pub name: String,
    pub item_index: usize,
    pub class_name: String,
    pub translation: Vector4,
    pub axis_x: Vector4,
    pub axis_y: Vector4,
    pub axis_z: Vector4,
    pub enabled: bool,
    pub shape: CollidableShape,
    /// Havok class of the referenced shape object and its `type` word. Two
    /// colliders only stand in for each other when both agree.
    pub shape_class_name: String,
    pub shape_kind: u32,
    pub pinch_detection_radius: f32,
    pub pinch_detection_priority: i8,
    pub pinch_detection_enabled: bool,
    pub virtual_collision_point_collision_enabled: bool,
}

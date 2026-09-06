use serde::Serialize;
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Vector4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

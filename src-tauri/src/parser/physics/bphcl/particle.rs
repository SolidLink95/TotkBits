use super::Vector4;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct Particle {
    pub index: usize,
    pub position: Vector4,
    pub fixed: bool,
    pub mass: f32,
    pub inverse_mass: f32,
    pub radius: f32,
    pub friction: f32,
}

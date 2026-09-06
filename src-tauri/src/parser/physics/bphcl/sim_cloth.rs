use super::Particle;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct SimCloth {
    pub index: usize,
    pub name: String,
    pub item_index: usize,
    pub particles: Vec<Particle>,
    pub fixed_particle_indices: Vec<u16>,
    pub constraint_item_indices: Vec<usize>,
    pub collidable_item_indices: Vec<usize>,
}

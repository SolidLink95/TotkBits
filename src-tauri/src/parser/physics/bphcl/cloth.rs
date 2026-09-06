use super::SimCloth;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct Cloth {
    pub index: usize,
    pub name: String,
    pub item_index: usize,
    pub simulations: Vec<SimCloth>,
}

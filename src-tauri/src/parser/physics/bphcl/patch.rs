#[derive(Clone, Debug)]
pub struct Patch {
    pub type_index: u32,
    pub offsets: Vec<u32>,
}

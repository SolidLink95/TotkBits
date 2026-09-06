#[derive(Clone, Debug)]
pub struct Item {
    pub flags: u32,
    pub type_index: u32,
    pub data_section_index: usize,
    pub data_offset: u32,
    pub count: u32,
}

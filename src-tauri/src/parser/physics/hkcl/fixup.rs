#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalFixup {
    pub section_index: usize,
    pub source_offset: u32,
    pub destination_offset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlobalFixup {
    pub section_index: usize,
    pub source_offset: u32,
    pub destination_section_index: usize,
    pub destination_offset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtualFixup {
    pub section_index: usize,
    pub source_offset: u32,
    pub class_name_section_index: usize,
    pub class_name_offset: u32,
}

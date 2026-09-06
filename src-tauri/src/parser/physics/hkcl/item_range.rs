#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemRange {
    pub start: u32,
    pub end: u32,
}

impl ItemRange {
    pub fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

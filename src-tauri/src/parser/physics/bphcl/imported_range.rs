#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportedRange {
    pub old_start: u32,
    pub old_end: u32,
    pub new_start: u32,
}

impl ImportedRange {
    pub fn contains(&self, offset: u32) -> bool {
        offset >= self.old_start && offset < self.old_end
    }

    pub fn relocate(&self, offset: u32) -> Option<u32> {
        self.contains(offset)
            .then(|| self.new_start + offset - self.old_start)
    }
}

use super::ImportedRange;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct CopiedItemGraph {
    pub data: Vec<u8>,
    pub ranges_by_old_start: HashMap<u32, ImportedRange>,
}

use std::collections::BTreeMap;

use super::Archive::{ArchiveCodec, ArchiveResult};

pub use crate::parser::mii::RflDbFile;

impl ArchiveCodec for RflDbFile {
    fn from_bytes(data: &[u8]) -> ArchiveResult<Self> {
        RflDbFile::parse(data)
    }

    fn to_bytes(&self) -> ArchiveResult<Vec<u8>> {
        RflDbFile::build(self)
    }

    fn entries(&self) -> &BTreeMap<String, Vec<u8>> {
        self.entries()
    }

    fn entries_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        self.entries_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supplied() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/mii/RFL_DB.dat");
        std::fs::read(path).unwrap()
    }

    #[test]
    fn opens_adds_removes_and_reopens_supplied_miis() {
        let source = supplied();
        let mut database = RflDbFile::from_bytes(&source).unwrap();
        let original_count = database.entries().len();
        assert!(original_count >= 3);

        let opened = database
            .entries()
            .iter()
            .take(3)
            .map(|(path, data)| (path.clone(), data.clone()))
            .collect::<Vec<_>>();
        assert!(opened.iter().all(|(path, data)| {
            path.starts_with("Miis/") && path.ends_with(".miigx") && data.len() == 74
        }));

        let removed_path = opened[0].0.clone();
        database.remove(&removed_path).unwrap();
        assert!(database.get(&removed_path).is_none());
        database
            .entries_mut()
            .insert("Miis/imported.miigx".into(), opened[1].1.clone());

        let rebuilt = database.to_bytes().unwrap();
        let reopened = RflDbFile::from_bytes(&rebuilt).unwrap();
        assert_eq!(reopened.entries().len(), original_count);
        assert!(reopened.entries().values().all(|data| data.len() == 74));
    }

    #[test]
    fn clear_all_rebuilds_a_valid_empty_database() {
        let mut database = RflDbFile::from_bytes(&supplied()).unwrap();
        database.entries_mut().clear();
        let reopened = RflDbFile::from_bytes(&database.to_bytes().unwrap()).unwrap();
        assert!(reopened.entries().is_empty());
    }
}

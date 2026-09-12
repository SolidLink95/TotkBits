//! Whole-mod RESTBL estimation and versioned table generation.

use crate::{
    parser::rstb::ResourceSizeTable,
    tools::RstbEstimate::RstbEstimator,
    Zstd::{TotkZstd, ZstdDictionary},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

const PRODUCT_PREFIX: &str = "ResourceSizeTable.Product.";
const PRODUCT_SUFFIX: &str = ".rsizetable.zs";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RstbGenerationReport {
    pub output: PathBuf,
    pub yaml: PathBuf,
    pub product_version: String,
    pub entries: BTreeMap<String, u32>,
    /// Modified resources whose freshly estimated value already matches vanilla.
    pub modified_matching_vanilla: BTreeMap<String, u32>,
}

pub struct ModRstbProcessor<'a> {
    clean_romfs: PathBuf,
    output_romfs: PathBuf,
    zstd: Arc<TotkZstd<'a>>,
    compression_level: Option<i32>,
}

impl<'a> ModRstbProcessor<'a> {
    pub fn new(clean_romfs: &Path, output_romfs: &Path, zstd: Arc<TotkZstd<'a>>) -> Self {
        Self {
            clean_romfs: clean_romfs.to_path_buf(),
            output_romfs: output_romfs.to_path_buf(),
            zstd,
            compression_level: None,
        }
    }

    /// Zstandard level for the written table. TKMM compresses with the level
    /// from its settings (default 7); pass the same value to get its bytes.
    pub fn with_compression_level(mut self, level: Option<i32>) -> Self {
        self.compression_level = level;
        self
    }

    /// Estimates every resource below the mod ROMFS the way TKMM does (every
    /// archive member included, vanilla or not), rebuilds the user's versioned
    /// ResourceSizeTable from the clean one, and emits `rstb.yaml` next to the
    /// ROMFS folder with unhashed resource paths for review.
    pub fn generate(&self) -> io::Result<RstbGenerationReport> {
        super::assets::ensure_output_outside_romfs(&self.clean_romfs, &self.output_romfs)?;
        if !self.output_romfs.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "mod ROMFS directory is missing: {}",
                    self.output_romfs.display()
                ),
            ));
        }
        let clean_resource = self.clean_romfs.join("System/Resource");
        let (product_version, source) =
            super::version::discover_product_file(&clean_resource, PRODUCT_PREFIX, PRODUCT_SUFFIX)?;
        let file_name = source
            .file_name()
            .ok_or_else(|| invalid_data("ResourceSizeTable filename is missing"))?;
        let output = self.output_romfs.join("System/Resource").join(file_name);

        let mut estimator = RstbEstimator::new(self.zstd.clone());
        estimator.set_vanilla_romfs(&self.clean_romfs);
        estimator.include_vanilla_sarc_members = true;
        estimator.ainb_empty_exb_header = false;
        estimator
            .estimate_folder(&self.output_romfs)
            .map_err(io::Error::other)?;
        let estimated_entries: BTreeMap<String, u32> = estimator
            .entries
            .iter()
            .map(|(path, value)| (path.replace('\\', "/"), *value))
            .collect();
        let source_bytes = fs::read(&source)?;
        let (raw, dictionary) = self.zstd.try_decompress_for_path(&source, &source_bytes)?;
        let clean_table = ResourceSizeTable::from_bytes(&raw).map_err(io::Error::other)?;
        let existing_table = if output.is_file() {
            let bytes = fs::read(&output)?;
            let (raw, _) = self.zstd.try_decompress_for_path(&output, &bytes)?;
            Some(ResourceSizeTable::from_bytes(&raw).map_err(io::Error::other)?)
        } else {
            None
        };
        let mut table = existing_table
            .clone()
            .unwrap_or_else(|| clean_table.clone());
        let mut needs_rebuild = existing_table.is_none();
        let mut entries = BTreeMap::new();
        let mut modified_matching_vanilla = BTreeMap::new();
        for (path, estimated) in estimated_entries {
            let vanilla = clean_table.get(path.clone()).copied();
            let existing = existing_table
                .as_ref()
                .and_then(|table| table.get(path.clone()).copied());
            if existing_table.is_some() && existing != Some(estimated) {
                needs_rebuild = true;
            }
            if vanilla == Some(estimated) {
                modified_matching_vanilla.insert(path, estimated);
                continue;
            }
            table.set(path.clone(), estimated);
            entries.insert(path, estimated);
        }
        estimator.entries = entries
            .iter()
            .map(|(path, value)| (path.clone(), *value))
            .collect();
        // The review file must not ship inside the ROMFS (it would get an RSTB
        // entry of its own), so it goes next to the ROMFS folder.
        let yaml = self
            .output_romfs
            .parent()
            .unwrap_or(&self.output_romfs)
            .join("rstb.yaml");
        estimator.save_yaml(&yaml).map_err(io::Error::other)?;
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        if needs_rebuild {
            let rebuilt = table.to_bytes_compact().map_err(io::Error::other)?;
            let compressed = match self.compression_level {
                Some(level) if dictionary == ZstdDictionary::Empty => {
                    self.zstd.compress_empty_with_level(&rebuilt, level)?
                }
                _ => self.zstd.compress_with_dictionary(&rebuilt, dictionary)?,
            };
            fs::write(&output, compressed)?;
        }

        let (verified, _) = self
            .zstd
            .try_decompress_for_path(&output, &fs::read(&output)?)?;
        let verified = ResourceSizeTable::from_bytes(&verified).map_err(io::Error::other)?;
        for (path, expected) in &entries {
            if verified.get(path.clone()) != Some(expected) {
                return Err(invalid_data(format!(
                    "generated ResourceSizeTable entry is missing: {path}"
                )));
            }
        }
        Ok(RstbGenerationReport {
            output,
            yaml,
            product_version,
            entries,
            modified_matching_vanilla,
        })
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tools::items_creator::{vendor::VendorProcessor, VendorTarget},
        TotkConfig::TotkConfig,
        Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
    };

    #[test]
    #[ignore = "requires a configured clean ROMFS"]
    fn real_mod_estimates_pack_and_only_changed_internal_entries() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let Ok((version, clean_rstb)) = super::super::version::discover_product_file(
            &clean_romfs.join("System/Resource"),
            PRODUCT_PREFIX,
            PRODUCT_SUFFIX,
        ) else {
            return;
        };
        if !clean_romfs
            .join("Pack/Actor/Npc_TripMaster_00.pack.zs")
            .is_file()
        {
            return;
        }
        let clean_rstb_bytes = fs::read(&clean_rstb).unwrap();
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let output_romfs =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/rstb_generated_romfs");
        let vendor = VendorTarget {
            actor_name: "Npc_TripMaster_00".into(),
            buying_price: None,
            selling_price: None,
            quantity: 2,
        };
        VendorProcessor::new(clean_romfs, &output_romfs, zstd.clone())
            .add_weapon("Weapon_Lsword_900", &vendor)
            .unwrap();

        let report = ModRstbProcessor::new(clean_romfs, &output_romfs, zstd.clone())
            .generate()
            .unwrap();
        assert_eq!(report.product_version, version);
        assert!(report
            .entries
            .contains_key("Pack/Actor/Npc_TripMaster_00.pack"));
        assert!(report.entries.contains_key(
            "Component/ShopParam/Npc_TripMaster_00.game__component__ShopParam.bgyml"
        ));
        assert!(!report
            .entries
            .contains_key("Actor/Npc_TripMaster_00.engine__actor__ActorParam.bgyml"));
        assert!(!report.entries.contains_key("rstb.yaml"));
        let yaml: BTreeMap<String, u32> =
            serde_yaml::from_slice(&fs::read(&report.yaml).unwrap()).unwrap();
        assert_eq!(yaml, report.entries);
        assert_eq!(fs::read(clean_rstb).unwrap(), clean_rstb_bytes);

        let (raw, _) = zstd
            .try_decompress_for_path(&report.output, &fs::read(&report.output).unwrap())
            .unwrap();
        let table = ResourceSizeTable::from_bytes(&raw).unwrap();
        for (path, value) in &report.entries {
            assert_eq!(table.get(path.clone()), Some(value));
        }
        fs::remove_dir_all(output_romfs).unwrap();
    }

    #[test]
    #[ignore = "recalculates only the existing tmp/test_sic RSTB outputs"]
    fn recalculate_test_sic_rstb() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let output_romfs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/test_sic/romfs");
        assert!(clean_romfs.is_dir(), "clean ROMFS is missing");
        assert!(output_romfs.is_dir(), "tmp/test_sic/romfs is missing");

        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let report = ModRstbProcessor::new(clean_romfs, &output_romfs, zstd)
            .generate()
            .unwrap();

        println!(
            "RSTB_CHANGED={}\n{}",
            report.entries.len(),
            serde_json::to_string_pretty(&report.entries).unwrap()
        );
        println!(
            "RSTB_MODIFIED_MATCHING_VANILLA={}\n{}",
            report.modified_matching_vanilla.len(),
            serde_json::to_string_pretty(&report.modified_matching_vanilla).unwrap()
        );
    }

    #[test]
    #[ignore = "diagnostic comparison of reference and generated test_sic RSTBs"]
    fn compare_test_sic_rstbs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mod_romfs = root.join("test_sic/romfs");
        let correct_path = root.join("works/ResourceSizeTable.Product.121.rsizetable.zs");
        let wrong_path =
            mod_romfs.join("System/Resource/ResourceSizeTable.Product.121.rsizetable.zs");

        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let read_table = |path: &Path| {
            let bytes = fs::read(path).unwrap();
            let (raw, _) = zstd.try_decompress_for_path(path, &bytes).unwrap();
            ResourceSizeTable::from_bytes(&raw).unwrap()
        };
        let correct = read_table(&correct_path);
        let wrong = read_table(&wrong_path);
        let (_, vanilla_path) = super::super::version::discover_product_file(
            &clean_romfs.join("System/Resource"),
            PRODUCT_PREFIX,
            PRODUCT_SUFFIX,
        )
        .unwrap();
        let vanilla = read_table(&vanilla_path);
        let hash_differences = correct
            .hash_table
            .iter()
            .filter(|(hash, value)| wrong.hash_table.get(hash) != Some(value))
            .count()
            + wrong
                .hash_table
                .keys()
                .filter(|hash| !correct.hash_table.contains_key(hash))
                .count();
        println!(
            "TABLE correct_version={:?} generated_version={:?} correct_hashes={} generated_hashes={} correct_overflow={} generated_overflow={} hash_differences={}",
            correct.version,
            wrong.version,
            correct.hash_table.len(),
            wrong.hash_table.len(),
            correct.overflow_table.len(),
            wrong.overflow_table.len(),
            hash_differences
        );
        let model =
            fs::read(mod_romfs.join("Model/Weapon_Lsword_005.Weapon_Lsword_005.bfres.mc")).unwrap();
        let model_raw = zstd.decompress_mcpk(&model).unwrap();
        let model_parsed =
            crate::file_format::Model3D::bfres::BfresFile::from_bytes(&model_raw).unwrap();
        println!(
            "MODEL compressed={} decompressed={} header_file_size={}",
            model.len(),
            model_raw.len(),
            model_parsed.header.file_size
        );
        let generated_yaml: BTreeMap<String, u32> =
            serde_yaml::from_slice(&fs::read(root.join("test_sic/rstb.yaml")).unwrap()).unwrap();
        let mut paths: Vec<_> = generated_yaml.keys().cloned().collect();
        paths.sort();
        let compared = paths.len();
        let mut differences = 0;
        for path in paths {
            let estimated = generated_yaml[&path];
            let correct_value = correct.get(path.clone()).copied();
            let wrong_value = wrong.get(path.clone()).copied();
            if correct_value != wrong_value {
                differences += 1;
                println!(
                    "DIFF {path}\testimate={estimated}\tcorrect={correct_value:?}\tgenerated={wrong_value:?}\tvanilla={:?}",
                    vanilla.get(path.clone()).copied()
                );
            }
        }
        println!("MOD_ENTRIES_COMPARED={compared} DIFFERENCES={differences}");
        assert_eq!(differences, 0, "generated RSTB differs on mod-owned paths");
    }

    #[test]
    #[ignore = "estimates the BotW Weapon Restoration mod and writes romfs/rstb.yaml"]
    fn restoration_mod_estimates_match_shipped_rstb() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let mod_romfs =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/BotW Weapon Restoration/romfs");
        let shipped_path =
            mod_romfs.join("System/Resource/ResourceSizeTable.Product.121.rsizetable.zs");
        assert!(clean_romfs.is_dir(), "clean ROMFS is missing");
        assert!(shipped_path.is_file(), "restoration RSTB is missing");

        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL).unwrap());
        let mut estimator = RstbEstimator::new(zstd.clone());
        estimator.set_vanilla_romfs(clean_romfs);
        estimator.estimate_folder(&mod_romfs).unwrap();
        estimator.save_yaml(mod_romfs.join("rstb.yaml")).unwrap();

        let (raw, _) = zstd
            .try_decompress_for_path(&shipped_path, &fs::read(&shipped_path).unwrap())
            .unwrap();
        let shipped = ResourceSizeTable::from_bytes(&raw).unwrap();
        let differences: BTreeMap<_, _> = estimator
            .entries
            .iter()
            .filter_map(|(path, estimated)| {
                let expected = shipped.get(path.clone()).copied();
                (expected != Some(*estimated)).then(|| (path.clone(), (*estimated, expected)))
            })
            .collect();
        let underestimates = differences
            .values()
            .filter(|(estimated, expected)| expected.is_some_and(|expected| estimated < &expected))
            .count();
        let overestimates = differences.len() - underestimates;
        println!(
            "RESTORATION_ENTRIES={} DIFFERENCES={} UNDERESTIMATES={} OVERESTIMATES={}",
            estimator.entries.len(),
            differences.len(),
            underestimates,
            overestimates
        );
        for (path, values) in differences.iter().take(30) {
            println!("DIFF {path}: {values:?}");
        }
        assert_eq!(
            underestimates, 0,
            "dynamic estimates must not be smaller than the shipped working RSTB"
        );
    }
}

//! Uniform geometric rescaling of a BPHCL cloth package.
//!
//! Cloth files authored for a differently sized model (a BOTW enemy cloth
//! moved onto Link, a model shrunk in the DCC tool) simulate at the wrong
//! scale: rest lengths, particle positions, bone offsets and skin offsets
//! all stay at the source size and rigid pieces get pulled apart. Multiplying
//! every length-dimensioned quantity by one factor keeps the simulation
//! self-consistent (rotations, masses, stiffness fractions and time
//! constants are untouched) and matches the target skeleton again.

use super::{BphclDocument, TransformSheet, TransformSpec};
use std::io::{self, ErrorKind};

pub const MIN_SCALE: f32 = 0.1;
pub const MAX_SCALE: f32 = 5.0;

/// Which members were touched, for the status report.
#[derive(Clone, Debug, Default)]
pub struct RescaleReport {
    pub scale: f32,
    pub edits: Vec<(String, usize)>,
}

impl BphclDocument {
    /// Returns the document bytes with every length scaled by `scale`
    /// (`0.1..=5.0`). Item graph, types and the AAMP registration are kept.
    /// This is the uniform-scale case of [`BphclDocument::apply_transform_sheet`].
    pub fn rescale_geometry(&self, scale: f32) -> io::Result<(Vec<u8>, RescaleReport)> {
        if !(MIN_SCALE..=MAX_SCALE).contains(&scale) || !scale.is_finite() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("scale {scale} is outside {MIN_SCALE}..={MAX_SCALE}"),
            ));
        }
        let sheet = TransformSheet {
            document: Some(TransformSpec::uniform_scale(scale)),
            ..TransformSheet::default()
        };
        let (raw, report) = self.apply_transform_sheet(&sheet)?;
        Ok((
            raw,
            RescaleReport {
                scale,
                edits: report.edits,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_scales_outside_the_supported_range() {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_bphcl");
        let Some(path) = std::fs::read_dir(&directory).ok().and_then(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|e| e == "bphcl"))
        }) else {
            return;
        };
        let document = BphclDocument::parse(&std::fs::read(path).unwrap()).unwrap();
        assert!(document.rescale_geometry(0.0).is_err());
        assert!(document.rescale_geometry(7.0).is_err());
        // 1.0 is the identity and writes nothing; a real factor must touch
        // the rest positions without changing the layout.
        let (bytes, report) = document.rescale_geometry(1.0).unwrap();
        assert_eq!(bytes, document.raw);
        let (bytes, report_scaled) = document.rescale_geometry(1.5).unwrap();
        assert_eq!(bytes.len(), document.raw.len());
        assert!(report.edits.is_empty());
        let report = report_scaled;
        assert!(report
            .edits
            .iter()
            .any(|(name, _)| name == "particle rest positions"));
    }
}

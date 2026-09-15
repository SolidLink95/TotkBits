//! Uniform geometric rescaling of a BPHCL cloth package.
//!
//! Cloth files authored for a differently sized model (a BOTW enemy cloth
//! moved onto Link, a model shrunk in the DCC tool) simulate at the wrong
//! scale: rest lengths, particle positions, bone offsets and skin offsets
//! all stay at the source size and rigid pieces get pulled apart. Multiplying
//! every length-dimensioned quantity by one factor keeps the simulation
//! self-consistent (rotations, masses, stiffness fractions and time
//! constants are untouched) and matches the target skeleton again.

use super::{reflect::Reflect, BphclDocument};
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
    pub fn rescale_geometry(&self, scale: f32) -> io::Result<(Vec<u8>, RescaleReport)> {
        if !(MIN_SCALE..=MAX_SCALE).contains(&scale) || !scale.is_finite() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("scale {scale} is outside {MIN_SCALE}..={MAX_SCALE}"),
            ));
        }
        let reflect = Reflect::new(self)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "BPHCL has no DATA section"))?;
        let mut scaler = Scaler {
            reflect: &reflect,
            raw: self.raw.clone(),
            scale,
            report: RescaleReport {
                scale,
                edits: Vec::new(),
            },
        };
        scaler.run()?;
        let raw = scaler.raw;
        let rebuilt = BphclDocument::parse(&raw)?;
        rebuilt.validate_item_graph()?;
        Ok((raw, scaler.report))
    }
}

struct Scaler<'a> {
    reflect: &'a Reflect<'a>,
    raw: Vec<u8>,
    scale: f32,
    report: RescaleReport,
}

impl<'a> Scaler<'a> {
    fn f32_at(&self, offset: u32) -> f32 {
        let at = self.reflect.base + offset as usize;
        f32::from_le_bytes(self.raw[at..at + 4].try_into().unwrap_or([0; 4]))
    }

    fn set_f32(&mut self, offset: u32, value: f32, what: &str) -> io::Result<()> {
        let at = self.reflect.base + offset as usize;
        let slot = self
            .raw
            .get_mut(at..at + 4)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "rescale write exceeds DATA"))?;
        slot.copy_from_slice(&value.to_le_bytes());
        match self.report.edits.iter_mut().find(|(name, _)| name == what) {
            Some((_, count)) => *count += 1,
            None => self.report.edits.push((what.to_owned(), 1)),
        }
        Ok(())
    }

    fn mul(&mut self, offset: u32, factor: f32, what: &str) -> io::Result<()> {
        let value = self.f32_at(offset);
        if value != 0.0 && value.is_finite() {
            self.set_f32(offset, value * factor, what)?;
        }
        Ok(())
    }

    fn mul_vec3(&mut self, offset: u32, factor: f32, what: &str) -> io::Result<()> {
        for component in 0..3 {
            self.mul(offset + component * 4, factor, what)?;
        }
        Ok(())
    }

    fn member(&self, type_index: u32, name: &str) -> io::Result<u32> {
        self.reflect.member_offset(type_index, name).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "type {} has no member {name}",
                    self.reflect.type_name(type_index)
                ),
            )
        })
    }

    /// `(storage offset, count, element size)` of the array at `field`.
    fn array(&self, field: u32) -> Option<(u32, u32, u32)> {
        self.reflect.array(field)
    }

    fn run(&mut self) -> io::Result<()> {
        let s = self.scale;
        let items: Vec<_> = self.reflect.document.items.clone();
        for item in &items {
            let type_name = self.reflect.type_name(item.type_index);
            let t = item.type_index;
            let base = item.data_offset;
            let element_size = self.reflect.size_of(t);
            match type_name.as_str() {
                "hkaSkeleton" => {
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "referencePose")?)
                    {
                        for k in 0..count {
                            self.mul_vec3(offset + k * size, s, "skeleton reference pose")?;
                        }
                    }
                }
                "hclSimClothPose" => {
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "positions")?)
                    {
                        for k in 0..count {
                            self.mul_vec3(offset + k * size, s, "particle rest positions")?;
                        }
                    }
                }
                "hclSimClothData" => {
                    let particles_field = base + self.member(t, "particleDatas")?;
                    if let Some(storage) = self.reflect.referenced(particles_field) {
                        let storage = self.reflect.document.items[storage].clone();
                        let size = self.reflect.size_of(storage.type_index);
                        let radius = self.member(storage.type_index, "radius")?;
                        for k in 0..storage.count {
                            self.mul(
                                storage.data_offset + k * size + radius,
                                s,
                                "particle radius",
                            )?;
                        }
                    }
                    self.mul(
                        base + self.member(t, "maxParticleRadius")?,
                        s,
                        "max particle radius",
                    )?;
                    let map = self
                        .reflect
                        .member(t, "collidableTransformMap")
                        .ok_or_else(|| {
                            io::Error::new(ErrorKind::InvalidData, "no collidableTransformMap")
                        })?;
                    if let Some((offset, count, size)) =
                        self.array(base + map.offset + self.member(map.type_index, "offsets")?)
                    {
                        for k in 0..count {
                            self.mul_vec3(
                                offset + k * size + 48,
                                s,
                                "collidable transform map offsets",
                            )?;
                        }
                    }
                    let landscape = self
                        .reflect
                        .member(t, "landscapeCollisionData")
                        .ok_or_else(|| {
                            io::Error::new(ErrorKind::InvalidData, "no landscapeCollisionData")
                        })?;
                    self.mul(
                        base + landscape.offset
                            + self.member(landscape.type_index, "landscapeRadius")?,
                        s,
                        "landscape radius",
                    )?;
                }
                "hclCollidable" => {
                    let transform = self.member(t, "transform")?;
                    self.mul_vec3(base + transform + 48, s, "collidable translation")?;
                    if let Some(shape_index) =
                        self.reflect.referenced(base + self.member(t, "shape")?)
                    {
                        let shape = self.reflect.document.items[shape_index].clone();
                        let st = shape.type_index;
                        let sb = shape.data_offset;
                        match self.reflect.type_name(st).as_str() {
                            "hclCapsuleShape" => {
                                let start = sb + self.member(st, "start")?;
                                let end = sb + self.member(st, "end")?;
                                self.mul_vec3(start, s, "capsule end points")?;
                                self.mul_vec3(end, s, "capsule end points")?;
                                self.mul(sb + self.member(st, "radius")?, s, "capsule radius")?;
                                let length_squared: f32 = (0..3)
                                    .map(|i| {
                                        (self.f32_at(end + i * 4) - self.f32_at(start + i * 4))
                                            .powi(2)
                                    })
                                    .sum();
                                self.set_f32(
                                    sb + self.member(st, "capLenSqrdInv")?,
                                    1.0 / length_squared.max(1.0e-4),
                                    "capsule inverse squared length",
                                )?;
                            }
                            "hclSphereShape" => {
                                let sphere = sb + self.member(st, "sphere")?;
                                for i in 0..4 {
                                    self.mul(sphere + i * 4, s, "sphere centre and radius")?;
                                }
                            }
                            "hclPlaneShape" => {
                                // plane equation (n, -d): only the distance scales
                                self.mul(
                                    sb + self.member(st, "planeEquation")? + 12,
                                    s,
                                    "plane distance",
                                )?;
                            }
                            _ => {}
                        }
                    }
                }
                "hclStandardLinkConstraintSet::Link" | "hclStretchLinkConstraintSet::Link" => {
                    let rest = self.member(t, "restLength")?;
                    for k in 0..item.count {
                        self.mul(base + k * element_size + rest, s, "link rest lengths")?;
                    }
                }
                "hclBendLinkConstraintSet::Link" => {
                    let (a, b) = (
                        self.member(t, "bendMinLength")?,
                        self.member(t, "stretchMaxLength")?,
                    );
                    for k in 0..item.count {
                        self.mul(base + k * element_size + a, s, "bend link lengths")?;
                        self.mul(base + k * element_size + b, s, "bend link lengths")?;
                    }
                }
                "hclBendStiffnessConstraintSet::Link" => {
                    let curvature = self.member(t, "restCurvature")?;
                    for k in 0..item.count {
                        self.mul(
                            base + k * element_size + curvature,
                            1.0 / s,
                            "rest curvature",
                        )?;
                    }
                }
                "hclBendStiffnessConstraintSet" => {
                    self.mul(
                        base + self.member(t, "maxRestPoseHeightSq")?,
                        s * s,
                        "max rest pose height",
                    )?;
                }
                "hclLocalRangeConstraintSet::LocalConstraint"
                | "hclLocalRangeConstraintSet::LocalStiffnessConstraint" => {
                    let radius = self.member(t, "shapeRadius")?;
                    let (max, min) = (
                        self.member(t, "maxNormalDistance")?,
                        self.member(t, "minNormalDistance")?,
                    );
                    for k in 0..item.count {
                        let e = base + k * element_size;
                        self.mul(e + radius, s, "local range radius")?;
                        for offset in [max, min] {
                            if self.f32_at(e + offset).abs() < 1.0e30 {
                                self.mul(e + offset, s, "local range normal distance")?;
                            }
                        }
                    }
                }
                "hclObjectSpaceSkinPOperator" => {
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "boneFromSkinMeshTransforms")?)
                    {
                        for k in 0..count {
                            self.mul_vec3(offset + k * size + 48, s, "skin bone transforms")?;
                        }
                    }
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "localPs")?)
                    {
                        // hkPackedVector3 blocks: 16 × (i16 x, y, z, u16 exponent)
                        for k in 0..count {
                            for v in 0..16u32 {
                                let at = self.reflect.base + (offset + k * size + v * 8) as usize;
                                for component in 0..3 {
                                    let slot = at + component * 2;
                                    let packed = i16::from_le_bytes(
                                        self.raw[slot..slot + 2].try_into().unwrap_or([0; 2]),
                                    );
                                    let scaled =
                                        ((packed as f32) * s).round().clamp(-32768.0, 32767.0)
                                            as i16;
                                    self.raw[slot..slot + 2].copy_from_slice(&scaled.to_le_bytes());
                                }
                            }
                            match self
                                .report
                                .edits
                                .iter_mut()
                                .find(|(name, _)| name == "packed skin positions")
                            {
                                Some((_, n)) => *n += 16,
                                None => {
                                    self.report.edits.push(("packed skin positions".into(), 16))
                                }
                            }
                        }
                    }
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "localUnpackedPs")?)
                    {
                        for k in 0..count {
                            for v in 0..(size / 16) {
                                self.mul_vec3(
                                    offset + k * size + v * 16,
                                    s,
                                    "unpacked skin positions",
                                )?;
                            }
                        }
                    }
                }
                "hclBoneSpaceSkinPOperator" => {
                    for member in ["localPs", "localUnpackedPs"] {
                        if let Some((offset, count, size)) =
                            self.array(base + self.member(t, member)?)
                        {
                            for k in 0..count {
                                for v in 0..(size / 16) {
                                    self.mul_vec3(
                                        offset + k * size + v * 16,
                                        s,
                                        "bone-space skin positions",
                                    )?;
                                }
                            }
                        }
                    }
                }
                "hclSimpleMeshBoneDeformOperator" => {
                    if let Some((offset, count, size)) =
                        self.array(base + self.member(t, "localBoneTransforms")?)
                    {
                        for k in 0..count {
                            self.mul_vec3(offset + k * size + 48, s, "mesh-bone local transforms")?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
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
        let (bytes, report) = document.rescale_geometry(1.0).unwrap();
        assert_eq!(bytes.len(), document.raw.len());
        assert!(report
            .edits
            .iter()
            .any(|(name, _)| name == "particle rest positions"));
    }
}

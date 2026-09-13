//! Switch Toolbox compatible BFRES v10 editing.
//!
//! Loads a model file the way Toolbox's Syroot fork does, applies the same
//! implicit rewrites Toolbox performs on save (see [`prepare`]) and serializes
//! with a port of `ResFileSwitchSaver`, so that a plain "open and save" or a
//! name/texture edit produces exactly the bytes Toolbox would write.

mod geometry_import;
mod loader;
mod matrix;
mod model;
mod opentk;
pub(crate) mod patricia;
mod prepare;
mod saver;
mod skeleton_import;
mod strings;

pub use geometry_import::GeometryImportReport;
pub use model::{Bone, Material, Mesh, ResFile, Shape, VertexAttrib, VertexBuffer, VertexData};
pub use skeleton_import::SkeletonImportReport;
pub use strings::ExternalStrings;

use super::BfresError;

impl ResFile {
    /// Parses a little-endian BFRES v10 file. Vanilla TOTK models need the
    /// external string table so their hashed names can be resolved.
    pub fn load(data: &[u8], external: &ExternalStrings) -> Result<Self, BfresError> {
        loader::load(data, external)
    }

    /// Serializes the file exactly like Switch Toolbox does (raw, uncompressed).
    pub fn save_like_toolbox(&self) -> Result<Vec<u8>, BfresError> {
        saver::save(self)
    }

    /// Changes the name stored in the header (`--swap_int_name`).
    pub fn set_internal_name(&mut self, name: &str) {
        self.name = name.to_owned();
    }

    /// Renames the first model (`--swap_model_name`). Fails when the file
    /// holds two or more models, matching the Toolbox CLI contract.
    pub fn rename_first_model(&mut self, name: &str) -> Result<(), BfresError> {
        match self.models.len() {
            0 => Err(BfresError::new(0, "BFRES contains no model")),
            1 => {
                self.models[0].name = name.to_owned();
                Ok(())
            }
            count => Err(BfresError::new(
                0,
                format!("--swap_model_name needs a single model but the file has {count}"),
            )),
        }
    }

    /// Applies `name.replace(from, to)` to every texture slot of every
    /// material in every model (`--rename_tex`). Returns the changed count.
    pub fn rename_texture_slots(&mut self, from: &str, to: &str) -> usize {
        let mut changed = 0;
        for model in &mut self.models {
            for material in &mut model.materials {
                for texture in &mut material.texture_refs {
                    let renamed = texture.replace(from, to);
                    if renamed != *texture {
                        *texture = renamed;
                        changed += 1;
                    }
                }
            }
        }
        changed
    }

    /// Renames every model-level string that contains `from` (model path,
    /// shape, material, bone and user-data strings). Texture references are
    /// left to [`Self::rename_texture_slots`]. Returns the changed count.
    pub fn rename_model_strings(&mut self, from: &str, to: &str) -> usize {
        if from.is_empty() {
            return 0;
        }
        let mut changed = 0;
        let mut rename = |value: &mut String| {
            if value.contains(from) {
                *value = value.replace(from, to);
                changed += 1;
            }
        };
        for model in &mut self.models {
            rename(&mut model.path);
            for shape in &mut model.shapes {
                rename(&mut shape.name);
            }
            for material in &mut model.materials {
                rename(&mut material.name);
                for data in &mut material.user_data {
                    for value in &mut data.strings {
                        rename(value);
                    }
                }
            }
            for bone in &mut model.skeleton.bones {
                rename(&mut bone.name);
                for data in &mut bone.user_data {
                    for value in &mut data.strings {
                        rename(value);
                    }
                }
            }
            for data in &mut model.user_data {
                for value in &mut data.strings {
                    rename(value);
                }
            }
        }
        changed
    }

    /// Lists `(field, value)` pairs of every string that contains `needle`.
    pub fn strings_containing(&self, needle: &str) -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut push = |field: String, value: &str| {
            if value.contains(needle) {
                found.push((field, value.to_owned()));
            }
        };
        push("header.name".into(), &self.name);
        for (m, model) in self.models.iter().enumerate() {
            push(format!("model[{m}].name"), &model.name);
            push(format!("model[{m}].path"), &model.path);
            for (i, shape) in model.shapes.iter().enumerate() {
                push(format!("model[{m}].shape[{i}]"), &shape.name);
            }
            for (i, material) in model.materials.iter().enumerate() {
                push(format!("model[{m}].material[{i}]"), &material.name);
                for texture in &material.texture_refs {
                    push(format!("model[{m}].material[{i}].texture"), texture);
                }
                for (key, value) in material
                    .attrib_assign
                    .iter()
                    .chain(&material.sampler_assign)
                    .chain(&material.options)
                {
                    push(format!("model[{m}].material[{i}].assign"), key);
                    push(format!("model[{m}].material[{i}].assign"), value);
                }
                for data in &material.user_data {
                    for value in &data.strings {
                        push(format!("model[{m}].material[{i}].user_data"), value);
                    }
                }
                for info in &material.render_infos {
                    for value in &info.strings {
                        push(format!("model[{m}].material[{i}].render_info"), value);
                    }
                }
            }
            for (i, bone) in model.skeleton.bones.iter().enumerate() {
                push(format!("model[{m}].bone[{i}]"), &bone.name);
                for data in &bone.user_data {
                    for value in &data.strings {
                        push(format!("model[{m}].bone[{i}].user_data"), value);
                    }
                }
            }
            for data in &model.user_data {
                for value in &data.strings {
                    push(format!("model[{m}].user_data"), value);
                }
            }
        }
        for file in &self.external_files {
            push("external_file".into(), &file.name);
        }
        for value in &self.original_strings {
            push("original_strings".into(), value);
        }
        found
    }

    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Switch Toolbox's "Import Bones" option: merges the FBX skeleton into
    /// the first model (bones re-ordered to the FBX hierarchy, transforms
    /// updated beyond a tolerance, new bones added, missing bones dropped),
    /// regenerates the skinning palette from the FBX weights and rebinds the
    /// existing shapes and vertex skin indices to the new bones.
    pub fn import_skeleton_like_toolbox(
        &mut self,
        fbx: &[u8],
    ) -> Result<SkeletonImportReport, BfresError> {
        self.apply_fbx_skinning_like_toolbox(fbx, true)
    }

    /// Imports only the bones of an FBX (a merged skeleton, for example):
    /// the hierarchy, order and transforms follow the FBX like "Import
    /// Bones", but the model's existing skinning palette, shape bone lists
    /// and vertex skin indices are kept and re-pointed at the new bone
    /// indices instead of being regenerated from FBX weights, which a
    /// bones-only FBX does not carry. Dropping a bone the model still skins
    /// to is an error.
    pub fn import_skeleton_bones_only(
        &mut self,
        fbx: &[u8],
    ) -> Result<SkeletonImportReport, BfresError> {
        let imported = crate::parser::fbx::toolbox_skeleton::import_skeleton_like_toolbox(fbx)
            .map_err(|error| BfresError::new(0, error.to_string()))?;
        if imported.bones.is_empty() {
            return Err(BfresError::new(0, "the FBX contains no skeleton nodes"));
        }
        let model = self
            .models
            .first_mut()
            .ok_or_else(|| BfresError::new(0, "BFRES contains no model"))?;
        let old_matrix_to_bone = model.skeleton.matrix_to_bone.clone();
        let old_bone_names: Vec<String> = model
            .skeleton
            .bones
            .iter()
            .map(|bone| bone.name.clone())
            .collect();
        let (old_to_new, mut report) =
            skeleton_import::merge_imported_bones(model, &imported.bones);
        skeleton_import::remap_existing_skinning(
            model,
            &old_matrix_to_bone,
            &old_to_new,
            &old_bone_names,
        )
        .map_err(|error| BfresError::new(0, error))?;
        report.smooth_count = model
            .skeleton
            .bones
            .iter()
            .filter(|bone| bone.smooth_matrix_index != -1)
            .count();
        report.rigid_count = model
            .skeleton
            .bones
            .iter()
            .filter(|bone| bone.rigid_matrix_index != -1)
            .count();
        Ok(report)
    }

    /// What every Toolbox model import does to the skeleton even without
    /// "Import Bones": the skinning palette and each shape's skin count and
    /// bone list are regenerated from the FBX weights; bones stay as they are.
    pub fn regenerate_skinning_like_toolbox(
        &mut self,
        fbx: &[u8],
    ) -> Result<SkeletonImportReport, BfresError> {
        self.apply_fbx_skinning_like_toolbox(fbx, false)
    }

    fn apply_fbx_skinning_like_toolbox(
        &mut self,
        fbx: &[u8],
        import_bones: bool,
    ) -> Result<SkeletonImportReport, BfresError> {
        let imported = crate::parser::fbx::toolbox_skeleton::import_skeleton_like_toolbox(fbx)
            .map_err(|error| BfresError::new(0, error.to_string()))?;
        let model = self
            .models
            .first_mut()
            .ok_or_else(|| BfresError::new(0, "BFRES contains no model"))?;
        let old_matrix_to_bone = model.skeleton.matrix_to_bone.clone();
        let (old_to_new, mut report) = if import_bones {
            if imported.bones.is_empty() {
                return Err(BfresError::new(0, "the FBX contains no skeleton nodes"));
            }
            skeleton_import::merge_imported_bones(model, &imported.bones)
        } else {
            let identity = (0..model.skeleton.bones.len())
                .map(|index| Some(index as u16))
                .collect();
            let report = SkeletonImportReport {
                bones_before: model.skeleton.bones.len(),
                bones_after: model.skeleton.bones.len(),
                ..Default::default()
            };
            (identity, report)
        };
        skeleton_import::regenerate_skinning_palette(model, &imported.mesh_bone_usage, &mut report);
        skeleton_import::rebind_geometry(
            model,
            &old_matrix_to_bone,
            &old_to_new,
            &imported.mesh_bone_usage,
            import_bones,
        )
        .map_err(|error| BfresError::new(0, error))?;
        Ok(report)
    }

    /// Switch Toolbox's model replacement from an FBX (`--fbx`, optionally
    /// with "Import Bones"): every shape of the first model is rebuilt from
    /// the Assimp meshes with Toolbox's attribute layout, skinning palette,
    /// tangents and bounding boxes, byte for byte like the Toolbox CLI.
    pub fn import_model_like_toolbox(
        &mut self,
        fbx: &[u8],
        import_bones: bool,
    ) -> Result<GeometryImportReport, BfresError> {
        let model = self
            .models
            .first_mut()
            .ok_or_else(|| BfresError::new(0, "BFRES contains no model"))?;
        geometry_import::import_model(model, fbx, import_bones)
    }

    pub fn first_model_name(&self) -> Option<&str> {
        self.models.first().map(|model| model.name.as_str())
    }
}

/// Bind-pose position of a bone in model space (Toolbox's world transform
/// of the bone chain, translation row).
pub fn bone_world_position(bones: &[Bone], index: usize) -> [f32; 3] {
    let world = matrix::world_transform(bones, index);
    [world.m41, world.m42, world.m43]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn external() -> Option<ExternalStrings> {
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        ExternalStrings::from_romfs(romfs).ok()
    }

    /// Diagnostic: every `tmp/_CLAUDE/parity/toolbox_raw/*.resave.bfres` is the
    /// decompressed Toolbox resave of `tmp/bfres/<name>.bfres`; the native
    /// resave has to match it byte for byte.
    #[test]
    #[ignore = "needs the TOTK dump and the Toolbox reference outputs"]
    fn resaves_match_toolbox_reference_outputs() {
        let Some(external) = external() else {
            return;
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/_CLAUDE/parity/toolbox_raw");
        let Ok(entries) = std::fs::read_dir(&root) else {
            return;
        };
        let mut failures = Vec::new();
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let original_name = name.replace(".resave.bfres", ".bfres");
            let original = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/bfres")
                    .join(&original_name),
            )
            .unwrap();
            let expected = std::fs::read(&path).unwrap();
            let actual = match ResFile::load(&original, &external)
                .and_then(|file| file.save_like_toolbox())
            {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("{name}: {error}");
                    failures.push(name);
                    continue;
                }
            };
            let first = actual
                .iter()
                .zip(&expected)
                .position(|(left, right)| left != right);
            let differing = actual
                .iter()
                .zip(&expected)
                .filter(|(left, right)| left != right)
                .count();
            eprintln!(
                "{name}: actual={} expected={} first_diff={first:?} differing_bytes={differing}",
                actual.len(),
                expected.len()
            );
            std::fs::write(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tmp/_CLAUDE/parity")
                    .join(format!("{original_name}.totkbits.bfres")),
                &actual,
            )
            .unwrap();
            if first.is_some() || actual.len() != expected.len() {
                failures.push(name);
            }
        }
        assert!(
            failures.is_empty(),
            "native resave differs from Toolbox: {failures:?}"
        );
    }

    /// Diagnostic: whole-file parity of the Toolbox model import. The
    /// `tmp/_CLAUDE/parity/skeleton` references are Toolbox CLI `--fbx`
    /// outputs on vanilla models and `tmp/_CLAUDE/cape_cmp/good.bfres` is
    /// the Toolbox CLI `--fbx --import_skeleton` output on a TotkBits
    /// generated model (`bad.bfres`). Every byte has to match; the native
    /// output is written next to each reference as `*.totkbits.bfres`.
    #[test]
    #[ignore = "needs the TOTK dump, the FBX fixtures and the Toolbox reference outputs"]
    fn model_import_matches_toolbox_reference_outputs() {
        let Some(external) = external() else {
            return;
        };
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let skeleton = manifest.join("../tmp/_CLAUDE/parity/skeleton");
        let cape = manifest.join("../tmp/_CLAUDE/cape_cmp");
        let mut cases: Vec<(
            String,
            std::path::PathBuf,
            std::path::PathBuf,
            bool,
            std::path::PathBuf,
        )> = vec![(
            "cape".to_owned(),
            cape.join("bad.bfres"),
            manifest.join("../res/1/untitled1.fbx"),
            true,
            cape.join("good.bfres"),
        )];
        for (tag, source, fbx) in [
            (
                "sword022",
                manifest.join("../tmp/bfres/Weapon_Sword_022.Weapon_Sword_022.bfres"),
                manifest.join("../tmp/Weapon_Sword_022.fbx"),
            ),
            (
                "lsword108_untitled",
                romfs.join("Model/Weapon_Lsword_108.Weapon_Lsword_108.bfres.mc"),
                manifest.join("../tmp/untitled.fbx"),
            ),
            (
                "lsword108_005",
                romfs.join("Model/Weapon_Lsword_108.Weapon_Lsword_108.bfres.mc"),
                manifest.join("../tmp/Weapon_Lsword_005.fbx"),
            ),
        ] {
            for import_bones in [true, false] {
                let variant = if import_bones { "import" } else { "plain" };
                cases.push((
                    format!("{tag}.{variant}"),
                    source.clone(),
                    fbx.clone(),
                    import_bones,
                    skeleton.join(format!("{tag}.{variant}.toolbox.bfres.mc.raw")),
                ));
            }
        }
        let mut failures = Vec::new();
        let mut checked = 0;
        for (name, source, fbx, import_bones, reference) in cases {
            let (Ok(source_bytes), Ok(fbx_bytes), Ok(expected)) = (
                std::fs::read(&source),
                std::fs::read(&fbx),
                std::fs::read(&reference),
            ) else {
                continue;
            };
            checked += 1;
            let raw = if crate::Settings::Magic::is_mcpk(&source_bytes) {
                crate::compression::meshcodec::MeshCodec::decompress(&source_bytes).unwrap()
            } else {
                source_bytes
            };
            let mut file = ResFile::load(&raw, &external).unwrap();
            if let Err(error) = file.import_model_like_toolbox(&fbx_bytes, import_bones) {
                failures.push(format!("{name}: import failed: {error}"));
                continue;
            }
            let actual = file.save_like_toolbox().unwrap();
            let output = reference.with_extension("totkbits.bfres");
            std::fs::write(&output, &actual).unwrap();
            // The references were decompressed from MCPK, which pads the
            // logical file size; only the logical bytes are compared.
            let expected = &expected[..expected.len().min(actual.len().max(4))];
            if actual != expected {
                let first = actual
                    .iter()
                    .zip(expected.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(actual.len().min(expected.len()));
                let differing = actual
                    .iter()
                    .zip(expected.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                failures.push(format!(
                    "{name}: {} vs {} bytes, first difference at 0x{first:x}, {differing} differing bytes in the common prefix ({})",
                    actual.len(),
                    expected.len(),
                    output.display()
                ));
            }
        }
        assert!(checked > 0, "no reference outputs found");
        assert!(
            failures.is_empty(),
            "{}",
            failures.join(
                "
"
            )
        );
    }

    /// Diagnostic: `tmp/_CLAUDE/parity/skeleton/<case>.<import|plain>.toolbox.bfres.mc.raw`
    /// are the decompressed outputs of the Toolbox CLI run with `--fbx` (and
    /// `--import_skeleton` for the `import` variant); see
    /// `tmp/_CLAUDE/parity/compare_skeleton.py`. The skeleton (bones, flags,
    /// transforms, matrix palette) and every shape's skinning written by the
    /// native path have to match them exactly.
    #[test]
    #[ignore = "needs the TOTK dump, the FBX fixtures and the Toolbox reference outputs"]
    fn skeleton_import_matches_toolbox_reference_outputs() {
        let Some(external) = external() else {
            return;
        };
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let cases = [
            (
                "sword022",
                manifest.join("../tmp/bfres/Weapon_Sword_022.Weapon_Sword_022.bfres"),
                manifest.join("../tmp/Weapon_Sword_022.fbx"),
            ),
            (
                "lsword108_untitled",
                romfs.join("Model/Weapon_Lsword_108.Weapon_Lsword_108.bfres.mc"),
                manifest.join("../tmp/untitled.fbx"),
            ),
            (
                "lsword108_005",
                romfs.join("Model/Weapon_Lsword_108.Weapon_Lsword_108.bfres.mc"),
                manifest.join("../tmp/Weapon_Lsword_005.fbx"),
            ),
        ];
        let references = manifest.join("../tmp/_CLAUDE/parity/skeleton");
        let mut failures = Vec::new();
        let mut checked = 0;
        for (tag, source, fbx) in cases {
            let (Ok(source_bytes), Ok(fbx_bytes)) = (std::fs::read(&source), std::fs::read(&fbx))
            else {
                continue;
            };
            let raw = if crate::Settings::Magic::is_mcpk(&source_bytes) {
                crate::compression::meshcodec::MeshCodec::decompress(&source_bytes).unwrap()
            } else {
                source_bytes
            };
            for import_bones in [true, false] {
                let variant = if import_bones { "import" } else { "plain" };
                let reference = references.join(format!("{tag}.{variant}.toolbox.bfres.mc.raw"));
                let Ok(expected_bytes) = std::fs::read(&reference) else {
                    continue;
                };
                checked += 1;
                let name = format!("{tag}.{variant}");
                let replaced =
                    crate::file_format::Model3D::bfres::BfresFile::replace_geometry_from_fbx(
                        &raw, &fbx_bytes,
                    )
                    .unwrap();
                let mut actual = ResFile::load(&replaced, &external).unwrap();
                let report = if import_bones {
                    actual.import_skeleton_like_toolbox(&fbx_bytes).unwrap()
                } else {
                    actual.regenerate_skinning_like_toolbox(&fbx_bytes).unwrap()
                };
                // Round-trip through the saver so the compared skeleton is the
                // one that reaches the file (inverse matrices included).
                let saved = actual.save_like_toolbox().unwrap();
                let actual = ResFile::load(&saved, &ExternalStrings::empty()).unwrap();
                let expected = ResFile::load(&expected_bytes, &ExternalStrings::empty()).unwrap();
                let (expected_model, actual_model) = (&expected.models[0], &actual.models[0]);
                eprintln!(
                    "{name}: bones {} -> {} (added {:?}, removed {:?}, updated {:?}), palette {} smooth + {} rigid",
                    report.bones_before,
                    report.bones_after,
                    report.added,
                    report.removed,
                    report.transforms_updated,
                    report.smooth_count,
                    report.rigid_count
                );
                if expected_model.skeleton != actual_model.skeleton {
                    eprintln!(
                        "{name}: skeleton differs\n  expected {:#?}\n  actual {:#?}",
                        expected_model.skeleton, actual_model.skeleton
                    );
                    failures.push(format!("{name}: skeleton"));
                }
                for expected_shape in &expected_model.shapes {
                    let Some(actual_shape) = actual_model
                        .shapes
                        .iter()
                        .find(|shape| shape.name == expected_shape.name)
                    else {
                        failures.push(format!("{name}: shape {} missing", expected_shape.name));
                        continue;
                    };
                    let expected_skin = (
                        expected_shape.bone_index,
                        expected_shape.vertex_skin_count,
                        &expected_shape.skin_bone_indices,
                    );
                    let actual_skin = (
                        actual_shape.bone_index,
                        actual_shape.vertex_skin_count,
                        &actual_shape.skin_bone_indices,
                    );
                    if expected_skin != actual_skin {
                        eprintln!(
                            "{name}: shape {} skinning differs: expected {expected_skin:?} actual {actual_skin:?}",
                            expected_shape.name
                        );
                        failures.push(format!("{name}: shape {}", expected_shape.name));
                    }
                }
            }
        }
        eprintln!("checked {checked} reference outputs");
        assert!(
            failures.is_empty(),
            "skeleton import differs from Toolbox: {failures:?}"
        );
    }
}

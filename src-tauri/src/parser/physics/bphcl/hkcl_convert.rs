//! Native HKCL (BOTW, hk2014) → BPHCL (TOTK, hk2022) cloth converter.
//!
//! Every object of the source cloth package is rebuilt through the TOTK type
//! reflection (`ObjectWriter`) following the conventions measured on the
//! 384-file vanilla corpus and on Nintendo's own BOTW→TOTK ports:
//! operator IDs become indices, operators and cloth states carry buffer /
//! transform-set usages with `TransformTracker` bit fields, every state gets a
//! linear `hclStateDependencyGraph`, simulate operators keep their solver
//! settings inside a `Default Config`, packed skin positions are re-encoded,
//! landscape collision is switched on and bones that belong to the player
//! skeleton get the `Link:` prefix.

use super::reflect::TypeKind;
use super::{ItemRef, ObjectWriter};
use crate::parser::physics::hkcl::{
    BlendLayouts, BlockLayout, ClothData, ClothPackage, ClothState, CollidableData, ConstraintKind,
    ConstraintSet, HkclDocument, Operator, ShapeData, SimClothData, SkeletonData, SkinOperator,
};
use roead::aamp::ParameterIO;
use std::{
    collections::{HashMap, HashSet},
    io::{self, ErrorKind},
};

#[derive(Clone, Debug, Default)]
pub struct ConvertOptions {
    /// Bones (and collidables attached to them) that belong to the player
    /// skeleton and are referenced as `Link:<bone>` by armor cloth files.
    pub link_bones: HashSet<String>,
    /// Convert only these cloths (all when empty).
    pub cloth_names: Vec<String>,
    /// `BaseBone` / `Preset` overrides for the embedded registration.
    pub presets: HashMap<String, (String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct ConvertReport {
    pub cloths: Vec<String>,
    pub collidables: Vec<String>,
    pub skeletons: Vec<String>,
    /// `(cloth, BaseBone, Preset)` written into the embedded registration.
    pub registrations: Vec<(String, String, String)>,
    pub warnings: Vec<String>,
}

const TARGET_PLATFORM: u32 = 0x20000;
const LANDSCAPE_BIT: u32 = 0x8000_0000;
const OBJECT_KINDS: [u32; 8] = [8, 7, 6, 5, 4, 3, 2, 1];
const BONE_KINDS: [u32; 4] = [4, 3, 2, 1];

/// Deformer block layouts from the TOTK type table.
pub fn blend_layouts(writer: &ObjectWriter<'_>) -> BlendLayouts {
    let reflect = writer.reflect();
    let mut layouts = BlendLayouts {
        object_space: HashMap::new(),
        bone_space: HashMap::new(),
    };
    for (prefix, kinds, map) in [
        ("hclObjectSpaceDeformer", 8u32, &mut layouts.object_space),
        ("hclBoneSpaceDeformer", 4u32, &mut layouts.bone_space),
    ] {
        for (n, word) in [
            (1u32, "One"),
            (2, "Two"),
            (3, "Three"),
            (4, "Four"),
            (5, "Five"),
            (6, "Six"),
            (7, "Seven"),
            (8, "Eight"),
        ] {
            if n > kinds {
                break;
            }
            let Some(t) = reflect.find_type(&format!("{prefix}::{word}BlendEntryBlock")) else {
                continue;
            };
            let (Some(vertices), Some(bones)) = (
                reflect.member(t, "vertexIndices"),
                reflect.member(t, "boneIndices"),
            ) else {
                continue;
            };
            let count = |type_index| match reflect.format_of(type_index).kind {
                TypeKind::InlineArray(c) => c,
                _ => 0,
            };
            let vertex_count = count(vertices.type_index);
            let bone_count = count(bones.type_index);
            map.insert(
                n,
                BlockLayout {
                    element_size: reflect.size_of(t),
                    vertex_count,
                    bones_per_vertex: if vertex_count > 0 {
                        bone_count / vertex_count
                    } else {
                        0
                    },
                    has_weights: reflect.member(t, "boneWeights").is_some(),
                },
            );
        }
    }
    layouts
}

/// Converts `hkcl` into a complete TOTK cloth file. `types` supplies the
/// TOTK TYPE section (see `type_donor`); the returned bytes include the
/// embedded AAMP registration.
pub fn convert_hkcl_to_bphcl(
    hkcl: &HkclDocument,
    types: &super::BphclDocument,
    type_section: &[u8],
    options: &ConvertOptions,
) -> io::Result<(Vec<u8>, ConvertReport)> {
    let writer = ObjectWriter::new(types)?;
    let layouts = blend_layouts(&writer);
    let package = hkcl.cloth_package(&layouts)?;
    let mut converter = Converter {
        writer,
        package: &package,
        options,
        report: ConvertReport::default(),
        collidable_items: Vec::new(),
        skeleton_items: HashMap::new(),
        string_type: 0,
        bone_names: HashMap::new(),
    };
    converter.string_type = converter.writer.type_index("hkStringPtr")?;
    let aamp = converter.run()?;
    let Converter { writer, report, .. } = converter;
    let bytes = writer.finish(type_section, &aamp)?;
    let rebuilt = super::BphclDocument::parse(&bytes)?;
    rebuilt.validate_item_graph()?;
    Ok((bytes, report))
}

struct Converter<'a> {
    writer: ObjectWriter<'a>,
    package: &'a ClothPackage,
    options: &'a ConvertOptions,
    report: ConvertReport,
    collidable_items: Vec<ItemRef>,
    /// Skeleton name → item, converted bone names.
    skeleton_items: HashMap<String, ItemRef>,
    string_type: u32,
    bone_names: HashMap<String, Vec<String>>,
}

impl<'a> Converter<'a> {
    fn prefixed_bone(&self, name: &str) -> String {
        if self.options.link_bones.contains(name) {
            format!("Link:{name}")
        } else {
            name.to_owned()
        }
    }

    fn run(&mut self) -> io::Result<Vec<u8>> {
        // Root container first so the file starts like a vanilla one.
        let root_type = self.writer.type_index("hkRootLevelContainer")?;
        let root = self.writer.alloc(root_type, 1, true)?;
        let variant_type = self
            .writer
            .type_index("hkRootLevelContainer::NamedVariant")?;
        let variants = self.writer.alloc(variant_type, 3, false)?;
        let variants_patch = self.writer.array_patch_type(variant_type)?;
        self.writer.set_array(root, 0, variants, variants_patch)?;

        let cloth_container_type = self.writer.type_index("hclClothContainer")?;
        let cloth_container = self.writer.alloc(cloth_container_type, 1, true)?;
        let memory_type = self.writer.type_index("hkMemoryResourceContainer")?;
        let memory = self.writer.alloc(memory_type, 1, true)?;
        let animation_type = self.writer.type_index("hkaAnimationContainer")?;
        let animation = self.writer.alloc(animation_type, 1, true)?;
        let ref_variant = self.writer.type_index("hkRefVariant")?;
        for (slot, (name, class, target)) in [
            ("Cloth Container", "hclClothContainer", cloth_container),
            ("Resource Data", "hkMemoryResourceContainer", memory),
            ("Animation Container", "hkaAnimationContainer", animation),
        ]
        .into_iter()
        .enumerate()
        {
            let base = slot as u32 * 24;
            self.writer.set_string(variants, base, name)?;
            self.writer.set_string(variants, base + 8, class)?;
            self.writer
                .set_pointer(variants, base + 16, target, ref_variant)?;
        }
        let memory_name = self.writer.member_offset(memory_type, "name")?;
        self.writer.set_string(memory, memory_name, "")?;

        // Collidables (shared by every cloth).
        let package = self.package;
        let attached = self.collidable_bones();
        for (index, collidable) in package.collidables.iter().enumerate() {
            let bone = attached.get(&index).cloned();
            let item = self.collidable(collidable, bone.as_deref())?;
            self.collidable_items.push(item);
        }
        let collidable_type = self.writer.type_index("hclCollidable")?;
        let collidable_storage =
            self.writer
                .pointer_storage(collidable_type, &self.collidable_items.clone(), true)?;
        let collidables_field = self
            .writer
            .member_offset(cloth_container_type, "collidables")?;
        let ref_ptr_collidable = self.writer.ref_ptr_patch_type(collidable_type)?;
        let collidables_patch = self.writer.array_patch_type(ref_ptr_collidable)?;
        self.writer.set_array(
            cloth_container,
            collidables_field,
            collidable_storage,
            collidables_patch,
        )?;

        // Skeletons.
        let selected_cloths: Vec<&ClothData> = package
            .cloths
            .iter()
            .filter(|cloth| {
                self.options.cloth_names.is_empty()
                    || self.options.cloth_names.contains(&cloth.name)
            })
            .collect();
        let needed_skeletons: HashSet<String> = selected_cloths
            .iter()
            .flat_map(|cloth| cloth.transform_sets.iter().map(|set| set.name.clone()))
            .collect();
        let mut skeleton_refs = Vec::new();
        for skeleton in &package.skeletons {
            if !needed_skeletons.contains(&skeleton.name) {
                continue;
            }
            let item = self.skeleton(skeleton)?;
            self.skeleton_items.insert(skeleton.name.clone(), item);
            skeleton_refs.push(item);
        }
        let skeleton_type = self.writer.type_index("hkaSkeleton")?;
        let skeleton_storage = self
            .writer
            .pointer_storage(skeleton_type, &skeleton_refs, true)?;
        let skeletons_field = self.writer.member_offset(animation_type, "skeletons")?;
        let ref_ptr_skeleton = self.writer.ref_ptr_patch_type(skeleton_type)?;
        let skeletons_patch = self.writer.array_patch_type(ref_ptr_skeleton)?;
        self.writer.set_array(
            animation,
            skeletons_field,
            skeleton_storage,
            skeletons_patch,
        )?;

        // Cloths.
        let mut cloth_refs = Vec::new();
        for cloth in &selected_cloths {
            let item = self.cloth(cloth)?;
            cloth_refs.push(item);
            self.report.cloths.push(cloth.name.clone());
        }
        let cloth_type = self.writer.type_index("hclClothData")?;
        let cloth_storage = self.writer.pointer_storage(cloth_type, &cloth_refs, true)?;
        let cloths_field = self
            .writer
            .member_offset(cloth_container_type, "clothDatas")?;
        let ref_ptr_cloth = self.writer.ref_ptr_patch_type(cloth_type)?;
        let cloths_patch = self.writer.array_patch_type(ref_ptr_cloth)?;
        self.writer
            .set_array(cloth_container, cloths_field, cloth_storage, cloths_patch)?;

        self.registration(&selected_cloths)
    }

    /// Collidable index → name of the bone it is attached to, from the
    /// cloths' collidable transform maps.
    fn collidable_bones(&self) -> HashMap<usize, String> {
        let mut out = HashMap::new();
        for cloth in &self.package.cloths {
            let skeleton = cloth
                .transform_sets
                .first()
                .and_then(|set| self.package.skeletons.iter().find(|s| s.name == set.name));
            for sim in &cloth.sim_cloths {
                for (slot, collidable) in sim.per_instance_collidables.iter().enumerate() {
                    if let (Some(bone), Some(skeleton)) =
                        (sim.collidable_transform_indices.get(slot), skeleton)
                    {
                        if let Some((name, _)) = skeleton.bones.get(*bone as usize) {
                            out.entry(*collidable).or_insert_with(|| name.clone());
                        }
                    }
                }
            }
        }
        out
    }

    fn collidable(&mut self, source: &CollidableData, bone: Option<&str>) -> io::Result<ItemRef> {
        let w = &mut self.writer;
        let t = w.type_index("hclCollidable")?;
        let item = w.alloc(t, 1, true)?;
        w.set_matrix4(item, w.member_offset(t, "transform")?, source.transform)?;
        w.set_vec4(
            item,
            w.member_offset(t, "linearVelocity")?,
            source.linear_velocity,
        )?;
        w.set_vec4(
            item,
            w.member_offset(t, "angularVelocity")?,
            source.angular_velocity,
        )?;
        let link = bone.is_some_and(|bone| self.options.link_bones.contains(bone))
            || (bone.is_none()
                && source
                    .name
                    .strip_prefix("Collidable_")
                    .is_some_and(|rest| self.options.link_bones.contains(rest)));
        let name = if link && !source.name.starts_with("Link:") {
            format!("Link:{}", source.name)
        } else {
            source.name.clone()
        };
        w.set_string(item, w.member_offset(t, "name")?, &name)?;
        w.set_f32(item, w.member_offset(t, "pinchDetectionRadius")?, 0.01)?;
        w.set_u8(
            item,
            w.member_offset(t, "pinchDetectionPriority")?,
            source.pinch_detection_priority as u8,
        )?;
        w.set_u8(
            item,
            w.member_offset(t, "pinchDetectionEnabled")?,
            source.pinch_detection_enabled as u8,
        )?;
        w.set_u8(
            item,
            w.member_offset(t, "virtualCollisionPointCollisionEnabled")?,
            0,
        )?;
        w.set_u8(item, w.member_offset(t, "enabled")?, 1)?;
        let shape_field = w.member_offset(t, "shape")?;
        let shape_patch = w.pointer_patch_type(w.type_index("hclShape")?)?;
        match &source.shape {
            Some(ShapeData::Capsule {
                start,
                end,
                dir,
                radius,
                cap_len_sqrd_inv,
            }) => {
                let st = w.type_index("hclCapsuleShape")?;
                let shape = w.alloc(st, 1, true)?;
                w.set_vec4(shape, w.member_offset(st, "start")?, *start)?;
                w.set_vec4(shape, w.member_offset(st, "end")?, *end)?;
                w.set_vec4(shape, w.member_offset(st, "dir")?, *dir)?;
                w.set_f32(shape, w.member_offset(st, "radius")?, *radius)?;
                let inverse = if *cap_len_sqrd_inv > 0.0 {
                    *cap_len_sqrd_inv
                } else {
                    let length_squared: f32 = (0..3).map(|i| (end[i] - start[i]).powi(2)).sum();
                    1.0 / length_squared.max(1.0e-4)
                };
                w.set_f32(shape, w.member_offset(st, "capLenSqrdInv")?, inverse)?;
                w.set_pointer(item, shape_field, shape, shape_patch)?;
            }
            Some(ShapeData::Sphere(sphere)) => {
                let st = w.type_index("hclSphereShape")?;
                let shape = w.alloc(st, 1, true)?;
                w.set_vec4(shape, w.member_offset(st, "sphere")?, *sphere)?;
                w.set_pointer(item, shape_field, shape, shape_patch)?;
            }
            Some(ShapeData::Plane(plane)) => {
                let st = w.type_index("hclPlaneShape")?;
                let shape = w.alloc(st, 1, true)?;
                w.set_vec4(shape, w.member_offset(st, "planeEquation")?, *plane)?;
                w.set_pointer(item, shape_field, shape, shape_patch)?;
            }
            None => self.report.warnings.push(format!(
                "collidable {} has an unsupported shape",
                source.name
            )),
        }
        self.report.collidables.push(name);
        Ok(item)
    }

    fn skeleton(&mut self, source: &SkeletonData) -> io::Result<ItemRef> {
        let names: Vec<String> = source
            .bones
            .iter()
            .map(|(name, _)| self.prefixed_bone(name))
            .collect();
        self.bone_names.insert(source.name.clone(), names.clone());
        let w = &mut self.writer;
        let t = w.type_index("hkaSkeleton")?;
        let item = w.alloc(t, 1, true)?;
        w.set_string(item, w.member_offset(t, "name")?, &source.name)?;
        let parents = w.i16_storage(&source.parent_indices)?;
        let int16 = w.type_index("hkInt16")?;
        w.set_array(
            item,
            w.member_offset(t, "parentIndices")?,
            parents,
            w.array_patch_type(int16)?,
        )?;
        let bone_type = w.type_index("hkaBone")?;
        let bones = w.alloc(bone_type, names.len() as u32, false)?;
        let bone_size = w.size_of(bone_type);
        let lock = w.member_offset(bone_type, "lockTranslation")?;
        for (k, (name, (_, locked))) in names.iter().zip(&source.bones).enumerate() {
            let base = k as u32 * bone_size;
            w.set_string(bones, base, name)?;
            w.set_u8(bones, base + lock, *locked as u8)?;
        }
        w.set_array(
            item,
            w.member_offset(t, "bones")?,
            bones,
            w.array_patch_type(bone_type)?,
        )?;
        let pose_type = w.type_index("hkQsTransform")?;
        let pose = w.alloc(pose_type, source.reference_pose.len() as u32, false)?;
        for (k, transform) in source.reference_pose.iter().enumerate() {
            let base = k as u32 * 48;
            w.set_vec4(pose, base, transform[0])?;
            w.set_vec4(pose, base + 16, transform[1])?;
            w.set_vec4(pose, base + 32, transform[2])?;
        }
        w.set_array(
            item,
            w.member_offset(t, "referencePose")?,
            pose,
            w.array_patch_type(pose_type)?,
        )?;
        self.report.skeletons.push(source.name.clone());
        Ok(item)
    }

    fn cloth(&mut self, source: &ClothData) -> io::Result<ItemRef> {
        let skeleton_name = source
            .transform_sets
            .first()
            .map(|set| set.name.clone())
            .unwrap_or_default();
        let bone_count = self
            .bone_names
            .get(&skeleton_name)
            .map(|names| names.len())
            .unwrap_or(0);
        let simulate_index = source
            .operators
            .iter()
            .position(|op| matches!(op, Operator::Simulate { .. }));

        let t = self.writer.type_index("hclClothData")?;
        let item = self.writer.alloc(t, 1, true)?;
        self.writer
            .set_string(item, self.writer.member_offset(t, "name")?, &source.name)?;

        // Simulation data.
        let mut sim_refs = Vec::new();
        for sim in &source.sim_cloths {
            sim_refs.push(self.sim_cloth(sim, simulate_index)?);
        }
        let sim_type = self.writer.type_index("hclSimClothData")?;
        let sims = self.writer.pointer_storage(sim_type, &sim_refs, false)?;
        let sim_ptr = self.writer.pointer_patch_type(sim_type)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "simClothDatas")?,
            sims,
            self.writer.array_patch_type(sim_ptr)?,
        )?;

        // Buffers.
        let mut buffer_refs = Vec::new();
        for buffer in &source.buffers {
            let w = &mut self.writer;
            let bt = w.type_index(if buffer.is_scratch {
                "hclScratchBufferDefinition"
            } else {
                "hclBufferDefinition"
            })?;
            let b = w.alloc(bt, 1, true)?;
            w.set_string(b, w.member_offset(bt, "meshName")?, &source.name)?;
            let buffer_name = if buffer.is_scratch {
                "SkinScratchBuf"
            } else {
                buffer.name.as_str()
            };
            w.set_string(b, w.member_offset(bt, "bufferName")?, buffer_name)?;
            w.set_u32(b, w.member_offset(bt, "type")?, buffer.buffer_type as u32)?;
            w.set_u32(b, w.member_offset(bt, "subType")?, buffer.sub_type as u32)?;
            w.set_u32(b, w.member_offset(bt, "numVertices")?, buffer.num_vertices)?;
            w.set_u32(
                b,
                w.member_offset(bt, "numTriangles")?,
                buffer.num_triangles,
            )?;
            w.set_bytes(b, w.member_offset(bt, "bufferLayout")?, &buffer.layout)?;
            if buffer.is_scratch {
                if !buffer.triangle_indices.is_empty() {
                    let storage = w.u16_storage(&buffer.triangle_indices)?;
                    let u16t = w.type_index("hkUint16")?;
                    w.set_array(
                        b,
                        w.member_offset(bt, "triangleIndices")?,
                        storage,
                        w.array_patch_type(u16t)?,
                    )?;
                }
                w.set_u8(
                    b,
                    w.member_offset(bt, "storeNormals")?,
                    buffer.store_normals as u8,
                )?;
                w.set_u8(
                    b,
                    w.member_offset(bt, "storeTangentsAndBiTangents")?,
                    buffer.store_tangents_and_bi_tangents as u8,
                )?;
            }
            buffer_refs.push(b);
        }
        let buffer_base = self.writer.type_index("hclBufferDefinition")?;
        let buffers = self
            .writer
            .pointer_storage(buffer_base, &buffer_refs, false)?;
        let buffer_ptr = self.writer.pointer_patch_type(buffer_base)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "bufferDefinitions")?,
            buffers,
            self.writer.array_patch_type(buffer_ptr)?,
        )?;

        // Transform sets.
        let mut set_refs = Vec::new();
        for set in &source.transform_sets {
            let w = &mut self.writer;
            let st = w.type_index("hclTransformSetDefinition")?;
            let s = w.alloc(st, 1, true)?;
            w.set_string(s, w.member_offset(st, "name")?, &set.name)?;
            w.set_u32(s, w.member_offset(st, "type")?, set.set_type as u32)?;
            w.set_u32(s, w.member_offset(st, "numTransforms")?, set.num_transforms)?;
            set_refs.push(s);
        }
        let set_type = self.writer.type_index("hclTransformSetDefinition")?;
        let sets = self.writer.pointer_storage(set_type, &set_refs, false)?;
        let set_ptr = self.writer.pointer_patch_type(set_type)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "transformSetDefinitions")?,
            sets,
            self.writer.array_patch_type(set_ptr)?,
        )?;

        // Operators.
        let mut op_refs = Vec::new();
        for (index, op) in source.operators.iter().enumerate() {
            op_refs.push(self.operator(op, index as u32, source, bone_count)?);
        }
        let op_type = self.writer.type_index("hclOperator")?;
        let ops = self.writer.pointer_storage(op_type, &op_refs, false)?;
        let op_ptr = self.writer.pointer_patch_type(op_type)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "operators")?,
            ops,
            self.writer.array_patch_type(op_ptr)?,
        )?;

        // States.
        let mut state_refs = Vec::new();
        for state in &source.states {
            state_refs.push(self.state(state, bone_count)?);
        }
        let state_type = self.writer.type_index("hclClothState")?;
        let states = self
            .writer
            .pointer_storage(state_type, &state_refs, false)?;
        let state_ptr = self.writer.pointer_patch_type(state_type)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "clothStateDatas")?,
            states,
            self.writer.array_patch_type(state_ptr)?,
        )?;

        self.writer
            .set_u8(item, self.writer.member_offset(t, "generatedAtRuntime")?, 0)?;
        self.writer.set_u32(
            item,
            self.writer.member_offset(t, "targetPlatform")?,
            TARGET_PLATFORM,
        )?;
        Ok(item)
    }

    fn sim_cloth(
        &mut self,
        source: &SimClothData,
        simulate_index: Option<usize>,
    ) -> io::Result<ItemRef> {
        let collidable_items = self.collidable_items.clone();
        let w = &mut self.writer;
        let t = w.type_index("hclSimClothData")?;
        let item = w.alloc(t, 1, true)?;
        w.set_string(item, w.member_offset(t, "name")?, &source.name)?;
        let info = w.member_offset(t, "simulationInfo")?;
        let info_type = w.member_type(t, "simulationInfo")?;
        w.set_vec4(
            item,
            info + w.member_offset(info_type, "gravity")?,
            source.gravity,
        )?;
        w.set_f32(
            item,
            info + w.member_offset(info_type, "globalDampingPerSecond")?,
            source.global_damping_per_second,
        )?;

        let particle_type = w.type_index("hclSimClothData::ParticleData")?;
        let particles = w.alloc(particle_type, source.particles.len() as u32, false)?;
        for (k, p) in source.particles.iter().enumerate() {
            let base = k as u32 * 16;
            w.set_f32(particles, base, p.mass)?;
            w.set_f32(particles, base + 4, p.inv_mass)?;
            w.set_f32(particles, base + 8, p.radius)?;
            w.set_f32(particles, base + 12, p.friction)?;
        }
        w.set_array(
            item,
            w.member_offset(t, "particleDatas")?,
            particles,
            w.array_patch_type(particle_type)?,
        )?;
        let u16t = w.type_index("hkUint16")?;
        let u16_patch = w.array_patch_type(u16t)?;
        let fixed = w.u16_storage(&source.fixed_particles)?;
        w.set_array(
            item,
            w.member_offset(t, "fixedParticles")?,
            fixed,
            u16_patch,
        )?;
        w.set_u8(
            item,
            w.member_offset(t, "doNormals")?,
            source.do_normals as u8,
        )?;
        if let Some(index) = simulate_index {
            let ids = w.uint_storage(&[index as u32])?;
            let uint = w.type_index("unsigned int")?;
            w.set_array(
                item,
                w.member_offset(t, "simOpIds")?,
                ids,
                w.array_patch_type(uint)?,
            )?;
        }

        // Poses.
        let pose_type = w.type_index("hclSimClothPose")?;
        let mut pose_refs = Vec::new();
        for (name, positions) in &source.poses {
            let pose = w.alloc(pose_type, 1, true)?;
            w.set_string(pose, w.member_offset(pose_type, "name")?, name)?;
            let storage = w.vec4_storage(positions)?;
            let vec4 = w.type_index("hkVector4")?;
            w.set_array(
                pose,
                w.member_offset(pose_type, "positions")?,
                storage,
                w.array_patch_type(vec4)?,
            )?;
            pose_refs.push(pose);
        }
        let poses = w.pointer_storage(pose_type, &pose_refs, false)?;
        let pose_ptr = w.pointer_patch_type(pose_type)?;
        w.set_array(
            item,
            w.member_offset(t, "simClothPoses")?,
            poses,
            w.array_patch_type(pose_ptr)?,
        )?;

        // Constraint sets.
        let set_base = w.type_index("hclConstraintSet")?;
        let set_ptr = w.pointer_patch_type(set_base)?;
        let set_array_patch = w.array_patch_type(set_ptr)?;
        // TOTK numbers the sets of a sim cloth sequentially (BOTW stores 0).
        let mut next_id = 0u32;
        let mut static_refs = Vec::new();
        for set in &source.static_constraint_sets {
            static_refs.push(constraint_set(w, set, next_id)?);
            next_id += 1;
        }
        let statics = w.pointer_storage(set_base, &static_refs, false)?;
        w.set_array(
            item,
            w.member_offset(t, "staticConstraintSets")?,
            statics,
            set_array_patch,
        )?;
        if !source.anti_pinch_constraint_sets.is_empty() {
            let mut refs = Vec::new();
            for set in &source.anti_pinch_constraint_sets {
                refs.push(constraint_set(w, set, next_id)?);
                next_id += 1;
            }
            let storage = w.pointer_storage(set_base, &refs, false)?;
            w.set_array(
                item,
                w.member_offset(t, "antiPinchConstraintSets")?,
                storage,
                set_array_patch,
            )?;
        }

        // Collidable transform map and instance collidables.
        let map = w.member_offset(t, "collidableTransformMap")?;
        let map_type = w.member_type(t, "collidableTransformMap")?;
        w.set_u32(
            item,
            map + w.member_offset(map_type, "transformSetIndex")?,
            source.collidable_transform_set_index as u32,
        )?;
        if !source.collidable_transform_indices.is_empty() {
            let indices = w.u32_storage(&source.collidable_transform_indices)?;
            let u32t = w.type_index("hkUint32")?;
            w.set_array(
                item,
                map + w.member_offset(map_type, "transformIndices")?,
                indices,
                w.array_patch_type(u32t)?,
            )?;
            let offsets = w.matrix4_storage(&source.collidable_offsets)?;
            let m4 = w.type_index("hkMatrix4")?;
            w.set_array(
                item,
                map + w.member_offset(map_type, "offsets")?,
                offsets,
                w.array_patch_type(m4)?,
            )?;
        }
        let instance_refs: Vec<ItemRef> = source
            .per_instance_collidables
            .iter()
            .filter_map(|index| collidable_items.get(*index).copied())
            .collect();
        if !instance_refs.is_empty() {
            let collidable_type = w.type_index("hclCollidable")?;
            let storage = w.pointer_storage(collidable_type, &instance_refs, false)?;
            let ptr = w.pointer_patch_type(collidable_type)?;
            w.set_array(
                item,
                w.member_offset(t, "perInstanceCollidables")?,
                storage,
                w.array_patch_type(ptr)?,
            )?;
        }
        w.set_f32(
            item,
            w.member_offset(t, "maxParticleRadius")?,
            source.max_particle_radius,
        )?;

        // Landscape collision: Nintendo's ports set the top bit on every
        // particle that collides in BOTW (non-zero mask, i.e. the free ones)
        // and keep the collision group bits; a cloth without any colliding
        // particle stays out of landscape collision altogether.
        let masks: Vec<u32> = source
            .static_collision_masks
            .iter()
            .map(|mask| if *mask != 0 { LANDSCAPE_BIT | *mask } else { 0 })
            .collect();
        let free = masks.iter().filter(|mask| **mask != 0).count() as u32;
        let landscape_enabled = free > 0;
        let mask_storage = w.u32_storage(&masks)?;
        let u32t = w.type_index("hkUint32")?;
        w.set_array(
            item,
            w.member_offset(t, "staticCollisionMasks")?,
            mask_storage,
            w.array_patch_type(u32t)?,
        )?;
        w.set_f32(item, w.member_offset(t, "totalMass")?, source.total_mass)?;
        let transfer = w.member_offset(t, "transferMotionData")?;
        let transfer_type = w.member_type(t, "transferMotionData")?;
        let tm = &source.transfer_motion;
        w.set_u8(
            item,
            transfer + w.member_offset(transfer_type, "transferTranslationMotion")?,
            tm.transfer_translation_motion as u8,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "minTranslationSpeed")?,
            tm.min_translation_speed,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "maxTranslationSpeed")?,
            tm.max_translation_speed,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "minTranslationBlend")?,
            tm.min_translation_blend,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "maxTranslationBlend")?,
            tm.max_translation_blend,
        )?;
        w.set_u8(
            item,
            transfer + w.member_offset(transfer_type, "transferRotationMotion")?,
            tm.transfer_rotation_motion as u8,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "minRotationSpeed")?,
            tm.min_rotation_speed,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "maxRotationSpeed")?,
            tm.max_rotation_speed,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "minRotationBlend")?,
            tm.min_rotation_blend,
        )?;
        w.set_f32(
            item,
            transfer + w.member_offset(transfer_type, "maxRotationBlend")?,
            tm.max_rotation_blend,
        )?;
        w.set_u8(
            item,
            w.member_offset(t, "transferMotionEnabled")?,
            source.transfer_motion_enabled as u8,
        )?;
        w.set_u8(
            item,
            w.member_offset(t, "landscapeCollisionEnabled")?,
            landscape_enabled as u8,
        )?;
        let landscape = w.member_offset(t, "landscapeCollisionData")?;
        let landscape_type = w.member_type(t, "landscapeCollisionData")?;
        w.set_f32(
            item,
            landscape + w.member_offset(landscape_type, "landscapeRadius")?,
            0.0,
        )?;
        w.set_u8(
            item,
            landscape + w.member_offset(landscape_type, "enableStuckParticleDetection")?,
            source.landscape.enable_stuck_particle_detection as u8,
        )?;
        w.set_f32(
            item,
            landscape + w.member_offset(landscape_type, "stuckParticlesStretchFactorSq")?,
            if source.landscape.stuck_particles_stretch_factor_sq > 0.0 {
                source.landscape.stuck_particles_stretch_factor_sq
            } else {
                9.0
            },
        )?;
        w.set_u8(
            item,
            landscape + w.member_offset(landscape_type, "pinchDetectionEnabled")?,
            0,
        )?;
        w.set_u8(
            item,
            landscape + w.member_offset(landscape_type, "pinchDetectionPriority")?,
            0,
        )?;
        w.set_f32(
            item,
            landscape + w.member_offset(landscape_type, "pinchDetectionRadius")?,
            0.01,
        )?;
        w.set_f32(
            item,
            landscape + w.member_offset(landscape_type, "collisionTolerance")?,
            if source.collision_tolerance > 0.0 {
                source.collision_tolerance
            } else {
                0.2
            },
        )?;
        w.set_u32(
            item,
            w.member_offset(t, "numLandscapeCollidableParticles")?,
            free,
        )?;
        let triangles = w.u16_storage(&source.triangle_indices)?;
        w.set_array(
            item,
            w.member_offset(t, "triangleIndices")?,
            triangles,
            u16_patch,
        )?;
        if !source.triangle_flips.is_empty() {
            let flips = w.u8_storage("hkBool", &source.triangle_flips)?;
            let hkbool = w.type_index("hkBool")?;
            w.set_array(
                item,
                w.member_offset(t, "triangleFlips")?,
                flips,
                w.array_patch_type(hkbool)?,
            )?;
        }
        w.set_u8(
            item,
            w.member_offset(t, "pinchDetectionEnabled")?,
            source.pinch_detection_enabled as u8,
        )?;
        let flags = w.u8_storage("hkBool", &source.per_particle_pinch_detection_enabled_flags)?;
        let hkbool = w.type_index("hkBool")?;
        w.set_array(
            item,
            w.member_offset(t, "perParticlePinchDetectionEnabledFlags")?,
            flags,
            w.array_patch_type(hkbool)?,
        )?;
        if !source.collidable_pinching_datas.is_empty() {
            let pinch_type = w.type_index("hclSimClothData::CollidablePinchingData")?;
            let storage = w.alloc(
                pinch_type,
                source.collidable_pinching_datas.len() as u32,
                false,
            )?;
            for (k, (enabled, priority, radius)) in
                source.collidable_pinching_datas.iter().enumerate()
            {
                let base = k as u32 * 8;
                w.set_u8(storage, base, *enabled as u8)?;
                w.set_u8(storage, base + 1, *priority as u8)?;
                w.set_f32(
                    storage,
                    base + 4,
                    if *radius > 0.0 { *radius } else { 0.01 },
                )?;
            }
            w.set_array(
                item,
                w.member_offset(t, "collidablePinchingDatas")?,
                storage,
                w.array_patch_type(pinch_type)?,
            )?;
        }
        w.set_u16(item, w.member_offset(t, "minPinchedParticleIndex")?, 0)?;
        w.set_u16(item, w.member_offset(t, "maxPinchedParticleIndex")?, 0)?;
        w.set_u32(item, w.member_offset(t, "maxCollisionPairs")?, 0)?;
        Ok(item)
    }

    fn buffer_access(
        &mut self,
        entries: &[(u32, [u8; 4], bool)],
    ) -> io::Result<Option<(ItemRef, u32)>> {
        if entries.is_empty() {
            return Ok(None);
        }
        let w = &mut self.writer;
        let t = w.type_index("hclClothState::BufferAccess")?;
        let item = w.alloc(t, entries.len() as u32, false)?;
        let usage = w.member_offset(t, "bufferUsage")?;
        let shadow = w.member_offset(t, "shadowBufferIndex")?;
        for (k, (index, flags, triangles)) in entries.iter().enumerate() {
            let base = k as u32 * 16;
            w.set_u32(item, base, *index)?;
            w.set_bytes(item, base + usage, flags)?;
            w.set_u8(item, base + usage + 4, *triangles as u8)?;
            w.set_u32(item, base + shadow, *index)?;
        }
        Ok(Some((item, w.array_patch_type(t)?)))
    }

    /// One `TransformSetAccess` with two trackers (read / readBeforeWrite /
    /// written words over `bone_count` bits; the second tracker is empty).
    fn transform_access(
        &mut self,
        transform_set_index: u32,
        flags: [u8; 2],
        read: &[u32],
        written: &[u32],
        bone_count: usize,
    ) -> io::Result<(ItemRef, u32)> {
        let words = (bone_count + 31) / 32;
        let w = &mut self.writer;
        let access_type = w.type_index("hclClothState::TransformSetAccess")?;
        let access = w.alloc(access_type, 1, false)?;
        w.set_u32(access, 0, transform_set_index)?;
        let usage = w.member_offset(access_type, "transformSetUsage")?;
        let usage_type = w.member_type(access_type, "transformSetUsage")?;
        w.set_bytes(access, usage, &flags)?;
        let tracker_type = w.type_index("hclTransformSetUsage::TransformTracker")?;
        let trackers = w.alloc(tracker_type, 2, false)?;
        let uint = w.type_index("unsigned int")?;
        let uint_patch = w.array_patch_type(uint)?;
        let zero = vec![0u32; words];
        for tracker in 0..2u32 {
            for (field, source) in ["read", "readBeforeWrite", "written"]
                .iter()
                .zip([read, read, written])
            {
                let bits = w.member_offset(tracker_type, field)?;
                let mut padded = source.to_vec();
                padded.resize(words, 0);
                let storage = w.uint_storage(if tracker == 0 { &padded } else { &zero })?;
                let at = tracker * 72 + bits;
                w.set_array(trackers, at, storage, uint_patch)?;
                w.set_u32(trackers, at + 16, bone_count as u32)?;
            }
        }
        let trackers_field =
            usage + w.member_offset(usage_type, "perComponentTransformTrackers")?;
        w.set_array(
            access,
            trackers_field,
            trackers,
            w.array_patch_type(tracker_type)?,
        )?;
        Ok((access, w.array_patch_type(access_type)?))
    }

    fn operator_common(
        &mut self,
        t: u32,
        item: ItemRef,
        name: &str,
        index: u32,
        buffers: &[(u32, [u8; 4], bool)],
        transforms: Option<(u32, [u8; 2], Vec<u32>, Vec<u32>, usize)>,
    ) -> io::Result<()> {
        self.writer
            .set_string(item, self.writer.member_offset(t, "name")?, name)?;
        self.writer
            .set_u32(item, self.writer.member_offset(t, "operatorID")?, index)?;
        self.writer
            .set_u32(item, self.writer.member_offset(t, "type")?, 0)?;
        if let Some((storage, patch)) = self.buffer_access(buffers)? {
            let field = self.writer.member_offset(t, "usedBuffers")?;
            self.writer.set_array(item, field, storage, patch)?;
        }
        if let Some((set, flags, read, written, bones)) = transforms {
            let (storage, patch) = self.transform_access(set, flags, &read, &written, bones)?;
            let field = self.writer.member_offset(t, "usedTransformSets")?;
            self.writer.set_array(item, field, storage, patch)?;
        }
        Ok(())
    }

    fn operator(
        &mut self,
        op: &Operator,
        index: u32,
        cloth: &ClothData,
        bone_count: usize,
    ) -> io::Result<ItemRef> {
        match op {
            Operator::Skin(skin) => self.skin_operator(skin, index, bone_count),
            Operator::MoveParticles {
                name,
                vertex_particle_pairs,
                sim_cloth_index,
                ref_buffer_idx,
                ..
            } => {
                let t = self.writer.type_index("hclMoveParticlesOperator")?;
                let item = self.writer.alloc(t, 1, true)?;
                self.operator_common(
                    t,
                    item,
                    name,
                    index,
                    &[
                        (*ref_buffer_idx, [1, 0, 0, 0], false),
                        (1, [2, 0, 0, 0], false),
                        (2, [2, 0, 0, 0], false),
                    ],
                    None,
                )?;
                let w = &mut self.writer;
                let pair_type = w.type_index("hclMoveParticlesOperator::VertexParticlePair")?;
                let pairs = w.alloc(pair_type, vertex_particle_pairs.len() as u32, false)?;
                for (k, (vertex, particle)) in vertex_particle_pairs.iter().enumerate() {
                    w.set_u16(pairs, k as u32 * 4, *vertex)?;
                    w.set_u16(pairs, k as u32 * 4 + 2, *particle)?;
                }
                w.set_array(
                    item,
                    w.member_offset(t, "vertexParticlePairs")?,
                    pairs,
                    w.array_patch_type(pair_type)?,
                )?;
                w.set_u32(item, w.member_offset(t, "simClothIndex")?, *sim_cloth_index)?;
                w.set_u32(item, w.member_offset(t, "refBufferIdx")?, *ref_buffer_idx)?;
                Ok(item)
            }
            Operator::Simulate {
                name,
                sim_cloth_index,
                sub_steps,
                number_of_solve_iterations,
                constraint_execution,
                ..
            } => {
                let t = self.writer.type_index("hclSimulateOperator")?;
                let item = self.writer.alloc(t, 1, true)?;
                let sim = cloth.sim_cloths.get(*sim_cloth_index as usize);
                let collidable_bones: Vec<u32> = sim
                    .map(|sim| {
                        let mut bones: Vec<u32> = sim.collidable_transform_indices.clone();
                        bones.sort_unstable();
                        bones.dedup();
                        bones
                    })
                    .unwrap_or_default();
                let transforms = if collidable_bones.is_empty() {
                    None
                } else {
                    let set = sim
                        .map(|sim| sim.collidable_transform_set_index.max(0) as u32)
                        .unwrap_or(0);
                    Some((
                        set,
                        [9, 0],
                        bits(&collidable_bones, bone_count),
                        vec![0; (bone_count + 31) / 32],
                        bone_count,
                    ))
                };
                self.operator_common(
                    t,
                    item,
                    name,
                    index,
                    &[
                        (0, [1, 0, 0, 0], false),
                        (1, [15, 0, 0, 0], false),
                        (2, [15, 0, 0, 0], false),
                    ],
                    transforms,
                )?;
                let w = &mut self.writer;
                w.set_u32(item, w.member_offset(t, "simClothIndex")?, *sim_cloth_index)?;
                let config_type = w.type_index("hclSimulateOperator::Config")?;
                let config = w.alloc(config_type, 1, false)?;
                w.set_string(
                    config,
                    w.member_offset(config_type, "name")?,
                    "Default Config",
                )?;
                let execution = w.i32_storage("hkInt32", constraint_execution)?;
                let i32t = w.type_index("hkInt32")?;
                w.set_array(
                    config,
                    w.member_offset(config_type, "constraintExecution")?,
                    execution,
                    w.array_patch_type(i32t)?,
                )?;
                w.set_u8(
                    config,
                    w.member_offset(config_type, "subSteps")?,
                    (*sub_steps).max(1) as u8,
                )?;
                w.set_u8(
                    config,
                    w.member_offset(config_type, "numberOfSolveIterations")?,
                    (*number_of_solve_iterations).max(1) as u8,
                )?;
                w.set_u8(
                    config,
                    w.member_offset(config_type, "useAllInstanceCollidables")?,
                    1,
                )?;
                w.set_u8(
                    config,
                    w.member_offset(config_type, "adaptConstraintStiffness")?,
                    0,
                )?;
                w.set_array(
                    item,
                    w.member_offset(t, "simulateOpConfigs")?,
                    config,
                    w.array_patch_type(config_type)?,
                )?;
                Ok(item)
            }
            Operator::MeshBone {
                name,
                input_buffer_idx,
                output_transform_set_idx,
                triangle_bone_pairs,
                local_bone_transforms,
                ..
            } => {
                let t = self.writer.type_index("hclSimpleMeshBoneDeformOperator")?;
                let item = self.writer.alloc(t, 1, true)?;
                let mut written: Vec<u32> = triangle_bone_pairs
                    .iter()
                    .map(|(bone, _)| (*bone / 64) as u32)
                    .collect();
                written.sort_unstable();
                written.dedup();
                let words = (bone_count + 31) / 32;
                self.operator_common(
                    t,
                    item,
                    name,
                    index,
                    &[(*input_buffer_idx, [1, 0, 0, 0], true)],
                    Some((
                        *output_transform_set_idx,
                        [2, 0],
                        vec![0; words],
                        bits(&written, bone_count),
                        bone_count,
                    )),
                )?;
                let w = &mut self.writer;
                w.set_u32(
                    item,
                    w.member_offset(t, "inputBufferIdx")?,
                    *input_buffer_idx,
                )?;
                w.set_u32(
                    item,
                    w.member_offset(t, "outputTransformSetIdx")?,
                    *output_transform_set_idx,
                )?;
                let pair_type =
                    w.type_index("hclSimpleMeshBoneDeformOperator::TriangleBonePair")?;
                let pairs = w.alloc(pair_type, triangle_bone_pairs.len() as u32, false)?;
                for (k, (bone, triangle)) in triangle_bone_pairs.iter().enumerate() {
                    w.set_u16(pairs, k as u32 * 4, *bone)?;
                    w.set_u16(pairs, k as u32 * 4 + 2, *triangle)?;
                }
                w.set_array(
                    item,
                    w.member_offset(t, "triangleBonePairs")?,
                    pairs,
                    w.array_patch_type(pair_type)?,
                )?;
                let transforms = w.matrix4_storage(local_bone_transforms)?;
                let m4 = w.type_index("hkMatrix4")?;
                w.set_array(
                    item,
                    w.member_offset(t, "localBoneTransforms")?,
                    transforms,
                    w.array_patch_type(m4)?,
                )?;
                w.set_u32(item, w.member_offset(t, "boneAxis")?, 3)?;
                Ok(item)
            }
            Operator::CopyVertices {
                name,
                input_buffer_idx,
                output_buffer_idx,
                number_of_vertices,
                start_vertex_in,
                start_vertex_out,
                copy_normals,
                ..
            } => {
                let t = self.writer.type_index("hclCopyVerticesOperator")?;
                let item = self.writer.alloc(t, 1, true)?;
                self.operator_common(
                    t,
                    item,
                    name,
                    index,
                    &[
                        (*input_buffer_idx, [1, 0, 0, 0], false),
                        (*output_buffer_idx, [6, 0, 0, 0], false),
                    ],
                    None,
                )?;
                let w = &mut self.writer;
                w.set_u32(
                    item,
                    w.member_offset(t, "inputBufferIdx")?,
                    *input_buffer_idx,
                )?;
                w.set_u32(
                    item,
                    w.member_offset(t, "outputBufferIdx")?,
                    *output_buffer_idx,
                )?;
                w.set_u32(
                    item,
                    w.member_offset(t, "numberOfVertices")?,
                    *number_of_vertices,
                )?;
                w.set_u32(item, w.member_offset(t, "startVertexIn")?, *start_vertex_in)?;
                w.set_u32(
                    item,
                    w.member_offset(t, "startVertexOut")?,
                    *start_vertex_out,
                )?;
                w.set_u8(
                    item,
                    w.member_offset(t, "copyNormals")?,
                    *copy_normals as u8,
                )?;
                Ok(item)
            }
            Operator::GatherAllVertices {
                name,
                vertex_input_from_vertex_output,
                input_buffer_idx,
                output_buffer_idx,
                gather_normals,
                partial_gather,
                ..
            } => {
                let t = self.writer.type_index("hclGatherAllVerticesOperator")?;
                let item = self.writer.alloc(t, 1, true)?;
                self.operator_common(
                    t,
                    item,
                    name,
                    index,
                    &[
                        (*input_buffer_idx, [1, 0, 0, 0], false),
                        (*output_buffer_idx, [6, 0, 0, 0], false),
                    ],
                    None,
                )?;
                let w = &mut self.writer;
                let map = w.i16_storage(vertex_input_from_vertex_output)?;
                let i16t = w.type_index("hkInt16")?;
                w.set_array(
                    item,
                    w.member_offset(t, "vertexInputFromVertexOutput")?,
                    map,
                    w.array_patch_type(i16t)?,
                )?;
                w.set_u32(
                    item,
                    w.member_offset(t, "inputBufferIdx")?,
                    *input_buffer_idx,
                )?;
                w.set_u32(
                    item,
                    w.member_offset(t, "outputBufferIdx")?,
                    *output_buffer_idx,
                )?;
                w.set_u8(
                    item,
                    w.member_offset(t, "gatherNormals")?,
                    *gather_normals as u8,
                )?;
                w.set_u8(
                    item,
                    w.member_offset(t, "partialGather")?,
                    *partial_gather as u8,
                )?;
                Ok(item)
            }
            Operator::Unsupported { class_name, name } => Err(io::Error::new(
                ErrorKind::Unsupported,
                format!("operator {name} of class {class_name} is not supported by the converter"),
            )),
        }
    }

    fn skin_operator(
        &mut self,
        skin: &SkinOperator,
        index: u32,
        bone_count: usize,
    ) -> io::Result<ItemRef> {
        let class = if skin.bone_space {
            "hclBoneSpaceSkinPOperator"
        } else {
            "hclObjectSpaceSkinPOperator"
        };
        let t = self.writer.type_index(class)?;
        let item = self.writer.alloc(t, 1, true)?;
        // bones read by the deformer: local indices mapped through the subset
        let mut locals: Vec<u16> = skin
            .deformer
            .blocks
            .values()
            .flat_map(|blocks| {
                blocks
                    .iter()
                    .flat_map(|block| block.bone_indices.iter().copied())
            })
            .collect();
        locals.sort_unstable();
        locals.dedup();
        let mut read: Vec<u32> = locals
            .iter()
            .map(|local| {
                skin.transform_subset
                    .get(*local as usize)
                    .copied()
                    .unwrap_or(*local) as u32
            })
            .filter(|bone| (*bone as usize) < bone_count)
            .collect();
        read.sort_unstable();
        read.dedup();
        let words = (bone_count + 31) / 32;
        self.operator_common(
            t,
            item,
            &skin.name,
            index,
            &[(skin.output_buffer_index, [6, 0, 0, 0], false)],
            Some((
                skin.transform_set_index,
                [9, 0],
                bits(&read, bone_count),
                vec![0; words],
                bone_count,
            )),
        )?;
        let w = &mut self.writer;
        if !skin.bone_space {
            let transforms = w.matrix4_storage(&skin.bone_from_skin_mesh_transforms)?;
            let m4 = w.type_index("hkMatrix4")?;
            w.set_array(
                item,
                w.member_offset(t, "boneFromSkinMeshTransforms")?,
                transforms,
                w.array_patch_type(m4)?,
            )?;
        }
        let subset = w.u16_storage(&skin.transform_subset)?;
        let u16t = w.type_index("hkUint16")?;
        w.set_array(
            item,
            w.member_offset(t, "transformSubset")?,
            subset,
            w.array_patch_type(u16t)?,
        )?;
        w.set_u32(
            item,
            w.member_offset(t, "outputBufferIndex")?,
            skin.output_buffer_index,
        )?;
        w.set_u32(
            item,
            w.member_offset(t, "transformSetIndex")?,
            skin.transform_set_index,
        )?;

        let deformer_member = if skin.bone_space {
            "boneSpaceDeformer"
        } else {
            "objectSpaceDeformer"
        };
        let deformer = w.member_offset(t, deformer_member)?;
        let deformer_type = w.member_type(t, deformer_member)?;
        let (prefix, kinds): (&str, &[u32]) = if skin.bone_space {
            ("hclBoneSpaceDeformer", &BONE_KINDS)
        } else {
            ("hclObjectSpaceDeformer", &OBJECT_KINDS)
        };
        for blend in kinds {
            let Some(blocks) = skin.deformer.blocks.get(blend) else {
                continue;
            };
            let word = [
                "", "one", "two", "three", "four", "five", "six", "seven", "eight",
            ][*blend as usize];
            let block_type =
                w.type_index(&format!("{prefix}::{}BlendEntryBlock", capitalize(word)))?;
            let storage = w.alloc(block_type, blocks.len() as u32, false)?;
            let size = w.size_of(block_type);
            let vertices_at = w.member_offset(block_type, "vertexIndices")?;
            let bones_at = w.member_offset(block_type, "boneIndices")?;
            let weights_at = w.member_offset(block_type, "boneWeights").ok();
            for (k, block) in blocks.iter().enumerate() {
                let base = k as u32 * size;
                for (v, vertex) in block.vertex_indices.iter().enumerate() {
                    w.set_u16(storage, base + vertices_at + v as u32 * 2, *vertex)?;
                }
                for (b, bone) in block.bone_indices.iter().enumerate() {
                    w.set_u16(storage, base + bones_at + b as u32 * 2, *bone)?;
                }
                if let Some(weights_at) = weights_at {
                    w.set_bytes(storage, base + weights_at, &block.bone_weights)?;
                }
            }
            let field =
                deformer + w.member_offset(deformer_type, &format!("{word}BlendEntries"))?;
            w.set_array(item, field, storage, w.array_patch_type(block_type)?)?;
        }
        if !skin.deformer.control_bytes.is_empty() {
            let control = w.u8_storage("hkUint8", &skin.deformer.control_bytes)?;
            let u8t = w.type_index("hkUint8")?;
            w.set_array(
                item,
                deformer + w.member_offset(deformer_type, "controlBytes")?,
                control,
                w.array_patch_type(u8t)?,
            )?;
        }
        w.set_u16(
            item,
            deformer + w.member_offset(deformer_type, "startVertexIndex")?,
            skin.deformer.start_vertex_index,
        )?;
        w.set_u16(
            item,
            deformer + w.member_offset(deformer_type, "endVertexIndex")?,
            skin.deformer.end_vertex_index,
        )?;
        w.set_u8(
            item,
            deformer + w.member_offset(deformer_type, "partialWrite")?,
            0,
        )?;

        // Local positions: object space packs them, bone space keeps vectors.
        let blocks: &[[[f32; 4]; 16]] = if skin.local_unpacked_ps.is_empty() {
            &skin.local_ps
        } else {
            &skin.local_unpacked_ps
        };
        if !blocks.is_empty() {
            if skin.bone_space {
                let block_type = w.type_index("hclBoneSpaceDeformer::LocalBlockP")?;
                let storage = w.alloc(block_type, blocks.len() as u32, false)?;
                for (k, block) in blocks.iter().enumerate() {
                    for (v, vector) in block.iter().enumerate() {
                        w.set_vec4(storage, (k * 256 + v * 16) as u32, *vector)?;
                    }
                }
                w.set_array(
                    item,
                    w.member_offset(t, "localPs")?,
                    storage,
                    w.array_patch_type(block_type)?,
                )?;
            } else {
                let block_type = w.type_index("hclObjectSpaceDeformer::LocalBlockP")?;
                let storage = w.alloc(block_type, blocks.len() as u32, false)?;
                for (k, block) in blocks.iter().enumerate() {
                    for (v, vector) in block.iter().enumerate() {
                        w.set_bytes(storage, (k * 128 + v * 8) as u32, &pack_vector3(vector))?;
                    }
                }
                w.set_array(
                    item,
                    w.member_offset(t, "localPs")?,
                    storage,
                    w.array_patch_type(block_type)?,
                )?;
            }
        }
        Ok(item)
    }

    fn state(&mut self, source: &ClothState, bone_count: usize) -> io::Result<ItemRef> {
        let t = self.writer.type_index("hclClothState")?;
        let item = self.writer.alloc(t, 1, true)?;
        self.writer
            .set_string(item, self.writer.member_offset(t, "name")?, &source.name)?;
        let operators = self.writer.u32_storage(&source.operators)?;
        let u32t = self.writer.type_index("hkUint32")?;
        let u32_patch = self.writer.array_patch_type(u32t)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "operators")?,
            operators,
            u32_patch,
        )?;
        let buffers: Vec<(u32, [u8; 4], bool)> = source
            .used_buffers
            .iter()
            .map(|access| {
                (
                    access.buffer_index,
                    access.per_component_flags,
                    access.triangles_read,
                )
            })
            .collect();
        if let Some((storage, patch)) = self.buffer_access(&buffers)? {
            self.writer.set_array(
                item,
                self.writer.member_offset(t, "usedBuffers")?,
                storage,
                patch,
            )?;
        }
        if let Some(access) = source.used_transform_sets.first() {
            let (read, written) = access
                .trackers
                .first()
                .map(|tracker| (tracker.read.0.clone(), tracker.written.0.clone()))
                .unwrap_or_default();
            let (storage, patch) = self.transform_access(
                access.transform_set_index,
                access.per_component_flags,
                &read,
                &written,
                bone_count,
            )?;
            self.writer.set_array(
                item,
                self.writer.member_offset(t, "usedTransformSets")?,
                storage,
                patch,
            )?;
        }
        let sims = self.writer.u32_storage(&source.used_sim_cloths)?;
        self.writer.set_array(
            item,
            self.writer.member_offset(t, "usedSimCloths")?,
            sims,
            u32_patch,
        )?;

        // Linear dependency graph over the state's operators.
        let w = &mut self.writer;
        let graph_type = w.type_index("hclStateDependencyGraph")?;
        let graph = w.alloc(graph_type, 1, true)?;
        let branch_type = w.type_index("hclStateDependencyGraph::Branch")?;
        let branch = w.alloc(branch_type, 1, false)?;
        let int = w.type_index("int")?;
        let int_patch = w.array_patch_type(int)?;
        let count = source.operators.len();
        let indices: Vec<i32> = (0..count as i32).collect();
        let index_storage = w.i32_storage("int", &indices)?;
        w.set_u32(branch, w.member_offset(branch_type, "branchId")?, 0)?;
        w.set_array(
            branch,
            w.member_offset(branch_type, "stateOperatorIndices")?,
            index_storage,
            int_patch,
        )?;
        w.set_array(
            graph,
            w.member_offset(graph_type, "branches")?,
            branch,
            w.array_patch_type(branch_type)?,
        )?;
        let roots = w.i32_storage("int", &[0])?;
        w.set_array(
            graph,
            w.member_offset(graph_type, "rootBranchIds")?,
            roots,
            int_patch,
        )?;
        let inner_type = w.array_patch_type(int)?; // hkArray<int>
        for (member, next) in [("children", true), ("parents", false)] {
            let lists = w.alloc(inner_type, count as u32, false)?;
            for k in 0..count {
                let neighbour = if next { k + 1 } else { k.wrapping_sub(1) };
                if neighbour < count {
                    let entry = w.i32_storage("int", &[neighbour as i32])?;
                    w.set_array(lists, k as u32 * 16, entry, int_patch)?;
                }
            }
            w.set_array(
                graph,
                w.member_offset(graph_type, member)?,
                lists,
                w.array_patch_type(inner_type)?,
            )?;
        }
        w.set_u8(graph, w.member_offset(graph_type, "multiThreadable")?, 0)?;
        let graph_ptr = w.pointer_patch_type(graph_type)?;
        w.set_pointer(
            item,
            w.member_offset(t, "dependencyGraph")?,
            graph,
            graph_ptr,
        )?;
        Ok(item)
    }

    /// Embedded AAMP registration (cloth mesh list + collidable list).
    fn registration(&mut self, cloths: &[&ClothData]) -> io::Result<Vec<u8>> {
        let mut yaml = String::from("!io\nversion: 0\ntype: phcl\nparam_root: !list\n  lists:\n    cloth_mesh_list: !list\n      lists: {}\n      objects:");
        yaml.push_str(if cloths.is_empty() { " {}\n" } else { "\n" });
        for (index, cloth) in cloths.iter().enumerate() {
            let (base_bone, preset) = self.registration_for(cloth);
            self.report
                .registrations
                .push((cloth.name.clone(), base_bone.clone(), preset.clone()));
            yaml.push_str(&format!(
                "        cloth_mesh_{index}: !obj\n          Name: {}\n          BaseBone: {base_bone}\n          BoneCorrection: false\n          BoneCorrectionAxisOrder: xyz\n          Twist: false\n          TwistSwingAxis: y\n          TwistAngleCoef: 0.0\n          TwistMaxAngle: 180.0\n          KeepBoneLength: false\n          Preset: {preset}\n",
                cloth.name
            ));
        }
        yaml.push_str("    collidable_list: !list\n      lists: {}\n      objects:");
        yaml.push_str(if self.report.collidables.is_empty() {
            " {}\n"
        } else {
            "\n"
        });
        for (index, name) in self.report.collidables.iter().enumerate() {
            yaml.push_str(&format!(
                "        collidable_{index}: !obj\n          Name: {name}\n          ForReplace: false\n"
            ));
        }
        yaml.push_str("  objects: {}\n");
        Ok(ParameterIO::from_text(&yaml)
            .map_err(io::Error::other)?
            .to_binary())
    }

    /// `BaseBone`: the (unprefixed) parent of the first cloth-driven bone;
    /// `Preset`: `hair` for hair cloths, `burlap` otherwise, unless overridden.
    fn registration_for(&self, cloth: &ClothData) -> (String, String) {
        if let Some(entry) = self.options.presets.get(&cloth.name) {
            return entry.clone();
        }
        let skeleton = cloth
            .transform_sets
            .first()
            .and_then(|set| self.package.skeletons.iter().find(|s| s.name == set.name));
        let mut base_bone = String::from("Root");
        if let Some(skeleton) = skeleton {
            for (index, (name, _)) in skeleton.bones.iter().enumerate() {
                if self.options.link_bones.contains(name) {
                    continue;
                }
                let mut parent = skeleton.parent_indices.get(index).copied().unwrap_or(-1);
                while parent >= 0 {
                    let (parent_name, _) = &skeleton.bones[parent as usize];
                    if self.options.link_bones.contains(parent_name)
                        || self.options.link_bones.is_empty()
                    {
                        base_bone = parent_name.clone();
                        break;
                    }
                    parent = skeleton
                        .parent_indices
                        .get(parent as usize)
                        .copied()
                        .unwrap_or(-1);
                }
                break;
            }
        }
        let lower = cloth.name.to_ascii_lowercase();
        let preset = if lower.contains("hair") {
            "hair"
        } else {
            "burlap"
        };
        (base_bone, preset.to_owned())
    }
}

fn constraint_set(w: &mut ObjectWriter<'_>, set: &ConstraintSet, id: u32) -> io::Result<ItemRef> {
    let t = w.type_index(&set.class_name)?;
    let item = w.alloc(t, 1, true)?;
    w.set_string(item, w.member_offset(t, "name")?, &set.name)?;
    w.set_u32(item, w.member_offset(t, "constraintId")?, id)?;
    w.set_u32(item, w.member_offset(t, "type")?, 0)?;
    match &set.kind {
        ConstraintKind::StandardLinks(links) | ConstraintKind::StretchLinks(links) => {
            let link_type = w.type_index(&format!("{}::Link", set.class_name))?;
            let storage = w.alloc(link_type, links.len() as u32, false)?;
            for (k, (a, b, rest, stiffness)) in links.iter().enumerate() {
                let base = k as u32 * 12;
                w.set_u16(storage, base, *a)?;
                w.set_u16(storage, base + 2, *b)?;
                w.set_f32(storage, base + 4, *rest)?;
                w.set_f32(storage, base + 8, *stiffness)?;
            }
            w.set_array(
                item,
                w.member_offset(t, "links")?,
                storage,
                w.array_patch_type(link_type)?,
            )?;
        }
        ConstraintKind::BendLinks(links) => {
            let link_type = w.type_index("hclBendLinkConstraintSet::Link")?;
            let storage = w.alloc(link_type, links.len() as u32, false)?;
            for (k, (a, b, min, max, bend, stretch)) in links.iter().enumerate() {
                let base = k as u32 * 20;
                w.set_u16(storage, base, *a)?;
                w.set_u16(storage, base + 2, *b)?;
                w.set_f32(storage, base + 4, *min)?;
                w.set_f32(storage, base + 8, *max)?;
                w.set_f32(storage, base + 12, *bend)?;
                w.set_f32(storage, base + 16, *stretch)?;
            }
            w.set_array(
                item,
                w.member_offset(t, "links")?,
                storage,
                w.array_patch_type(link_type)?,
            )?;
        }
        ConstraintKind::BendStiffness {
            links,
            max_rest_pose_height_sq,
        } => {
            let link_type = w.type_index("hclBendStiffnessConstraintSet::Link")?;
            let storage = w.alloc(link_type, links.len() as u32, false)?;
            for (k, (weights, bend, curvature, particles)) in links.iter().enumerate() {
                let base = k as u32 * 32;
                for (i, weight) in weights.iter().enumerate() {
                    w.set_f32(storage, base + i as u32 * 4, *weight)?;
                }
                w.set_f32(storage, base + 16, *bend)?;
                w.set_f32(storage, base + 20, *curvature)?;
                for (i, particle) in particles.iter().enumerate() {
                    w.set_u16(storage, base + 24 + i as u32 * 2, *particle)?;
                }
            }
            w.set_array(
                item,
                w.member_offset(t, "links")?,
                storage,
                w.array_patch_type(link_type)?,
            )?;
            w.set_f32(
                item,
                w.member_offset(t, "maxRestPoseHeightSq")?,
                *max_rest_pose_height_sq,
            )?;
            w.set_u8(item, w.member_offset(t, "clampBendStiffness")?, 1)?;
            w.set_u8(item, w.member_offset(t, "useRestPoseConfig")?, 1)?;
        }
        ConstraintKind::LocalRange {
            constraints,
            reference_mesh_buffer_idx,
            stiffness,
            shape_type,
            apply_normal_component,
        } => {
            let local_type = w.type_index("hclLocalRangeConstraintSet::LocalConstraint")?;
            let storage = w.alloc(local_type, constraints.len() as u32, false)?;
            for (k, (particle, reference, radius, max, min)) in constraints.iter().enumerate() {
                let base = k as u32 * 16;
                w.set_u16(storage, base, *particle)?;
                w.set_u16(storage, base + 2, *reference)?;
                w.set_f32(storage, base + 4, *radius)?;
                w.set_f32(storage, base + 8, *max)?;
                w.set_f32(storage, base + 12, *min)?;
            }
            w.set_array(
                item,
                w.member_offset(t, "localConstraints")?,
                storage,
                w.array_patch_type(local_type)?,
            )?;
            w.set_u32(
                item,
                w.member_offset(t, "referenceMeshBufferIdx")?,
                *reference_mesh_buffer_idx,
            )?;
            w.set_f32(item, w.member_offset(t, "stiffness")?, *stiffness)?;
            w.set_u32(item, w.member_offset(t, "shapeType")?, *shape_type)?;
            w.set_u8(
                item,
                w.member_offset(t, "applyNormalComponent")?,
                *apply_normal_component as u8,
            )?;
        }
        ConstraintKind::Transition {
            per_particle,
            to_anim_period,
            to_anim_plus_delay_period,
            to_sim_period,
            to_sim_plus_delay_period,
            reference_mesh_buffer_idx,
        } => {
            let entry_type = w.type_index("hclTransitionConstraintSet::PerParticle")?;
            let storage = w.alloc(entry_type, per_particle.len() as u32, false)?;
            for (k, (particle, reference, anim_delay, sim_delay, max_distance)) in
                per_particle.iter().enumerate()
            {
                let base = k as u32 * 16;
                w.set_u16(storage, base, *particle)?;
                w.set_u16(storage, base + 2, *reference)?;
                w.set_f32(storage, base + 4, *anim_delay)?;
                w.set_f32(storage, base + 8, *sim_delay)?;
                w.set_f32(storage, base + 12, *max_distance)?;
            }
            w.set_array(
                item,
                w.member_offset(t, "perParticleData")?,
                storage,
                w.array_patch_type(entry_type)?,
            )?;
            w.set_f32(item, w.member_offset(t, "toAnimPeriod")?, *to_anim_period)?;
            w.set_f32(
                item,
                w.member_offset(t, "toAnimPlusDelayPeriod")?,
                *to_anim_plus_delay_period,
            )?;
            w.set_f32(item, w.member_offset(t, "toSimPeriod")?, *to_sim_period)?;
            w.set_f32(
                item,
                w.member_offset(t, "toSimPlusDelayPeriod")?,
                *to_sim_plus_delay_period,
            )?;
            w.set_u32(
                item,
                w.member_offset(t, "referenceMeshBufferIdx")?,
                *reference_mesh_buffer_idx,
            )?;
        }
        ConstraintKind::CompressibleLinks(links) => {
            let link_type = w.type_index("hclCompressibleLinkConstraintSet::Link")?;
            let storage = w.alloc(link_type, links.len() as u32, false)?;
            for (k, (a, b, rest, compression, stiffness)) in links.iter().enumerate() {
                let base = k as u32 * 16;
                w.set_u16(storage, base, *a)?;
                w.set_u16(storage, base + 2, *b)?;
                w.set_f32(storage, base + 4, *rest)?;
                w.set_f32(storage, base + 8, *compression)?;
                w.set_f32(storage, base + 12, *stiffness)?;
            }
            w.set_array(
                item,
                w.member_offset(t, "links")?,
                storage,
                w.array_patch_type(link_type)?,
            )?;
        }
        ConstraintKind::BonePlanes {
            planes,
            transform_set_index,
        } => {
            let plane_type = w.type_index("hclBonePlanesConstraintSet::BonePlane")?;
            let storage = w.alloc(plane_type, planes.len() as u32, false)?;
            for (k, (plane, particle, transform, stiffness)) in planes.iter().enumerate() {
                let base = k as u32 * 32;
                w.set_vec4(storage, base, *plane)?;
                w.set_u16(storage, base + 16, *particle)?;
                w.set_u16(storage, base + 18, *transform)?;
                w.set_f32(storage, base + 20, *stiffness)?;
            }
            w.set_array(
                item,
                w.member_offset(t, "bonePlanes")?,
                storage,
                w.array_patch_type(plane_type)?,
            )?;
            w.set_u32(
                item,
                w.member_offset(t, "transformSetIndex")?,
                *transform_set_index,
            )?;
        }
        ConstraintKind::Volume {
            frame_datas,
            apply_datas,
        } => {
            for (member, class, entries) in [
                ("frameDatas", "hclVolumeConstraint::FrameData", frame_datas),
                ("applyDatas", "hclVolumeConstraint::ApplyData", apply_datas),
            ] {
                let entry_type = w.type_index(class)?;
                let storage = w.alloc(entry_type, entries.len() as u32, false)?;
                for (k, (vector, particle, value)) in entries.iter().enumerate() {
                    let base = k as u32 * 32;
                    w.set_vec4(storage, base, *vector)?;
                    w.set_u16(storage, base + 16, *particle)?;
                    w.set_f32(storage, base + 20, *value)?;
                }
                w.set_array(
                    item,
                    w.member_offset(t, member)?,
                    storage,
                    w.array_patch_type(entry_type)?,
                )?;
            }
        }
        ConstraintKind::Unsupported => {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                format!("constraint set class {} is not supported", set.class_name),
            ))
        }
    }
    Ok(item)
}

/// Bit words (32 bits each) with the listed bone indices set.
fn bits(bones: &[u32], bone_count: usize) -> Vec<u32> {
    let mut words = vec![0u32; (bone_count + 31) / 32];
    for bone in bones {
        if (*bone as usize) < bone_count {
            words[(*bone / 32) as usize] |= 1 << (bone % 32);
        }
    }
    words
}

/// `hkPackedVector3`: three int16 components sharing a power-of-two scale
/// whose `f32(scale / 65536)` upper half is stored as the fourth word.
fn pack_vector3(vector: &[f32; 4]) -> [u8; 8] {
    let magnitude = vector[..3].iter().fold(0.0f32, |acc, v| acc.max(v.abs()));
    let exponent = if magnitude > 0.0 {
        (magnitude / 32767.0).log2().ceil() as i32
    } else {
        -14
    };
    let scale = (2.0f32).powi(exponent);
    let mut out = [0u8; 8];
    for (i, component) in vector[..3].iter().enumerate() {
        let packed = (component / scale).round().clamp(-32768.0, 32767.0) as i16;
        out[i * 2..i * 2 + 2].copy_from_slice(&packed.to_le_bytes());
    }
    let word = ((scale / 65536.0).to_bits() >> 16) as u16;
    out[6..8].copy_from_slice(&word.to_le_bytes());
    out
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

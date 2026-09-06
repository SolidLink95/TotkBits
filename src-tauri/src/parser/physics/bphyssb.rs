//! Native editing of BotW support-bone sidecars (`.bphyssb`, AAMP type
//! `physsb`) and the conversion from TotK helper-bone graphs.
//!
//! BotW keeps separate main-bone (driver), support-bone (driven), curve and
//! output tables. Curves pack their driver index into the high half of a
//! bone-attribute word, and output handles with `0x0001_0000` set address
//! the double-output list instead of the single-output list.

use super::{
    aamp_tree::{crc32, invalid, AampList, AampObject, AampParameter, AampTree, KIND_STRING64},
    bphhb::{
        self, mirror_transform_record, read_sidecar_header, read_string_pool, remap_bone_ids,
        required_int, HelperBoneDocument, SidecarHeader,
    },
    sidecar::{
        bone_name, distinct_non_negative, swap_side_tokens, unique_bone_name, SidecarDriverGroup,
        UnionFind,
    },
};
use serde::Serialize;
use std::{collections::HashMap, io};

pub const PARAM_ROOT: u32 = 2_767_637_356;
pub const SUPPORT_BONE_HEADER: u32 = 2_417_381_405;
pub const SUPPORT_BONE_DATA: u32 = 2_663_467_289;
pub const BONE_LIST: u32 = 3_240_679_892;
pub const CONNECTION_CURVE_LIST: u32 = 1_471_120_930;
pub const OUTPUT_SINGLE_LIST: u32 = 2_123_203_909;
pub const OUTPUT_DOUBLE_LIST: u32 = 2_452_354_690;
pub const MAIN_BONE_LIST: u32 = 3_785_260_662;
pub const SUPPORT_BONE_LIST: u32 = 2_012_889_698;

pub const HEADER_STEP: u32 = 423_031_007;
pub const BONE_COUNT: u32 = 1_498_594_108;
pub const UNKNOWN_HEADER_COUNT: u32 = 3_864_144_338;
pub const CURVE_COUNT: u32 = 3_617_964_918;
pub const SINGLE_OUTPUT_COUNT: u32 = 745_487_375;
pub const DOUBLE_OUTPUT_COUNT: u32 = 3_924_745_678;
pub const MAIN_BONE_COUNT: u32 = 2_154_867_273;
pub const SUPPORT_BONE_COUNT: u32 = 3_991_154_691;

pub const NAME: u32 = 1_579_384_326;
pub const TARGET_BONE_ATTRIBUTE: u32 = 554_147_540;
pub const TARGET_CONSTANT_IN: u32 = 1_411_838_321;
pub const TARGET_CONSTANT_OUT: u32 = 260_541_147;
pub const TARGET_CONNECTION: u32 = 704_082_790;
pub const TARGET_WEIGHT: u32 = 130_897_217;
pub const TARGET_CONNECTION0: u32 = 2_693_310_104;
pub const TARGET_WEIGHT0: u32 = 2_822_815_546;
pub const TARGET_CONNECTION1: u32 = 3_616_511_502;
pub const TARGET_WEIGHT1: u32 = 3_746_009_004;
pub const TARGET_BONE: u32 = 893_828_983;
pub const TARGET_AIM: u32 = 828_945_678;
pub const TARGET_UP: u32 = 1_133_833_840;
pub const TARGET_SPACE: u32 = 695_386_426;
pub const TARGET_BEND_H: u32 = 318_199_082;
pub const TARGET_BEND_V: u32 = 3_908_593_737;
pub const TARGET_ROLL: u32 = 783_626_958;
pub const TARGET_TRANSLATE_X: u32 = 2_040_119_991;
pub const TARGET_TRANSLATE_Y: u32 = 245_297_697;
pub const TARGET_TRANSLATE_Z: u32 = 2_543_297_435;

pub const SUPPORT_OUTPUT_HASHES: [u32; 6] = [
    TARGET_BEND_H,
    TARGET_BEND_V,
    TARGET_ROLL,
    TARGET_TRANSLATE_X,
    TARGET_TRANSLATE_Y,
    TARGET_TRANSLATE_Z,
];

const DOUBLE_HANDLE_FLAG: i32 = 0x0001_0000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoseBone {
    pub bone_id: i32,
    pub space_id: i32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBoneGraph {
    pub bones: Vec<String>,
    pub main_bones: Vec<PoseBone>,
    pub support_bones: Vec<PoseBone>,
    pub curve_count: usize,
    pub output_single_count: usize,
    pub output_double_count: usize,
}

#[derive(Clone, Debug)]
pub struct SupportBoneDocument {
    pub bytes: Vec<u8>,
    pub header: SidecarHeader,
    pub tree: AampTree,
    pub graph: Option<SupportBoneGraph>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBoneSummary {
    pub file_size: usize,
    pub bone_count: usize,
    pub curve_count: usize,
    pub main_bone_count: usize,
    pub support_bone_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct DriverGroupData {
    pub summary: SidecarDriverGroup,
    pub driver_indices: Vec<usize>,
    pub curve_indices: Vec<usize>,
    pub output_handles: Vec<i32>,
    pub support_indices: Vec<usize>,
    pub required_bone_indices: Vec<usize>,
}

pub fn is_double_output_handle(handle: i32) -> bool {
    (handle as u32) & 0xffff_0000 == 0x0001_0000
}

pub fn decode_curve_reference(reference: i32) -> i32 {
    if reference < 0 {
        -1
    } else if is_double_output_handle(reference) {
        reference & 0xffff
    } else {
        reference
    }
}

fn encode_curve_reference(curve_index: i32) -> i32 {
    if curve_index < 0 {
        -1
    } else {
        DOUBLE_HANDLE_FLAG | curve_index
    }
}

impl SupportBoneDocument {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let header = read_sidecar_header(bytes, "physsb")?;
        let tree = AampTree::parse(bytes)?;
        let graph = read_graph(&tree);
        Ok(Self {
            bytes: bytes.to_vec(),
            header,
            tree,
            graph,
        })
    }

    pub fn bone_names(&self) -> Vec<String> {
        match &self.graph {
            Some(graph) if !graph.bones.is_empty() => graph.bones.clone(),
            _ => read_string_pool(&self.bytes, self.header.string_pool_size),
        }
    }

    /// Checks the graph tables exist and reports their sizes.
    pub fn validate(&self) -> io::Result<SupportBoneSummary> {
        if !self.tree.archive_type.eq_ignore_ascii_case("physsb") {
            return Err(invalid(&format!(
                "expected a physsb AAMP archive, found '{}'",
                self.tree.archive_type
            )));
        }
        let data = single_data_root(&self.tree.root)
            .ok_or_else(|| invalid("BPHYSSB is missing its support-bone data list"))?;
        let bones = data.require_child(BONE_LIST, "bone_list")?;
        let curves = data.require_child(CONNECTION_CURVE_LIST, "connection_curve_list")?;
        let main = data.require_child(MAIN_BONE_LIST, "main_bone_list")?;
        let support = data.require_child(SUPPORT_BONE_LIST, "support_bone_list")?;
        Ok(SupportBoneSummary {
            file_size: self.bytes.len(),
            bone_count: bones.objects.len(),
            curve_count: curves.objects.len(),
            main_bone_count: main.objects.len(),
            support_bone_count: support.objects.len(),
        })
    }

    pub fn driver_groups(&self) -> io::Result<Vec<SidecarDriverGroup>> {
        Ok(build_driver_groups(&self.tree)?
            .into_iter()
            .map(|group| group.summary)
            .collect())
    }

    pub fn update_bone(
        &self,
        bone_index: usize,
        name: &str,
        space_index: i32,
        translation: [f32; 3],
        rotation: [f32; 4],
    ) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bones = data.require_child_mut(BONE_LIST, "bone_list")?;
            let bone = bones
                .objects
                .get_mut(bone_index)
                .ok_or_else(|| invalid("bone index is out of range"))?;
            bone.set_string(NAME, name);
            for list in [MAIN_BONE_LIST, SUPPORT_BONE_LIST] {
                if let Some(list) = data.find_child_mut(list) {
                    for record in &mut list.objects {
                        if record.int(TARGET_BONE) == bone_index as i32 {
                            record.set_int(TARGET_SPACE, space_index);
                            record.set_vec3(bphhb::BASE_TRANSLATION, translation);
                            record.set_vec4(bphhb::BASE_ROTATION, rotation);
                        }
                    }
                }
            }
            Ok(())
        })
    }

    pub fn duplicate_bone(
        &self,
        source_bone_index: usize,
        requested_name: Option<&str>,
    ) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bones = data.require_child_mut(BONE_LIST, "bone_list")?;
            let source = bones
                .objects
                .get(source_bone_index)
                .ok_or_else(|| invalid("bone index is out of range"))?;
            let new_bone_index = bones.objects.len();
            let source_name = source
                .string(NAME)
                .unwrap_or_else(|| format!("SupportBone_{source_bone_index}"));
            let used: Vec<String> = bones
                .objects
                .iter()
                .filter_map(|bone| bone.string(NAME))
                .collect();
            let new_name = match requested_name
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                Some(name) => name.to_owned(),
                None => unique_bone_name(&used, &source_name),
            };
            let mut new_bone = source.clone_as(&format!("bone_{new_bone_index}"));
            new_bone.set_string(NAME, &new_name);
            bones.objects.push(new_bone);
            clone_pose_records(
                data.find_child_mut(MAIN_BONE_LIST),
                source_bone_index,
                new_bone_index,
                "main_bone",
            );
            clone_pose_records(
                data.find_child_mut(SUPPORT_BONE_LIST),
                source_bone_index,
                new_bone_index,
                "support_bone",
            );
            Ok(())
        })
    }

    pub fn add_bone(&self, preferred_source_index: usize) -> io::Result<Self> {
        let count = self.graph.as_ref().map_or(0, |graph| graph.bones.len());
        if count == 0 {
            return Err(invalid(
                "this support-bone file has no source bone to clone",
            ));
        }
        self.duplicate_bone(
            preferred_source_index.min(count - 1),
            Some(&format!("SupportBone_{count}")),
        )
    }

    /// Creates support for a new or existing target bone by cloning the
    /// selected affected bone's native support records. Output handles stay
    /// shared with the source because BPHYSSB uses them as reusable driver
    /// mappings.
    pub fn create_support_from_bone(
        &self,
        source_bone_index: usize,
        target_bone_name: &str,
    ) -> io::Result<(Self, usize)> {
        let mut target_index = 0usize;
        let rebuilt = self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bones = data.require_child_mut(BONE_LIST, "bone_list")?;
            let source = bones
                .objects
                .get(source_bone_index)
                .cloned()
                .ok_or_else(|| invalid("bone index is out of range"))?;
            target_index = match bones
                .objects
                .iter()
                .position(|bone| bone.string(NAME).as_deref() == Some(target_bone_name))
            {
                Some(index) if index == source_bone_index => {
                    return Err(invalid("choose a different target bone name"));
                }
                Some(index) => index,
                None => {
                    let index = bones.objects.len();
                    let mut bone = source.clone_as(&format!("bone_{index}"));
                    bone.set_string(NAME, target_bone_name);
                    bones.objects.push(bone);
                    index
                }
            };
            let support = data.require_child_mut(SUPPORT_BONE_LIST, "support_bone_list")?;
            let sources: Vec<AampObject> = support
                .objects
                .iter()
                .filter(|record| record.int(TARGET_BONE) == source_bone_index as i32)
                .cloned()
                .collect();
            if sources.is_empty() {
                return Err(invalid("the selected bone has no support graph to clone"));
            }
            if support
                .objects
                .iter()
                .any(|record| record.int(TARGET_BONE) == target_index as i32)
            {
                return Err(invalid(&format!(
                    "{target_bone_name} already has support data in this file"
                )));
            }
            for record in sources {
                let mut cloned =
                    record.clone_as(&format!("support_bone_{}", support.objects.len()));
                cloned.set_int(TARGET_BONE, target_index as i32);
                support.objects.push(cloned);
            }
            Ok(())
        })?;
        Ok((rebuilt, target_index))
    }

    /// Reflects every input and affected pose in one connected driver group.
    pub fn mirror_driver_group_across_x(&self, group_index: usize) -> io::Result<Self> {
        let groups = build_driver_groups(&self.tree)?;
        let group = groups
            .get(group_index)
            .ok_or_else(|| invalid("driver group index is out of range"))?;
        let mut bones = group.summary.driver_bone_indices.clone();
        for bone in &group.summary.driven_bone_indices {
            if !bones.contains(bone) {
                bones.push(*bone);
            }
        }
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = data.require_child_mut(BONE_LIST, "bone_list")?;
            for bone in &bones {
                if let Some(record) = bone_list.objects.get_mut(*bone) {
                    swap_name(record);
                }
            }
            let main = data.require_child_mut(MAIN_BONE_LIST, "main_bone_list")?;
            for index in &group.driver_indices {
                if let Some(record) = main.objects.get_mut(*index) {
                    mirror_support_record(record);
                }
            }
            let support = data.require_child_mut(SUPPORT_BONE_LIST, "support_bone_list")?;
            for index in &group.support_indices {
                if let Some(record) = support.objects.get_mut(*index) {
                    mirror_support_record(record);
                }
            }
            Ok(())
        })
    }

    pub fn mirror_bone_across_x(&self, bone_index: usize) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = data.require_child_mut(BONE_LIST, "bone_list")?;
            let bone = bone_list
                .objects
                .get_mut(bone_index)
                .ok_or_else(|| invalid("bone index is out of range"))?;
            swap_name(bone);
            for list in [MAIN_BONE_LIST, SUPPORT_BONE_LIST] {
                if let Some(list) = data.find_child_mut(list) {
                    for record in &mut list.objects {
                        if record.int(TARGET_BONE) == bone_index as i32 {
                            mirror_support_record(record);
                        }
                    }
                }
            }
            Ok(())
        })
    }

    pub fn move_bone(
        &self,
        source_bone_index: usize,
        destination_bone_index: usize,
    ) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = data.require_child_mut(BONE_LIST, "bone_list")?;
            let count = bone_list.objects.len();
            if source_bone_index >= count || destination_bone_index >= count {
                return Err(invalid("bone index is out of range"));
            }
            if source_bone_index == destination_bone_index {
                return Ok(());
            }
            let mut order: Vec<usize> = (0..count).collect();
            let moved = order.remove(source_bone_index);
            order.insert(destination_bone_index, moved);
            let mut remap = vec![0i32; count];
            for (new_index, old_index) in order.iter().enumerate() {
                remap[*old_index] = new_index as i32;
            }
            let bone = bone_list.objects.remove(source_bone_index);
            bone_list.objects.insert(destination_bone_index, bone);
            bone_list.normalize_object_names("bone");
            remap_bone_ids(
                data.find_child_mut(MAIN_BONE_LIST),
                &remap,
                &[TARGET_BONE, TARGET_SPACE],
            );
            remap_bone_ids(
                data.find_child_mut(SUPPORT_BONE_LIST),
                &remap,
                &[TARGET_BONE, TARGET_SPACE],
            );
            Ok(())
        })
    }

    /// Imports one connected BotW support-bone graph and remaps its compact
    /// driver, curve, output and bone references into this archive.
    pub fn merge_driver_group(&self, source: &Self, group_index: usize) -> io::Result<Self> {
        let groups = build_driver_groups(&source.tree)?;
        let group = groups
            .get(group_index)
            .ok_or_else(|| invalid("driver group index is out of range"))?;
        let source_data = find_data_root(&source.tree.root)
            .ok_or_else(|| invalid("the reference BPHYSSB has no support-bone data root"))?;
        let source_bones = source_data.require_child(BONE_LIST, "bone_list")?;
        let source_curves =
            source_data.require_child(CONNECTION_CURVE_LIST, "connection_curve_list")?;
        let source_singles = source_data.require_child(OUTPUT_SINGLE_LIST, "output_single_list")?;
        let source_doubles = source_data.find_child(OUTPUT_DOUBLE_LIST);
        let source_main = source_data.require_child(MAIN_BONE_LIST, "main_bone_list")?;
        let source_support = source_data.require_child(SUPPORT_BONE_LIST, "support_bone_list")?;

        self.rebuild(|root| {
            let target = require_data_root_mut(root)?;
            target.require_child(CONNECTION_CURVE_LIST, "connection_curve_list")?;
            target.require_child(MAIN_BONE_LIST, "main_bone_list")?;
            target.require_child(SUPPORT_BONE_LIST, "support_bone_list")?;
            let single_index = target.child_index(OUTPUT_SINGLE_LIST).ok_or_else(|| {
                invalid("the BPHYSSB graph does not contain 'output_single_list'")
            })?;
            if target.find_child(OUTPUT_DOUBLE_LIST).is_none() {
                target
                    .children
                    .insert(single_index + 1, AampList::new(OUTPUT_DOUBLE_LIST));
            }

            let mut bone_map = HashMap::<i32, i32>::new();
            let mut map_bone =
                |target: &mut AampList, source_bone: i32| -> io::Result<i32> {
                    if source_bone < 0 {
                        return Ok(-1);
                    }
                    if let Some(mapped) = bone_map.get(&source_bone) {
                        return Ok(*mapped);
                    }
                    let bone = source_bones
                        .objects
                        .get(source_bone as usize)
                        .ok_or_else(|| {
                            invalid(&format!(
                                "the reference driver group uses missing bone #{source_bone}"
                            ))
                        })?;
                    let name = bone
                        .string(NAME)
                        .unwrap_or_else(|| format!("bone_{source_bone}"));
                    let target_bones = target.require_child_mut(BONE_LIST, "bone_list")?;
                    if let Some(index) = target_bones.objects.iter().position(|candidate| {
                        candidate.string(NAME).as_deref() == Some(name.as_str())
                    }) {
                        bone_map.insert(source_bone, index as i32);
                        return Ok(index as i32);
                    }
                    let index = target_bones.objects.len();
                    target_bones
                        .objects
                        .push(bone.clone_as(&format!("bone_{index}")));
                    bone_map.insert(source_bone, index as i32);
                    Ok(index as i32)
                };

            for bone in &group.required_bone_indices {
                map_bone(target, *bone as i32)?;
            }

            let mut driver_map = HashMap::<usize, i32>::new();
            for source_index in &group.driver_indices {
                let driver = &source_main.objects[*source_index];
                let bone = map_bone(target, driver.int(TARGET_BONE))?;
                let space = map_bone(target, driver.int(TARGET_SPACE))?;
                let main = target.require_child_mut(MAIN_BONE_LIST, "main_bone_list")?;
                if let Some(existing) = main
                    .objects
                    .iter()
                    .position(|candidate| candidate.int(TARGET_BONE) == bone)
                {
                    driver_map.insert(*source_index, existing as i32);
                    continue;
                }
                let mut cloned = driver.clone_as(&format!("main_bone_{}", main.objects.len()));
                cloned.set_int(TARGET_BONE, bone);
                cloned.set_int(TARGET_SPACE, space);
                driver_map.insert(*source_index, main.objects.len() as i32);
                main.objects.push(cloned);
            }

            let mut curve_map = HashMap::<i32, i32>::new();
            let curves =
                target.require_child_mut(CONNECTION_CURVE_LIST, "connection_curve_list")?;
            for source_index in &group.curve_indices {
                let curve = &source_curves.objects[*source_index];
                let packed = curve.int(TARGET_BONE_ATTRIBUTE);
                let source_driver = (packed as u32 >> 16) as usize;
                let target_driver = driver_map.get(&source_driver).ok_or_else(|| {
                    invalid(&format!(
                        "curve #{source_index} escapes the selected driver group"
                    ))
                })?;
                let mut cloned =
                    curve.clone_as(&format!("connection_curve_{}", curves.objects.len()));
                cloned.set_int(
                    TARGET_BONE_ATTRIBUTE,
                    (target_driver << 16) | (packed & 0xffff),
                );
                curve_map.insert(*source_index as i32, curves.objects.len() as i32);
                curves.objects.push(cloned);
            }

            let mut output_map = HashMap::<i32, i32>::new();
            for handle in &group.output_handles {
                if is_double_output_handle(*handle) {
                    let index = (*handle & 0xffff) as usize;
                    let output = source_doubles
                        .and_then(|list| list.objects.get(index))
                        .ok_or_else(|| {
                            invalid(&format!(
                                "the reference driver group uses missing double output #{index}"
                            ))
                        })?;
                    let doubles =
                        target.require_child_mut(OUTPUT_DOUBLE_LIST, "output_double_list")?;
                    let mut cloned =
                        output.clone_as(&format!("output_double_{}", doubles.objects.len()));
                    remap_packed_curve(&mut cloned, TARGET_CONNECTION0, &curve_map)?;
                    remap_packed_curve(&mut cloned, TARGET_CONNECTION1, &curve_map)?;
                    output_map.insert(*handle, DOUBLE_HANDLE_FLAG | doubles.objects.len() as i32);
                    doubles.objects.push(cloned);
                } else {
                    let output = usize::try_from(*handle)
                        .ok()
                        .and_then(|index| source_singles.objects.get(index))
                        .ok_or_else(|| {
                            invalid(&format!(
                                "the reference driver group uses missing single output #{handle}"
                            ))
                        })?;
                    let singles =
                        target.require_child_mut(OUTPUT_SINGLE_LIST, "output_single_list")?;
                    let mut cloned =
                        output.clone_as(&format!("output_single_{}", singles.objects.len()));
                    if output.int(TARGET_CONNECTION) >= 0 {
                        remap_packed_curve(&mut cloned, TARGET_CONNECTION, &curve_map)?;
                    }
                    output_map.insert(*handle, singles.objects.len() as i32);
                    singles.objects.push(cloned);
                }
            }

            for source_index in &group.support_indices {
                let record = &source_support.objects[*source_index];
                let bone = map_bone(target, record.int(TARGET_BONE))?;
                let space = map_bone(target, record.int(TARGET_SPACE))?;
                let support = target.require_child_mut(SUPPORT_BONE_LIST, "support_bone_list")?;
                let mut cloned =
                    record.clone_as(&format!("support_bone_{}", support.objects.len()));
                cloned.set_int(TARGET_BONE, bone);
                cloned.set_int(TARGET_SPACE, space);
                for hash in SUPPORT_OUTPUT_HASHES {
                    let handle = record.int(hash);
                    if handle >= 0 {
                        let target_handle = output_map.get(&handle).ok_or_else(|| {
                            invalid("a support-bone output escapes the selected driver group")
                        })?;
                        cloned.set_int(hash, *target_handle);
                    }
                }
                support.objects.push(cloned);
            }

            if target
                .find_child(OUTPUT_DOUBLE_LIST)
                .is_some_and(|doubles| doubles.objects.is_empty())
            {
                target
                    .children
                    .retain(|child| child.hash != OUTPUT_DOUBLE_LIST);
            }
            Ok(())
        })
    }

    /// Converts the editable TotK helper-bone graph into BotW's separate
    /// support-bone layout. The two share curves and base poses, but their
    /// output tables and reference packing differ.
    pub fn from_helper_bones(source: &HelperBoneDocument) -> io::Result<Self> {
        let bytes = build_from_helper_bones(source)?;
        let document = Self::parse(&bytes)?;
        document.validate()?;
        Ok(document)
    }

    fn rebuild(&self, edit: impl FnOnce(&mut AampList) -> io::Result<()>) -> io::Result<Self> {
        let mut root = self.tree.root.clone();
        edit(&mut root)?;
        synchronize_header_counts(&mut root);
        let tree = AampTree {
            archive_type: self.tree.archive_type.clone(),
            archive_version: self.header.archive_version,
            format_version: self.header.format_version,
            parameter_io_version: self.header.parameter_io_version,
            root,
        };
        Self::parse(&tree.to_bytes()?)
    }
}

pub(crate) fn build_driver_groups(tree: &AampTree) -> io::Result<Vec<DriverGroupData>> {
    let data = find_data_root(&tree.root)
        .ok_or_else(|| invalid("BPHYSSB has no support-bone data root"))?;
    let bones = data.require_child(BONE_LIST, "bone_list")?;
    let curves = data.require_child(CONNECTION_CURVE_LIST, "connection_curve_list")?;
    let singles = data.require_child(OUTPUT_SINGLE_LIST, "output_single_list")?;
    let doubles = data.find_child(OUTPUT_DOUBLE_LIST);
    let main = data.require_child(MAIN_BONE_LIST, "main_bone_list")?;
    let support = data.require_child(SUPPORT_BONE_LIST, "support_bone_list")?;
    if main.objects.is_empty() {
        return Ok(Vec::new());
    }
    let driver_count = main.objects.len();
    let curve_drivers: Vec<i32> = curves
        .objects
        .iter()
        .map(|curve| (curve.int(TARGET_BONE_ATTRIBUTE) as u32 >> 16) as i32)
        .collect();
    let curves_for_handle = |handle: i32| -> Vec<i32> {
        if is_double_output_handle(handle) {
            let index = (handle & 0xffff) as usize;
            match doubles.and_then(|list| list.objects.get(index)) {
                Some(output) => vec![
                    decode_curve_reference(output.int(TARGET_CONNECTION0)),
                    decode_curve_reference(output.int(TARGET_CONNECTION1)),
                ],
                None => Vec::new(),
            }
        } else {
            match usize::try_from(handle)
                .ok()
                .and_then(|index| singles.objects.get(index))
            {
                Some(output) => vec![decode_curve_reference(output.int(TARGET_CONNECTION))],
                None => Vec::new(),
            }
        }
    };
    let drivers_for_curves = |curve_indices: Vec<i32>| -> Vec<usize> {
        distinct_non_negative(
            curve_indices
                .into_iter()
                .filter_map(|curve| usize::try_from(curve).ok())
                .filter_map(|curve| curve_drivers.get(curve).copied())
                .filter(|driver| *driver >= 0 && (*driver as usize) < driver_count),
        )
    };
    let all_handles: Vec<i32> = (0..singles.objects.len() as i32)
        .chain(
            (0..doubles.map_or(0, |list| list.objects.len()) as i32)
                .map(|index| DOUBLE_HANDLE_FLAG | index),
        )
        .collect();

    let mut sets = UnionFind::new(driver_count);
    for handle in &all_handles {
        let linked = drivers_for_curves(curves_for_handle(*handle));
        for driver in linked.iter().skip(1) {
            sets.union(linked[0], *driver);
        }
    }
    for record in &support.objects {
        let linked = drivers_for_curves(
            SUPPORT_OUTPUT_HASHES
                .iter()
                .map(|hash| record.int(*hash))
                .filter(|handle| *handle >= 0)
                .flat_map(curves_for_handle)
                .collect(),
        );
        for driver in linked.iter().skip(1) {
            sets.union(linked[0], *driver);
        }
    }

    let bone_names: Vec<String> = bones
        .objects
        .iter()
        .enumerate()
        .map(|(index, bone)| bone.string(NAME).unwrap_or_else(|| format!("bone_{index}")))
        .collect();
    Ok(sets
        .groups()
        .into_iter()
        .enumerate()
        .map(|(group_index, driver_indices)| {
            let curve_indices: Vec<usize> = curve_drivers
                .iter()
                .enumerate()
                .filter(|(_, driver)| {
                    usize::try_from(**driver).is_ok_and(|driver| driver_indices.contains(&driver))
                })
                .map(|(index, _)| index)
                .collect();
            let active_handles: Vec<i32> = all_handles
                .iter()
                .copied()
                .filter(|handle| {
                    curves_for_handle(*handle).iter().any(|curve| {
                        usize::try_from(*curve).is_ok_and(|curve| curve_indices.contains(&curve))
                    })
                })
                .collect();
            let support_indices: Vec<usize> = support
                .objects
                .iter()
                .enumerate()
                .filter(|(_, record)| {
                    SUPPORT_OUTPUT_HASHES
                        .iter()
                        .any(|hash| active_handles.contains(&record.int(*hash)))
                })
                .map(|(index, _)| index)
                .collect();
            let mut output_handles = Vec::new();
            for index in &support_indices {
                for hash in SUPPORT_OUTPUT_HASHES {
                    let handle = support.objects[*index].int(hash);
                    if handle >= 0 && !output_handles.contains(&handle) {
                        output_handles.push(handle);
                    }
                }
            }
            let driver_bones = distinct_non_negative(
                driver_indices
                    .iter()
                    .map(|index| main.objects[*index].int(TARGET_BONE)),
            );
            let driven_bones = distinct_non_negative(
                support_indices
                    .iter()
                    .map(|index| support.objects[*index].int(TARGET_BONE)),
            );
            let required = distinct_non_negative(
                driver_indices
                    .iter()
                    .flat_map(|index| {
                        [
                            main.objects[*index].int(TARGET_BONE),
                            main.objects[*index].int(TARGET_SPACE),
                        ]
                    })
                    .chain(support_indices.iter().flat_map(|index| {
                        [
                            support.objects[*index].int(TARGET_BONE),
                            support.objects[*index].int(TARGET_SPACE),
                        ]
                    })),
            );
            let names = |indices: &[usize]| -> Vec<String> {
                indices
                    .iter()
                    .map(|index| bone_name(&bone_names, *index as i32))
                    .collect()
            };
            DriverGroupData {
                summary: SidecarDriverGroup {
                    index: group_index,
                    driver_indices: driver_indices.clone(),
                    driver_bone_indices: driver_bones.clone(),
                    driven_bone_indices: driven_bones.clone(),
                    driver_bone_names: names(&driver_bones),
                    driven_bone_names: names(&driven_bones),
                    required_bone_indices: required.clone(),
                    required_bone_names: names(&required),
                    curve_count: curve_indices.len(),
                    output_count: output_handles.len(),
                },
                driver_indices,
                curve_indices,
                output_handles,
                support_indices,
                required_bone_indices: required,
            }
        })
        .collect())
}

fn read_graph(tree: &AampTree) -> Option<SupportBoneGraph> {
    if !tree.archive_type.eq_ignore_ascii_case("physsb") {
        return None;
    }
    let data = find_data_root(&tree.root)?;
    let bone_list = data.find_child(BONE_LIST)?;
    let curve_list = data.find_child(CONNECTION_CURVE_LIST)?;
    let main_list = data.find_child(MAIN_BONE_LIST)?;
    let support_list = data.find_child(SUPPORT_BONE_LIST)?;
    Some(SupportBoneGraph {
        bones: bone_list
            .objects
            .iter()
            .enumerate()
            .map(|(index, bone)| bone.string(NAME).unwrap_or_else(|| format!("bone_{index}")))
            .collect(),
        main_bones: main_list.objects.iter().map(read_pose_bone).collect(),
        support_bones: support_list.objects.iter().map(read_pose_bone).collect(),
        curve_count: curve_list.objects.len(),
        output_single_count: data
            .find_child(OUTPUT_SINGLE_LIST)
            .map_or(0, |list| list.objects.len()),
        output_double_count: data
            .find_child(OUTPUT_DOUBLE_LIST)
            .map_or(0, |list| list.objects.len()),
    })
}

fn read_pose_bone(record: &AampObject) -> PoseBone {
    PoseBone {
        bone_id: record.int(TARGET_BONE),
        space_id: record.int(TARGET_SPACE),
        translation: record.vec3(bphhb::BASE_TRANSLATION).unwrap_or([0.0; 3]),
        rotation: record
            .vec4(bphhb::BASE_ROTATION)
            .unwrap_or([0.0, 0.0, 0.0, 1.0]),
    }
}

/// A support-bone archive can contain unrelated top-level lists; the graph
/// owner is the one carrying all four core tables.
fn find_data_root(root: &AampList) -> Option<&AampList> {
    root.children.iter().find(|child| {
        [
            BONE_LIST,
            CONNECTION_CURVE_LIST,
            MAIN_BONE_LIST,
            SUPPORT_BONE_LIST,
        ]
        .iter()
        .all(|hash| child.find_child(*hash).is_some())
    })
}

fn single_data_root(root: &AampList) -> Option<&AampList> {
    (root.children.len() == 1).then(|| &root.children[0])
}

fn require_data_root_mut(root: &mut AampList) -> io::Result<&mut AampList> {
    if root.children.len() != 1 {
        return Err(invalid(
            "the BPHYSSB archive does not contain one editable data root",
        ));
    }
    Ok(&mut root.children[0])
}

fn synchronize_header_counts(root: &mut AampList) {
    if root.objects.is_empty() || root.children.len() != 1 {
        return;
    }
    let counts: Vec<(u32, i32)> = [
        (BONE_COUNT, BONE_LIST),
        (CURVE_COUNT, CONNECTION_CURVE_LIST),
        (SINGLE_OUTPUT_COUNT, OUTPUT_SINGLE_LIST),
        (DOUBLE_OUTPUT_COUNT, OUTPUT_DOUBLE_LIST),
        (MAIN_BONE_COUNT, MAIN_BONE_LIST),
        (SUPPORT_BONE_COUNT, SUPPORT_BONE_LIST),
    ]
    .iter()
    .map(|(count_hash, list_hash)| {
        (
            *count_hash,
            root.children[0]
                .find_child(*list_hash)
                .map_or(0, |list| list.objects.len() as i32),
        )
    })
    .collect();
    let header = &mut root.objects[0];
    for (hash, value) in counts {
        header.set_existing_int(hash, value);
    }
}

fn clone_pose_records(
    list: Option<&mut AampList>,
    source_bone: usize,
    new_bone: usize,
    prefix: &str,
) {
    let Some(list) = list else {
        return;
    };
    let matching: Vec<AampObject> = list
        .objects
        .iter()
        .filter(|record| record.int(TARGET_BONE) == source_bone as i32)
        .cloned()
        .collect();
    for record in matching {
        let mut cloned = record.clone_as(&format!("{prefix}_{}", list.objects.len()));
        cloned.set_int(TARGET_BONE, new_bone as i32);
        list.objects.push(cloned);
    }
}

fn swap_name(record: &mut AampObject) {
    if let Some(name) = record.string(NAME).filter(|name| !name.trim().is_empty()) {
        record.set_string(NAME, &swap_side_tokens(&name));
    }
}

/// BotW pose records use the same base-translate/rotate hashes as TotK but
/// name their aim and up axes differently.
fn mirror_support_record(record: &mut AampObject) {
    mirror_transform_record(record);
    for hash in [TARGET_AIM, TARGET_UP] {
        if let Some(mut direction) = record.vec3(hash) {
            direction[0] = -direction[0];
            record.set_vec3(hash, direction);
        }
    }
}

fn remap_packed_curve(
    record: &mut AampObject,
    hash: u32,
    curve_map: &HashMap<i32, i32>,
) -> io::Result<()> {
    let reference = record.int(hash);
    let source_curve = decode_curve_reference(reference);
    let target_curve = (source_curve >= 0)
        .then(|| curve_map.get(&source_curve))
        .flatten()
        .ok_or_else(|| invalid("an output references a curve outside the selected driver group"))?;
    record.set_int(
        hash,
        if is_double_output_handle(reference) {
            DOUBLE_HANDLE_FLAG | *target_curve
        } else {
            *target_curve
        },
    );
    Ok(())
}

// ---- BPHHB -> BPHYSSB conversion ------------------------------------------

fn build_from_helper_bones(source: &HelperBoneDocument) -> io::Result<Vec<u8>> {
    use bphhb::{
        AIM_AXIS, BASE_BONE_ID as _, BASE_ROTATION, BASE_TRANSLATION, BEND_H_OUTPUT, BEND_V_OUTPUT,
        BONE_ID, CURVE_ATTRIBUTE, CURVE_DRIVER, CURVE_KEY0, CURVE_KEY1, CURVE_KEY2, CURVE_KEY3,
        CURVE_KEY_COUNT, OUTPUT_CONNECTION0, OUTPUT_CONNECTION1, ROLL_OUTPUT, TRANSLATE_X_OUTPUT,
        TRANSLATE_Y_OUTPUT, TRANSLATE_Z_OUTPUT, UP_AXIS,
    };
    if !source.tree.archive_type.eq_ignore_ascii_case("phhb") {
        return Err(invalid(
            "only TotK BPHHB helper-bone files can be converted to BPHYSSB",
        ));
    }
    let source_header = match source.tree.root.objects.as_slice() {
        [header] => header,
        _ => return Err(invalid("BPHHB is missing its helper-bone header")),
    };
    let source_data = single_data_root(&source.tree.root)
        .ok_or_else(|| invalid("BPHHB is missing its helper-bone data list"))?;
    let source_bones = source_data.require_child(BONE_LIST, "bone_list")?;
    let source_curves =
        source_data.require_child(CONNECTION_CURVE_LIST, "connection_curve_list")?;
    let source_outputs = source_data.require_child(bphhb::OUTPUT_LIST, "output_list")?;
    let source_drivers = source_data.require_child(bphhb::DRIVER_LIST, "driver_bone_list")?;
    let source_driven = source_data.require_child(bphhb::DRIVEN_LIST, "driven_bone_list")?;
    let source_poses = source_data.require_child(bphhb::POSE_DRIVEN_LIST, "pose_driven_list")?;
    if source_driven.objects.len() != source_poses.objects.len() {
        return Err(invalid(&format!(
            "BPHHB has {} driven records but {} pose-driven records",
            source_driven.objects.len(),
            source_poses.objects.len()
        )));
    }
    let source_bone_names: Vec<String> = source_bones
        .objects
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            bone.string(NAME)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("bone_{index}"))
        })
        .collect();
    let driver_bone_ids: Vec<i32> = source_drivers
        .objects
        .iter()
        .map(|record| required_int(record, BONE_ID, "driver_bone_list"))
        .collect::<io::Result<_>>()?;
    let driven_bone_ids: Vec<i32> = source_driven
        .objects
        .iter()
        .map(|record| required_int(record, BONE_ID, "driven_bone_list"))
        .collect::<io::Result<_>>()?;

    // Native BPHYSSB files keep a compact bone table containing the driver
    // and driven bones only. BPHHB base_bone_id is helper-graph context, not
    // an extra BotW support-bone table entry.
    let mut required_bone_ids: Vec<i32> = Vec::new();
    for bone_id in driver_bone_ids.iter().chain(&driven_bone_ids) {
        if *bone_id >= 0 && !required_bone_ids.contains(bone_id) {
            required_bone_ids.push(*bone_id);
        }
    }
    for bone_id in &required_bone_ids {
        if *bone_id as usize >= source_bone_names.len() {
            return Err(invalid(&format!(
                "BPHHB references helper bone #{bone_id}, but its bone list contains {} entries",
                source_bone_names.len()
            )));
        }
    }
    let target_bone_ids: HashMap<i32, i32> = required_bone_ids
        .iter()
        .enumerate()
        .map(|(target, source)| (*source, target as i32))
        .collect();

    let mut root = AampList::new(PARAM_ROOT);
    let mut bone_list = AampList::new(BONE_LIST);
    let mut curve_list = AampList::new(CONNECTION_CURVE_LIST);
    let mut single_list = AampList::new(OUTPUT_SINGLE_LIST);
    let mut double_list = AampList::new(OUTPUT_DOUBLE_LIST);
    let mut main_list = AampList::new(MAIN_BONE_LIST);
    let mut support_list = AampList::new(SUPPORT_BONE_LIST);

    for source_bone in &required_bone_ids {
        let mut bone = AampObject::named(&format!("bone_{}", bone_list.objects.len()));
        bone.push(AampParameter::string(
            NAME,
            KIND_STRING64,
            &source_bone_names[*source_bone as usize],
        ));
        bone_list.objects.push(bone);
    }

    for (index, source_curve) in source_curves.objects.iter().enumerate() {
        let source_driver = required_int(source_curve, CURVE_DRIVER, "connection_curve_list")?;
        if source_driver < 0 || source_driver as usize >= source_drivers.objects.len() {
            return Err(invalid(&format!(
                "BPHHB curve #{index} references missing driver #{source_driver}"
            )));
        }
        let attribute = required_int(source_curve, CURVE_ATTRIBUTE, "connection_curve_list")?;
        let mut curve = AampObject::named(&format!("connection_curve_{index}"));
        curve.push(AampParameter::int(
            TARGET_BONE_ATTRIBUTE,
            pack_bone_attribute(source_driver, attribute)?,
        ));
        curve.push(AampParameter::boolean(TARGET_CONSTANT_IN, false));
        curve.push(AampParameter::boolean(TARGET_CONSTANT_OUT, false));
        copy_curve_keys(
            &mut curve,
            source_curve,
            CURVE_KEY_COUNT,
            [CURVE_KEY0, CURVE_KEY1, CURVE_KEY2, CURVE_KEY3],
        )?;
        curve_list.objects.push(curve);
    }

    let curve_count = curve_list.objects.len() as i32;
    let source_connections: Vec<(i32, i32)> = source_outputs
        .objects
        .iter()
        .enumerate()
        .map(|(index, output)| {
            let connection0 = output.typed_int(OUTPUT_CONNECTION0).unwrap_or(-1);
            let connection1 = output.typed_int(OUTPUT_CONNECTION1).unwrap_or(-1);
            for connection in [connection0, connection1] {
                if connection >= curve_count {
                    return Err(invalid(&format!(
                        "BPHHB output #{index} references curve #{connection}, but only {curve_count} curves exist"
                    )));
                }
            }
            Ok((connection0, connection1))
        })
        .collect::<io::Result<_>>()?;

    let mut output_handles = HashMap::<i32, i32>::new();
    let mut add_inactive_single = |singles: &mut AampList| -> i32 {
        let index = singles.objects.len() as i32;
        let mut output = AampObject::named(&format!("output_single_{index}"));
        output.push(AampParameter::int(TARGET_CONNECTION, -1));
        output.push(AampParameter::float(TARGET_WEIGHT, 0.0));
        singles.objects.push(output);
        index
    };
    let mut handle_for_output =
        |singles: &mut AampList, doubles: &mut AampList, source_output: i32| -> io::Result<i32> {
            if let Some(existing) = output_handles.get(&source_output) {
                return Ok(*existing);
            }
            let (connection0, connection1) = *usize::try_from(source_output)
                .ok()
                .and_then(|index| source_connections.get(index))
                .ok_or_else(|| {
                    invalid(&format!(
                        "BPHHB pose references missing output #{source_output}"
                    ))
                })?;
            let handle = if connection0 >= 0 && connection1 >= 0 {
                let index = doubles.objects.len() as i32;
                let mut output = AampObject::named(&format!("output_double_{index}"));
                output.push(AampParameter::int(
                    TARGET_CONNECTION0,
                    encode_curve_reference(connection0),
                ));
                output.push(AampParameter::float(TARGET_WEIGHT0, 1.0));
                output.push(AampParameter::int(
                    TARGET_CONNECTION1,
                    encode_curve_reference(connection1),
                ));
                output.push(AampParameter::float(TARGET_WEIGHT1, 1.0));
                doubles.objects.push(output);
                DOUBLE_HANDLE_FLAG | index
            } else if connection0 >= 0 || connection1 >= 0 {
                let index = singles.objects.len() as i32;
                let mut output = AampObject::named(&format!("output_single_{index}"));
                output.push(AampParameter::int(
                    TARGET_CONNECTION,
                    encode_curve_reference(connection0.max(connection1)),
                ));
                output.push(AampParameter::float(TARGET_WEIGHT, 1.0));
                singles.objects.push(output);
                index
            } else {
                add_inactive_single(singles)
            };
            output_handles.insert(source_output, handle);
            Ok(handle)
        };

    let target_bone = |source_bone: i32, role: &str, index: usize| -> io::Result<i32> {
        target_bone_ids.get(&source_bone).copied().ok_or_else(|| {
            invalid(&format!(
                "BPHHB {role} #{index} references bone #{source_bone}, which is not a usable helper bone"
            ))
        })
    };
    if source_poses.objects.len() < source_driven.objects.len() {
        return Err(invalid(&format!(
            "BPHHB has {} driven bones but only {} pose records",
            source_driven.objects.len(),
            source_poses.objects.len()
        )));
    }

    for (index, record) in source_drivers.objects.iter().enumerate() {
        let mut main = AampObject::named(&format!("main_bone_{}", main_list.objects.len()));
        main.push(AampParameter::int(
            TARGET_BONE,
            target_bone(driver_bone_ids[index], "driver", index)?,
        ));
        main.copy_from(record, BASE_TRANSLATION, None)?;
        main.copy_from(record, BASE_ROTATION, None)?;
        main.copy_from(record, AIM_AXIS, Some(TARGET_AIM))?;
        main.copy_from(record, UP_AXIS, Some(TARGET_UP))?;
        main.push(AampParameter::int(TARGET_SPACE, -1));
        main_list.objects.push(main);
    }

    for (index, pose) in source_poses
        .objects
        .iter()
        .take(source_driven.objects.len())
        .enumerate()
    {
        let mut support =
            AampObject::named(&format!("support_bone_{}", support_list.objects.len()));
        support.push(AampParameter::int(
            TARGET_BONE,
            target_bone(driven_bone_ids[index], "driven bone", index)?,
        ));
        support.copy_from(pose, BASE_TRANSLATION, None)?;
        support.copy_from(pose, BASE_ROTATION, None)?;
        support.copy_from(pose, AIM_AXIS, Some(TARGET_AIM))?;
        support.copy_from(pose, UP_AXIS, Some(TARGET_UP))?;
        support.push(AampParameter::int(TARGET_SPACE, -1));
        // BotW stores the three rotational outputs as positional channels.
        // Even an unused bend/roll channel owns a null single-output slot;
        // translation channels are sparse and stay -1 when unconnected.
        for (target_hash, source_hash, rotation) in [
            (TARGET_BEND_H, BEND_H_OUTPUT, true),
            (TARGET_BEND_V, BEND_V_OUTPUT, true),
            (TARGET_ROLL, ROLL_OUTPUT, true),
            (TARGET_TRANSLATE_X, TRANSLATE_X_OUTPUT, false),
            (TARGET_TRANSLATE_Y, TRANSLATE_Y_OUTPUT, false),
            (TARGET_TRANSLATE_Z, TRANSLATE_Z_OUTPUT, false),
        ] {
            let source_output = pose.typed_int(source_hash).unwrap_or(-1);
            let handle = if source_output >= 0 {
                handle_for_output(&mut single_list, &mut double_list, source_output)?
            } else if rotation {
                add_inactive_single(&mut single_list)
            } else {
                -1
            };
            support.push(AampParameter::int(target_hash, handle));
        }
        support_list.objects.push(support);
    }

    let mut header = AampObject::new(SUPPORT_BONE_HEADER);
    match source_header.find(HEADER_STEP) {
        Some(step) => header.push(step.clone()),
        None => header.push(AampParameter::float(HEADER_STEP, 0.1)),
    }
    header.push(AampParameter::int(
        BONE_COUNT,
        bone_list.objects.len() as i32,
    ));
    header.push(AampParameter::int(UNKNOWN_HEADER_COUNT, 0));
    header.push(AampParameter::int(
        CURVE_COUNT,
        curve_list.objects.len() as i32,
    ));
    header.push(AampParameter::int(
        SINGLE_OUTPUT_COUNT,
        single_list.objects.len() as i32,
    ));
    header.push(AampParameter::int(
        DOUBLE_OUTPUT_COUNT,
        double_list.objects.len() as i32,
    ));
    header.push(AampParameter::int(
        MAIN_BONE_COUNT,
        main_list.objects.len() as i32,
    ));
    header.push(AampParameter::int(
        SUPPORT_BONE_COUNT,
        support_list.objects.len() as i32,
    ));
    root.objects.push(header);

    let mut data = AampList::new(SUPPORT_BONE_DATA);
    data.children.push(bone_list);
    data.children.push(curve_list);
    data.children.push(single_list);
    // BotW omits this node when no output uses two curves.
    if !double_list.objects.is_empty() {
        data.children.push(double_list);
    }
    data.children.push(main_list);
    data.children.push(support_list);
    root.children.push(data);

    AampTree {
        archive_type: "physsb".into(),
        archive_version: source.header.archive_version,
        format_version: source.header.format_version,
        parameter_io_version: source.header.parameter_io_version,
        root,
    }
    .to_bytes()
}

/// phhb and physsb use compact channel IDs in different contexts. Attributes
/// 3-5 are translation X/Y/Z drivers; BPHYSSB stores the corresponding axis in
/// the same three compact channels as rotation.
fn pack_bone_attribute(driver_index: i32, attribute: i32) -> io::Result<i32> {
    if !(0..=0xffff).contains(&driver_index) {
        return Err(invalid(&format!(
            "BPHHB driver index {driver_index} is outside the BPHYSSB packing range"
        )));
    }
    let channel = match attribute {
        0 => 2,
        1 => 0,
        2 => 1,
        3 => 0,
        4 => 1,
        5 => 2,
        other => {
            return Err(invalid(&format!(
                "unsupported BPHHB curve attribute {other}"
            )))
        }
    };
    Ok((driver_index << 16) | channel)
}

/// BotW stores three curve keys. TotK's four-key form folds its inner
/// tangents into the middle BotW key; a clamped two-key curve gains a flat
/// key on the missing side.
fn copy_curve_keys(
    target: &mut AampObject,
    source: &AampObject,
    key_count_hash: u32,
    keys: [u32; 4],
) -> io::Result<()> {
    let [key0, key1, key2, key3] = keys;
    let key_count = source.typed_int(key_count_hash).unwrap_or(-1);
    if key_count == 4 {
        if let (Some(first), Some(second), Some(terminal)) =
            (source.find(key0), source.find(key1), source.find(key3))
        {
            let middle = second.as_vec4().unwrap_or_default();
            let end = terminal.as_vec4().unwrap_or_default();
            target.push(first.clone());
            target.push(AampParameter::vec4(
                key1,
                [middle[0], middle[1], middle[2], end[2]],
            ));
            target.push(AampParameter::raw(
                key2,
                terminal.kind,
                terminal.bytes.clone(),
            ));
            return Ok(());
        }
    }
    if key_count == 2 {
        if let (Some(first), Some(second)) = (source.vec4(key0), source.vec4(key1)) {
            const EPSILON: f32 = 1.0e-5;
            let span = (second[0] - first[0]).abs().max(1.0e-3);
            if first[0].abs() <= EPSILON && second[0] > EPSILON {
                target.push(AampParameter::vec4(
                    key0,
                    [first[0] - span, first[1], 0.0, 0.0],
                ));
                target.push(AampParameter::vec4(
                    key1,
                    [first[0], first[1], 0.0, first[3]],
                ));
                target.push(AampParameter::vec4(key2, second));
            } else {
                target.push(AampParameter::vec4(key0, first));
                target.push(AampParameter::vec4(
                    key1,
                    [second[0], second[1], second[2], 0.0],
                ));
                target.push(AampParameter::vec4(
                    key2,
                    [second[0] + span, second[1], 0.0, 0.0],
                ));
            }
            return Ok(());
        }
    }
    target.copy_from(source, key0, None)?;
    target.copy_from(source, key1, None)?;
    match source.find(key2) {
        Some(parameter) => target.push(parameter.clone()),
        // Curve keys are hkVector4-style values; a scalar float has the wrong
        // byte width and changes the curve evaluator's interpretation.
        None => target.push(AampParameter::vec4(key2, [0.0; 4])),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_their_parameter_names() {
        assert_eq!(crc32("param_root"), PARAM_ROOT);
        assert_eq!(crc32("support_bone_data"), SUPPORT_BONE_DATA);
        assert_eq!(crc32("bone_list"), BONE_LIST);
        assert_eq!(crc32("connection_curve_list"), CONNECTION_CURVE_LIST);
        assert_eq!(crc32("output_single_list"), OUTPUT_SINGLE_LIST);
        assert_eq!(crc32("output_double_list"), OUTPUT_DOUBLE_LIST);
        assert_eq!(crc32("main_bone_list"), MAIN_BONE_LIST);
        assert_eq!(crc32("support_bone_list"), SUPPORT_BONE_LIST);
    }

    #[test]
    fn double_output_handles_carry_the_high_flag() {
        assert!(is_double_output_handle(0x0001_0003));
        assert!(!is_double_output_handle(3));
        assert!(!is_double_output_handle(-1));
        assert_eq!(decode_curve_reference(0x0001_0007), 7);
        assert_eq!(decode_curve_reference(7), 7);
        assert_eq!(decode_curve_reference(-1), -1);
        assert_eq!(encode_curve_reference(4), 0x0001_0004);
    }

    #[test]
    fn bone_attributes_pack_driver_and_channel() {
        assert_eq!(pack_bone_attribute(2, 0).unwrap(), (2 << 16) | 2);
        assert_eq!(pack_bone_attribute(5, 4).unwrap(), (5 << 16) | 1);
        assert!(pack_bone_attribute(1, 9).is_err());
    }
}

//! Native editing of TotK helper-bone sidecars (`.bphhb`, AAMP type `phhb`).
//!
//! The graph lives in one `helper_bone_data` list: a bone table, connection
//! curves, outputs that combine one or two curves, driver bones that provide
//! motion input, and driven bones paired index-for-index with the pose
//! records that receive the output. Every edit rebuilds the archive from the
//! decoded tree so unrelated parameters survive untouched.

use super::{
    aamp_tree::{crc32, invalid, AampList, AampObject, AampParameter, AampTree, KIND_STRING256},
    bphyssb::{self, SupportBoneDocument},
    sidecar::{
        bone_name, distinct_non_negative, mirror_quaternion_across_x, swap_side_tokens,
        unique_bone_name, SidecarDriverGroup, UnionFind,
    },
};
use serde::Serialize;
use std::{collections::HashMap, io};

pub const BONE_ID: u32 = 3_407_736_066;
pub const BASE_BONE_ID: u32 = 4_279_981_158;
pub const DRIVER_LIST: u32 = 3_993_686_733;
pub const DRIVEN_LIST: u32 = 1_256_621_249;
pub const POSE_DRIVEN_LIST: u32 = 1_386_686_749;
pub const OUTPUT_LIST: u32 = 1_224_233_410;
pub const NAME: u32 = 1_579_384_326;
pub const CURVE_DRIVER: u32 = 249_727_330;
pub const CURVE_ATTRIBUTE: u32 = 75_375_380;
pub const CURVE_KEY_COUNT: u32 = 139_113_150;
pub const CURVE_KEY0: u32 = 1_692_570_776;
pub const CURVE_KEY1: u32 = 333_816_846;
pub const CURVE_KEY2: u32 = 2_330_785_204;
pub const CURVE_KEY3: u32 = 4_260_087_074;
pub const OUTPUT_CONNECTION0: u32 = 2_390_703_269;
pub const OUTPUT_CONNECTION1: u32 = 918_772_672;
pub const BASE_TRANSLATION: u32 = 4_181_521_146;
pub const BASE_ROTATION: u32 = 1_609_157_221;
pub const AIM_AXIS: u32 = 3_442_436_866;
pub const UP_AXIS: u32 = 907_422_927;
pub const ROLL_OUTPUT: u32 = 2_869_652_774;
pub const BEND_H_OUTPUT: u32 = 3_079_588_026;
pub const BEND_V_OUTPUT: u32 = 122_218_518;
pub const TRANSLATE_X_OUTPUT: u32 = 3_595_547_632;
pub const TRANSLATE_Y_OUTPUT: u32 = 1_861_473_429;
pub const TRANSLATE_Z_OUTPUT: u32 = 2_084_993_915;
pub const HEADER_STEP: u32 = 423_031_007;

pub const POSE_OUTPUT_HASHES: [u32; 6] = [
    ROLL_OUTPUT,
    BEND_H_OUTPUT,
    BEND_V_OUTPUT,
    TRANSLATE_X_OUTPUT,
    TRANSLATE_Y_OUTPUT,
    TRANSLATE_Z_OUTPUT,
];

const HEADER_SIZE: usize = 0x30;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverBone {
    pub bone_id: i32,
    pub base_bone_id: i32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrivenBone {
    pub bone_id: i32,
    pub translate_driven_id: i32,
    pub rotate_driven_id: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoseDrivenBone {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub base_bone_id: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLink {
    pub connection0: i32,
    pub connection1: Option<i32>,
}

/// The helper-bone relationships decoded from the `phhb` archive.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperBoneGraph {
    pub bones: Vec<String>,
    pub drivers: Vec<DriverBone>,
    pub driven: Vec<DrivenBone>,
    pub pose_driven: Vec<PoseDrivenBone>,
    pub outputs: Vec<OutputLink>,
    pub curve_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarHeader {
    pub file_size: usize,
    pub archive_version: u32,
    pub format_version: u32,
    pub parameter_io_version: u32,
    pub list_count: u32,
    pub object_count: u32,
    pub parameter_count: u32,
    pub data_size: u32,
    pub string_pool_size: u32,
}

#[derive(Clone, Debug)]
pub struct HelperBoneDocument {
    pub bytes: Vec<u8>,
    pub header: SidecarHeader,
    pub tree: AampTree,
    pub graph: Option<HelperBoneGraph>,
}

/// One driver group with the record indices the merge needs.
#[derive(Clone, Debug)]
pub(crate) struct DriverGroupData {
    pub summary: SidecarDriverGroup,
    pub driver_indices: Vec<usize>,
    pub curve_indices: Vec<usize>,
    pub output_indices: Vec<usize>,
    pub driven_indices: Vec<usize>,
    pub required_bone_indices: Vec<usize>,
}

pub(crate) fn read_sidecar_header(bytes: &[u8], expected_type: &str) -> io::Result<SidecarHeader> {
    if bytes.len() < HEADER_SIZE {
        return Err(invalid("the sidecar is shorter than an AAMP header"));
    }
    if &bytes[..4] != b"AAMP" {
        return Err(invalid("the sidecar must begin with the AAMP signature"));
    }
    let word = |offset: usize| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let declared = word(0x0c) as usize;
    if declared != bytes.len() {
        return Err(invalid(&format!(
            "AAMP header declares {declared} bytes, but the file contains {}",
            bytes.len()
        )));
    }
    let root_offset = word(0x14) as usize;
    if HEADER_SIZE.saturating_add(root_offset) >= bytes.len() {
        return Err(invalid("AAMP parameter root offset lies outside the file"));
    }
    let string_pool_size = word(0x28);
    if string_pool_size as usize > bytes.len() {
        return Err(invalid("AAMP string pool lies outside the file"));
    }
    let type_bytes = &bytes[HEADER_SIZE..HEADER_SIZE + root_offset];
    let type_end = type_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(type_bytes.len());
    let archive_type = String::from_utf8_lossy(&type_bytes[..type_end]);
    if !archive_type.eq_ignore_ascii_case(expected_type) {
        return Err(invalid(&format!(
            "expected an AAMP {expected_type} file, found '{archive_type}'"
        )));
    }
    Ok(SidecarHeader {
        file_size: bytes.len(),
        archive_version: word(0x04),
        format_version: word(0x08),
        parameter_io_version: word(0x10),
        list_count: word(0x18),
        object_count: word(0x1c),
        parameter_count: word(0x20),
        data_size: word(0x24),
        string_pool_size,
    })
}

impl HelperBoneDocument {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let header = read_sidecar_header(bytes, "phhb")?;
        let tree = AampTree::parse(bytes)?;
        let graph = read_graph(&tree);
        Ok(Self {
            bytes: bytes.to_vec(),
            header,
            tree,
            graph,
        })
    }

    /// Helper bone names from the graph, or every string in the pool when the
    /// graph could not be decoded.
    pub fn bone_names(&self) -> Vec<String> {
        match &self.graph {
            Some(graph) if !graph.bones.is_empty() => graph.bones.clone(),
            _ => read_string_pool(&self.bytes, self.header.string_pool_size),
        }
    }

    pub fn driver_groups(&self) -> io::Result<Vec<SidecarDriverGroup>> {
        Ok(build_driver_groups(&self.tree)?
            .into_iter()
            .map(|group| group.summary)
            .collect())
    }

    /// Imports one connected helper-bone driver graph. Bone names are matched
    /// against the target while driver, curve, output, driven and pose indices
    /// are rebuilt in the target's native tables.
    pub fn merge_driver_group(&self, source: &Self, group_index: usize) -> io::Result<Self> {
        let groups = build_driver_groups(&source.tree)?;
        let group = groups
            .get(group_index)
            .ok_or_else(|| invalid("driver group index is out of range"))?;
        let source_data = data_root(&source.tree.root)
            .ok_or_else(|| invalid("the reference BPHHB has no helper-bone data root"))?;
        let source_bones = require(source_data, "bone_list")?;
        let source_curves = require(source_data, "connection_curve_list")?;
        let source_outputs = require(source_data, "output_list")?;
        let source_drivers = require(source_data, "driver_bone_list")?;
        let source_driven = require(source_data, "driven_bone_list")?;
        let source_poses = require(source_data, "pose_driven_list")?;

        self.rebuild(|root| {
            let target = require_data_root_mut(root)?;
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
                    let target_bones = require_mut(target, "bone_list")?;
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
                let driver = &source_drivers.objects[*source_index];
                let bone = map_bone(target, driver.int(BONE_ID))?;
                let base = map_bone(target, driver.int(BASE_BONE_ID))?;
                let drivers = require_mut(target, "driver_bone_list")?;
                if let Some(existing) = drivers
                    .objects
                    .iter()
                    .position(|candidate| candidate.int(BONE_ID) == bone)
                {
                    driver_map.insert(*source_index, existing as i32);
                    continue;
                }
                let mut cloned = driver.clone_as(&format!("driver_bone_{}", drivers.objects.len()));
                cloned.set_int(BONE_ID, bone);
                cloned.set_int(BASE_BONE_ID, base);
                driver_map.insert(*source_index, drivers.objects.len() as i32);
                drivers.objects.push(cloned);
            }

            let mut curve_map = HashMap::<i32, i32>::new();
            let curves = require_mut(target, "connection_curve_list")?;
            for source_index in &group.curve_indices {
                let curve = &source_curves.objects[*source_index];
                let source_driver = curve.int(CURVE_DRIVER);
                let target_driver = usize::try_from(source_driver)
                    .ok()
                    .and_then(|index| driver_map.get(&index))
                    .ok_or_else(|| {
                        invalid(&format!(
                            "curve #{source_index} escapes the selected driver group"
                        ))
                    })?;
                let mut cloned =
                    curve.clone_as(&format!("connection_curve_{}", curves.objects.len()));
                cloned.set_int(CURVE_DRIVER, *target_driver);
                curve_map.insert(*source_index as i32, curves.objects.len() as i32);
                curves.objects.push(cloned);
            }

            let mut output_map = HashMap::<i32, i32>::new();
            let outputs = require_mut(target, "output_list")?;
            for source_index in &group.output_indices {
                let mut cloned = source_outputs.objects[*source_index]
                    .clone_as(&format!("output_{}", outputs.objects.len()));
                remap_required(&mut cloned, OUTPUT_CONNECTION0, &curve_map, "curve")?;
                remap_optional(&mut cloned, OUTPUT_CONNECTION1, &curve_map);
                output_map.insert(*source_index as i32, outputs.objects.len() as i32);
                outputs.objects.push(cloned);
            }

            for source_index in &group.driven_indices {
                let (Some(driven), Some(pose)) = (
                    source_driven.objects.get(*source_index),
                    source_poses.objects.get(*source_index),
                ) else {
                    return Err(invalid(&format!(
                        "driven record #{source_index} has no matching pose record"
                    )));
                };
                let bone = map_bone(target, driven.int(BONE_ID))?;
                let base = map_bone(target, pose.int(BASE_BONE_ID))?;
                let pose_index = require(target, "pose_driven_list")?.objects.len();
                let driven_count = require(target, "driven_bone_list")?.objects.len();
                let mut cloned_driven = driven.clone_as(&format!("driven_bone_{driven_count}"));
                cloned_driven.set_int(BONE_ID, bone);
                for name in ["translate_driven_id", "rotate_driven_id", "aim_driven_id"] {
                    remap_pose_driven_id(&mut cloned_driven, crc32(name), pose_index as i32);
                }
                let mut cloned_pose = pose.clone_as(&format!("pose_driven_{pose_index}"));
                cloned_pose.set_int(BASE_BONE_ID, base);
                for hash in POSE_OUTPUT_HASHES {
                    remap_optional(&mut cloned_pose, hash, &output_map);
                }
                require_mut(target, "driven_bone_list")?
                    .objects
                    .push(cloned_driven);
                require_mut(target, "pose_driven_list")?
                    .objects
                    .push(cloned_pose);
            }
            Ok(())
        })
    }

    /// Reflects every input and affected pose in one connected driver group.
    /// Curves and outputs remain attached because both endpoint frames are
    /// reflected together.
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
            let bone_list = require_mut(data, "bone_list")?;
            for bone in &bones {
                if let Some(record) = bone_list.objects.get_mut(*bone) {
                    swap_name(record);
                }
            }
            let drivers = require_mut(data, "driver_bone_list")?;
            for index in &group.driver_indices {
                if let Some(record) = drivers.objects.get_mut(*index) {
                    mirror_transform_record(record);
                }
            }
            let poses = require_mut(data, "pose_driven_list")?;
            for index in &group.driven_indices {
                if let Some(record) = poses.objects.get_mut(*index) {
                    mirror_transform_record(record);
                }
            }
            Ok(())
        })
    }

    /// Reflects the whole helper graph across X and swaps L/R name tokens.
    pub fn mirror_bones_across_x(&self) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            for bone in &mut require_mut(data, "bone_list")?.objects {
                swap_name(bone);
            }
            if let Some(drivers) = data.find_child_mut(DRIVER_LIST) {
                drivers.objects.iter_mut().for_each(mirror_transform_record);
            }
            if let Some(poses) = data.find_child_mut(POSE_DRIVEN_LIST) {
                poses.objects.iter_mut().for_each(mirror_transform_record);
            }
            Ok(())
        })
    }

    /// Reflects one helper bone's base poses across X.
    pub fn mirror_bone_across_x(&self, bone_index: usize) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = require_mut(data, "bone_list")?;
            let bone = bone_list
                .objects
                .get_mut(bone_index)
                .ok_or_else(|| invalid("bone index is out of range"))?;
            swap_name(bone);
            if let Some(drivers) = data.find_child_mut(DRIVER_LIST) {
                for record in &mut drivers.objects {
                    if record.int(BONE_ID) == bone_index as i32 {
                        mirror_transform_record(record);
                    }
                }
            }
            let driven_bones: Vec<i32> = data
                .find_child(DRIVEN_LIST)
                .map(|driven| {
                    driven
                        .objects
                        .iter()
                        .map(|record| record.int(BONE_ID))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(poses) = data.find_child_mut(POSE_DRIVEN_LIST) {
                for (index, record) in poses.objects.iter_mut().enumerate() {
                    if driven_bones.get(index) == Some(&(bone_index as i32)) {
                        mirror_transform_record(record);
                    }
                }
            }
            Ok(())
        })
    }

    /// Updates the named helper-bone record and its matching base pose records.
    pub fn update_bone(
        &self,
        bone_index: usize,
        name: &str,
        base_bone_index: i32,
        translation: [f32; 3],
        rotation: [f32; 4],
    ) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = require_mut(data, "bone_list")?;
            let bone = bone_list
                .objects
                .get_mut(bone_index)
                .ok_or_else(|| invalid("bone index is out of range"))?;
            bone.set_string(NAME, name);
            if let Some(drivers) = data.find_child_mut(DRIVER_LIST) {
                for record in &mut drivers.objects {
                    if record.int(BONE_ID) == bone_index as i32 {
                        record.set_int(BASE_BONE_ID, base_bone_index);
                        record.set_vec3(BASE_TRANSLATION, translation);
                        record.set_vec4(BASE_ROTATION, rotation);
                    }
                }
            }
            let driven_bones: Vec<i32> = data
                .find_child(DRIVEN_LIST)
                .map(|driven| {
                    driven
                        .objects
                        .iter()
                        .map(|record| record.int(BONE_ID))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(poses) = data.find_child_mut(POSE_DRIVEN_LIST) {
                for (index, record) in poses.objects.iter_mut().enumerate() {
                    if driven_bones.get(index) == Some(&(bone_index as i32)) {
                        record.set_int(BASE_BONE_ID, base_bone_index);
                        record.set_vec3(BASE_TRANSLATION, translation);
                        record.set_vec4(BASE_ROTATION, rotation);
                    }
                }
            }
            Ok(())
        })
    }

    /// Adds a helper bone by cloning an existing graph: its driver record,
    /// driven records, poses, and the outputs and curves those poses use.
    pub fn duplicate_bone(
        &self,
        source_bone_index: usize,
        requested_name: Option<&str>,
    ) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = require_mut(data, "bone_list")?;
            let source = bone_list
                .objects
                .get(source_bone_index)
                .ok_or_else(|| invalid("bone index is out of range"))?;
            let new_bone_index = bone_list.objects.len();
            let source_name = source
                .string(NAME)
                .unwrap_or_else(|| format!("HelperBone_{source_bone_index}"));
            let used: Vec<String> = bone_list
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
            bone_list.objects.push(new_bone);

            if let Some(drivers) = data.find_child_mut(DRIVER_LIST) {
                let matching: Vec<AampObject> = drivers
                    .objects
                    .iter()
                    .filter(|record| record.int(BONE_ID) == source_bone_index as i32)
                    .cloned()
                    .collect();
                for record in matching {
                    let mut cloned =
                        record.clone_as(&format!("driver_bone_{}", drivers.objects.len()));
                    cloned.set_int(BONE_ID, new_bone_index as i32);
                    drivers.objects.push(cloned);
                }
            }
            clone_driven_graph(
                data,
                source_bone_index as i32,
                new_bone_index,
                Some(source_bone_index as i32),
            )
        })
    }

    pub fn add_bone(&self, preferred_source_index: usize) -> io::Result<Self> {
        let count = self.graph.as_ref().map_or(0, |graph| graph.bones.len());
        if count == 0 {
            return Err(invalid("this helper-bone file has no source bone to clone"));
        }
        self.duplicate_bone(
            preferred_source_index.min(count - 1),
            Some(&format!("HelperBone_{count}")),
        )
    }

    /// Creates a new affected bone from a known-good driven graph. Existing
    /// driver records are retained; pose outputs and curves are cloned so the
    /// new support can be edited independently later.
    pub fn create_support_from_bone(
        &self,
        source_bone_index: usize,
        target_bone_name: &str,
    ) -> io::Result<(Self, usize)> {
        let mut target_index = 0usize;
        let rebuilt = self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bones = require_mut(data, "bone_list")?;
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
            let driven = require(data, "driven_bone_list")?;
            if !driven
                .objects
                .iter()
                .any(|record| record.int(BONE_ID) == source_bone_index as i32)
            {
                return Err(invalid(
                    "the selected bone has no driven support graph to clone",
                ));
            }
            if driven
                .objects
                .iter()
                .any(|record| record.int(BONE_ID) == target_index as i32)
            {
                return Err(invalid(&format!(
                    "{target_bone_name} already has support data in this file"
                )));
            }
            clone_driven_graph(data, source_bone_index as i32, target_index, None)
        })?;
        Ok((rebuilt, target_index))
    }

    /// Moves a bone-list entry and rewrites every bone-index reference in the
    /// helper graph. Output and curve IDs stay stable because they address
    /// their own lists.
    pub fn move_bone(
        &self,
        source_bone_index: usize,
        destination_bone_index: usize,
    ) -> io::Result<Self> {
        self.rebuild(|root| {
            let data = require_data_root_mut(root)?;
            let bone_list = require_mut(data, "bone_list")?;
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
                data.find_child_mut(DRIVER_LIST),
                &remap,
                &[BONE_ID, BASE_BONE_ID],
            );
            remap_bone_ids(data.find_child_mut(DRIVEN_LIST), &remap, &[BONE_ID]);
            remap_bone_ids(
                data.find_child_mut(POSE_DRIVEN_LIST),
                &remap,
                &[BASE_BONE_ID],
            );
            remap_bone_ids(
                data.find_child_mut(crc32("connection_curve_list")),
                &remap,
                &[crc32("driver_bone_id")],
            );
            Ok(())
        })
    }

    /// Rebuilds a TotK helper-bone graph from BotW's support-bone graph.
    /// BPHYSSB does not retain every BPHHB field, so an invalid or missing
    /// BotW space link falls back to an inferred parent instead of inventing a
    /// reference unrelated to the exported helper graph.
    pub fn from_support_bones(source: &SupportBoneDocument) -> io::Result<Self> {
        let bytes = build_from_support_bones(source)?;
        let document = Self::parse(&bytes)?;
        if document.graph.is_none() {
            return Err(invalid(
                "the converted BPHHB could not be decoded after it was written",
            ));
        }
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

/// Clones every driven record of `source_bone` (and the pose, output and
/// curve records they use) onto `new_bone`. `rename_driver` rewrites cloned
/// curves that were driven by the source bone so they follow the copy.
fn clone_driven_graph(
    data: &mut AampList,
    source_bone: i32,
    new_bone: usize,
    rename_driver: Option<i32>,
) -> io::Result<()> {
    let driven_records: Vec<(usize, AampObject)> = require(data, "driven_bone_list")?
        .objects
        .iter()
        .enumerate()
        .filter(|(_, record)| record.int(BONE_ID) == source_bone)
        .map(|(index, record)| (index, record.clone()))
        .collect();
    let mut output_map = HashMap::<i32, i32>::new();
    let mut curve_map = HashMap::<i32, i32>::new();
    for (index, record) in driven_records {
        let pose = require(data, "pose_driven_list")?
            .objects
            .get(index)
            .cloned()
            .ok_or_else(|| {
                invalid(&format!(
                    "driven record #{index} has no matching pose record"
                ))
            })?;
        let pose_index = require(data, "pose_driven_list")?.objects.len();
        let driven_count = require(data, "driven_bone_list")?.objects.len();
        let mut cloned_driven = record.clone_as(&format!("driven_bone_{driven_count}"));
        cloned_driven.set_int(BONE_ID, new_bone as i32);
        for name in ["translate_driven_id", "rotate_driven_id", "aim_driven_id"] {
            remap_pose_driven_id(&mut cloned_driven, crc32(name), pose_index as i32);
        }
        let mut cloned_pose = pose.clone_as(&format!("pose_driven_{pose_index}"));
        for hash in POSE_OUTPUT_HASHES {
            clone_driven_output(
                &mut cloned_pose,
                hash,
                data,
                &mut output_map,
                &mut curve_map,
                rename_driver,
                new_bone as i32,
            )?;
        }
        require_mut(data, "driven_bone_list")?
            .objects
            .push(cloned_driven);
        require_mut(data, "pose_driven_list")?
            .objects
            .push(cloned_pose);
    }
    Ok(())
}

fn clone_driven_output(
    pose: &mut AampObject,
    output_hash: u32,
    data: &mut AampList,
    output_map: &mut HashMap<i32, i32>,
    curve_map: &mut HashMap<i32, i32>,
    rename_driver: Option<i32>,
    new_bone: i32,
) -> io::Result<()> {
    let source_output = pose.int(output_hash);
    if source_output < 0 {
        return Ok(());
    }
    let Some(outputs) = data.find_child(OUTPUT_LIST) else {
        return Ok(());
    };
    let Some(output) = outputs.objects.get(source_output as usize).cloned() else {
        return Ok(());
    };
    let new_output = match output_map.get(&source_output) {
        Some(existing) => *existing,
        None => {
            let index = outputs.objects.len() as i32;
            let mut cloned = output.clone_as(&format!("output_{index}"));
            for name in ["connection_0_id", "connection_1_id"] {
                clone_output_curve(
                    &mut cloned,
                    crc32(name),
                    data,
                    curve_map,
                    rename_driver,
                    new_bone,
                );
            }
            require_mut(data, "output_list")?.objects.push(cloned);
            output_map.insert(source_output, index);
            index
        }
    };
    pose.set_int(output_hash, new_output);
    Ok(())
}

fn clone_output_curve(
    output: &mut AampObject,
    connection_hash: u32,
    data: &mut AampList,
    curve_map: &mut HashMap<i32, i32>,
    rename_driver: Option<i32>,
    new_bone: i32,
) {
    let source_curve = output.int(connection_hash);
    if source_curve < 0 {
        return;
    }
    let Some(curves) = data.find_child_mut(crc32("connection_curve_list")) else {
        return;
    };
    let Some(curve) = curves.objects.get(source_curve as usize).cloned() else {
        return;
    };
    let new_curve = match curve_map.get(&source_curve) {
        Some(existing) => *existing,
        None => {
            let index = curves.objects.len() as i32;
            let mut cloned = curve.clone_as(&format!("connection_curve_{index}"));
            let driver_bone = crc32("driver_bone_id");
            if rename_driver.is_some_and(|source| cloned.int(driver_bone) == source) {
                cloned.set_int(driver_bone, new_bone);
            }
            curves.objects.push(cloned);
            curve_map.insert(source_curve, index);
            index
        }
    };
    output.set_int(connection_hash, new_curve);
}

pub(crate) fn build_driver_groups(tree: &AampTree) -> io::Result<Vec<DriverGroupData>> {
    let data =
        data_root(&tree.root).ok_or_else(|| invalid("BPHHB has no helper-bone data root"))?;
    let bones = require(data, "bone_list")?;
    let curves = require(data, "connection_curve_list")?;
    let outputs = require(data, "output_list")?;
    let drivers = require(data, "driver_bone_list")?;
    let driven = require(data, "driven_bone_list")?;
    let poses = require(data, "pose_driven_list")?;
    if drivers.objects.is_empty() {
        return Ok(Vec::new());
    }
    let channels = [
        (crc32("translate_driven_type"), crc32("translate_driven_id")),
        (crc32("rotate_driven_type"), crc32("rotate_driven_id")),
        (crc32("aim_driven_type"), crc32("aim_driven_id")),
    ];
    let pose_index_for_driven = |driven_index: usize| -> Option<usize> {
        let record = driven.objects.get(driven_index)?;
        for (type_hash, id_hash) in channels {
            let id = record.int(id_hash);
            let kind = record.int(type_hash);
            if id >= 0 && (id as usize) < poses.objects.len() && matches!(kind, -1 | 0) {
                return Some(id as usize);
            }
        }
        (driven_index < poses.objects.len()).then_some(driven_index)
    };

    let curve_drivers: Vec<i32> = curves
        .objects
        .iter()
        .map(|curve| curve.int(CURVE_DRIVER))
        .collect();
    let driver_count = drivers.objects.len();
    let linked_drivers = |output: &AampObject| -> Vec<usize> {
        distinct_non_negative(
            output_connections(output)
                .into_iter()
                .filter_map(|curve| usize::try_from(curve).ok())
                .filter_map(|curve| curve_drivers.get(curve).copied())
                .filter(|driver| *driver >= 0 && (*driver as usize) < driver_count),
        )
    };
    let mut sets = UnionFind::new(driver_count);
    for output in &outputs.objects {
        let linked = linked_drivers(output);
        for driver in linked.iter().skip(1) {
            sets.union(linked[0], *driver);
        }
    }
    // Different channels on one driven pose are still one motion graph, even
    // when each channel owns a separate output record.
    for pose in &poses.objects {
        let mut linked = Vec::new();
        for hash in POSE_OUTPUT_HASHES {
            if let Some(output) = usize::try_from(pose.int(hash))
                .ok()
                .and_then(|index| outputs.objects.get(index))
            {
                for driver in linked_drivers(output) {
                    if !linked.contains(&driver) {
                        linked.push(driver);
                    }
                }
            }
        }
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
    let groups = sets.groups();
    Ok(groups
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
            let output_indices: Vec<usize> = outputs
                .objects
                .iter()
                .enumerate()
                .filter(|(_, output)| {
                    output_connections(output).iter().any(|curve| {
                        usize::try_from(*curve).is_ok_and(|curve| curve_indices.contains(&curve))
                    })
                })
                .map(|(index, _)| index)
                .collect();
            let driven_indices: Vec<usize> = (0..driven.objects.len())
                .filter(|index| {
                    pose_index_for_driven(*index).is_some_and(|pose_index| {
                        POSE_OUTPUT_HASHES.iter().any(|hash| {
                            usize::try_from(poses.objects[pose_index].int(*hash))
                                .is_ok_and(|output| output_indices.contains(&output))
                        })
                    })
                })
                .collect();
            let driver_bones = distinct_non_negative(
                driver_indices
                    .iter()
                    .map(|index| drivers.objects[*index].int(BONE_ID)),
            );
            let driven_bones = distinct_non_negative(
                driven_indices
                    .iter()
                    .map(|index| driven.objects[*index].int(BONE_ID)),
            );
            let required = distinct_non_negative(
                driver_indices
                    .iter()
                    .flat_map(|index| {
                        [
                            drivers.objects[*index].int(BONE_ID),
                            drivers.objects[*index].int(BASE_BONE_ID),
                        ]
                    })
                    .chain(driven_indices.iter().flat_map(|index| {
                        let base = pose_index_for_driven(*index)
                            .map_or(-1, |pose| poses.objects[pose].int(BASE_BONE_ID));
                        [driven.objects[*index].int(BONE_ID), base]
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
                    output_count: output_indices.len(),
                },
                driver_indices,
                curve_indices,
                output_indices,
                driven_indices,
                required_bone_indices: required,
            }
        })
        .collect())
}

fn output_connections(output: &AampObject) -> [i32; 2] {
    [
        output.int(OUTPUT_CONNECTION0),
        output.int(OUTPUT_CONNECTION1),
    ]
}

fn read_graph(tree: &AampTree) -> Option<HelperBoneGraph> {
    if !tree.archive_type.eq_ignore_ascii_case("phhb") {
        return None;
    }
    let data = data_root(&tree.root)?;
    let bone_list = data.find_child(crc32("bone_list"))?;
    let curve_list = data.find_child(crc32("connection_curve_list"))?;
    let output_list = data.find_child(OUTPUT_LIST)?;
    let driver_list = data.find_child(DRIVER_LIST)?;
    let driven_list = data.find_child(DRIVEN_LIST)?;
    let pose_list = data.find_child(POSE_DRIVEN_LIST);
    Some(HelperBoneGraph {
        bones: bone_list
            .objects
            .iter()
            .map(|bone| bone.string(NAME).unwrap_or_default())
            .collect(),
        drivers: driver_list
            .objects
            .iter()
            .map(|driver| DriverBone {
                bone_id: driver.int(BONE_ID),
                base_bone_id: driver.int(BASE_BONE_ID),
                translation: driver.vec3(BASE_TRANSLATION).unwrap_or([0.0; 3]),
                rotation: driver.vec4(BASE_ROTATION).unwrap_or([0.0, 0.0, 0.0, 1.0]),
            })
            .collect(),
        driven: driven_list
            .objects
            .iter()
            .map(|driven| DrivenBone {
                bone_id: driven.int(BONE_ID),
                translate_driven_id: driven.int(crc32("translate_driven_id")),
                rotate_driven_id: driven.int(crc32("rotate_driven_id")),
            })
            .collect(),
        pose_driven: pose_list
            .map(|poses| {
                poses
                    .objects
                    .iter()
                    .map(|pose| PoseDrivenBone {
                        translation: pose.vec3(BASE_TRANSLATION).unwrap_or([0.0; 3]),
                        rotation: pose.vec4(BASE_ROTATION).unwrap_or([0.0, 0.0, 0.0, 1.0]),
                        base_bone_id: pose.int(BASE_BONE_ID),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        outputs: output_list
            .objects
            .iter()
            .map(|output| OutputLink {
                connection0: output.int(OUTPUT_CONNECTION0),
                connection1: output.typed_int(OUTPUT_CONNECTION1),
            })
            .collect(),
        curve_count: curve_list.objects.len(),
    })
}

/// A phhb archive keeps one data list under the root.
fn data_root(root: &AampList) -> Option<&AampList> {
    (root.children.len() == 1).then(|| &root.children[0])
}

fn require_data_root_mut(root: &mut AampList) -> io::Result<&mut AampList> {
    if root.children.len() != 1 {
        return Err(invalid(
            "the BPHHB archive does not contain one editable data root",
        ));
    }
    Ok(&mut root.children[0])
}

fn require<'a>(parent: &'a AampList, name: &str) -> io::Result<&'a AampList> {
    parent.require_child(crc32(name), name)
}

fn require_mut<'a>(parent: &'a mut AampList, name: &str) -> io::Result<&'a mut AampList> {
    parent.require_child_mut(crc32(name), name)
}

/// The top-level phhb object carries redundant list totals. They must agree
/// with the actual lists after an edit, otherwise game-side readers can walk
/// past the intended graph.
fn synchronize_header_counts(root: &mut AampList) {
    if root.objects.is_empty() || root.children.len() != 1 {
        return;
    }
    let counts: Vec<(u32, i32)> = [
        ("bone_num", "bone_list"),
        ("connection_curve_num", "connection_curve_list"),
        ("output_num", "output_list"),
        ("driver_bone_num", "driver_bone_list"),
        ("driven_bone_num", "driven_bone_list"),
        ("pose_driven_num", "pose_driven_list"),
        ("position_driven_num", "position_driven_list"),
        ("rotate_driven_num", "rotate_driven_list"),
        ("aim_driven_num", "aim_driven_list"),
    ]
    .iter()
    .map(|(count_name, list_name)| {
        (
            crc32(count_name),
            root.children[0]
                .find_child(crc32(list_name))
                .map_or(0, |list| list.objects.len() as i32),
        )
    })
    .collect();
    let header = &mut root.objects[0];
    for (hash, value) in counts {
        header.set_existing_int(hash, value);
    }
}

fn swap_name(record: &mut AampObject) {
    if let Some(name) = record.string(NAME).filter(|name| !name.trim().is_empty()) {
        record.set_string(NAME, &swap_side_tokens(&name));
    }
}

pub(crate) fn mirror_transform_record(record: &mut AampObject) {
    if let Some(mut translation) = record.vec3(BASE_TRANSLATION) {
        translation[0] = -translation[0];
        record.set_vec3(BASE_TRANSLATION, translation);
    }
    if let Some(rotation) = record.vec4(BASE_ROTATION) {
        record.set_vec4(BASE_ROTATION, mirror_quaternion_across_x(rotation));
    }
    for hash in [AIM_AXIS, UP_AXIS] {
        if let Some(mut direction) = record.vec3(hash) {
            direction[0] = -direction[0];
            record.set_vec3(hash, direction);
        }
    }
}

fn remap_pose_driven_id(record: &mut AampObject, hash: u32, pose_index: i32) {
    if record.int(hash) >= 0 {
        record.set_int(hash, pose_index);
    }
}

pub(crate) fn remap_required(
    record: &mut AampObject,
    hash: u32,
    remap: &HashMap<i32, i32>,
    label: &str,
) -> io::Result<()> {
    let source = record.int(hash);
    let target = (source >= 0)
        .then(|| remap.get(&source))
        .flatten()
        .ok_or_else(|| {
            invalid(&format!(
                "the selected driver group references a {label} outside its dependency closure"
            ))
        })?;
    record.set_int(hash, *target);
    Ok(())
}

pub(crate) fn remap_optional(record: &mut AampObject, hash: u32, remap: &HashMap<i32, i32>) {
    let source = record.int(hash);
    if source >= 0 {
        if let Some(target) = remap.get(&source) {
            record.set_int(hash, *target);
        }
    }
}

pub(crate) fn remap_bone_ids(list: Option<&mut AampList>, remap: &[i32], hashes: &[u32]) {
    let Some(list) = list else {
        return;
    };
    for record in &mut list.objects {
        for hash in hashes {
            let old = record.int(*hash);
            if let Some(new) = usize::try_from(old).ok().and_then(|old| remap.get(old)) {
                record.set_int(*hash, *new);
            }
        }
    }
}

pub(crate) fn read_string_pool(bytes: &[u8], string_pool_size: u32) -> Vec<String> {
    let Some(start) = bytes.len().checked_sub(string_pool_size as usize) else {
        return Vec::new();
    };
    let mut values: Vec<String> = Vec::new();
    for chunk in bytes[start..].split(|byte| *byte == 0) {
        if chunk.is_empty() {
            continue;
        }
        let value = String::from_utf8_lossy(chunk).into_owned();
        if value.chars().all(|character| !character.is_control()) && !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

// ---- BPHYSSB -> BPHHB conversion ------------------------------------------

fn build_from_support_bones(source: &SupportBoneDocument) -> io::Result<Vec<u8>> {
    use bphyssb::{
        BONE_LIST as SB_BONE_LIST, CONNECTION_CURVE_LIST as SB_CURVE_LIST,
        MAIN_BONE_LIST as SB_MAIN_LIST, OUTPUT_DOUBLE_LIST as SB_DOUBLE_LIST,
        OUTPUT_SINGLE_LIST as SB_SINGLE_LIST, SUPPORT_BONE_LIST as SB_SUPPORT_LIST, TARGET_AIM,
        TARGET_BEND_H, TARGET_BEND_V, TARGET_BONE, TARGET_BONE_ATTRIBUTE, TARGET_CONNECTION,
        TARGET_CONNECTION0, TARGET_CONNECTION1, TARGET_ROLL, TARGET_SPACE, TARGET_TRANSLATE_X,
        TARGET_TRANSLATE_Y, TARGET_TRANSLATE_Z, TARGET_UP,
    };
    if !source.tree.archive_type.eq_ignore_ascii_case("physsb") {
        return Err(invalid(&format!(
            "expected a physsb AAMP archive, found '{}'",
            source.tree.archive_type
        )));
    }
    let source_data = source
        .tree
        .root
        .children
        .iter()
        .find(|child| {
            [SB_BONE_LIST, SB_CURVE_LIST, SB_MAIN_LIST, SB_SUPPORT_LIST]
                .iter()
                .all(|hash| child.find_child(*hash).is_some())
        })
        .ok_or_else(|| invalid("BPHYSSB does not contain a complete support-bone graph"))?;
    let source_bones = source_data.require_child(SB_BONE_LIST, "bone_list")?;
    let source_curves = source_data.require_child(SB_CURVE_LIST, "connection_curve_list")?;
    let source_singles = source_data.find_child(SB_SINGLE_LIST);
    let source_doubles = source_data.find_child(SB_DOUBLE_LIST);
    let source_main = source_data.require_child(SB_MAIN_LIST, "main_bone_list")?;
    let source_support = source_data.require_child(SB_SUPPORT_LIST, "support_bone_list")?;
    let mut bone_names: Vec<String> = source_bones
        .objects
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            bone.string(NAME)
                .ok_or_else(|| invalid(&format!("BPHYSSB bone #{index} is missing its name")))
        })
        .collect::<io::Result<_>>()?;
    let main_bone_ids: Vec<i32> = source_main
        .objects
        .iter()
        .map(|record| required_int(record, TARGET_BONE, "main_bone_list"))
        .collect::<io::Result<_>>()?;

    let mut root = AampList::named("param_root");
    let mut header = AampObject::named("helper_bone_header");
    match source
        .tree
        .root
        .objects
        .first()
        .and_then(|object| object.find(HEADER_STEP))
    {
        Some(step) => header.push(step.clone()),
        None => header.push(AampParameter::float(HEADER_STEP, 0.1)),
    }
    root.objects.push(header);

    // A native BPHHB always carries these nine child tables in this order.
    let mut bones = AampList::named("bone_list");
    let mut curves = AampList::named("connection_curve_list");
    let mut outputs = AampList::named("output_list");
    let mut drivers = AampList::named("driver_bone_list");
    let mut driven = AampList::named("driven_bone_list");
    let mut poses = AampList::named("pose_driven_list");

    for source_bone in &source_bones.objects {
        let mut bone = AampObject::named(&format!("bone_{}", bones.objects.len()));
        bone.copy_from(source_bone, NAME, None)?;
        bones.objects.push(bone);
    }

    let local_name = |value: &str| -> String {
        value
            .rsplit_once(':')
            .map_or(value, |(_, local)| local)
            .to_owned()
    };
    let mut get_or_add_bone =
        |bones: &mut AampList, bone_names: &mut Vec<String>, name: &str| -> usize {
            if let Some(index) = bone_names.iter().position(|value| value == name) {
                return index;
            }
            let local = local_name(name);
            let local_matches: Vec<usize> = bone_names
                .iter()
                .enumerate()
                .filter(|(_, value)| local_name(value) == local)
                .map(|(index, _)| index)
                .take(2)
                .collect();
            if local_matches.len() == 1 {
                return local_matches[0];
            }
            let mut name = name.to_owned();
            if !name.contains(':') {
                if let Some(prefix) = bone_names.iter().find_map(|value| {
                    value
                        .rsplit_once(':')
                        .map(|(prefix, _)| format!("{prefix}:"))
                }) {
                    name = format!("{prefix}{name}");
                }
            }
            let mut bone = AampObject::named(&format!("bone_{}", bones.objects.len()));
            bone.push(AampParameter::string(NAME, KIND_STRING256, &name));
            bone_names.push(name);
            bones.objects.push(bone);
            bones.objects.len() - 1
        };

    let mut resolve_base = |bones: &mut AampList,
                            bone_names: &mut Vec<String>,
                            record: &AampObject,
                            bone_id: i32|
     -> io::Result<i32> {
        let space = record.typed_int(TARGET_SPACE).unwrap_or(-1);
        if space >= 0 && (space as usize) < bone_names.len() && space != bone_id {
            return Ok(space);
        }
        let bone_name = usize::try_from(bone_id)
            .ok()
            .and_then(|index| bone_names.get(index).cloned())
            .unwrap_or_default();
        if let Some(inferred) = infer_parent_bone_name(&bone_name) {
            if inferred != bone_name {
                return Ok(get_or_add_bone(bones, bone_names, &inferred) as i32);
            }
        }
        if let Some(fallback) = main_bone_ids.iter().copied().find(|candidate| {
            *candidate >= 0 && (*candidate as usize) < bone_names.len() && *candidate != bone_id
        }) {
            return Ok(fallback);
        }
        if let Some(fallback) = (0..bone_names.len() as i32).find(|candidate| *candidate != bone_id)
        {
            return Ok(fallback);
        }
        Err(invalid(&format!(
            "BPHYSSB bone '{bone_name}' has no usable base bone; a helper graph cannot use itself as its base"
        )))
    };

    for (index, source_driver) in source_main.objects.iter().enumerate() {
        let bone_id = required_int(source_driver, TARGET_BONE, "main_bone_list")?;
        let mut driver = AampObject::named(&format!("driver_bone_{index}"));
        driver.push(AampParameter::int(BONE_ID, bone_id));
        driver.copy_from(source_driver, BASE_TRANSLATION, None)?;
        driver.copy_from(source_driver, BASE_ROTATION, None)?;
        driver.copy_from(source_driver, TARGET_AIM, Some(AIM_AXIS))?;
        driver.copy_from(source_driver, TARGET_UP, Some(UP_AXIS))?;
        let base = resolve_base(&mut bones, &mut bone_names, source_driver, bone_id)?;
        driver.push(AampParameter::int(BASE_BONE_ID, base));
        drivers.objects.push(driver);
    }

    for (index, source_curve) in source_curves.objects.iter().enumerate() {
        let packed = required_int(source_curve, TARGET_BONE_ATTRIBUTE, "connection_curve_list")?;
        let driver_index = (packed as u32 >> 16) as usize;
        if driver_index >= source_main.objects.len() {
            return Err(invalid(&format!(
                "BPHYSSB curve #{index} references driver #{driver_index}, but only {} driver records exist",
                source_main.objects.len()
            )));
        }
        let mut curve = AampObject::named(&format!("connection_curve_{index}"));
        curve.push(AampParameter::int(CURVE_DRIVER, driver_index as i32));
        curve.push(AampParameter::int(
            CURVE_ATTRIBUTE,
            unpack_bone_attribute(packed & 0xffff, index)?,
        ));
        curve.push(AampParameter::int(CURVE_KEY_COUNT, 3));
        curve.copy_from(source_curve, CURVE_KEY0, None)?;
        curve.copy_from(source_curve, CURVE_KEY1, None)?;
        curve.copy_from(source_curve, CURVE_KEY2, None)?;
        curves.objects.push(curve);
    }

    let mut output_map = HashMap::<i32, i32>::new();
    let mut map_active_output = |outputs: &mut AampList, handle: i32| -> io::Result<i32> {
        if handle < 0 {
            return Ok(-1);
        }
        if let Some(existing) = output_map.get(&handle) {
            return Ok(*existing);
        }
        let (connection0, connection1) = if bphyssb::is_double_output_handle(handle) {
            let index = (handle & 0xffff) as usize;
            let output = source_doubles
                .and_then(|list| list.objects.get(index))
                .ok_or_else(|| {
                    invalid(&format!(
                        "BPHYSSB references missing double output #{index}"
                    ))
                })?;
            (
                bphyssb::decode_curve_reference(required_int(
                    output,
                    TARGET_CONNECTION0,
                    "output_double_list",
                )?),
                bphyssb::decode_curve_reference(required_int(
                    output,
                    TARGET_CONNECTION1,
                    "output_double_list",
                )?),
            )
        } else {
            let output = source_singles
                .and_then(|list| list.objects.get(handle as usize))
                .ok_or_else(|| {
                    invalid(&format!(
                        "BPHYSSB references missing single output #{handle}"
                    ))
                })?;
            (
                bphyssb::decode_curve_reference(required_int(
                    output,
                    TARGET_CONNECTION,
                    "output_single_list",
                )?),
                -1,
            )
        };
        if connection0 < 0 && connection1 < 0 {
            return Ok(-1);
        }
        let curve_count = curves.objects.len() as i32;
        if connection0 >= curve_count || connection1 >= curve_count {
            return Err(invalid(&format!(
                "BPHYSSB output 0x{handle:08X} references a curve outside the converted curve list"
            )));
        }
        let mut output = AampObject::named(&format!("output_{}", outputs.objects.len()));
        output.push(AampParameter::int(
            OUTPUT_CONNECTION0,
            if connection0 >= 0 {
                connection0
            } else {
                connection1
            },
        ));
        if connection0 >= 0 && connection1 >= 0 {
            output.push(AampParameter::int(OUTPUT_CONNECTION1, connection1));
        }
        let index = outputs.objects.len() as i32;
        outputs.objects.push(output);
        output_map.insert(handle, index);
        Ok(index)
    };

    for (index, source_bone) in source_support.objects.iter().enumerate() {
        let bone_id = required_int(source_bone, TARGET_BONE, "support_bone_list")?;
        let roll = map_active_output(
            &mut outputs,
            source_bone.typed_int(TARGET_ROLL).unwrap_or(-1),
        )?;
        let bend_h = map_active_output(
            &mut outputs,
            source_bone.typed_int(TARGET_BEND_H).unwrap_or(-1),
        )?;
        let bend_v = map_active_output(
            &mut outputs,
            source_bone.typed_int(TARGET_BEND_V).unwrap_or(-1),
        )?;
        let translate_x = map_active_output(
            &mut outputs,
            source_bone.typed_int(TARGET_TRANSLATE_X).unwrap_or(-1),
        )?;
        let translate_y = map_active_output(
            &mut outputs,
            source_bone.typed_int(TARGET_TRANSLATE_Y).unwrap_or(-1),
        )?;
        let translate_z = map_active_output(
            &mut outputs,
            source_bone.typed_int(TARGET_TRANSLATE_Z).unwrap_or(-1),
        )?;
        let has_rotation = roll >= 0 || bend_h >= 0 || bend_v >= 0;
        let has_translation = translate_x >= 0 || translate_y >= 0 || translate_z >= 0;

        let mut record = AampObject::named(&format!("driven_bone_{index}"));
        record.push(AampParameter::int(BONE_ID, bone_id));
        if has_translation {
            record.push(AampParameter::int(crc32("translate_driven_type"), 0));
            record.push(AampParameter::int(
                crc32("translate_driven_id"),
                index as i32,
            ));
        }
        if has_rotation {
            record.push(AampParameter::int(crc32("rotate_driven_type"), 0));
            record.push(AampParameter::int(crc32("rotate_driven_id"), index as i32));
        }
        driven.objects.push(record);

        let mut pose = AampObject::named(&format!("pose_driven_{index}"));
        pose.copy_from(source_bone, BASE_TRANSLATION, None)?;
        pose.copy_from(source_bone, BASE_ROTATION, None)?;
        pose.copy_from(source_bone, TARGET_AIM, Some(AIM_AXIS))?;
        pose.copy_from(source_bone, TARGET_UP, Some(UP_AXIS))?;
        let base = resolve_base(&mut bones, &mut bone_names, source_bone, bone_id)?;
        pose.push(AampParameter::int(BASE_BONE_ID, base));
        for (output, output_hash, default_hash) in [
            (roll, ROLL_OUTPUT, TARGET_ROLL),
            (bend_h, BEND_H_OUTPUT, TARGET_BEND_H),
            (bend_v, BEND_V_OUTPUT, TARGET_BEND_V),
            (translate_x, TRANSLATE_X_OUTPUT, TARGET_TRANSLATE_X),
            (translate_y, TRANSLATE_Y_OUTPUT, TARGET_TRANSLATE_Y),
            (translate_z, TRANSLATE_Z_OUTPUT, TARGET_TRANSLATE_Z),
        ] {
            pose.push(if output >= 0 {
                AampParameter::int(output_hash, output)
            } else {
                AampParameter::int(default_hash, 0)
            });
        }
        poses.objects.push(pose);
    }

    let counts = [
        ("bone_num", bones.objects.len()),
        ("connection_curve_num", curves.objects.len()),
        ("output_num", outputs.objects.len()),
        ("driver_bone_num", drivers.objects.len()),
        ("driven_bone_num", driven.objects.len()),
        ("pose_driven_num", poses.objects.len()),
        ("position_driven_num", 0),
        ("rotate_driven_num", 0),
        ("aim_driven_num", 0),
    ];
    for (name, value) in counts {
        root.objects[0].push(AampParameter::int(crc32(name), value as i32));
    }

    let mut data = AampList::named("helper_bone_data");
    data.children.push(bones);
    data.children.push(curves);
    data.children.push(outputs);
    data.children.push(drivers);
    data.children.push(driven);
    data.children.push(poses);
    data.children.push(AampList::named("position_driven_list"));
    data.children.push(AampList::named("rotate_driven_list"));
    data.children.push(AampList::named("aim_driven_list"));
    root.children.push(data);

    AampTree {
        archive_type: "phhb".into(),
        archive_version: source.header.archive_version,
        format_version: source.header.format_version,
        parameter_io_version: source.header.parameter_io_version,
        root,
    }
    .to_bytes()
}

fn unpack_bone_attribute(channel: i32, curve_index: usize) -> io::Result<i32> {
    match channel {
        2 => Ok(0),
        0 => Ok(1),
        1 => Ok(2),
        other => Err(invalid(&format!(
            "BPHYSSB curve #{curve_index} uses unsupported channel {other}"
        ))),
    }
}

pub(crate) fn required_int(record: &AampObject, hash: u32, owner: &str) -> io::Result<i32> {
    record.typed_int(hash).ok_or_else(|| {
        invalid(&format!(
            "BPHYSSB '{owner}' is missing integer parameter 0x{hash:08X}"
        ))
    })
}

/// Guesses a helper bone's parent from BotW naming conventions when the
/// support-bone record carries no usable space link.
fn infer_parent_bone_name(bone_name: &str) -> Option<String> {
    if bone_name.trim().is_empty() {
        return None;
    }
    let (prefix, local) = match bone_name.rsplit_once(':') {
        Some((prefix, local)) => (format!("{prefix}:"), local.to_owned()),
        None => (String::new(), bone_name.to_owned()),
    };
    let mut assist = local.replace("_Assist_", "_");
    if let Some(stripped) = assist.strip_suffix("_Assist") {
        assist = stripped.to_owned();
    }
    if assist != local {
        return Some(format!("{prefix}{assist}"));
    }
    let side = if local.ends_with("_L") {
        "_L"
    } else if local.ends_with("_R") {
        "_R"
    } else {
        ""
    };
    let core = &local[..local.len() - side.len()];
    let explicit = match core {
        "Head" => Some("Neck"),
        "Neck" => Some("Spine_2"),
        "Spine_2" => Some("Spine_1"),
        "Ankle" => Some("Leg_2"),
        "Wrist" | "Elbow" => Some("Arm_2"),
        "Arm_2" => Some("Arm_1"),
        "Arm_1" => Some("Clavicle"),
        "Clavicle" => Some("Spine_2"),
        "Leg_2" => Some("Leg_1"),
        "Leg_1" => Some("Waist"),
        _ => None,
    };
    if let Some(parent) = explicit {
        return Some(format!("{prefix}{parent}{side}"));
    }
    let mut parts: Vec<String> = local.split('_').map(str::to_owned).collect();
    for index in (0..parts.len()).rev() {
        if let Ok(number) = parts[index].parse::<i64>() {
            if number > 1 {
                parts[index] = (number - 1).to_string();
                return Some(format!("{prefix}{}", parts.join("_")));
            }
        }
    }
    if local.starts_with("Hair_Back_") {
        return Some(format!("{prefix}Hair_Root"));
    }
    if ["Hair", "Head", "Knot", "Osage", "Pony"]
        .iter()
        .any(|token| local.starts_with(token))
    {
        return Some(format!("{prefix}Head"));
    }
    if local.starts_with("Skirt") {
        return Some(format!("{prefix}Waist"));
    }
    if local.starts_with("Necklace") {
        return Some(format!("{prefix}Neck"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_their_parameter_names() {
        assert_eq!(crc32("bone_id"), BONE_ID);
        assert_eq!(crc32("base_bone_id"), BASE_BONE_ID);
        assert_eq!(crc32("driver_bone_list"), DRIVER_LIST);
        assert_eq!(crc32("driven_bone_list"), DRIVEN_LIST);
        assert_eq!(crc32("pose_driven_list"), POSE_DRIVEN_LIST);
        assert_eq!(crc32("output_list"), OUTPUT_LIST);
        assert_eq!(crc32("base_translate"), BASE_TRANSLATION);
        assert_eq!(crc32("base_rotate"), BASE_ROTATION);
        assert_eq!(crc32("aim_axis"), AIM_AXIS);
        assert_eq!(crc32("up_axis"), UP_AXIS);
        assert_eq!(crc32("connection_0_id"), OUTPUT_CONNECTION0);
        assert_eq!(crc32("connection_1_id"), OUTPUT_CONNECTION1);
        assert_eq!(crc32("driver_bone_id"), CURVE_DRIVER);
    }

    #[test]
    fn parent_inference_follows_botw_naming() {
        assert_eq!(
            infer_parent_bone_name("Link:Head").as_deref(),
            Some("Link:Neck")
        );
        assert_eq!(
            infer_parent_bone_name("Arm_2_L").as_deref(),
            Some("Arm_1_L")
        );
        assert_eq!(infer_parent_bone_name("Hair_3").as_deref(), Some("Hair_2"));
        assert_eq!(
            infer_parent_bone_name("Hair_Back_1").as_deref(),
            Some("Hair_Root")
        );
        assert_eq!(
            infer_parent_bone_name("Skirt_A_1").as_deref(),
            Some("Waist")
        );
        assert_eq!(
            infer_parent_bone_name("Belt_Assist").as_deref(),
            Some("Belt")
        );
        assert_eq!(infer_parent_bone_name("Mystery"), None);
    }
}

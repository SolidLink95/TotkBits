use super::{
    animation_slot::AnimationSlot,
    asb_command::AsbCommand,
    asb_header::AsbHeader,
    auxiliary::{read_tags, read_x68, X68Entry},
    blackboard::LocalBlackboard,
    event::{read_events, AsbEvent},
    exb::Exb,
    node::AsbNode,
    node_tables::{read_bones, read_markings, read_x38, read_x40, BoneGroup, X38Entry, X40Entry},
    transition::{read_command_groups, read_transitions, Transition},
    x2c::{read_x2c, X2cEntry},
};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsbInfo {
    #[serde(rename = "Magic")]
    pub magic: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Filename")]
    pub filename: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asb {
    #[serde(rename = "Info")]
    pub info: AsbInfo,
    #[serde(rename = "Local Blackboard Parameters")]
    pub local_blackboard_parameters: LocalBlackboard,
    #[serde(rename = "Commands")]
    pub commands: Vec<AsbCommand>,
    #[serde(rename = "Transitions")]
    pub transitions: Vec<Transition>,
    #[serde(rename = "Animation Slots")]
    pub animation_slots: Vec<AnimationSlot>,
    #[serde(rename = "Nodes")]
    pub nodes: BTreeMap<u32, AsbNode>,
    #[serde(rename = "Valid Tag List")]
    pub valid_tag_list: Vec<String>,
    #[serde(rename = "0x68 Section")]
    pub x68_section: Vec<X68Entry>,
    #[serde(rename = "EXB Section", skip_serializing_if = "Option::is_none")]
    pub exb: Option<Exb>,
    #[serde(skip)]
    pub events: Vec<AsbEvent>,
    #[serde(skip)]
    pub x2c_entries: Vec<X2cEntry>,
    #[serde(skip)]
    pub x38_entries: Vec<X38Entry>,
    #[serde(skip)]
    pub x40_entries: Vec<X40Entry>,
    #[serde(skip)]
    pub bone_groups: Vec<BoneGroup>,
    #[serde(skip)]
    pub as_markings: Vec<Vec<String>>,
    #[serde(skip)]
    pub header: AsbHeader,
    #[serde(skip)]
    original_data: Vec<u8>,
}

impl Asb {
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let mut reader = BinaryReader::new(data);
        let header = AsbHeader::read(&mut reader)?;
        header.validate_offsets(data.len())?;
        let pool = BinaryReader::new(&data[header.string_pool_offset as usize..]);
        let filename = pool.read_c_string_at(header.filename_offset as usize)?;
        reader.seek(header.local_blackboard_offset as usize)?;
        let local_blackboard_parameters = LocalBlackboard::read(&mut reader, &pool)?;
        reader.seek(if header.version == 0x417 { 0x6c } else { 0x68 })?;
        let mut commands = Vec::new();
        for _ in 0..header.command_count {
            commands.push(AsbCommand::read(&mut reader, &pool, header.version)?);
        }
        let node_start = reader.position();
        let command_groups = read_command_groups(&mut reader, &pool, header.command_groups_offset)?;
        let transitions = read_transitions(
            &mut reader,
            &pool,
            header.transitions_offset,
            &command_groups,
        )?;
        reader.seek(header.slots_offset as usize)?;
        let mut animation_slots = Vec::new();
        for _ in 0..header.slot_count {
            animation_slots.push(AnimationSlot::read(&mut reader, &pool)?);
        }
        let events = read_events(
            &mut reader,
            &pool,
            header.event_offsets_offset,
            header.event_count,
        )?;
        let x2c_entries = read_x2c(&mut reader, &pool, header.x2c_offset)?;
        let valid_tag_list = read_tags(&mut reader, &pool, header.tag_list_offset)?;
        let x68_section = read_x68(&mut reader, &pool, header.x68_offset)?;
        let exb = if header.exb_offset != 0 {
            Some(Exb::from_bytes(&data[header.exb_offset as usize..])?)
        } else {
            None
        };
        let x38_entries = read_x38(&mut reader, &pool, header.x38_offset, header.x38_count)?;
        let x40_entries = read_x40(
            &mut reader,
            header.x40_offset,
            header.x40_count,
            header.version,
        )?;
        let bone_groups = read_bones(
            &mut reader,
            &pool,
            header.bone_group_offset,
            header.bone_group_count,
        )?;
        let as_markings = read_markings(&mut reader, &pool, header.as_markings_offset)?;
        reader.seek(node_start)?;
        let mut nodes = BTreeMap::new();
        for index in 0..header.node_count {
            nodes.insert(
                index,
                AsbNode::read(
                    &mut reader,
                    &pool,
                    &x38_entries,
                    &x40_entries,
                    &as_markings,
                    header.x38_index_offset,
                    header.version,
                    &x2c_entries,
                    &events,
                    &bone_groups,
                )
                .map_err(|error| io::Error::new(error.kind(), format!("node {index}: {error}")))?,
            );
        }
        Ok(Self {
            info: AsbInfo {
                magic: "ASB ".into(),
                version: format!("{:#x}", header.version),
                filename,
            },
            local_blackboard_parameters,
            commands,
            transitions,
            animation_slots,
            nodes,
            valid_tag_list,
            x68_section,
            exb,
            events,
            x2c_entries,
            x38_entries,
            x40_entries,
            bone_groups,
            as_markings,
            header,
            original_data: data.to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.original_data.clone()
    }

    pub fn to_native_bytes(&self) -> io::Result<Vec<u8>> {
        super::writer::AsbWriter::new(self)?.write()
    }

    pub fn to_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(self).map_err(io::Error::other)
    }

    pub fn from_yaml(text: &str) -> io::Result<Self> {
        serde_yaml::from_str(text).map_err(io::Error::other)
    }

    pub fn events_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(&self.events).map_err(io::Error::other)
    }

    pub fn connections_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(&self.x2c_entries).map_err(io::Error::other)
    }

    pub fn node_tables_yaml(&self) -> io::Result<String> {
        serde_yaml::to_string(&(
            &self.x38_entries,
            &self.x40_entries,
            &self.bone_groups,
            &self.as_markings,
        ))
        .map_err(io::Error::other)
    }
}

use super::{
    actor::Actor,
    container::{read_container, Container},
    radix_tree::read_string_ptr,
};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::io::{self, ErrorKind};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "Type")]
pub enum Event {
    Action(ActionEvent),
    Switch(SwitchEvent),
    Fork(ForkEvent),
    Join(JoinEvent),
    Subflow(SubflowEvent),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActionEvent {
    pub name: String,
    pub next_event_index: i16,
    pub actor_index: i16,
    pub actor_action_index: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Container>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_action: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SwitchEvent {
    pub name: String,
    pub actor_index: i16,
    pub actor_query_index: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Container>,
    pub switch_cases: Vec<SwitchCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_query: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SwitchCase {
    pub value: i32,
    pub event_index: u16,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ForkEvent {
    pub name: String,
    pub join_event_index: i16,
    pub fork_event_indicies: Vec<i16>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct JoinEvent {
    pub name: String,
    pub next_event_index: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_event_name: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SubflowEvent {
    pub name: String,
    pub next_event_index: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Container>,
    pub flowchart_name: String,
    pub entry_point_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_event_name: Option<String>,
}

impl Event {
    pub fn read(data: &[u8], offset: u64) -> io::Result<Self> {
        let mut r = BinaryReader::new(data);
        r.seek(offset as usize)?;
        let name = read_string_ptr(&mut r, data)?;
        let kind = r.read_u8()?;
        r.skip(1)?;
        Ok(match kind {
            0 => {
                let next_event_index = r.read_i16()?;
                let actor_index = r.read_i16()?;
                let actor_action_index = r.read_i16()?;
                let p = r.read_u64()?;
                Self::Action(ActionEvent {
                    name,
                    next_event_index,
                    actor_index,
                    actor_action_index,
                    parameters: optional_container(data, p)?,
                    next_event_name: None,
                    actor_name: None,
                    actor_action: None,
                })
            }
            1 => {
                let count = r.read_u16()? as usize;
                let actor_index = r.read_i16()?;
                let actor_query_index = r.read_i16()?;
                let p = r.read_u64()?;
                let cases_ptr = r.read_u64()?;
                let mut cr = BinaryReader::new(data);
                cr.seek(cases_ptr as usize)?;
                let mut switch_cases = Vec::new();
                for _ in 0..count {
                    switch_cases.push(SwitchCase {
                        value: cr.read_i32()?,
                        event_index: cr.read_u16()?,
                    });
                    cr.skip(2)?;
                }
                Self::Switch(SwitchEvent {
                    name,
                    actor_index,
                    actor_query_index,
                    parameters: optional_container(data, p)?,
                    switch_cases,
                    actor_name: None,
                    actor_query: None,
                })
            }
            2 => {
                let count = r.read_u16()? as usize;
                let join_event_index = r.read_i16()?;
                r.skip(2)?;
                let ptr = r.read_u64()?;
                let mut fr = BinaryReader::new(data);
                fr.seek(ptr as usize)?;
                let fork_event_indicies = (0..count)
                    .map(|_| fr.read_i16())
                    .collect::<io::Result<_>>()?;
                Self::Fork(ForkEvent {
                    name,
                    join_event_index,
                    fork_event_indicies,
                })
            }
            3 => {
                let next_event_index = r.read_i16()?;
                Self::Join(JoinEvent {
                    name,
                    next_event_index,
                    next_event_name: None,
                })
            }
            4 => {
                let next_event_index = r.read_i16()?;
                r.skip(4)?;
                let p = r.read_u64()?;
                let flowchart_name = read_string_ptr(&mut r, data)?;
                let entry_point_name = read_string_ptr(&mut r, data)?;
                Self::Subflow(SubflowEvent {
                    name,
                    next_event_index,
                    parameters: optional_container(data, p)?,
                    flowchart_name,
                    entry_point_name,
                    next_event_name: None,
                })
            }
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("unsupported event type {kind}"),
                ))
            }
        })
    }

    pub fn resolve(&mut self, names: &[String], actors: &[Actor]) {
        let event_name = |index: i16| {
            usize::try_from(index)
                .ok()
                .and_then(|i| names.get(i))
                .cloned()
        };
        match self {
            Self::Action(e) => {
                e.next_event_name = event_name(e.next_event_index);
                if let Ok(i) = usize::try_from(e.actor_index) {
                    e.actor_name = actors.get(i).map(|a| a.name.clone());
                    e.actor_action = actors
                        .get(i)
                        .and_then(|a| {
                            usize::try_from(e.actor_action_index)
                                .ok()
                                .and_then(|j| a.actions.get(j))
                        })
                        .cloned();
                }
            }
            Self::Switch(e) => {
                if let Ok(i) = usize::try_from(e.actor_index) {
                    e.actor_name = actors.get(i).map(|a| a.name.clone());
                    e.actor_query = actors
                        .get(i)
                        .and_then(|a| {
                            usize::try_from(e.actor_query_index)
                                .ok()
                                .and_then(|j| a.queries.get(j))
                        })
                        .cloned();
                }
            }
            Self::Join(e) => e.next_event_name = event_name(e.next_event_index),
            Self::Subflow(e) => e.next_event_name = event_name(e.next_event_index),
            Self::Fork(_) => {}
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Action(e) => &e.name,
            Self::Switch(e) => &e.name,
            Self::Fork(e) => &e.name,
            Self::Join(e) => &e.name,
            Self::Subflow(e) => &e.name,
        }
    }
}
fn optional_container(data: &[u8], offset: u64) -> io::Result<Option<Container>> {
    if offset == 0 {
        Ok(None)
    } else {
        Ok(Some(read_container(data, offset)?))
    }
}

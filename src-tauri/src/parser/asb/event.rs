use super::parameter::{read_parameter, ParameterType};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::io::{self, ErrorKind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AsbEvent {
    #[serde(rename = "Trigger Events")]
    pub trigger_events: Vec<TriggerEvent>,
    #[serde(rename = "Hold Events")]
    pub hold_events: Vec<HoldEvent>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriggerEvent {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Unknown 1")]
    pub unknown_1: u32,
    #[serde(rename = "Unknown Hash")]
    pub unknown_hash: String,
    #[serde(rename = "Start Frame")]
    pub start_frame: f64,
    #[serde(rename = "Parameters")]
    pub parameters: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoldEvent {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Unknown 1")]
    pub unknown_1: u32,
    #[serde(rename = "Unknown Hash")]
    pub unknown_hash: String,
    #[serde(rename = "Start Frame")]
    pub start_frame: f64,
    #[serde(rename = "End Frame")]
    pub end_frame: f64,
    #[serde(rename = "Parameters")]
    pub parameters: Vec<Value>,
}

pub fn read_events(
    r: &mut BinaryReader<'_>,
    p: &BinaryReader<'_>,
    offset: u32,
    count: u32,
) -> io::Result<Vec<AsbEvent>> {
    r.seek(offset as usize)?;
    let mut result = Vec::new();
    for _ in 0..count {
        let at = r.read_u32()? as usize;
        let ret = r.position();
        r.seek(at)?;
        let tc = r.read_u32()?;
        let hc = r.read_u32()?;
        let mut trigger_events = Vec::new();
        let mut hold_events = Vec::new();
        for _ in 0..tc {
            trigger_events.push(read_trigger(r, p)?)
        }
        for _ in 0..hc {
            hold_events.push(read_hold(r, p)?)
        }
        r.seek(ret)?;
        result.push(AsbEvent {
            trigger_events,
            hold_events,
        });
    }
    Ok(result)
}
fn read_trigger(r: &mut BinaryReader<'_>, p: &BinaryReader<'_>) -> io::Result<TriggerEvent> {
    let name = p.read_c_string_at(r.read_u32()? as usize)?;
    let unknown_1 = r.read_u32()?;
    let at = r.read_u32()? as usize;
    r.read_u32()?;
    let unknown_hash = format!("{:#x}", r.read_u32()?);
    let start_frame = f64::from(r.read_f32()?);
    let ret = r.position();
    r.seek(at)?;
    let parameters = read_parameters(r, p)?;
    r.seek(ret)?;
    Ok(TriggerEvent {
        name,
        unknown_1,
        unknown_hash,
        start_frame,
        parameters,
    })
}
fn read_hold(r: &mut BinaryReader<'_>, p: &BinaryReader<'_>) -> io::Result<HoldEvent> {
    let name = p.read_c_string_at(r.read_u32()? as usize)?;
    let unknown_1 = r.read_u32()?;
    let at = r.read_u32()? as usize;
    r.read_u32()?;
    let unknown_hash = format!("{:#x}", r.read_u32()?);
    let start_frame = f64::from(r.read_f32()?);
    let end_frame = f64::from(r.read_f32()?);
    let ret = r.position();
    r.seek(at)?;
    let parameters = read_parameters(r, p)?;
    r.seek(ret)?;
    Ok(HoldEvent {
        name,
        unknown_1,
        unknown_hash,
        start_frame,
        end_frame,
        parameters,
    })
}
fn read_parameters(r: &mut BinaryReader<'_>, p: &BinaryReader<'_>) -> io::Result<Vec<Value>> {
    let count = r.read_u32()?;
    let mut offsets = Vec::new();
    for _ in 0..count {
        offsets.push(r.read_u32()?)
    }
    let mut values = Vec::new();
    for tagged in offsets {
        let kind = match tagged >> 24 {
            0x40 => ParameterType::String,
            0x30 => ParameterType::Float,
            0x20 => ParameterType::Int,
            0x10 => ParameterType::Bool,
            v => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid event parameter tag {v:#x}"),
                ))
            }
        };
        r.seek((tagged & 0xff_ffff) as usize)?;
        values.push(read_parameter(r, p, kind)?)
    }
    Ok(values)
}

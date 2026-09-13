use super::{baev_array::BaevArray, baev_parameter::BaevParameter};
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TriggerEvent {
    #[serde(rename = "Parameters")]
    pub parameters: Vec<BaevParameter>,
    #[serde(rename = "Start Frame")]
    pub start_frame: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HoldEvent {
    #[serde(rename = "Parameters")]
    pub parameters: Vec<BaevParameter>,
    #[serde(rename = "Start Frame")]
    pub start_frame: f64,
    #[serde(rename = "End Frame")]
    pub end_frame: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Event {
    #[serde(
        rename = "Trigger Array",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub triggers: Vec<TriggerEvent>,
    #[serde(rename = "Hold Array", skip_serializing_if = "Vec::is_empty", default)]
    pub holds: Vec<HoldEvent>,
}

impl Event {
    pub fn read(reader: &mut BinaryReader<'_>) -> io::Result<(String, Self)> {
        let name_offset = usize::try_from(reader.read_u64()?).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "BAEV event name offset exceeds usize",
            )
        })?;
        let trigger_array = BaevArray::read(reader)?;
        let hold_array = BaevArray::read(reader)?;
        reader.read_u32()?;
        reader.read_u32()?;
        let name = reader.read_c_string_at(name_offset)?;
        let triggers = read_triggers(reader, trigger_array)?;
        let holds = read_holds(reader, hold_array)?;
        Ok((name, Self { triggers, holds }))
    }
}

fn read_parameters(
    reader: &mut BinaryReader<'_>,
    array: BaevArray,
) -> io::Result<Vec<BaevParameter>> {
    if array.count == 0 {
        return Ok(Vec::new());
    }
    let return_position = reader.position();
    reader.seek(array.offset()?)?;
    let mut offsets = Vec::new();
    for _ in 0..array.count {
        offsets.push(usize::try_from(reader.read_u64()?).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "BAEV parameter offset exceeds usize",
            )
        })?);
    }
    let mut values = Vec::with_capacity(offsets.len());
    for offset in offsets {
        values.push(BaevParameter::read_at(reader, offset)?);
    }
    reader.seek(return_position)?;
    Ok(values)
}

fn read_triggers(reader: &mut BinaryReader<'_>, array: BaevArray) -> io::Result<Vec<TriggerEvent>> {
    if array.count == 0 {
        return Ok(Vec::new());
    }
    let return_position = reader.position();
    reader.seek(array.offset()?)?;
    let mut values = Vec::new();
    for _ in 0..array.count {
        let parameters = BaevArray::read(reader)?;
        let start_frame = f64::from(reader.read_f32()?);
        reader.read_f32()?;
        values.push(TriggerEvent {
            parameters: read_parameters(reader, parameters)?,
            start_frame,
        });
    }
    reader.seek(return_position)?;
    Ok(values)
}

fn read_holds(reader: &mut BinaryReader<'_>, array: BaevArray) -> io::Result<Vec<HoldEvent>> {
    if array.count == 0 {
        return Ok(Vec::new());
    }
    let return_position = reader.position();
    reader.seek(array.offset()?)?;
    let mut values = Vec::new();
    for _ in 0..array.count {
        let parameters = BaevArray::read(reader)?;
        let start_frame = f64::from(reader.read_f32()?);
        let end_frame = f64::from(reader.read_f32()?);
        values.push(HoldEvent {
            parameters: read_parameters(reader, parameters)?,
            start_frame,
            end_frame,
        });
    }
    reader.seek(return_position)?;
    Ok(values)
}

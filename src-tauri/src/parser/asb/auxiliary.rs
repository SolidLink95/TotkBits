use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct X68Entry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Unknown")]
    pub unknown: f64,
}
pub fn read_tags(
    r: &mut BinaryReader<'_>,
    p: &BinaryReader<'_>,
    offset: u32,
) -> io::Result<Vec<String>> {
    r.seek(offset as usize)?;
    let count = r.read_u32()?;
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(p.read_c_string_at(r.read_u32()? as usize)?)
    }
    Ok(out)
}
pub fn read_x68(
    r: &mut BinaryReader<'_>,
    p: &BinaryReader<'_>,
    offset: Option<u32>,
) -> io::Result<Vec<X68Entry>> {
    let Some(offset) = offset else {
        return Ok(Vec::new());
    };
    r.seek(offset as usize)?;
    let count = r.read_u32()?;
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(X68Entry {
            name: p.read_c_string_at(r.read_u32()? as usize)?,
            unknown: f64::from(r.read_f32()?),
        })
    }
    Ok(out)
}

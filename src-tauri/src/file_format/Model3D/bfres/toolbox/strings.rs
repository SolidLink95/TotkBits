//! TOTK's `Shader/ExternalBinaryString.bfres.mc` holds the render-info,
//! shader-parameter and option names that version 10 models reference by a
//! 64-bit key instead of a local string pointer. Toolbox loads it into a
//! static `StringCache` and resolves every string pointer through it first.

use super::super::BfresError;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct ExternalStrings {
    pub map: HashMap<u64, String>,
}

impl ExternalStrings {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parses the decompressed external string binary (a BFRES whose header
    /// carries the `HoldsExternalStrings` flag).
    pub fn from_bfres(data: &[u8]) -> Result<Self, BfresError> {
        if data.get(..4) != Some(b"FRES") {
            return Err(BfresError::new(0, "external string binary is not a BFRES"));
        }
        let keys = read_u64(data, 0xb8)? as usize;
        let dict = read_u64(data, 0xc0)? as usize;
        if keys == 0 || dict == 0 {
            return Err(BfresError::new(
                0xb8,
                "external string binary has no key table",
            ));
        }
        let count = read_u32(data, dict + 4)? as usize;
        let mut map = HashMap::with_capacity(count);
        for index in 0..count {
            let node = dict + 8 + (index + 1) * 16;
            let name_pointer = read_u64(data, node + 8)? as usize;
            let name = read_res_string(data, name_pointer)?;
            let key = read_u64(data, keys + index * 8)?;
            map.insert(key, name);
        }
        Ok(Self { map })
    }

    /// Loads `<romfs>/Shader/ExternalBinaryString.bfres.mc`.
    pub fn from_romfs(romfs: &std::path::Path) -> Result<Self, BfresError> {
        let path = romfs.join("Shader").join("ExternalBinaryString.bfres.mc");
        let compressed = std::fs::read(&path).map_err(|error| {
            BfresError::new(0, format!("cannot read {}: {error}", path.display()))
        })?;
        let raw = if crate::Settings::Magic::is_mcpk(&compressed) {
            crate::compression::meshcodec::MeshCodec::decompress(&compressed)
                .map_err(|error| BfresError::new(0, format!("{}: {error}", path.display())))?
        } else {
            compressed
        };
        Self::from_bfres(&raw)
    }
}

pub(super) fn read_u16(data: &[u8], offset: usize) -> Result<u16, BfresError> {
    data.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .ok_or_else(|| BfresError::new(offset, "truncated u16"))
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Result<u32, BfresError> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .ok_or_else(|| BfresError::new(offset, "truncated u32"))
}

pub(super) fn read_u64(data: &[u8], offset: usize) -> Result<u64, BfresError> {
    data.get(offset..offset + 8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .ok_or_else(|| BfresError::new(offset, "truncated u64"))
}

pub(super) fn read_f32(data: &[u8], offset: usize) -> Result<f32, BfresError> {
    read_u32(data, offset).map(f32::from_bits)
}

/// Reads a `ResString`: a u16 length followed by the bytes and a NUL.
pub(super) fn read_res_string(data: &[u8], offset: usize) -> Result<String, BfresError> {
    let len = read_u16(data, offset)? as usize;
    let bytes = data
        .get(offset + 2..offset + 2 + len)
        .ok_or_else(|| BfresError::new(offset, "truncated string"))?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

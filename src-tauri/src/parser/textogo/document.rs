use super::{TexToGoError, TexToGoHeader, TexToGoSurface};
use crate::parser::binary::BinaryReader;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TexToGoFile {
    pub header: TexToGoHeader,
    pub surfaces: Vec<TexToGoSurface>,
}

impl TexToGoFile {
    pub fn parse(data: &[u8]) -> Result<Self, TexToGoError> {
        let reader = BinaryReader::new(data);
        if bytes_at(&reader, 4, 4)? != b"6PK0" {
            return Err(TexToGoError::new(4, "missing 6PK0 magic"));
        }
        let header = TexToGoHeader {
            header_size: u16_at(&reader, 0)?,
            version: u16_at(&reader, 2)?,
            width: u16_at(&reader, 8)?,
            height: u16_at(&reader, 10)?,
            depth: u16_at(&reader, 12)?,
            mip_count: byte(&reader, 14)?,
            format_flag: byte(&reader, 19)?,
            format_setting: u32_at(&reader, 20)?,
            component_selectors: array_at(&reader, 24)?,
            hash: array_at(&reader, 28)?,
            format: u16_at(&reader, 60)?,
            texture_settings: [
                u32_at(&reader, 64)?,
                u32_at(&reader, 68)?,
                u32_at(&reader, 72)?,
                u32_at(&reader, 76)?,
            ],
        };
        let count = usize::from(header.depth)
            .checked_mul(usize::from(header.mip_count))
            .ok_or_else(|| TexToGoError::new(12, "surface count overflow"))?;
        let mut surfaces = Vec::new();
        let descriptors = usize::from(header.header_size);
        let descriptor_bytes = count
            .checked_mul(4)
            .ok_or_else(|| TexToGoError::new(descriptors, "descriptor table size overflow"))?;
        let sizes = descriptors
            .checked_add(descriptor_bytes)
            .ok_or_else(|| TexToGoError::new(descriptors, "size table offset overflow"))?;
        let size_bytes = count
            .checked_mul(8)
            .ok_or_else(|| TexToGoError::new(sizes, "size table overflow"))?;
        let mut payload = sizes
            .checked_add(size_bytes)
            .ok_or_else(|| TexToGoError::new(sizes, "payload offset overflow"))?;
        bytes_at(&reader, descriptors, descriptor_bytes)?;
        bytes_at(&reader, sizes, size_bytes)?;
        for i in 0..count {
            let descriptor = descriptors + i * 4;
            let size_entry = sizes + i * 8;
            let compressed_size = u32_at(&reader, size_entry)?;
            let compressed = bytes_at(&reader, payload, compressed_size as usize)?;
            let decoded = crate::Zstd::TotkZstd::decompress_empty(compressed).map_err(|e| {
                TexToGoError::new(payload, format!("invalid Zstandard surface: {e}"))
            })?;
            surfaces.push(TexToGoSurface {
                array_level: u16_at(&reader, descriptor)?,
                mip_level: byte(&reader, descriptor + 2)?,
                surface_count: byte(&reader, descriptor + 3)?,
                compressed_size,
                compression_type: u32_at(&reader, size_entry + 4)?,
                data: decoded,
            });
            payload = payload
                .checked_add(compressed_size as usize)
                .ok_or_else(|| TexToGoError::new(payload, "surface payload offset overflow"))?;
        }
        Ok(Self { header, surfaces })
    }
}
fn bytes_at<'a>(reader: &BinaryReader<'a>, p: usize, n: usize) -> Result<&'a [u8], TexToGoError> {
    reader
        .read_bytes_at(p, n)
        .map_err(|error| TexToGoError::new(p, error.to_string()))
}
fn array_at<const N: usize>(reader: &BinaryReader<'_>, p: usize) -> Result<[u8; N], TexToGoError> {
    reader
        .read_array_at(p)
        .map_err(|error| TexToGoError::new(p, error.to_string()))
}
fn byte(reader: &BinaryReader<'_>, p: usize) -> Result<u8, TexToGoError> {
    reader
        .read_u8_at(p)
        .map_err(|error| TexToGoError::new(p, error.to_string()))
}
fn u16_at(reader: &BinaryReader<'_>, p: usize) -> Result<u16, TexToGoError> {
    reader
        .read_u16_at(p)
        .map_err(|error| TexToGoError::new(p, error.to_string()))
}
fn u32_at(reader: &BinaryReader<'_>, p: usize) -> Result<u32, TexToGoError> {
    reader
        .read_u32_at(p)
        .map_err(|error| TexToGoError::new(p, error.to_string()))
}

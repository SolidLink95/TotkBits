use std::{collections::HashMap, io};

use crate::parser::binary::{BinaryReader, Endian};

const KOD_SIGNATURE: u32 = 0x4b4f_445f;
const KODI_SIGNATURE: u32 = 0x4b4f_4449;
const KODR_SIGNATURE: u32 = 0x4b4f_4452;

#[derive(Debug, Clone, Default)]
pub struct KidsObjFile {
    textures: HashMap<u32, u32>,
}

impl KidsObjFile {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let mut reader = BinaryReader::with_endian(data, Endian::Little);
        if reader.read_u32()? != KOD_SIGNATURE {
            return Err(invalid("invalid KidsObj signature"));
        }
        reader.skip(12)?;
        let entry_count = reader.read_u32()? as usize;
        reader.skip(8)?;
        let mut textures = HashMap::new();

        for _ in 0..entry_count {
            let signature = reader.read_u32()?;
            reader.skip(8)?; // version and advisory entry size
            let object_hash = reader.read_u32()?;
            let column_count = match signature {
                KODI_SIGNATURE => {
                    reader.skip(4)?; // object type
                    reader.read_u32()? as usize
                }
                KODR_SIGNATURE => {
                    reader.skip(8)?; // parent object file and parent object
                    reader.read_u32()? as usize
                }
                _ => return Err(invalid("invalid KidsObj entry signature")),
            };
            if column_count > data.len() / 12 {
                return Err(invalid("invalid KidsObj column count"));
            }

            let mut row_counts = Vec::with_capacity(column_count);
            for _ in 0..column_count {
                reader.skip(4)?; // column type
                row_counts.push(reader.read_u32()? as usize);
                reader.skip(4)?; // column name
            }

            let mut first_texture = None;
            for row_count in row_counts {
                for _ in 0..row_count {
                    let value = reader.read_u32()?;
                    if value != 0 && first_texture.is_none() {
                        first_texture = Some(value);
                    }
                }
            }
            // KODR entries describe inheritance. Like the Python importer,
            // texture lookup is indexed only by concrete KODI objects.
            if signature == KODI_SIGNATURE {
                if let Some(texture) = first_texture {
                    textures.insert(object_hash, texture);
                }
            }
        }
        Ok(Self { textures })
    }

    pub fn texture_for(&self, object_hash: u32) -> Option<u32> {
        self.textures.get(&object_hash).copied()
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u32(data: &mut Vec<u8>, value: u32) {
        data.extend_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn handles_inherited_kodr_records() {
        let mut data = Vec::new();
        for value in [KOD_SIGNATURE, 0x3030_3030, 28, 16, 2, 0, 0] {
            push_u32(&mut data, value);
        }
        for value in [
            KODR_SIGNATURE,
            0x3030_3030,
            44,
            0x1111_1111,
            0x2222_2222,
            0x3333_3333,
            1,
            5,
            1,
            0x4444_4444,
            0x5555_5555,
        ] {
            push_u32(&mut data, value);
        }
        for value in [
            KODI_SIGNATURE,
            0x3030_3030,
            40,
            0xaaaa_aaaa,
            0,
            1,
            5,
            1,
            0xbbbb_bbbb,
            0xcccc_cccc,
        ] {
            push_u32(&mut data, value);
        }

        let parsed = KidsObjFile::parse(&data).unwrap();
        assert_eq!(parsed.texture_for(0xaaaa_aaaa), Some(0xcccc_cccc));
        assert_eq!(parsed.texture_for(0x1111_1111), None);
    }
}

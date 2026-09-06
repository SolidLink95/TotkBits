use crate::parser::binary::{BinaryReader, Endian};
use std::io::{self, ErrorKind};

#[derive(Clone, Debug)]
pub struct Section {
    pub signature: String,
    pub kind: u8,
    pub offset: usize,
    pub size: usize,
    pub payload_offset: usize,
    pub children: Vec<Section>,
}
impl Section {
    pub fn read(
        data: &[u8],
        offset: usize,
        limit: usize,
        expected: Option<&str>,
    ) -> io::Result<Self> {
        if offset.checked_add(8).is_none_or(|v| v > limit) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "truncated BPHCL section",
            ));
        }
        let mut r = BinaryReader::with_endian(data, Endian::Big);
        r.seek(offset)?;
        let word = r.read_u32()?;
        let kind = (word >> 30) as u8;
        let size = (word & 0x3fff_ffff) as usize;
        let signature = String::from_utf8_lossy(r.read_bytes(4)?).into_owned();
        if size < 8
            || offset.checked_add(size).is_none_or(|v| v > limit)
            || expected.is_some_and(|e| e != signature)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("invalid {signature} section at {offset:#x}"),
            ));
        }
        let mut s = Self {
            signature,
            kind,
            offset,
            size,
            payload_offset: offset + 8,
            children: vec![],
        };
        if matches!(s.signature.as_str(), "TAG0" | "TYPE" | "INDX") {
            let mut p = s.payload_offset;
            while p < offset + size {
                let child = Self::read(data, p, offset + size, None)?;
                p += child.size;
                s.children.push(child);
            }
        }
        Ok(s)
    }
    pub fn find(&self, name: &str) -> Option<&Section> {
        self.children.iter().find_map(|c| {
            if c.signature == name {
                Some(c)
            } else {
                c.find(name)
            }
        })
    }
    pub fn payload_end(&self) -> usize {
        self.offset + self.size
    }
}

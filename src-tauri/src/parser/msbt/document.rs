use super::{
    attribute::{AttributeOffsets, AttributeSection},
    header::Header,
    label::{Label, LabelSection},
    numeric_label::NumericLabelSection,
    section::Section,
    style::StyleSection,
    text,
    token::TextPart,
};
use crate::parser::binary::{BinaryReader, BinaryWriter};
use std::io::{self, ErrorKind};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub label: Option<String>,
    pub id: Option<u32>,
    pub attribute: Vec<u8>,
    pub style: Option<u32>,
    pub parts: Vec<TextPart>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msbt {
    pub header: Header,
    pub sections: Vec<Section>,
    pub messages: Vec<Message>,
    pub label_groups: u32,
    pub attribute_offsets: Vec<u32>,
    pub attribute_string_pool: Vec<u8>,
}
impl Msbt {
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let header = Header::read(data)?;
        let limit = header.file_size as usize;
        let bounded = data
            .get(..limit)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid MSBT file size"))?;
        let mut r = BinaryReader::with_endian(bounded, header.endian);
        r.seek(32)?;
        let mut sections = Vec::new();
        for _ in 0..header.section_count {
            if r.position() % 16 != 0 {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "unaligned MSBT section",
                ));
            }
            let mut magic = [0; 4];
            magic.copy_from_slice(r.read_bytes(4)?);
            let size = r.read_u32()? as usize;
            let mut reserved = [0; 8];
            reserved.copy_from_slice(r.read_bytes(8)?);
            let body = r.read_bytes(size)?.to_vec();
            let pad = (16 - (r.position() % 16)) % 16;
            let padding = r.read_bytes(pad)?.to_vec();
            sections.push(Section {
                magic,
                reserved,
                data: body,
                padding,
            });
        }
        let txt = sections
            .iter()
            .find(|s| &s.magic == b"TXT2")
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "missing TXT2"))?;
        let mut tr = BinaryReader::with_endian(&txt.data, header.endian);
        let count = tr.read_u32()? as usize;
        if count > txt.data.len() / 4 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "TXT2 count out of bounds",
            ));
        }
        let mut offs = Vec::new();
        for _ in 0..count {
            offs.push(tr.read_u32()? as usize)
        }
        let labels = sections
            .iter()
            .find(|s| &s.magic == b"LBL1")
            .map(|s| LabelSection::read(&s.data, header.endian))
            .transpose()?;
        let ids = sections
            .iter()
            .find(|s| &s.magic == b"NLI1")
            .map(|s| NumericLabelSection::read(&s.data, header.endian))
            .transpose()?;
        let attributes = sections
            .iter()
            .find(|s| &s.magic == b"ATR1")
            .map(|s| AttributeSection::read(&s.data, header.endian))
            .transpose()?;
        let attribute_offsets = sections
            .iter()
            .find(|s| &s.magic == b"ATO1")
            .map(|s| AttributeOffsets::read(&s.data, header.endian))
            .transpose()?
            .map(|x| x.0)
            .unwrap_or_default();
        let styles = sections
            .iter()
            .find(|s| &s.magic == b"TSY1")
            .map(|s| StyleSection::read(&s.data, header.endian))
            .transpose()?;
        let label_groups = labels.as_ref().map(|x| x.group_count).unwrap_or(0);
        let attribute_string_pool = attributes
            .as_ref()
            .map(|x| x.string_pool.clone())
            .unwrap_or_default();
        let mut messages = Vec::new();
        for i in 0..count {
            let start = offs[i];
            let end = offs.get(i + 1).copied().unwrap_or(txt.data.len());
            if start > end || end > txt.data.len() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "TXT2 offset out of bounds",
                ));
            }
            let label = labels
                .as_ref()
                .and_then(|l| l.labels.iter().find(|x| x.index as usize == i))
                .map(|x| x.name.clone());
            messages.push(Message {
                label,
                id: ids
                    .as_ref()
                    .and_then(|x| x.0.iter().find(|(_, index)| *index as usize == i))
                    .map(|(id, _)| *id),
                attribute: attributes
                    .as_ref()
                    .and_then(|x| x.records.get(i))
                    .cloned()
                    .unwrap_or_default(),
                style: styles.as_ref().and_then(|x| x.0.get(i)).copied(),
                parts: text::decode(&txt.data[start..end], header.encoding, header.endian)?,
            });
        }
        Ok(Self {
            header,
            sections,
            messages,
            label_groups,
            attribute_offsets,
            attribute_string_pool,
        })
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let mut sections = self.sections.clone();
        if let Some(section) = sections.iter_mut().find(|s| &s.magic == b"LBL1") {
            let labels = self
                .messages
                .iter()
                .enumerate()
                .filter_map(|(index, m)| {
                    m.label.as_ref().map(|name| Label {
                        name: name.clone(),
                        index: index as u32,
                    })
                })
                .collect();
            section.data = LabelSection {
                group_count: self.label_groups.max(1),
                labels,
            }
            .write(self.header.endian)?;
        }
        if let Some(section) = sections.iter_mut().find(|s| &s.magic == b"NLI1") {
            section.data = NumericLabelSection(
                self.messages
                    .iter()
                    .enumerate()
                    .filter_map(|(index, m)| m.id.map(|id| (id, index as u32)))
                    .collect(),
            )
            .write(self.header.endian);
        }
        if let Some(section) = sections.iter_mut().find(|s| &s.magic == b"ATO1") {
            section.data =
                AttributeOffsets(self.attribute_offsets.clone()).write(self.header.endian);
        }
        if let Some(section) = sections.iter_mut().find(|s| &s.magic == b"ATR1") {
            let item_size = self
                .messages
                .iter()
                .map(|m| m.attribute.len())
                .max()
                .unwrap_or(0) as u32;
            let records = self.messages.iter().map(|m| m.attribute.clone()).collect();
            section.data = AttributeSection {
                item_size,
                records,
                string_pool: self.attribute_string_pool.clone(),
            }
            .write(self.header.endian)?;
        }
        if let Some(section) = sections.iter_mut().find(|s| &s.magic == b"TSY1") {
            section.data =
                StyleSection(self.messages.iter().map(|m| m.style.unwrap_or(0)).collect())
                    .write(self.header.endian);
        }
        if let Some(txt) = sections.iter_mut().find(|s| &s.magic == b"TXT2") {
            let mut bodies = Vec::new();
            for m in &self.messages {
                bodies.push(text::encode(
                    &m.parts,
                    self.header.encoding,
                    self.header.endian,
                )?)
            }
            let base = 4 + 4 * bodies.len();
            let mut w = BinaryWriter::with_endian(self.header.endian);
            w.write_u32(bodies.len() as u32);
            let mut off = base;
            for b in &bodies {
                w.write_u32(off as u32);
                off += b.len()
            }
            for b in bodies {
                w.write_bytes(&b)
            }
            txt.data = w.into_inner();
        }
        let mut w = BinaryWriter::with_endian(self.header.endian);
        self.header.write(&mut w, 0, sections.len() as u16);
        for s in &sections {
            w.align(16)?;
            w.write_bytes(&s.magic);
            w.write_u32(s.data.len() as u32);
            w.write_bytes(&s.reserved);
            w.write_bytes(&s.data);
            let need = (16 - w.position() % 16) % 16;
            if s.padding.len() == need {
                w.write_bytes(&s.padding)
            } else {
                w.write_bytes(&vec![0xab; need])
            }
        }
        let size = w.position() as u32;
        w.seek(0);
        self.header.write(&mut w, size, sections.len() as u16);
        Ok(w.into_inner())
    }

    /// Rewrites text for existing messages without rebuilding LBL1 or any
    /// unchanged TXT2 body. This is the safe path for large game message files
    /// whose labels and metadata are not being added, removed, or reordered.
    pub fn to_bytes_preserving_layout(&self) -> io::Result<Vec<u8>> {
        let original_txt = self
            .sections
            .iter()
            .find(|section| &section.magic == b"TXT2")
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "missing TXT2"))?;
        let mut reader = BinaryReader::with_endian(&original_txt.data, self.header.endian);
        let count = reader.read_u32()? as usize;
        if count > self.messages.len() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "layout-preserving MSBT save cannot remove messages",
            ));
        }
        let mut offsets = Vec::with_capacity(count);
        for _ in 0..count {
            offsets.push(reader.read_u32()? as usize);
        }
        let labels = self
            .sections
            .iter()
            .find(|section| &section.magic == b"LBL1")
            .map(|section| LabelSection::read(&section.data, self.header.endian))
            .transpose()?;
        for (index, message) in self.messages.iter().take(count).enumerate() {
            let original = labels
                .as_ref()
                .and_then(|labels| {
                    labels
                        .labels
                        .iter()
                        .find(|label| label.index as usize == index)
                })
                .map(|label| label.name.as_str());
            if original != message.label.as_deref() {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "layout-preserving MSBT save cannot change labels",
                ));
            }
        }

        let mut bodies = Vec::with_capacity(self.messages.len());
        for (index, message) in self.messages.iter().enumerate() {
            if index >= count {
                bodies.push(text::encode(
                    &message.parts,
                    self.header.encoding,
                    self.header.endian,
                )?);
                continue;
            }
            let start = offsets[index];
            let end = offsets
                .get(index + 1)
                .copied()
                .unwrap_or(original_txt.data.len());
            if start > end || end > original_txt.data.len() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "TXT2 offset out of bounds",
                ));
            }
            let original = &original_txt.data[start..end];
            let original_parts = text::decode(original, self.header.encoding, self.header.endian)?;
            if original_parts == message.parts {
                bodies.push(original.to_vec());
            } else {
                bodies.push(text::encode(
                    &message.parts,
                    self.header.encoding,
                    self.header.endian,
                )?);
            }
        }

        let mut txt_writer = BinaryWriter::with_endian(self.header.endian);
        txt_writer.write_u32(self.messages.len() as u32);
        let mut offset = 4 + self.messages.len() * 4;
        for body in &bodies {
            txt_writer.write_u32(offset as u32);
            offset += body.len();
        }
        for body in bodies {
            txt_writer.write_bytes(&body);
        }
        let mut sections = self.sections.clone();
        if self.messages.len() != count {
            let labels = self
                .messages
                .iter()
                .enumerate()
                .filter_map(|(index, message)| {
                    message.label.as_ref().map(|name| Label {
                        name: name.clone(),
                        index: index as u32,
                    })
                })
                .collect();
            sections
                .iter_mut()
                .find(|section| &section.magic == b"LBL1")
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "missing LBL1"))?
                .data = LabelSection {
                group_count: self.label_groups.max(1),
                labels,
            }
            .write(self.header.endian)?;
        }
        sections
            .iter_mut()
            .find(|section| &section.magic == b"TXT2")
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "missing TXT2"))?
            .data = txt_writer.into_inner();

        let mut writer = BinaryWriter::with_endian(self.header.endian);
        self.header.write(&mut writer, 0, sections.len() as u16);
        for section in &sections {
            writer.align(16)?;
            writer.write_bytes(&section.magic);
            writer.write_u32(section.data.len() as u32);
            writer.write_bytes(&section.reserved);
            writer.write_bytes(&section.data);
            let required = (16 - writer.position() % 16) % 16;
            if section.padding.len() == required {
                writer.write_bytes(&section.padding);
            } else {
                writer.write_bytes(&vec![0xab; required]);
            }
        }
        let size = writer.position() as u32;
        writer.seek(0);
        self.header.write(&mut writer, size, sections.len() as u16);
        Ok(writer.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn totk_corpus_parses_and_rebuilds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp/EUen");
        if !root.exists() {
            return;
        }
        let mut count = 0;
        for entry in walkdir::WalkDir::new(root) {
            let entry = entry.unwrap();
            if entry.path().extension().and_then(|x| x.to_str()) != Some("msbt") {
                continue;
            }
            let data = fs::read(entry.path()).unwrap();
            let parsed = Msbt::from_bytes(&data)
                .unwrap_or_else(|e| panic!("{}: {e}", entry.path().display()));
            let text = crate::parser::msbt::editable::serialize(&parsed);
            let edited = crate::parser::msbt::editable::deserialize(&parsed, &text)
                .unwrap_or_else(|e| panic!("{} editable: {e}", entry.path().display()));
            let rebuilt = edited.to_bytes().unwrap();
            Msbt::from_bytes(&rebuilt)
                .unwrap_or_else(|e| panic!("{} rebuilt: {e}", entry.path().display()));
            count += 1;
        }
        assert_eq!(count, 342);
    }
}

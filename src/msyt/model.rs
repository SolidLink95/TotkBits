use crate::{botw::Control, Result};

use anyhow::Context;
use byteordered::{Endian, Endianness};
use indexmap::IndexMap;
use msbt::{builder::MsbtBuilder, section::Atr1, Encoding, Header, Msbt};
use serde_derive::{Deserialize, Serialize};

use std::{
    collections::BTreeMap,
    convert::TryFrom,
    fs::File,
    io::{BufReader, BufWriter, Cursor, Read, Seek},
    path::Path,
    pin::Pin,
};

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Msyt {
    #[serde(flatten)]
    pub msbt: MsbtInfo,
    pub entries: IndexMap<String, Entry>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct MsbtInfo {
    pub group_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atr1_unknown: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::util::option_serde_base64"
    )]
    pub ato1: Option<Vec<u8>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::util::option_serde_base64"
    )]
    pub tsy1: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nli1: Option<Nli1>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<String>,
    pub contents: Vec<Content>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Nli1 {
    pub id_count: u32,
    pub global_ids: BTreeMap<u32, u32>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case", untagged)]
pub enum Content {
    Text(String),
    Control(Control),
}

impl Content {
    pub fn write_all(header: &Header, contents: &[Content]) -> Result<Vec<u8>> {
        let mut buf = Vec::new();

        for content in contents {
            match *content {
                Content::Text(ref s) => match header.encoding() {
                    Encoding::Utf16 => {
                        let mut inner_buf = [0; 2];
                        let mut bytes: Vec<u8> = s
                            .encode_utf16()
                            .flat_map(|x| {
                                header
                                    .endianness()
                                    .write_u16(&mut inner_buf[..], x)
                                    .expect("failed writing to array");
                                inner_buf.to_vec()
                            })
                            .collect();
                        buf.append(&mut bytes);
                    }
                    Encoding::Utf8 => {
                        buf.append(&mut s.as_bytes().to_vec());
                    }
                },
                Content::Control(ref c) => c.write(header, &mut buf)?,
            }
        }

        // add \u0000
        buf.push(0);
        buf.push(0);

        Ok(buf)
    }
}

#[allow(dead_code)]
impl Msyt {
    pub fn write_as_msbt<W: std::io::Write>(
        self,
        writer: &mut W,
        endianness: Endianness,
    ) -> Result<()> {
        self.write_as_msbt_with_encoding(writer, Encoding::Utf16, endianness)
    }

    pub fn write_as_msbt_with_encoding<W: std::io::Write>(
        self,
        writer: &mut W,
        encoding: Encoding,
        endianness: Endianness,
    ) -> Result<()> {
        let msbt = self.try_into_msbt_with_encoding(encoding, endianness)?;
        msbt.write_to(BufWriter::new(writer))
            .with_context(|| "could not write msbt to {}")?;
        Ok(())
    }

    pub fn try_into_msbt(self, endianness: Endianness) -> Result<Pin<Box<Msbt>>> {
        self.try_into_msbt_with_encoding(Encoding::Utf16, endianness)
    }

    pub fn try_into_msbt_with_encoding(
        self,
        encoding: Encoding,
        endianness: Endianness,
    ) -> Result<Pin<Box<Msbt>>> {
        let mut builder = MsbtBuilder::new(endianness, encoding, Some(self.msbt.group_count));
        if let Some(unknown_bytes) = self.msbt.ato1 {
            builder = builder.ato1(msbt::section::Ato1::new_unlinked(unknown_bytes));
        }
        if let Some(unknown_1) = self.msbt.atr1_unknown {
            // ATR1 should have exactly the same amount of entries as TXT2. In the BotW files, sometimes
            // an ATR1 section is specified to have that amount but the section is actually empty. For
            // self's purposes, if the self does not contain the same amount of attributes as it does
            // text entries (i.e. not every label has an `attributes` node), it will be assumed that the
            // ATR1 section should specify that it has the correct amount of entries but actually be
            // empty.
            let strings: Option<Vec<String>> = self
                .entries
                .iter()
                .map(|(_, e)| e.attributes.clone())
                .map(|s| s.map(crate::util::append_nul))
                .collect();
            let atr_len = match strings {
                Some(ref s) => s.len(),
                None => self.entries.len(),
            };
            let strings = strings.unwrap_or_default();
            builder = builder.atr1(msbt::section::Atr1::new_unlinked(
                atr_len as u32,
                unknown_1,
                strings,
            ));
        }
        if let Some(unknown_bytes) = self.msbt.tsy1 {
            builder = builder.tsy1(msbt::section::Tsy1::new_unlinked(unknown_bytes));
        }
        if let Some(nli1) = self.msbt.nli1 {
            builder = builder.nli1(msbt::section::Nli1::new_unlinked(
                nli1.id_count,
                nli1.global_ids,
            ));
        }
        for (label, entry) in self.entries.into_iter() {
            let new_val = Content::write_all(builder.header(), &entry.contents)?;
            builder = builder.add_label(label, new_val);
        }
        Ok(builder.build())
    }

    pub fn into_msbt_bytes(self, endianness: Endianness) -> Result<Vec<u8>> {
        let mut data: Vec<u8> = vec![];
        self.write_as_msbt(&mut data, endianness)?;
        Ok(data)
    }

    pub fn into_msbt_bytes_with_encoding(
        self,
        encoding: Encoding,
        endianness: Endianness,
    ) -> Result<Vec<u8>> {
        let mut data: Vec<u8> = vec![];
        self.write_as_msbt_with_encoding(&mut data, encoding, endianness)?;
        Ok(data)
    }

    pub fn from_msbt_reader<R: Read + Seek>(reader: R) -> Result<Self> {
        Self::try_from(Msbt::from_reader(reader)?)
    }

    pub fn from_msbt_bytes<B: AsRef<[u8]>>(bytes: B) -> Result<Self> {
        Self::from_msbt_reader(Cursor::new(bytes.as_ref()))
    }

    pub fn from_msbt_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        Self::from_msbt_reader(BufReader::new(File::open(file)?))
    }
}

impl TryFrom<Pin<Box<Msbt>>> for Msyt {
    type Error = anyhow::Error;
    fn try_from(msbt: Pin<Box<Msbt>>) -> Result<Msyt> {
        let lbl1 = match msbt.lbl1() {
            Some(lbl) => lbl,
            None => anyhow::bail!("invalid msbt: missing lbl1"),
        };

        let mut entries = IndexMap::with_capacity(lbl1.labels().len());

        for label in lbl1.labels() {
            let mut all_content = Vec::new();

            let raw_value = label.value_raw().ok_or_else(|| {
                anyhow::format_err!("invalid msbt: missing string for label {}", label.name(),)
            })?;
            let mut parts = crate::botw::parse_controls(msbt.header(), raw_value)
                .with_context(|| "could not parse control sequences")?;
            all_content.append(&mut parts);
            let entry = Entry {
                attributes: msbt.atr1().and_then(|a| {
                    a.strings()
                        .get(label.index() as usize)
                        .map(|s| crate::util::strip_nul(s))
                        .map(ToString::to_string)
                }),
                contents: all_content,
            };
            entries.insert(label.name().to_string(), entry);
        }

        entries.sort_keys();

        Ok(Msyt {
            entries,
            msbt: MsbtInfo {
                group_count: lbl1.group_count(),
                atr1_unknown: msbt.atr1().map(Atr1::unknown_1),
                ato1: msbt.ato1().map(|a| a.unknown_bytes().to_vec()),
                tsy1: msbt.tsy1().map(|a| a.unknown_bytes().to_vec()),
                nli1: msbt.nli1().map(|a| crate::model::Nli1 {
                    id_count: a.id_count(),
                    global_ids: a.global_ids().clone(),
                }),
            },
        })
    }
}

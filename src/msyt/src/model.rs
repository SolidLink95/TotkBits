use crate::{
  Result,
  botw::Control,
};

use byteordered::Endian;
use indexmap::IndexMap;
use msbt::{Encoding, Header};
use serde_derive::{Deserialize, Serialize};

use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct Msyt {
  #[serde(flatten)]
  pub msbt: MsbtInfo,
  pub entries: IndexMap<String, Entry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MsbtInfo {
  pub group_count: u32,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub atr1_unknown: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::util::option_serde_base64")]
  pub ato1: Option<Vec<u8>>,
  #[serde(default, skip_serializing_if = "Option::is_none", with = "crate::util::option_serde_base64")]
  pub tsy1: Option<Vec<u8>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub nli1: Option<Nli1>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Entry {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub attributes: Option<String>,
  pub contents: Vec<Content>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Nli1 {
  pub id_count: u32,
  pub global_ids: BTreeMap<u32, u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
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
            let mut bytes: Vec<u8> = s.encode_utf16()
              .flat_map(|x| {
                header.endianness().write_u16(&mut inner_buf[..], x).expect("failed writing to array");
                inner_buf.to_vec()
              })
              .collect();
            buf.append(&mut bytes);
          }
          Encoding::Utf8 => buf.append(&mut s.as_bytes().to_vec()),
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

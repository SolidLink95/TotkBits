use crate::{
  Result,
  botw::{Control, RawControl, SubControl},
};
use super::Control1;

use byteordered::Endian;

use failure::ResultExt;

use msbt::{Encoding, Header};

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control1_9 {
  unknown_1: Option<[u8; 12]>,
  strings: [Control1_9String; 4],
  field_3: u16,
  field_4: u16,
  unknown_2: Option<[u8; 12]>,
  field_6: [u8; 2],
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Control1_9String {
  field_1: u16,
  string: String,
}

const UNKNOWN: [u8; 12] = [255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0];

impl SubControl for Control1_9 {
  fn marker(&self) -> u16 {
    9
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let mut unknown_buf = [0; 12];

    let payload_length = header.endianness().read_u16(&mut reader).with_context(|_| "could not read length")?;

    reader.read_exact(&mut unknown_buf[..]).with_context(|_| "could not read for unknown bytes")?;
    let unknown_1 = if unknown_buf == UNKNOWN {
      Some(unknown_buf)
    } else {
      reader.seek(SeekFrom::Current(-12)).with_context(|_| "could not seek backward")?;
      None
    };

    let mut strings = [
      Control1_9String::default(),
      Control1_9String::default(),
      Control1_9String::default(),
      Control1_9String::default(),
    ];
    for cstring in strings.iter_mut() {
      let field_1 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read cstring field_1")?;
      let str_len = header.endianness().read_u16(&mut reader).with_context(|_| "could not read cstring length")?;

      let mut str_bytes = vec![0; str_len as usize];
      reader.read_exact(&mut str_bytes).with_context(|_| "could not read string bytes")?;

      let string = match header.encoding() {
        Encoding::Utf16 => {
          let utf16_str: Vec<u16> = str_bytes.chunks(2)
            .map(|bs| header.endianness().read_u16(bs).map_err(Into::into))
            .collect::<Result<_>>()
            .with_context(|_| "could not read u16s from string bytes")?;
          String::from_utf16(&utf16_str).with_context(|_| "could not parse utf-16 string")?
        },
        Encoding::Utf8 => String::from_utf8(str_bytes).with_context(|_| "could not parse utf-8 string")?,
      };

      *cstring = Control1_9String {
        field_1,
        string,
      };
    }

    let field_3 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_3")?;
    let field_4 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_4")?;

    let unknown_2 = if reader.get_ref().len() > reader.position() as usize + 12 {
      reader.read_exact(&mut unknown_buf[..]).with_context(|_| "could not read unknown_buf")?;
      if unknown_buf == UNKNOWN {
        Some(unknown_buf)
      } else {
        reader.seek(SeekFrom::Current(-12)).with_context(|_| "could not seek backwards")?;
        None
      }
    } else {
      None
    };

    let mut field_6 = [0; 2];
    reader.read_exact(&mut field_6).with_context(|_| "could not read field_6")?;

    debug_assert_eq!(u64::from(payload_length), reader.position() - 4);

    Ok(Control::Raw(RawControl::One(Control1::Nine(Control1_9 {
      unknown_1,
      strings,
      field_3,
      field_4,
      unknown_2,
      field_6,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    let payload_length: usize = self.unknown_1.map(|x| x.len()).unwrap_or(0)
      + self.strings.iter().map(|s| std::mem::size_of_val(&s.field_1) + std::mem::size_of::<u16>() + match header.encoding() {
        Encoding::Utf16 => s.string.encode_utf16().count() * std::mem::size_of::<u16>(),
        Encoding::Utf8 => s.string.len(),
      }).sum::<usize>()
      + std::mem::size_of_val(&self.field_3)
      + std::mem::size_of_val(&self.field_4)
      + self.unknown_2.map(|x| x.len()).unwrap_or(0)
      + self.field_6.len();

    header.endianness().write_u16(&mut writer, payload_length as u16).with_context(|_| "could not write length")?;

    if let Some(ref unknown_1) = self.unknown_1 {
      writer.write_all(&unknown_1[..]).with_context(|_| "could not write unknown_1")?;
    }

    for string in &self.strings {
      header.endianness().write_u16(&mut writer, string.field_1).with_context(|_| "could not write cstring field_1")?;
      let str_bytes = match header.encoding() {
        Encoding::Utf16 => {
          let mut buf = [0; 2];
          string.string
            .encode_utf16()
            .flat_map(|x| {
              header.endianness().write_u16(&mut buf[..], x).expect("failed to write to array");
              buf.to_vec()
            })
            .collect()
        }
        Encoding::Utf8 => string.string.as_bytes().to_vec(),
      };
      header.endianness().write_u16(&mut writer, str_bytes.len() as u16).with_context(|_| "could not write cstring length")?;
      writer.write_all(&str_bytes).with_context(|_| "could not write cstring bytes")?;
    }

    header.endianness().write_u16(&mut writer, self.field_3).with_context(|_| "could not write field_3")?;
    header.endianness().write_u16(&mut writer, self.field_4).with_context(|_| "could not write field_4")?;

    if let Some(ref unknown_2) = self.unknown_2 {
      writer.write_all(&unknown_2[..]).with_context(|_| "could not write unknown_2")?;
    }

    writer.write_all(&self.field_6[..]).with_context(|_| "could not write field_6")?;

    Ok(())
  }
}

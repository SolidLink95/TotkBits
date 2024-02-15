use crate::{
  Result,
  botw::{Control, SubControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::{Encoding, Header};

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control4_2 {
  pub(crate) field_1: u16,
  pub(crate) string: String,
}

impl SubControl for Control4_2 {
  fn marker(&self) -> u16 {
    2
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let _field_1 = header.endianness().read_u16(&mut reader)
      .with_context(|_| "could not read field_1")?;
    let str_len = header.endianness().read_u16(&mut reader)
      .with_context(|_| "could not read string length")?;

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

    Ok(Control::Animation {
      name: string,
    })
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1)
      .with_context(|_| "could not write field_1")?;

    let str_bytes = match header.encoding() {
      Encoding::Utf16 => {
        let mut buf = [0; 2];
        self.string.encode_utf16()
          .flat_map(|x| {
            header.endianness().write_u16(&mut buf[..], x).expect("failed to write to array");
            buf.to_vec()
          })
          .collect()
      },
      Encoding::Utf8 => self.string.as_bytes().to_vec(),
    };

    header.endianness().write_u16(&mut writer, str_bytes.len() as u16)
      .with_context(|_| "could not write string bytes length")?;
    writer.write_all(&str_bytes).with_context(|_| "could not write string bytes")?;

    Ok(())
  }
}

use crate::Result;

use byteordered::Endian;

use msbt::{Encoding, Header};

use failure::ResultExt;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control201Localisation {
  pub strings: Vec<String>,
}

impl Control201Localisation {
  pub fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Self> {
    let len = header.endianness().read_u16(&mut reader).with_context(|_| "could not read len")? as usize;
    let mut strings = Vec::with_capacity(2);

    let mut total = 0;
    while total != len {
      let str_len = match header.encoding() {
        Encoding::Utf16 => header.endianness().read_u16(&mut reader).with_context(|_| "could not read str_len")? as usize,
        Encoding::Utf8 => {
          let mut buf = [0; 1];
          reader.read_exact(&mut buf).with_context(|_| "could not read str_len")?;
          buf[0] as usize
        },
      };
      total += str_len as usize + 2;
      if str_len == 0 {
        strings.push(Default::default());
        continue;
      }

      let mut str_bytes = vec![0; str_len];
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

      strings.push(string);
    }

    Ok(Control201Localisation {
      strings,
    })
  }

  pub fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    let mut encoded_strs = Vec::with_capacity(self.strings.len());

    for s in &self.strings {
      let str_bytes = match header.encoding() {
        Encoding::Utf16 => {
          let mut buf = [0; 2];
          s.encode_utf16()
            .flat_map(|x| {
              header.endianness().write_u16(&mut buf[..], x).expect("failed to write to array");
              buf.to_vec()
            })
            .collect()
        },
        Encoding::Utf8 => s.as_bytes().to_vec(),
      };

      encoded_strs.push(str_bytes);
    }

    let len_size = match header.encoding() {
      Encoding::Utf16 => encoded_strs.len() * 2,
      Encoding::Utf8 => encoded_strs.len(),
    };

    let len = encoded_strs.iter().map(Vec::len).sum::<usize>() + len_size;

    header.endianness().write_u16(&mut writer, len as u16).with_context(|_| "could not write len")?;

    for s in encoded_strs {
      match header.encoding() {
        Encoding::Utf16 => header.endianness().write_u16(&mut writer, s.len() as u16).with_context(|_| "could not write str_len")?,
        Encoding::Utf8 => writer.write_all(&[s.len() as u8]).with_context(|_| "could not write str_len")?,
      }
      writer.write_all(&s).with_context(|_| "could not write string")?;
    }

    Ok(())
  }
}

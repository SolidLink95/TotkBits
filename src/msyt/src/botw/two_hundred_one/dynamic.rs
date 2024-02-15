use crate::Result;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control201Dynamic {
  pub len: u16,
  pub field_2: Vec<u8>,
}

impl Control201Dynamic {
  pub(crate) fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Self> {
    let len = header.endianness().read_u16(&mut reader).with_context(|_| "could not read len")?;

    let mut field_2 = vec![0; len as usize];
    reader.read_exact(&mut field_2).with_context(|_| "could not read field_2")?;

    Ok(Control201Dynamic {
      len,
      field_2,
    })
  }

  pub(crate) fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.len).with_context(|_| "could not write len")?;
    writer.write_all(&self.field_2).with_context(|_| "could not write field_2")?;

    Ok(())
  }
}

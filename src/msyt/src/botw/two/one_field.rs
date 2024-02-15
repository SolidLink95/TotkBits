use crate::Result;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control2OneField {
  field_1: u16,
}

impl Control2OneField {
  pub(crate) fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Self> {
    Ok(Control2OneField {
      field_1: header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?,
    })
  }

  pub(crate) fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;

    Ok(())
  }
}

use crate::{
  Result,
  botw::{Control, RawControl, SubControl},
};
use super::Control0;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control0_0 {
  field_1: u16,
  field_2: u16,
  field_3: u16,
}

impl SubControl for Control0_0 {
  fn marker(&self) -> u16 {
    0
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    Ok(Control::Raw(RawControl::Zero(Control0::Zero(Control0_0 {
      field_1: header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?,
      field_2: header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_2")?,
      field_3: header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_3")?,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;
    header.endianness().write_u16(&mut writer, self.field_2).with_context(|_| "could not write field_2")?;
    header.endianness().write_u16(&mut writer, self.field_3).with_context(|_| "could not write field_3")?;

    Ok(())
  }
}

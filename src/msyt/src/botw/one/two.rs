use crate::{
  Result,
  botw::{Control, RawControl, SubControl},
};
use super::Control1;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control1_2 {
  field_1: u16,
}

impl SubControl for Control1_2 {
  fn marker(&self) -> u16 {
    2
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    Ok(Control::Raw(RawControl::One(Control1::Two(Control1_2 {
      field_1: header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;

    Ok(())
  }
}

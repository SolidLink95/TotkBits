use crate::{
  Result,
  botw::{Control, PauseKind, RawControl, SubControl},
};
use super::Control1;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control1_0 {
  pub field_1: u16,
  pub field_2: u32,
}

impl SubControl for Control1_0 {
  fn marker(&self) -> u16 {
    0
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let field_1 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?;
    let field_2 = header.endianness().read_u32(&mut reader).with_context(|_| "could not read field_2")?;

    if field_1 == 4 {
      return Ok(Control::Pause(PauseKind::Frames(field_2)));
    }

    Ok(Control::Raw(RawControl::One(Control1::Zero(Control1_0 {
      field_1,
      field_2,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;
    header.endianness().write_u32(&mut writer, self.field_2).with_context(|_| "could not write field_2")?;

    Ok(())
  }
}

use crate::{
  Result,
  botw::{Control, Icon, RawControl, SubControl},
};
use super::Control1;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control1_7 {
  pub(crate) field_1: u16,
  pub(crate) field_2: [u8; 2],
}

impl SubControl for Control1_7 {
  fn marker(&self) -> u16 {
    7
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let field_1 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?;

    let mut field_2 = [0; 2];
    reader.read_exact(&mut field_2).with_context(|_| "could not read field_2")?;

    if field_1 == 2 && field_2[1] == 205 {
      let icon = Icon::from_u8(field_2[0]);
      return Ok(Control::Icon { icon });
    }

    Ok(Control::Raw(RawControl::One(Control1::Seven(Control1_7 {
      field_1,
      field_2,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;
    writer.write_all(&self.field_2[..]).with_context(|_| "could not write field_2")?;

    Ok(())
  }
}

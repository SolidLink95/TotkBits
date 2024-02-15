use crate::{
  Result,
  botw::{Colour, Control, RawControl, SubControl},
};
use super::Control0;

use failure::ResultExt;

use byteordered::Endian;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control0_3 {
  pub(crate) field_1: u16,
  pub(crate) field_2: u16,
}

impl SubControl for Control0_3 {
  fn marker(&self) -> u16 {
    3
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let field_1 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?;
    let field_2 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_2")?;

    if field_1 == 2 {
      if field_2 == 65535 {
        return Ok(Control::ResetColour);
      }
      if let Some(colour) = Colour::from_u16(field_2) {
        return Ok(Control::SetColour { colour });
      }
    }

    Ok(Control::Raw(RawControl::Zero(Control0::Three(Control0_3 {
      field_1,
      field_2,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;
    header.endianness().write_u16(&mut writer, self.field_2).with_context(|_| "could not write field_2")?;

    Ok(())
  }
}

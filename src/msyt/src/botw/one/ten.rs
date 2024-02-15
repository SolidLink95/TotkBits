use crate::{
  Result,
  botw::{Control, RawControl, SubControl},
};
use super::Control1;

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control1_10 {
  pub field_1: u16,
  pub field_2: u16,
  pub field_3: [u8; 2],
}

impl SubControl for Control1_10 {
  fn marker(&self) -> u16 {
    10
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let field_1 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?;
    let field_2 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_2")?;

    let mut field_3 = [0; 2];
    reader.read_exact(&mut field_3[..]).with_context(|_| "could not read field_3")?;

    if field_1 == 4 && field_3 == [1, 205] {
      return Ok(Control::SingleChoice {
        label: field_2,
      });
    }

    Ok(Control::Raw(RawControl::One(Control1::Ten(Control1_10 {
      field_1,
      field_2,
      field_3,
    }))))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;
    header.endianness().write_u16(&mut writer, self.field_2).with_context(|_| "could not write field_2")?;
    writer.write_all(&self.field_3[..]).with_context(|_| "could not write field_3")?;

    Ok(())
  }
}

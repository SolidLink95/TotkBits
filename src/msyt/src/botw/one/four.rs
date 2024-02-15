use crate::{
  Result,
  botw::{Control, SubControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control1_4 {
  pub(crate) field_1: u16,
  pub(crate) field_2: u16,
  pub(crate) field_3: u16,
  pub(crate) field_4: [u8; 2],
}

impl SubControl for Control1_4 {
  fn marker(&self) -> u16 {
    4
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let mut field_4 = [0; 2];
    let field_1 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_1")?;
    let field_2 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_2")?;
    let field_3 = header.endianness().read_u16(&mut reader).with_context(|_| "could not read field_3")?;
    reader.read_exact(&mut field_4[..]).with_context(|_| "could not read field_4")?;

    Ok(Control::Choice {
      unknown: field_1,
      choice_labels: vec![field_2, field_3],
      selected_index: field_4[0],
      cancel_index: field_4[1],
    })
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1).with_context(|_| "could not write field_1")?;
    header.endianness().write_u16(&mut writer, self.field_2).with_context(|_| "could not write field_2")?;
    header.endianness().write_u16(&mut writer, self.field_3).with_context(|_| "could not write field_3")?;
    writer.write_all(&self.field_4[..]).with_context(|_| "could not write field_4")?;

    Ok(())
  }
}

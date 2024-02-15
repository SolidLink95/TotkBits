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
pub struct Control4_1 {
  pub field_1: Vec<u8>,
}

impl SubControl for Control4_1 {
  fn marker(&self) -> u16 {
    1
  }

  fn parse(header: &Header, mut reader: &mut Cursor<&[u8]>) -> Result<Control> {
    let field_1_len = header.endianness().read_u16(&mut reader)
      .with_context(|_| "could not read field_1 length")?;
    let mut field_1 = vec![0; field_1_len as usize];
    reader.read_exact(&mut field_1).with_context(|_| "could not read field_1")?;

    Ok(Control::Sound2 {
      unknown: field_1,
    })
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1.len() as u16)
      .with_context(|_| "could not write field_1 length")?;
    writer.write_all(&self.field_1).with_context(|_| "could not write field_1")?;

    Ok(())
  }
}

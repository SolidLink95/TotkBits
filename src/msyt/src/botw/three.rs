use crate::{
  Result,
  botw::{Control, MainControl, RawControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Read, Write};

#[derive(Debug, Deserialize, Serialize)]
pub struct Control3 {
  pub field_1: u16,
  pub field_2: Vec<u8>,
}

impl MainControl for Control3 {
  fn marker(&self) -> u16 {
    3
  }

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)> {
    let mut c = Cursor::new(buf);
    let field_1 = header.endianness().read_u16(&mut c).with_context(|_| "could not read field_1")?;
    let field_2_len = header.endianness().read_u16(&mut c).with_context(|_| "could not read field_2 length")?;
    let mut field_2 = vec![0; field_2_len as usize];
    c.read_exact(&mut field_2).with_context(|_| "could not read field_2")?;

    if field_1 == 1 {
      return Ok((c.position() as usize, Control::Sound { unknown: field_2 }));
    }

    Ok((
      c.position() as usize,
      Control::Raw(RawControl::Three(Control3 {
        field_1,
        field_2,
      }))
    ))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, self.field_1)
      .with_context(|_| "could not write field_1")?;
    header.endianness().write_u16(&mut writer, self.field_2.len() as u16)
      .with_context(|_| "could not write field_2 length")?;
    writer.write_all(&self.field_2).with_context(|_| "could not write field_2")?;

    Ok(())
  }
}

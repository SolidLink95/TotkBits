use crate::{
  Result,
  botw::{Control, MainControl, SubControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

pub(crate) mod zero;
pub(crate) mod one;
pub(crate) mod two;
pub(crate) mod three;

use self::{
  zero::Control4_0,
  one::Control4_1,
  two::Control4_2,
  three::Control4_3,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control4 {
  Zero(Control4_0),
  One(Control4_1),
  Two(Control4_2),
  Three(Control4_3),
}

impl MainControl for Control4 {
  fn marker(&self) -> u16 {
    4
  }

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)> {
    let mut c = Cursor::new(buf);

    let kind = header.endianness().read_u16(&mut c).with_context(|_| "could not read control subtype marker")?;
    let control = match kind {
      0 => Control4_0::parse(header, &mut c).with_context(|_| "could not parse control subtype 0")?,
      1 => Control4_1::parse(header, &mut c).with_context(|_| "could not parse control subtype 1")?,
      2 => Control4_2::parse(header, &mut c).with_context(|_| "could not parse control subtype 2")?,
      3 => Control4_3::parse(header, &mut c).with_context(|_| "could not parse control subtype 3")?,
      x => failure::bail!("unknown control 4 subtype: {}", x),
    };

    Ok((
      c.position() as usize,
      control,
    ))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    let sub = match *self {
      Control4::Zero(ref c) => c as &dyn SubControl,
      Control4::One(ref c) => c as &dyn SubControl,
      Control4::Two(ref c) => c as &dyn SubControl,
      Control4::Three(ref c) => c as &dyn SubControl,
    };
    header.endianness().write_u16(&mut writer, sub.marker())
      .with_context(|_| format!("could not write control subtype marker {}", sub.marker()))?;
    sub.write(header, &mut writer)
      .with_context(|_| format!("could not write control subtype {}", sub.marker()))
      .map_err(Into::into)
  }
}

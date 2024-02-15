use crate::{
  Result,
  botw::{Control, MainControl, SubControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[allow(clippy::module_inception)]
pub(crate) mod zero;
pub(crate) mod one;
pub(crate) mod two;
pub(crate) mod three;
pub(crate) mod four;

use self::{
  zero::Control0_0,
  one::Control0_1,
  two::Control0_2,
  three::Control0_3,
  four::Control0_4,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control0 {
  Zero(Control0_0),
  One(Control0_1),
  Two(Control0_2),
  Three(Control0_3),
  Four(Control0_4),
}

impl MainControl for Control0 {
  fn marker(&self) -> u16 {
    0
  }

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)> {
    let mut c = Cursor::new(buf);

    let kind = header.endianness().read_u16(&mut c)?;
    let control = match kind {
      0 => Control0_0::parse(header, &mut c).with_context(|_| "could not parse control subtype 0")?,
      1 => Control0_1::parse(header, &mut c).with_context(|_| "could not parse control subtype 1")?,
      2 => Control0_2::parse(header, &mut c).with_context(|_| "could not parse control subtype 2")?,
      3 => Control0_3::parse(header, &mut c).with_context(|_| "could not parse control subtype 3")?,
      4 => Control0_4::parse(header, &mut c).with_context(|_| "could not parse control subtype 4")?,
      x => failure::bail!("unknown control 0 type: {}", x),
    };

    Ok((
      c.position() as usize,
      control,
    ))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    let sub = match *self {
      Control0::Zero(ref c) => c as &dyn SubControl,
      Control0::One(ref c) => c as &dyn SubControl,
      Control0::Two(ref c) => c as &dyn SubControl,
      Control0::Three(ref c) => c as &dyn SubControl,
      Control0::Four(ref c) => c as &dyn SubControl,
    };

    header.endianness().write_u16(&mut writer, sub.marker())
      .with_context(|_| format!("could not write control subtype marker {}", sub.marker()))?;
    sub.write(header, &mut writer)
      .with_context(|_| format!("could not write control subtype {}", sub.marker()))
      .map_err(Into::into)
  }
}

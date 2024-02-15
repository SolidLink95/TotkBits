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
#[allow(clippy::module_inception)]
pub(crate) mod one;
pub(crate) mod two;
pub(crate) mod three;
pub(crate) mod four;
pub(crate) mod five;
pub(crate) mod six;
pub(crate) mod seven;
pub(crate) mod eight;
pub(crate) mod nine;
pub(crate) mod ten;

use self::{
  zero::Control1_0,
  one::Control1_1,
  two::Control1_2,
  three::Control1_3,
  four::Control1_4,
  five::Control1_5,
  six::Control1_6,
  seven::Control1_7,
  eight::Control1_8,
  nine::Control1_9,
  ten::Control1_10,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control1 {
  Zero(Control1_0),
  One(Control1_1),
  Two(Control1_2),
  Three(Control1_3),
  Four(Control1_4),
  Five(Control1_5),
  Six(Control1_6),
  Seven(Control1_7),
  Eight(Control1_8),
  Nine(Control1_9),
  Ten(Control1_10),
}

impl MainControl for Control1 {
  fn marker(&self) -> u16 {
    1
  }

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)> {
    let mut c = Cursor::new(buf);

    let kind = header.endianness().read_u16(&mut c)?;
    let control = match kind {
      0 => Control1_0::parse(header, &mut c).with_context(|_| "could not parse control subtype 0")?,
      1 => Control1_1::parse(header, &mut c).with_context(|_| "could not parse control subtype 1")?,
      2 => Control1_2::parse(header, &mut c).with_context(|_| "could not parse control subtype 2")?,
      3 => Control1_3::parse(header, &mut c).with_context(|_| "could not parse control subtype 3")?,
      4 => Control1_4::parse(header, &mut c).with_context(|_| "could not parse control subtype 4")?,
      5 => Control1_5::parse(header, &mut c).with_context(|_| "could not parse control subtype 5")?,
      6 => Control1_6::parse(header, &mut c).with_context(|_| "could not parse control subtype 6")?,
      7 => Control1_7::parse(header, &mut c).with_context(|_| "could not parse control subtype 7")?,
      8 => Control1_8::parse(header, &mut c).with_context(|_| "could not parse control subtype 8")?,
      9 => Control1_9::parse(header, &mut c).with_context(|_| "could not parse control subtype 9")?,
      10 => Control1_10::parse(header, &mut c).with_context(|_| "could not parse control subtype 10")?,
      x => failure::bail!("unknown control 1 type: {}", x),
    };

    Ok((
      c.position() as usize,
      control,
    ))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    let sub = match *self {
      Control1::Zero(ref c) => c as &dyn SubControl,
      Control1::One(ref c) => c as &dyn SubControl,
      Control1::Two(ref c) => c as &dyn SubControl,
      Control1::Three(ref c) => c as &dyn SubControl,
      Control1::Four(ref c) => c as &dyn SubControl,
      Control1::Five(ref c) => c as &dyn SubControl,
      Control1::Six(ref c) => c as &dyn SubControl,
      Control1::Seven(ref c) => c as &dyn SubControl,
      Control1::Eight(ref c) => c as &dyn SubControl,
      Control1::Nine(ref c) => c as &dyn SubControl,
      Control1::Ten(ref c) => c as &dyn SubControl,
    };

    header.endianness().write_u16(&mut writer, sub.marker())
      .with_context(|_| format!("could not write marker for subtype {}", sub.marker()))?;
    sub.write(header, &mut writer)
      .with_context(|_| format!("could not write subtype {}", sub.marker()))
      .map_err(Into::into)
  }
}

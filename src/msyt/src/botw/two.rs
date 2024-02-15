use crate::{
  Result,
  botw::{Control, MainControl, RawControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

pub(crate) mod one_field;
pub(crate) mod variable;

use self::{
  one_field::Control2OneField,
  variable::Control2Variable,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control2 {
  OneField(u16, Control2OneField),
  Variable(u16, Control2Variable),
}

impl MainControl for Control2 {
  fn marker(&self) -> u16 {
    2
  }

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)> {
    let mut c = Cursor::new(buf);

    let kind = header.endianness().read_u16(&mut c)?;
    let control = match kind {
      3 => Control2::OneField(kind, Control2OneField::parse(header, &mut c).with_context(|_| "could not parse control subtype 3")?),
      4 => Control2::OneField(kind, Control2OneField::parse(header, &mut c).with_context(|_| "could not parse control subtype 4")?),
      7 => Control2::OneField(kind, Control2OneField::parse(header, &mut c).with_context(|_| "could not parse control subtype 7")?),
      8 => Control2::OneField(kind, Control2OneField::parse(header, &mut c).with_context(|_| "could not parse control subtype 8")?),
      10 => Control2::OneField(kind, Control2OneField::parse(header, &mut c).with_context(|_| "could not parse control subtype 10")?),
      13 => Control2::OneField(kind, Control2OneField::parse(header, &mut c).with_context(|_| "could not parse control subtype 13")?),
      1 | 2 | 9 | 11 | 12 | 14 | 15 | 16 | 17 | 18 | 19 => {
        let v = Control2Variable::parse(header, &mut c).with_context(|_| "could not parse control subtype 18")?;
        if v.field_3 == 0 {
          return Ok((
            c.position() as usize,
            Control::Variable {
              variable_kind: kind,
              name: v.string
            },
          ));
        }
        Control2::Variable(kind, v)
      },
      x => failure::bail!("unknown control 2 type: {}", x),
    };

    Ok((
      c.position() as usize,
      Control::Raw(RawControl::Two(control)),
    ))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    match *self {
      Control2::OneField(marker, ref control) => {
        header.endianness().write_u16(&mut writer, marker)
          .with_context(|_| format!("could not write marker for subtype {}", marker))?;
        control.write(header, &mut writer)
          .with_context(|_| format!("could not write subtype {}", marker))
          .map_err(Into::into)
      },
      Control2::Variable(marker, ref control) => {
        header.endianness().write_u16(&mut writer, marker)
          .with_context(|_| format!("could not write marker for subtype {}", marker))?;
        control.write(header, &mut writer)
          .with_context(|_| format!("could not write subtype {}", marker))
          .map_err(Into::into)
      },
    }
  }
}

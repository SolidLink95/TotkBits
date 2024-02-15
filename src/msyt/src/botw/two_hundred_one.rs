use crate::{
  Result,
  botw::{Control, MainControl, Localisation, RawControl},
};

use byteordered::Endian;

use failure::ResultExt;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

pub(crate) mod dynamic;
pub(crate) mod one_field;
pub(crate) mod localisation;

use self::{
  dynamic::Control201Dynamic,
  one_field::Control201OneField,
  localisation::Control201Localisation,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control201 {
  Dynamic(u16, Control201Dynamic),
  OneField(u16, Control201OneField),
  Localisation(Localisation, Control201Localisation),
}

impl MainControl for Control201 {
  fn marker(&self) -> u16 {
    201
  }

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)> {
    let mut c = Cursor::new(buf);

    let kind = header.endianness().read_u16(&mut c)?;
    let control = match kind {
      0 => Control201::Dynamic(kind, Control201Dynamic::parse(header, &mut c).with_context(|_| "could not parse control subtype dynamic")?),
      1 | 2 | 3 | 4 => Control201::OneField(kind, Control201OneField::parse(header, &mut c).with_context(|_| "could not parse control two fields")?),
      5 | 6 | 7 | 8 => {
        let localisation_kind = Localisation::from_u16(kind);
        let sub = Control201Localisation::parse(header, &mut c).with_context(|_| "could not parse control subtype localisation")?;
        return Ok((
          c.position() as usize,
          Control::Localisation {
            localisation_kind,
            options: sub.strings,
          },
        ));
      },
      x => failure::bail!("unknown control 201 type: {}", x),
    };

    Ok((
      c.position() as usize,
      Control::Raw(RawControl::TwoHundredOne(control)),
    ))
  }

  fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    match *self {
      Control201::Dynamic(marker, ref control) => {
        header.endianness().write_u16(&mut writer, marker)
          .with_context(|_| format!("could not write marker for subtype {}", marker))?;
        control.write(header, &mut writer)
          .with_context(|_| format!("could not write subtype {}", marker))
          .map_err(Into::into)
      },
      Control201::OneField(marker, ref control) => {
        header.endianness().write_u16(&mut writer, marker)
          .with_context(|_| format!("could not write marker for subtype {}", marker))?;
        control.write(header, &mut writer)
          .with_context(|_| format!("could not write subtype {}", marker))
          .map_err(Into::into)
      },
      Control201::Localisation(kind, ref c) => {
        header.endianness().write_u16(&mut writer, kind.as_u16())
          .with_context(|_| format!("could not write control subtype marker {}", kind.as_u16()))?;
        c.write(header, &mut writer)
          .with_context(|_| format!("could not write control subtype {}", kind.as_u16()))
          .map_err(Into::into)
      },
    }
  }
}

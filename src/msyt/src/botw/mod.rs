use crate::{
  Result,
  model::Content,
};
use byteordered::Endian;
use failure::ResultExt;
use msbt::Header;
use serde_derive::{Deserialize, Serialize};
use std::{
  boxed::Box,
  io::{Cursor, Write},
};

pub mod zero;
pub mod one;
pub mod two;
pub mod three;
pub mod four;
pub mod five;
pub mod two_hundred_one;

pub fn parse_controls(header: &Header, s: &[u8]) -> Result<Vec<Content>> {
  let mut parts = Vec::new();
  let mut last_was_marker = false;
  let mut skip = 0;
  let mut text_index = None;

  for i in 0..s.len() {
    if skip > 0 {
      skip -= 1;
      continue;
    }
    if i + 1 < s.len() {
      let chunk = &s[i..=i + 1];
      let u = header.endianness().read_u16(chunk).with_context(|_| "could not control sequence marker")?;
      skip += 1;
      if last_was_marker {
        let body = &s[i + 2..];
        let (read, ctl) = match u {
          0x00 => self::zero::Control0::parse(header, body).with_context(|_| "could not parse control sequence 0")?,
          0x01 => self::one::Control1::parse(header, body).with_context(|_| "could not parse control sequence 1")?,
          0x02 => self::two::Control2::parse(header, body).with_context(|_| "could not parse control sequence 2")?,
          0x03 => self::three::Control3::parse(header, body).with_context(|_| "could not parse control sequence 3")?,
          0x04 => self::four::Control4::parse(header, body).with_context(|_| "could not parse control sequence 4")?,
          0x05 => self::five::Control5::parse(header, body).with_context(|_| "could not parse control sequence 5")?,
          0xc9 => self::two_hundred_one::Control201::parse(header, body).with_context(|_| "could not parse control sequence 201")?,
          x => failure::bail!("unknown control sequence: {}", x),
        };
        let part = Content::Control(ctl);
        skip = read + 1;
        parts.push(part);
      }
      if text_index.is_none() && !last_was_marker && u != 0x0e {
        text_index = Some(i);
      }
      if u == 0x0e {
        last_was_marker = true;
        if let Some(text_index) = text_index {
          let bytes: Vec<u16> = s[text_index..i]
            .chunks(2)
            .map(|x| header.endianness().read_u16(x)
              .with_context(|_| "could not read bytes")
              .map_err(Into::into))
            .collect::<Result<_>>()?;
          let string = String::from_utf16(&bytes).with_context(|_| "could not parse utf-16 string")?;
          parts.push(Content::Text(string));
        }
        text_index = None;
      } else {
        last_was_marker = false;
      }
    }

  }

  if let Some(text_index) = text_index {
    let bytes: Vec<u16> = s[text_index..]
      .chunks(2)
      .map(|x| header.endianness().read_u16(x)
        .with_context(|_| "could not read bytes")
        .map_err(Into::into))
      .collect::<Result<_>>()?;
    let from = if bytes[bytes.len() - 1] == 0 {
      &bytes[..bytes.len() - 1]
    } else {
      &bytes
    };
    let string = String::from_utf16(&from).with_context(|_| "could not parse utf-16 string")?;
    if !string.is_empty() {
      parts.push(Content::Text(string));
    }
  }

  Ok(parts)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Control {
  Raw(RawControl),

  SetColour { colour: Colour },
  ResetColour,
  Pause(PauseKind),
  Icon { icon: Icon },
  Variable {
    variable_kind: u16,
    name: String,
  },
  Choice {
    choice_labels: Vec<u16>,
    selected_index: u8,
    cancel_index: u8,
    unknown: u16,
  },
  SingleChoice {
    label: u16,
  },
  Sound { unknown: Vec<u8> },
  Sound2 { unknown: Vec<u8> },
  Animation { name: String },
  TextSize { percent: u16 },
  AutoAdvance { frames: u32 },
  Localisation {
    localisation_kind: Localisation,
    options: Vec<String>,
  },
  Font { font_kind: Font },
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Font {
  Normal,
  Hylian,
}

impl Font {
  pub fn as_u16(self) -> u16 {
    match self {
      Font::Normal => 0xFFFF,
      Font::Hylian => 0x0000,
    }
  }

  pub fn from_u16(u: u16) -> Option<Self> {
    let f = match u {
      0xFFFF => Font::Normal,
      0x0000 => Font::Hylian,
      _ => return None,
    };

    Some(f)
  }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Localisation {
  Gender,
  Plural,
  Unknown(u16),
}

impl Localisation {
  pub fn as_u16(self) -> u16 {
    match self {
      Localisation::Gender => 5,
      Localisation::Plural => 6,
      Localisation::Unknown(x) => x,
    }
  }

  pub fn from_u16(u: u16) -> Self {
    match u {
      5 => Localisation::Gender,
      6 => Localisation::Plural,
      x => Localisation::Unknown(x),
    }
  }
}

enum MainControlRef<'a> {
  Borrowed(&'a dyn MainControl),
  Owned(Box<dyn MainControl>),
}

impl<'a> MainControl for MainControlRef<'a> {
  fn marker(&self) -> u16 {
    match *self {
      MainControlRef::Borrowed(ref b) => b.marker(),
      MainControlRef::Owned(ref b) => b.marker(),
    }
  }

  fn parse(_header: &Header, _buf: &[u8]) -> Result<(usize, Control)>
    where Self: Sized
  {
    unimplemented!()
  }

  fn write(&self, header: &Header, writer: &mut dyn Write) -> Result<()> {
    match *self {
      MainControlRef::Borrowed(ref b) => b.write(header, writer),
      MainControlRef::Owned(ref b) => b.write(header, writer),
    }
  }
}

impl Control {
  fn as_main_control(&self) -> Result<MainControlRef> {
    let b: Box<dyn MainControl> = match *self {
      Control::Raw(ref raw) => return Ok(MainControlRef::Borrowed(raw.as_main_control())),

      Control::SetColour { colour } => Box::new(self::zero::Control0::Three(self::zero::three::Control0_3 {
        field_1: 2,
        field_2: colour.as_u16(),
      })),
      Control::ResetColour => Box::new(self::zero::Control0::Three(self::zero::three::Control0_3 {
        field_1: 2,
        field_2: 65535,
      })),
      Control::Pause(PauseKind::Length(length)) => Box::new(self::five::Control5 {
        field_1: length.as_u16(),
        field_2: 0,
      }),
      Control::Pause(PauseKind::Frames(frames)) => Box::new(self::one::Control1::Zero(self::one::zero::Control1_0 {
        field_1: 4,
        field_2: frames,
      })),
      Control::Icon { icon } => Box::new(self::one::Control1::Seven(self::one::seven::Control1_7 {
        field_1: 2,
        field_2: [
          icon.as_u8(),
          205,
        ],
      })),
      Control::Variable { variable_kind, ref name } => Box::new(self::two::Control2::Variable(variable_kind, self::two::variable::Control2Variable {
        field_1: name.len() as u16 * 2 + 4,
        string: name.clone(),
        field_3: 0,
      })),
      Control::Choice { ref choice_labels, selected_index, cancel_index, unknown } => {
        match choice_labels.len() + 2 {
          4 => Box::new(self::one::Control1::Four(self::one::four::Control1_4 {
            field_1: unknown,
            field_2: choice_labels[0],
            field_3: choice_labels[1],
            field_4: [selected_index, cancel_index],
          })),
          5 => Box::new(self::one::Control1::Five(self::one::five::Control1_5 {
            field_1: unknown,
            field_2: choice_labels[0],
            field_3: choice_labels[1],
            field_4: choice_labels[2],
            field_5: [selected_index, cancel_index],
          })),
          6 => Box::new(self::one::Control1::Six(self::one::six::Control1_6 {
            field_1: unknown,
            field_2: choice_labels[0],
            field_3: choice_labels[1],
            field_4: choice_labels[2],
            field_5: choice_labels[3],
            field_6: [selected_index, cancel_index],
          })),
          _ => failure::bail!("invalid choice: only 2 to 4 options allowed but got {}", choice_labels.len()),
        }
      },
      Control::SingleChoice { label } => Box::new(self::one::Control1::Ten(self::one::ten::Control1_10 {
        field_1: 4,
        field_2: label,
        field_3: [1, 205],
      })),
      Control::Sound { ref unknown } => Box::new(self::three::Control3 {
        field_1: 1,
        field_2: unknown.clone(),
      }),
      Control::Sound2 { ref unknown } => Box::new(self::four::Control4::One(self::four::one::Control4_1 {
        field_1: unknown.clone(),
      })),
      Control::Animation { ref name } => Box::new(self::four::Control4::Two(self::four::two::Control4_2 {
        field_1: name.len() as u16 * 2 + 2,
        string: name.clone(),
      })),
      Control::TextSize { percent } => Box::new(self::zero::Control0::Two(self::zero::two::Control0_2 {
        field_1: 2,
        field_2: percent,
      })),
      Control::AutoAdvance { frames } => Box::new(self::one::Control1::Three(self::one::three::Control1_3 {
        field_1: 4,
        field_2: frames,
      })),
      Control::Localisation { localisation_kind, ref options } => Box::new(self::two_hundred_one::Control201::Localisation(localisation_kind, self::two_hundred_one::localisation::Control201Localisation {
        strings: options.clone(),
      })),
      Control::Font { font_kind } => Box::new(self::zero::Control0::One(self::zero::one::Control0_1 {
        field_1: 2,
        field_2: font_kind.as_u16(),
      })),
    };

    Ok(MainControlRef::Owned(b))
  }

  pub fn write(&self, header: &Header, mut writer: &mut dyn Write) -> Result<()> {
    header.endianness().write_u16(&mut writer, 0x0e).with_context(|_| "could not write control marker")?;
    let control = self.as_main_control()?;
    header.endianness().write_u16(&mut writer, control.marker())
      .with_context(|_| format!("could not write control marker for type {}", control.marker()))?;
    control.write(header, &mut writer)
      .with_context(|_| format!("could not write control type {}", control.marker()))
      .map_err(Into::into)
  }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RawControl {
  Zero(self::zero::Control0),
  One(self::one::Control1),
  Two(self::two::Control2),
  Three(self::three::Control3),
  Four(self::four::Control4),
  Five(self::five::Control5),
  TwoHundredOne(self::two_hundred_one::Control201),
}

impl RawControl {
  fn as_main_control(&self) -> &dyn MainControl {
    match *self {
      RawControl::Zero(ref c) => c as &dyn MainControl,
      RawControl::One(ref c) => c as &dyn MainControl,
      RawControl::Two(ref c) => c as &dyn MainControl,
      RawControl::Three(ref c) => c as &dyn MainControl,
      RawControl::Four(ref c) => c as &dyn MainControl,
      RawControl::Five(ref c) => c as &dyn MainControl,
      RawControl::TwoHundredOne(ref c) => c as &dyn MainControl,
    }
  }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
  Red,
  LightGreen1,
  Blue,
  Grey,
  LightGreen4,
  Orange,
  LightGrey,
}

impl Colour {
  pub(crate) fn from_u16(c: u16) -> Option<Colour> {
    let c = match c {
      0 => Colour::Red,
      1 => Colour::LightGreen1,
      2 => Colour::Blue,
      3 => Colour::Grey,
      4 => Colour::LightGreen4,
      5 => Colour::Orange,
      6 => Colour::LightGrey,
      _ => return None,
    };
    Some(c)
  }

  pub(crate) fn as_u16(self) -> u16 {
    match self {
      Colour::Red => 0,
      Colour::LightGreen1 => 1,
      Colour::Blue => 2,
      Colour::Grey => 3,
      Colour::LightGreen4 => 4,
      Colour::Orange => 5,
      Colour::LightGrey => 6,
    }
  }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PauseKind {
  Frames(u32),
  Length(PauseLength),
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PauseLength {
  Short,
  Long,
  Longer,
}

impl PauseLength {
  pub(crate) fn from_u16(u: u16) -> Option<Self> {
    let p = match u {
      0 => PauseLength::Short,
      1 => PauseLength::Long,
      2 => PauseLength::Longer,
      _ => return None,
    };

    Some(p)
  }

  pub(crate) fn as_u16(self) -> u16 {
    match self {
      PauseLength::Short => 0,
      PauseLength::Long => 1,
      PauseLength::Longer => 2,
    }
  }
}
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
  Zl(u8),
  L,
  R,
  Y,
  X(u8),
  A(u8),
  B,
  Plus,
  Minus,
  DPadDown,
  DPadLeft,
  DPadRight,
  DPadUp,
  RStickHorizontal,
  RStickPress,
  RStickVertical,
  LStickBack,
  LStickForward,
  LStickLeft,
  LStickPress,
  LStickRight,
  Gamepad,

  LeftArrow,
  RightArrow,
  UpArrow,

  Unknown(u8),
}

impl Icon {
  pub fn from_u8(u: u8) -> Self {
    match u {
      0 => Icon::LStickForward,
      1 => Icon::LStickBack,
      2 => Icon::LStickLeft,
      3 => Icon::LStickRight,
      4 => Icon::RStickVertical,
      5 => Icon::RStickHorizontal,
      6 => Icon::DPadUp,
      7 => Icon::DPadDown,
      8 => Icon::DPadLeft,
      9 => Icon::DPadRight,
      x @ 10 | x @ 11 => Icon::A(x),
      x @ 12 | x @ 37 | x @ 38 => Icon::X(x),
      13 => Icon::Y,
      x @ 14 | x @ 15 => Icon::Zl(x),
      17 => Icon::B,
      20 => Icon::L,
      21 => Icon::R,
      23 => Icon::Plus,
      24 => Icon::Minus,
      25 => Icon::RightArrow,
      26 => Icon::LeftArrow,
      27 => Icon::UpArrow,
      33 => Icon::LStickPress,
      34 => Icon::RStickPress,
      36 => Icon::Gamepad,

      x => Icon::Unknown(x),
    }
  }

  pub fn as_u8(self) -> u8 {
    match self {
      Icon::LStickForward => 0,
      Icon::LStickBack => 1,
      Icon::LStickLeft => 2,
      Icon::LStickRight => 3,
      Icon::RStickVertical => 4,
      Icon::RStickHorizontal => 5,
      Icon::DPadUp => 6,
      Icon::DPadDown => 7,
      Icon::DPadLeft => 8,
      Icon::DPadRight => 9,
      Icon::A(x) => x,
      Icon::X(x) => x,
      Icon::Y => 13,
      Icon::Zl(x) => x,
      Icon::B => 17,
      Icon::L => 20,
      Icon::R => 21,
      Icon::Plus => 23,
      Icon::Minus => 24,
      Icon::RightArrow => 25,
      Icon::LeftArrow => 26,
      Icon::UpArrow => 27,
      Icon::LStickPress => 33,
      Icon::RStickPress => 34,
      Icon::Gamepad => 36,

      Icon::Unknown(u) => u,
    }
  }
}

pub(crate) trait MainControl {
  fn marker(&self) -> u16;

  fn parse(header: &Header, buf: &[u8]) -> Result<(usize, Control)>
    where Self: Sized;

  fn write(&self, header: &Header, writer: &mut dyn Write) -> Result<()>;
}

pub(crate) trait SubControl {
  fn marker(&self) -> u16;

  fn parse(header: &Header, reader: &mut Cursor<&[u8]>) -> Result<Control>
    where Self: Sized;

  fn write(&self, header: &Header, writer: &mut dyn Write) -> Result<()>;
}

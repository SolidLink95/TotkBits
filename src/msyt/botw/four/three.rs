use super::Control4;
use crate::{
    botw::{Control, RawControl, SubControl},
    Result,
};

use byteordered::Endian;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Control4_3 {
    field_1: u16,
}

impl SubControl for Control4_3 {
    fn marker(&self) -> u16 {
        3
    }

    fn parse(header: &Header, reader: &mut Cursor<&[u8]>) -> Result<Control> {
        Ok(Control::Raw(RawControl::Four(Control4::Three(
            Control4_3 {
                field_1: header.endianness().read_u16(reader)?,
            },
        ))))
    }

    fn write(&self, header: &Header, writer: &mut dyn Write) -> Result<()> {
        header.endianness().write_u16(writer, self.field_1)?;

        Ok(())
    }
}

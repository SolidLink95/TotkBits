use super::Control1;
use crate::{
    botw::{Control, RawControl, SubControl},
    Result,
};

use byteordered::Endian;

use anyhow::Context;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Control1_2 {
    field_1: u16,
}

impl SubControl for Control1_2 {
    fn marker(&self) -> u16 {
        2
    }

    fn parse(header: &Header, reader: &mut Cursor<&[u8]>) -> Result<Control> {
        Ok(Control::Raw(RawControl::One(Control1::Two(Control1_2 {
            field_1: header
                .endianness()
                .read_u16(reader)
                .with_context(|| "could not read field_1")?,
        }))))
    }

    fn write(&self, header: &Header, writer: &mut dyn Write) -> Result<()> {
        header
            .endianness()
            .write_u16(writer, self.field_1)
            .with_context(|| "could not write field_1")?;

        Ok(())
    }
}

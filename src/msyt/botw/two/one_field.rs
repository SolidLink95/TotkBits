use crate::Result;

use byteordered::Endian;

use anyhow::Context;

use msbt::Header;

use serde_derive::{Deserialize, Serialize};

use std::io::{Cursor, Write};

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Control2OneField {
    field_1: u16,
}

impl Control2OneField {
    pub(crate) fn parse(header: &Header, reader: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Control2OneField {
            field_1: header
                .endianness()
                .read_u16(reader)
                .with_context(|| "could not read field_1")?,
        })
    }

    pub(crate) fn write(&self, header: &Header, writer: &mut dyn Write) -> Result<()> {
        header
            .endianness()
            .write_u16(writer, self.field_1)
            .with_context(|| "could not write field_1")?;

        Ok(())
    }
}

use std::io::{self, ErrorKind};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Endian {
    #[default]
    Little,
    Big,
}

impl Endian {
    pub fn u16_from_bytes(self, bytes: [u8; 2]) -> u16 {
        match self {
            Endian::Little => u16::from_le_bytes(bytes),
            Endian::Big => u16::from_be_bytes(bytes),
        }
    }
    pub fn u32_from_bytes(self, bytes: [u8; 4]) -> u32 {
        match self {
            Endian::Little => u32::from_le_bytes(bytes),
            Endian::Big => u32::from_be_bytes(bytes),
        }
    }
    pub fn u64_from_bytes(self, bytes: [u8; 8]) -> u64 {
        match self {
            Endian::Little => u64::from_le_bytes(bytes),
            Endian::Big => u64::from_be_bytes(bytes),
        }
    }
    pub fn f32_from_bytes(self, bytes: [u8; 4]) -> f32 {
        f32::from_bits(self.u32_from_bytes(bytes))
    }
    pub fn u16_to_bytes(self, value: u16) -> [u8; 2] {
        match self {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        }
    }
    pub fn u32_to_bytes(self, value: u32) -> [u8; 4] {
        match self {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        }
    }
    pub fn u64_to_bytes(self, value: u64) -> [u8; 8] {
        match self {
            Endian::Little => value.to_le_bytes(),
            Endian::Big => value.to_be_bytes(),
        }
    }
    pub fn f32_to_bytes(self, value: f32) -> [u8; 4] {
        self.u32_to_bytes(value.to_bits())
    }
}

pub struct BinaryReader<'a> {
    data: &'a [u8],
    position: usize,
    endian: Endian,
}

impl<'a> BinaryReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            endian: Endian::Little,
        }
    }

    pub fn with_endian(data: &'a [u8], endian: Endian) -> Self {
        Self {
            data,
            position: 0,
            endian,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    pub fn seek(&mut self, position: usize) -> io::Result<()> {
        if position > self.data.len() {
            return Err(self.error("seek exceeds input"));
        }
        self.position = position;
        Ok(())
    }

    pub fn skip(&mut self, count: usize) -> io::Result<()> {
        self.seek(
            self.position
                .checked_add(count)
                .ok_or_else(|| self.error("offset overflow"))?,
        )
    }

    pub fn align(&mut self, alignment: usize) -> io::Result<()> {
        if alignment == 0 {
            return Err(self.error("alignment cannot be zero"));
        }
        let aligned = self
            .position
            .checked_add(alignment - 1)
            .ok_or_else(|| self.error("alignment overflow"))?
            / alignment
            * alignment;
        self.seek(aligned)
    }

    pub fn read_bytes(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| self.error("read overflow"))?;
        if end > self.data.len() {
            return Err(self.error("unexpected end of input"));
        }
        let value = &self.data[self.position..end];
        self.position = end;
        Ok(value)
    }

    pub fn slice(&self, start: usize, end: usize) -> io::Result<&'a [u8]> {
        if start > end {
            return Err(self.error_at(start, "slice start exceeds end"));
        }
        self.data
            .get(start..end)
            .ok_or_else(|| self.error_at(start, "slice exceeds input"))
    }

    pub fn read_bytes_at(&self, offset: usize, count: usize) -> io::Result<&'a [u8]> {
        let end = offset
            .checked_add(count)
            .ok_or_else(|| self.error_at(offset, "read overflow"))?;
        self.slice(offset, end)
    }

    pub fn read_array_at<const N: usize>(&self, offset: usize) -> io::Result<[u8; N]> {
        let mut value = [0; N];
        value.copy_from_slice(self.read_bytes_at(offset, N)?);
        Ok(value)
    }

    pub fn read_u8_at(&self, offset: usize) -> io::Result<u8> {
        Ok(self.read_array_at::<1>(offset)?[0])
    }

    pub fn read_i8_at(&self, offset: usize) -> io::Result<i8> {
        Ok(self.read_u8_at(offset)? as i8)
    }

    pub fn read_u16_at(&self, offset: usize) -> io::Result<u16> {
        Ok(self.endian.u16_from_bytes(self.read_array_at(offset)?))
    }

    pub fn read_i16_at(&self, offset: usize) -> io::Result<i16> {
        Ok(self.read_u16_at(offset)? as i16)
    }

    pub fn read_u32_at(&self, offset: usize) -> io::Result<u32> {
        Ok(self.endian.u32_from_bytes(self.read_array_at(offset)?))
    }

    pub fn read_i32_at(&self, offset: usize) -> io::Result<i32> {
        Ok(self.read_u32_at(offset)? as i32)
    }

    pub fn read_u64_at(&self, offset: usize) -> io::Result<u64> {
        Ok(self.endian.u64_from_bytes(self.read_array_at(offset)?))
    }

    pub fn read_i64_at(&self, offset: usize) -> io::Result<i64> {
        Ok(self.read_u64_at(offset)? as i64)
    }

    pub fn read_f32_at(&self, offset: usize) -> io::Result<f32> {
        Ok(f32::from_bits(self.read_u32_at(offset)?))
    }

    pub fn read_f64_at(&self, offset: usize) -> io::Result<f64> {
        Ok(f64::from_bits(self.read_u64_at(offset)?))
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }
    pub fn read_i8(&mut self) -> io::Result<i8> {
        Ok(self.read_u8()? as i8)
    }
    pub fn read_u16(&mut self) -> io::Result<u16> {
        let mut b = [0; 2];
        b.copy_from_slice(self.read_bytes(2)?);
        Ok(self.endian.u16_from_bytes(b))
    }
    pub fn read_i16(&mut self) -> io::Result<i16> {
        Ok(self.read_u16()? as i16)
    }
    pub fn read_u32(&mut self) -> io::Result<u32> {
        let mut b = [0; 4];
        b.copy_from_slice(self.read_bytes(4)?);
        Ok(self.endian.u32_from_bytes(b))
    }
    pub fn read_i32(&mut self) -> io::Result<i32> {
        Ok(self.read_u32()? as i32)
    }
    pub fn read_u64(&mut self) -> io::Result<u64> {
        let mut b = [0; 8];
        b.copy_from_slice(self.read_bytes(8)?);
        Ok(self.endian.u64_from_bytes(b))
    }
    pub fn read_i64(&mut self) -> io::Result<i64> {
        Ok(self.read_u64()? as i64)
    }
    pub fn read_f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_bits(self.read_u32()?))
    }
    pub fn read_f64(&mut self) -> io::Result<f64> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    pub fn read_c_string_at(&self, offset: usize) -> io::Result<String> {
        // An offset at the end of an empty pool represents an empty string in
        // ASB and several related Nintendo formats.
        if offset == self.data.len() {
            return Ok(String::new());
        }
        let tail = self.data.get(offset..).ok_or_else(|| {
            self.error(&format!(
                "string offset {offset:#x} exceeds input length {:#x}",
                self.data.len()
            ))
        })?;
        let end = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| self.error("unterminated string"))?;
        String::from_utf8(tail[..end].to_vec())
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }

    fn error(&self, message: &str) -> io::Error {
        self.error_at(self.position, message)
    }

    fn error_at(&self, offset: usize, message: &str) -> io::Error {
        io::Error::new(
            ErrorKind::UnexpectedEof,
            format!("{message} at {offset:#x}"),
        )
    }
}

#[derive(Default)]
pub struct BinaryWriter {
    data: Vec<u8>,
    position: usize,
    endian: Endian,
}

impl BinaryWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_endian(endian: Endian) -> Self {
        Self {
            data: Vec::new(),
            position: 0,
            endian,
        }
    }
    pub fn from_vec(data: Vec<u8>, endian: Endian) -> Self {
        Self {
            data,
            position: 0,
            endian,
        }
    }
    /// Wraps existing bytes with the cursor at the end, so subsequent writes
    /// append instead of overwriting.
    pub fn appending(data: Vec<u8>, endian: Endian) -> Self {
        let position = data.len();
        Self {
            data,
            position,
            endian,
        }
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn endian(&self) -> Endian {
        self.endian
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }
    pub fn seek(&mut self, position: usize) {
        self.position = position;
        if position > self.data.len() {
            self.data.resize(position, 0);
        }
    }
    pub fn truncate(&mut self, position: usize) {
        self.data.truncate(position);
        self.position = self.position.min(position);
    }
    pub fn align(&mut self, alignment: usize) -> io::Result<()> {
        if alignment == 0 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "alignment cannot be zero",
            ));
        }
        let position = (self.position + alignment - 1) / alignment * alignment;
        self.seek(position);
        Ok(())
    }
    pub fn write_bytes(&mut self, value: &[u8]) {
        let end = self.position + value.len();
        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[self.position..end].copy_from_slice(value);
        self.position = end;
    }
    pub fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }
    pub fn write_i8(&mut self, v: i8) {
        self.write_u8(v as u8);
    }
    pub fn write_u16(&mut self, v: u16) {
        self.write_bytes(&self.endian.u16_to_bytes(v));
    }
    pub fn write_i16(&mut self, v: i16) {
        self.write_u16(v as u16);
    }
    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&self.endian.u32_to_bytes(v));
    }
    pub fn write_i32(&mut self, v: i32) {
        self.write_u32(v as u32);
    }
    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&self.endian.u64_to_bytes(v));
    }
    pub fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }
    pub fn write_f32(&mut self, v: f32) {
        self.write_u32(v.to_bits());
    }
    pub fn write_f64(&mut self, v: f64) {
        self.write_u64(v.to_bits());
    }
    pub fn write_c_string(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
        self.write_u8(0);
    }
    pub fn write_u8_at(&mut self, offset: usize, value: u8) {
        let position = self.position;
        self.seek(offset);
        self.write_u8(value);
        self.seek(position);
    }
    pub fn write_u16_at(&mut self, offset: usize, value: u16) {
        let position = self.position;
        self.seek(offset);
        self.write_u16(value);
        self.seek(position);
    }
    pub fn write_u32_at(&mut self, offset: usize, value: u32) {
        let position = self.position;
        self.seek(offset);
        self.write_u32(value);
        self.seek(position);
    }
    pub fn write_u64_at(&mut self, offset: usize, value: u64) {
        let position = self.position;
        self.seek(offset);
        self.write_u64(value);
        self.seek(position);
    }
    pub fn write_f32_at(&mut self, offset: usize, value: f32) {
        let position = self.position;
        self.seek(offset);
        self.write_f32(value);
        self.seek(position);
    }
}

/// Bounds-checked in-place writer over an existing byte slice. Unlike
/// [`BinaryWriter`] it never grows the buffer: every write must fit inside
/// the slice it was created with, which is the contract for patching fields
/// of an already laid-out file.
pub struct BinaryPatcher<'a> {
    data: &'a mut [u8],
    endian: Endian,
}

impl<'a> BinaryPatcher<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self {
            data,
            endian: Endian::Little,
        }
    }
    pub fn with_endian(data: &'a mut [u8], endian: Endian) -> Self {
        Self { data, endian }
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn write_bytes_at(&mut self, offset: usize, value: &[u8]) -> io::Result<()> {
        let end = offset
            .checked_add(value.len())
            .ok_or_else(|| Self::error(offset, "write overflow"))?;
        self.data
            .get_mut(offset..end)
            .ok_or_else(|| Self::error(offset, "write exceeds buffer"))?
            .copy_from_slice(value);
        Ok(())
    }
    pub fn write_u8_at(&mut self, offset: usize, value: u8) -> io::Result<()> {
        self.write_bytes_at(offset, &[value])
    }
    pub fn write_i8_at(&mut self, offset: usize, value: i8) -> io::Result<()> {
        self.write_u8_at(offset, value as u8)
    }
    pub fn write_u16_at(&mut self, offset: usize, value: u16) -> io::Result<()> {
        self.write_bytes_at(offset, &self.endian.u16_to_bytes(value))
    }
    pub fn write_i16_at(&mut self, offset: usize, value: i16) -> io::Result<()> {
        self.write_u16_at(offset, value as u16)
    }
    pub fn write_u32_at(&mut self, offset: usize, value: u32) -> io::Result<()> {
        self.write_bytes_at(offset, &self.endian.u32_to_bytes(value))
    }
    pub fn write_i32_at(&mut self, offset: usize, value: i32) -> io::Result<()> {
        self.write_u32_at(offset, value as u32)
    }
    pub fn write_u64_at(&mut self, offset: usize, value: u64) -> io::Result<()> {
        self.write_bytes_at(offset, &self.endian.u64_to_bytes(value))
    }
    pub fn write_i64_at(&mut self, offset: usize, value: i64) -> io::Result<()> {
        self.write_u64_at(offset, value as u64)
    }
    pub fn write_f32_at(&mut self, offset: usize, value: f32) -> io::Result<()> {
        self.write_u32_at(offset, value.to_bits())
    }
    pub fn write_f64_at(&mut self, offset: usize, value: f64) -> io::Result<()> {
        self.write_u64_at(offset, value.to_bits())
    }

    fn error(offset: usize, message: &str) -> io::Error {
        io::Error::new(
            ErrorKind::UnexpectedEof,
            format!("{message} at {offset:#x}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_writer_round_trip_primitives_and_seek() {
        let mut writer = BinaryWriter::new();
        writer.write_u8(0x7f);
        writer.write_i16(-1234);
        writer.write_u32(0xdead_beef);
        writer.write_f32(12.5);
        writer.write_c_string("TotkBits");
        writer.seek(1);
        writer.write_i16(-1234);
        let bytes = writer.into_inner();

        let mut reader = BinaryReader::new(&bytes);
        assert_eq!(reader.read_u8().unwrap(), 0x7f);
        assert_eq!(reader.read_i16().unwrap(), -1234);
        assert_eq!(reader.read_u32().unwrap(), 0xdead_beef);
        assert_eq!(reader.read_f32().unwrap(), 12.5);
        assert_eq!(
            reader.read_c_string_at(reader.position()).unwrap(),
            "TotkBits"
        );
    }

    #[test]
    fn big_endian_is_supported() {
        let mut writer = BinaryWriter::with_endian(Endian::Big);
        writer.write_u32(0x0102_0304);
        let bytes = writer.into_inner();
        assert_eq!(bytes, [1, 2, 3, 4]);
        assert_eq!(
            BinaryReader::with_endian(&bytes, Endian::Big)
                .read_u32()
                .unwrap(),
            0x0102_0304
        );
    }

    #[test]
    fn string_at_end_of_input_is_empty() {
        let reader = BinaryReader::new(&[]);
        assert_eq!(reader.read_c_string_at(0).unwrap(), "");
    }

    #[test]
    fn patcher_writes_in_place_and_rejects_out_of_range() {
        let mut bytes = [0u8; 8];
        let mut patcher = BinaryPatcher::with_endian(&mut bytes, Endian::Big);
        patcher.write_u16_at(1, 0x0102).unwrap();
        patcher.write_f32_at(4, 1.0).unwrap();
        assert!(patcher.write_u32_at(6, 1).is_err());
        assert!(patcher.write_bytes_at(usize::MAX, &[1]).is_err());
        assert_eq!(bytes, [0, 1, 2, 0, 0x3f, 0x80, 0, 0]);
        let reader = BinaryReader::with_endian(&bytes, Endian::Big);
        assert_eq!(reader.read_i16_at(1).unwrap(), 0x0102);
        assert_eq!(reader.read_f32_at(4).unwrap(), 1.0);
    }

    #[test]
    fn appending_writer_keeps_existing_bytes() {
        let mut writer = BinaryWriter::appending(vec![1, 2], Endian::Little);
        writer.write_u16(0x0403);
        assert_eq!(writer.len(), 4);
        assert_eq!(writer.into_inner(), [1, 2, 3, 4]);
    }

    #[test]
    fn random_access_reads_are_checked_and_preserve_position() {
        let reader = BinaryReader::new(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(reader.read_u16_at(1).unwrap(), 0x0302);
        assert_eq!(reader.read_u32_at(4).unwrap(), 0x0807_0605);
        assert_eq!(reader.position(), 0);
        assert!(reader.read_u64_at(1).is_err());
        assert!(reader.read_bytes_at(usize::MAX, 2).is_err());
    }
}

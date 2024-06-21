use std::io::{self, Read, Seek, SeekFrom, Write};
use std::io::Cursor;
use byteorder::{ByteOrder, LittleEndian, BigEndian, ReadBytesExt, WriteBytesExt};

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    fn read_u16<R: Read>(self, rdr: &mut R) -> io::Result<u16> {
        match self {
            Endian::Little => rdr.read_u16::<LittleEndian>(),
            Endian::Big => rdr.read_u16::<BigEndian>(),
        }
    }

    fn read_i16<R: Read>(self, rdr: &mut R) -> io::Result<i16> {
        match self {
            Endian::Little => rdr.read_i16::<LittleEndian>(),
            Endian::Big => rdr.read_i16::<BigEndian>(),
        }
    }

    fn read_u32<R: Read>(self, rdr: &mut R) -> io::Result<u32> {
        match self {
            Endian::Little => rdr.read_u32::<LittleEndian>(),
            Endian::Big => rdr.read_u32::<BigEndian>(),
        }
    }

    fn read_i32<R: Read>(self, rdr: &mut R) -> io::Result<i32> {
        match self {
            Endian::Little => rdr.read_i32::<LittleEndian>(),
            Endian::Big => rdr.read_i32::<BigEndian>(),
        }
    }

    fn read_u64<R: Read>(self, rdr: &mut R) -> io::Result<u64> {
        match self {
            Endian::Little => rdr.read_u64::<LittleEndian>(),
            Endian::Big => rdr.read_u64::<BigEndian>(),
        }
    }

    fn read_f32<R: Read>(self, rdr: &mut R) -> io::Result<f32> {
        match self {
            Endian::Little => rdr.read_f32::<LittleEndian>(),
            Endian::Big => rdr.read_f32::<BigEndian>(),
        }
    }

    fn write_u16<W: Write>(self, wtr: &mut W, n: u16) -> io::Result<()> {
        match self {
            Endian::Little => wtr.write_u16::<LittleEndian>(n),
            Endian::Big => wtr.write_u16::<BigEndian>(n),
        }
    }

    fn write_i16<W: Write>(self, wtr: &mut W, n: i16) -> io::Result<()> {
        match self {
            Endian::Little => wtr.write_i16::<LittleEndian>(n),
            Endian::Big => wtr.write_i16::<BigEndian>(n),
        }
    }

    fn write_u32<W: Write>(self, wtr: &mut W, n: u32) -> io::Result<()> {
        match self {
            Endian::Little => wtr.write_u32::<LittleEndian>(n),
            Endian::Big => wtr.write_u32::<BigEndian>(n),
        }
    }

    fn write_i32<W: Write>(self, wtr: &mut W, n: i32) -> io::Result<()> {
        match self {
            Endian::Little => wtr.write_i32::<LittleEndian>(n),
            Endian::Big => wtr.write_i32::<BigEndian>(n),
        }
    }

    fn write_u64<W: Write>(self, wtr: &mut W, n: u64) -> io::Result<()> {
        match self {
            Endian::Little => wtr.write_u64::<LittleEndian>(n),
            Endian::Big => wtr.write_u64::<BigEndian>(n),
        }
    }

    fn write_f32<W: Write>(self, wtr: &mut W, n: f32) -> io::Result<()> {
        match self {
            Endian::Little => wtr.write_f32::<LittleEndian>(n),
            Endian::Big => wtr.write_f32::<BigEndian>(n),
        }
    }
}

fn get_string(data: &[u8], offset: usize) -> io::Result<String> {
    let end = data[offset..].iter().position(|&b| b == 0).unwrap_or(data.len() - offset);
    Ok(String::from_utf8(data[offset..offset + end].to_vec()).unwrap())
}

struct Stream<T: Seek + Read> {
    stream: T,
}

impl<T: Seek + Read> Stream<T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.stream.seek(pos)
    }

    fn tell(&mut self) -> io::Result<u64> {
        self.stream.seek(SeekFrom::Current(0))
    }

    fn skip(&mut self, skip_size: u64) -> io::Result<u64> {
        self.stream.seek(SeekFrom::Current(skip_size as i64))
    }
}

struct ReadStream {
    stream: Cursor<Vec<u8>>,
    endian: Endian,
}

impl ReadStream {
    fn new(data: &[u8], endian: Endian) -> ReadStream {
        ReadStream {
            stream: Cursor::new(data.to_vec()),
            endian,
        }
    }

    fn read(&mut self, size: usize) -> io::Result<Vec<u8>> {
        let mut buffer = vec![0; size];
        self.stream.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        self.stream.read_u8()
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        self.endian.read_u16(&mut self.stream)
    }

    fn read_s16(&mut self) -> io::Result<i16> {
        self.endian.read_i16(&mut self.stream)
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        self.endian.read_u32(&mut self.stream)
    }

    fn read_s32(&mut self) -> io::Result<i32> {
        self.endian.read_i32(&mut self.stream)
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        self.endian.read_u64(&mut self.stream)
    }

    fn read_f32(&mut self) -> io::Result<f32> {
        self.endian.read_f32(&mut self.stream)
    }

    fn read_string(&mut self, offset: Option<usize>, size: usize) -> io::Result<String> {
        let pos = self.stream.position();
        let ptr = if let Some(offset) = offset {
            offset as u64
        } else {
            match size {
                4 => self.read_u32()? as u64,
                2 => self.read_u16()? as u64,
                _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid size")),
            }
        };
        let string = get_string(&self.stream.get_ref(), ptr as usize)?;
        self.stream.seek(SeekFrom::Start(pos))?;
        Ok(string)
    }
}

struct PlaceholderWriter {
    offset: u64,
}

impl PlaceholderWriter {
    fn new(offset: u64) -> PlaceholderWriter {
        PlaceholderWriter { offset }
    }

    fn write<W: Seek + Write>(&self, stream: &mut W, data: &[u8]) -> io::Result<()> {
        let pos = stream.seek(SeekFrom::Current(0))?;
        stream.seek(SeekFrom::Start(self.offset))?;
        stream.write_all(data)?;
        stream.seek(SeekFrom::Start(pos))?;
        Ok(())
    }
}

struct WriteStream<W: Write + Seek> {
    stream: W,
    endian: Endian,
    string_list: Vec<String>,
    strings: Vec<u8>,
    string_refs: std::collections::HashMap<String, usize>,
    string_list_exb: Vec<String>,
    strings_exb: Vec<u8>,
    string_refs_exb: std::collections::HashMap<String, usize>,
}

impl<W: Write + Seek> WriteStream<W> {
    fn new(stream: W, endian: Endian) -> WriteStream<W> {
        WriteStream {
            stream,
            endian,
            string_list: Vec::new(),
            strings: Vec::new(),
            string_refs: std::collections::HashMap::new(),
            string_list_exb: Vec::new(),
            strings_exb: Vec::new(),
            string_refs_exb: std::collections::HashMap::new(),
        }
    }

    fn add_string(&mut self, string: &str) {
        if !self.string_list.contains(&string.to_string()) {
            let encoded = string.as_bytes();
            self.string_list.push(string.to_string());
            self.string_refs.insert(string.to_string(), self.strings.len());
            self.strings.extend_from_slice(encoded);
            if encoded.last() != Some(&0) {
                self.strings.push(0);
            }
        }
    }

    fn add_string_exb(&mut self, string: &str) {
        if !self.string_list_exb.contains(&string.to_string()) {
            let encoded = string.as_bytes();
            self.string_list_exb.push(string.to_string());
            self.string_refs_exb.insert(string.to_string(), self.strings_exb.len());
            self.strings_exb.extend_from_slice(encoded);
            if encoded.last() != Some(&0) {
                self.strings_exb.push(0);
            }
        }
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.stream.write_all(data)
    }

    fn write_u16(&mut self, value: u16) -> io::Result<()> {
        self.endian.write_u16(&mut self.stream, value)
    }

    fn write_i16(&mut self, value: i16) -> io::Result<()> {
        self.endian.write_i16(&mut self.stream, value)
    }

    fn write_u32(&mut self, value: u32) -> io::Result<()> {
        self.endian.write_u32(&mut self.stream, value)
    }

    fn write_i32(&mut self, value: i32) -> io::Result<()> {
        self.endian.write_i32(&mut self.stream, value)
    }

    fn write_u64(&mut self, value: u64) -> io::Result<()> {
        self.endian.write_u64(&mut self.stream, value)
    }

    fn write_f32(&mut self, value: f32) -> io::Result<()> {
        self.endian.write_f32(&mut self.stream, value)
    }
}

fn u8(value: u8) -> Vec<u8> {
    vec![value]
}

fn u16(value: u16, endian: Endian) -> Vec<u8> {
    let mut buffer = vec![0; 2];
    match endian {
        Endian::Little => LittleEndian::write_u16(&mut buffer, value),
        Endian::Big => BigEndian::write_u16(&mut buffer, value),
    }
    buffer
}

fn s16(value: i16, endian: Endian) -> Vec<u8> {
    let mut buffer = vec![0; 2];
    match endian {
        Endian::Little => LittleEndian::write_i16(&mut buffer, value),
        Endian::Big => BigEndian::write_i16(&mut buffer, value),
    }
    buffer
}

fn u32(value: u32, endian: Endian) -> Vec<u8> {
    let mut buffer = vec![0; 4];
    match endian {
        Endian::Little => LittleEndian::write_u32(&mut buffer, value),
        Endian::Big => BigEndian::write_u32(&mut buffer, value),
    }
    buffer
}

fn s32(value: i32, endian: Endian) -> Vec<u8> {
    let mut buffer = vec![0; 4];
    match endian {
        Endian::Little => LittleEndian::write_i32(&mut buffer, value),
        Endian::Big => BigEndian::write_i32(&mut buffer, value),
    }
    buffer
}

fn u64(value: u64, endian: Endian) -> Vec<u8> {
    let mut buffer = vec![0; 8];
    match endian {
        Endian::Little => LittleEndian::write_u64(&mut buffer, value),
        Endian::Big => BigEndian::write_u64(&mut buffer, value),
    }
    buffer
}

fn f32(value: f32, endian: Endian) -> Vec<u8> {
    let mut buffer = vec![0; 4];
    match endian {
        Endian::Little => LittleEndian::write_f32(&mut buffer, value),
        Endian::Big => BigEndian::write_f32(&mut buffer, value),
    }
    buffer
}

fn string(value: &str) -> Vec<u8> {
    value.as_bytes().to_vec()
}

fn vec3f(values: &[f32], endian: Endian) -> Vec<u8> {
    values.iter().flat_map(|&v| f32(v, endian)).collect()
}

fn byte_custom(value: &[u8], size: usize) -> Vec<u8> {
    let mut buffer = vec![0; size];
    buffer[..value.len()].copy_from_slice(value);
    buffer
}

fn padding() -> Vec<u8> {
    vec![0]
}

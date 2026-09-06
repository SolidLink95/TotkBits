use super::token::TextPart;
use crate::parser::binary::Endian;
use encoding_rs::SHIFT_JIS;
use std::io::{self, ErrorKind};
fn u16at(b: &[u8], i: usize, e: Endian) -> u16 {
    e.u16_from_bytes([b[i], b[i + 1]])
}
pub fn decode(raw: &[u8], enc: u8, e: Endian) -> io::Result<Vec<TextPart>> {
    if enc == 1 {
        decode_utf16(raw, e)
    } else {
        decode_bytes(raw, enc, e)
    }
}
fn decode_utf16(raw: &[u8], e: Endian) -> io::Result<Vec<TextPart>> {
    if raw.len() % 2 != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "odd UTF-16 TXT2 string",
        ));
    }
    let mut out = Vec::new();
    let mut text = Vec::new();
    let flush = |out: &mut Vec<TextPart>, v: &mut Vec<u16>| -> io::Result<()> {
        if !v.is_empty() {
            out.push(TextPart::Text(String::from_utf16(v).map_err(|_| {
                io::Error::new(ErrorKind::InvalidData, "invalid UTF-16")
            })?));
            v.clear();
        }
        Ok(())
    };
    let mut i = 0;
    while i + 1 < raw.len() {
        let c = u16at(raw, i, e);
        i += 2;
        if c == 0 {
            break;
        }
        if c == 0x0e {
            flush(&mut out, &mut text)?;
            if i + 6 > raw.len() {
                return Err(io::Error::new(ErrorKind::InvalidData, "truncated MSBT tag"));
            }
            let g = u16at(raw, i, e);
            let k = u16at(raw, i + 2, e);
            let n = u16at(raw, i + 4, e) as usize;
            i += 6;
            if i + n > raw.len() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "tag arguments out of bounds",
                ));
            }
            out.push(TextPart::Start {
                group: g,
                kind: k,
                args: raw[i..i + n].to_vec(),
            });
            i += n;
        } else if c == 0x0f {
            flush(&mut out, &mut text)?;
            if i + 4 > raw.len() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "truncated MSBT end tag",
                ));
            }
            out.push(TextPart::End {
                group: u16at(raw, i, e),
                kind: u16at(raw, i + 2, e),
            });
            i += 4;
        } else {
            text.push(c)
        }
    }
    flush(&mut out, &mut text)?;
    Ok(out)
}
fn decode_bytes(raw: &[u8], enc: u8, _e: Endian) -> io::Result<Vec<TextPart>> {
    let end = raw.iter().position(|x| *x == 0).unwrap_or(raw.len());
    let b = &raw[..end];
    let s = match enc {
        3 => SHIFT_JIS
            .decode_without_bom_handling_and_without_replacement(b)
            .map(|x| x.into_owned()),
        _ => String::from_utf8(b.to_vec()).ok(),
    }
    .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid encoded TXT2 string"))?;
    Ok(vec![TextPart::Text(s)])
}
pub fn encode(parts: &[TextPart], enc: u8, e: Endian) -> io::Result<Vec<u8>> {
    if enc != 1 {
        let s = parts
            .iter()
            .map(|p| match p {
                TextPart::Text(s) => Ok(s.as_str()),
                _ => Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "tags require UTF-16 encoding",
                )),
            })
            .collect::<io::Result<String>>()?;
        let mut v = if enc == 3 {
            SHIFT_JIS.encode(&s).0.into_owned()
        } else {
            s.into_bytes()
        };
        v.push(0);
        return Ok(v);
    }
    let mut v = Vec::new();
    let put = |v: &mut Vec<u8>, x: u16| v.extend(e.u16_to_bytes(x));
    for p in parts {
        match p {
            TextPart::Text(s) => {
                for c in s.encode_utf16() {
                    put(&mut v, c)
                }
            }
            TextPart::Start { group, kind, args } => {
                put(&mut v, 0x0e);
                put(&mut v, *group);
                put(&mut v, *kind);
                put(&mut v, args.len() as u16);
                v.extend(args)
            }
            TextPart::End { group, kind } => {
                put(&mut v, 0x0f);
                put(&mut v, *group);
                put(&mut v, *kind)
            }
        }
    }
    put(&mut v, 0);
    Ok(v)
}

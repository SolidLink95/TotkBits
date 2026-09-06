use super::tag_config::{self, TagArgument};
use crate::parser::binary::Endian;
use std::collections::HashMap;
use std::io::{self, ErrorKind};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.into())
}

fn take<'a>(data: &'a [u8], offset: &mut usize, count: usize) -> io::Result<&'a [u8]> {
    let end = offset
        .checked_add(count)
        .ok_or_else(|| invalid("tag argument overflow"))?;
    let value = data
        .get(*offset..end)
        .ok_or_else(|| invalid("truncated tag argument"))?;
    *offset = end;
    Ok(value)
}

fn u16_at(data: &[u8], offset: &mut usize, endian: Endian) -> io::Result<u16> {
    let source = take(data, offset, 2)?;
    Ok(endian.u16_from_bytes([source[0], source[1]]))
}

fn argument_text(
    arg: &TagArgument,
    data: &[u8],
    offset: &mut usize,
    endian: Endian,
) -> io::Result<String> {
    let numeric = match arg.data_type.as_str() {
        "u8" => Some(take(data, offset, 1)?[0] as i64),
        "bool" => return Ok((take(data, offset, 1)?[0] == 1).to_string()),
        "u16" => Some(u16_at(data, offset, endian)? as i64),
        "s16" => Some(u16_at(data, offset, endian)? as i16 as i64),
        "str" => {
            let byte_len = u16_at(data, offset, endian)? as usize;
            let bytes = take(data, offset, byte_len)?;
            let mut units = bytes
                .chunks_exact(2)
                .map(|b| endian.u16_from_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            if byte_len % 2 != 0 {
                units.push(0xFFFD);
            }
            return Ok(String::from_utf16_lossy(&units));
        }
        other => return Err(invalid(format!("unsupported tag argument type {other}"))),
    };
    let numeric = numeric.ok_or_else(|| invalid("tag argument did not produce a value"))?;
    Ok(arg
        .mapped_name(numeric)
        .unwrap_or_else(|| numeric.to_string()))
}

pub fn format_start(group: u16, kind: u16, args: &[u8], endian: Endian) -> String {
    let Some(config) = tag_config::by_id(group, kind) else {
        return format_default(group, kind, args);
    };
    let mut result = format!("{{{{{}", config.name);
    let mut offset = 0;
    for arg in &config.arguments {
        result.push_str(&format!(" {}=\"", arg.name));
        let Ok(value) = argument_text(arg, args, &mut offset, endian) else {
            result.push_str(&format_default(group, kind, args));
            return result;
        };
        result.push_str(&escape(&value));
        result.push('"');
    }
    let remainder = args[offset..]
        .strip_suffix(&[0xCD])
        .unwrap_or(&args[offset..]);
    if !remainder.is_empty() {
        result.push_str(&format!(" otherArg=\"0x{}\"", hex(remainder)));
    }
    result.push_str("}}");
    result
}

pub fn format_end(group: u16, kind: u16) -> String {
    tag_config::by_id(group, kind)
        .map(|tag| format!("{{{{/{}}}}}", tag.name))
        .unwrap_or_else(|| format!("{{{{/{group}:{kind}}}}}"))
}

pub fn parse_start(
    name: &str,
    values: &HashMap<String, String>,
    endian: Endian,
) -> io::Result<(u16, u16, Vec<u8>)> {
    let config = tag_config::by_name(name).ok_or_else(|| invalid(format!("unknown tag {name}")))?;
    let mut data = Vec::new();
    for arg in &config.arguments {
        let value = values
            .get(&arg.name)
            .ok_or_else(|| invalid(format!("tag {} is missing {}", config.name, arg.name)))?;
        encode_argument(&mut data, arg, value, endian)?;
    }
    Ok((config.group, config.kind, data))
}

pub fn parse_end(name: &str) -> io::Result<(u16, u16)> {
    let tag =
        tag_config::by_name(name).ok_or_else(|| invalid(format!("unknown closing tag {name}")))?;
    Ok((tag.group, tag.kind))
}

fn push_u16(data: &mut Vec<u8>, value: u16, endian: Endian) {
    data.extend_from_slice(&endian.u16_to_bytes(value));
}
fn encode_argument(
    data: &mut Vec<u8>,
    arg: &TagArgument,
    text: &str,
    endian: Endian,
) -> io::Result<()> {
    let mapped = arg.mapped_value(text);
    match arg.data_type.as_str() {
        "u8" => data.push(
            mapped
                .unwrap_or_else(|| text.parse().unwrap_or_default())
                .try_into()
                .map_err(|_| invalid("u8 tag argument out of range"))?,
        ),
        "bool" => data.push(match text.to_ascii_lowercase().as_str() {
            "true" | "1" => 1,
            "false" | "0" => 0,
            _ => return Err(invalid("invalid boolean tag argument")),
        }),
        "u16" => push_u16(
            data,
            mapped
                .unwrap_or_else(|| text.parse().unwrap_or_default())
                .try_into()
                .map_err(|_| invalid("u16 tag argument out of range"))?,
            endian,
        ),
        "s16" => {
            let value: i16 = mapped
                .unwrap_or_else(|| text.parse().unwrap_or_default())
                .try_into()
                .map_err(|_| invalid("s16 tag argument out of range"))?;
            push_u16(data, value as u16, endian);
        }
        "str" => {
            let units = text.encode_utf16().collect::<Vec<_>>();
            push_u16(
                data,
                (units.len() * 2)
                    .try_into()
                    .map_err(|_| invalid("tag string too long"))?,
                endian,
            );
            for unit in units {
                push_u16(data, unit, endian);
            }
        }
        other => return Err(invalid(format!("unsupported tag argument type {other}"))),
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn format_default(group: u16, kind: u16, args: &[u8]) -> String {
    if args.is_empty() {
        format!("{{{{{group}:{kind}}}}}")
    } else {
        format!("{{{{{group}:{kind} arg=\"0x{}\"}}}}", hex(args))
    }
}

use super::{
    document::{Message, Msbt},
    token::TextPart,
};
use std::io::{self, ErrorKind};
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}
fn string_pool(data: &[u8], encoding: u8, endian: crate::parser::binary::Endian) -> Vec<String> {
    if encoding == 1 {
        let mut result = Vec::new();
        let mut current = Vec::new();
        for bytes in data.chunks_exact(2) {
            let unit = endian.u16_from_bytes([bytes[0], bytes[1]]);
            if unit == 0 {
                if !current.is_empty() {
                    result.push(String::from_utf16_lossy(&current));
                    current.clear();
                }
            } else {
                current.push(unit);
            }
        }
        if !current.is_empty() {
            result.push(String::from_utf16_lossy(&current));
        }
        return result;
    }
    data.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}
fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}
pub fn serialize(m: &Msbt) -> String {
    let has = |n: &[u8; 4]| m.sections.iter().any(|s| &s.magic == n);
    let groups = m
        .sections
        .iter()
        .find(|s| &s.magic == b"LBL1")
        .and_then(|s| super::label::LabelSection::read(&s.data, m.header.endian).ok())
        .map(|x| x.group_count)
        .unwrap_or(0);
    let enc = match m.header.encoding {
        0 => "utf-8",
        1 => "utf-16",
        2 => "utf-32",
        3 => "shift-jis",
        _ => "utf-8",
    };
    let mut o=format!("%%%\nbigEndian: {}\nbigEndianLabels: {}\nversion: {}\nencoding: {}\nhasNLI1: {}\nhasLBL1: {}\n",m.header.endian==crate::parser::binary::Endian::Big,m.header.endian==crate::parser::binary::Endian::Big,m.header.version,enc,has(b"NLI1"),has(b"LBL1"));
    if has(b"LBL1") {
        o += &format!("labelGroups: {groups}\n")
    }
    o += &format!("hasATR1: {}\n", has(b"ATR1"));
    if !m.attribute_string_pool.is_empty() {
        o += &format!(
            "attributeStringPool: [{}]\n",
            string_pool(&m.attribute_string_pool, m.header.encoding, m.header.endian)
                .iter()
                .map(|x| quote(x))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    if !m.attribute_offsets.is_empty() {
        o += &format!(
            "attributeOffsets: [{}]\n",
            m.attribute_offsets
                .iter()
                .map(|value| (*value as i32).to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    o += &format!(
        "hasTSY1: {}\nhasTXTW: {}\n%%%\n",
        has(b"TSY1"),
        has(b"TXTW")
    );
    for x in &m.messages {
        o += "\n---\n";
        if let Some(id) = x.id {
            o += &format!("id: {id}\n")
        }
        if let Some(label) = &x.label {
            o += &format!("label: {label}\n")
        }
        if has(b"ATR1") {
            o += &format!("attribute: {}\n", hex(&x.attribute))
        }
        if let Some(style) = x.style {
            o += &format!("styleIndex: {style}\n")
        }
        o += "---\n";
        for p in &x.parts {
            match p {
                TextPart::Text(s) => {
                    for line in s.split_inclusive('\n') {
                        let z = line.strip_suffix('\n').unwrap_or(line);
                        o += match z {
                            "---" => "{{---}}",
                            "{{---}}" => "{{{{---}}}}",
                            _ => z,
                        };
                        if line.ends_with('\n') {
                            o.push('\n')
                        }
                    }
                }
                TextPart::Start { group, kind, args } => {
                    o += &super::tag::format_start(*group, *kind, args, m.header.endian)
                }
                TextPart::End { group, kind } => o += &super::tag::format_end(*group, *kind),
            }
        }
        o.push('\n')
    }
    o.replace('\n', "\r\n")
}
pub fn deserialize(template: &Msbt, s: &str) -> io::Result<Msbt> {
    if serialize(template) == s {
        return Ok(template.clone());
    }
    let normalized;
    let s = if s.contains("\r\n") {
        normalized = s.replace("\r\n", "\n");
        normalized.as_str()
    } else {
        s
    };
    let mut out = template.clone();
    if let Some(header) = s.split("%%%\n").nth(1) {
        for line in header.lines() {
            if let Some(value) = line.strip_prefix("labelGroups: ") {
                out.label_groups = value
                    .parse()
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad labelGroups"))?;
            }
            if let Some(value) = line
                .strip_prefix("attributeOffsets: [")
                .and_then(|x| x.strip_suffix(']'))
            {
                out.attribute_offsets = if value.is_empty() {
                    Vec::new()
                } else {
                    value
                        .split(',')
                        .map(|x| {
                            x.trim()
                                .parse::<i32>()
                                .map(|value| value as u32)
                                .map_err(|_| {
                                    io::Error::new(ErrorKind::InvalidData, "bad attribute offset")
                                })
                        })
                        .collect::<io::Result<_>>()?
                };
            }
        }
    }
    let chunks: Vec<&str> = s.split("\n---\n").collect();
    if chunks.len() < 2 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "missing message delimiter",
        ));
    }
    let mut msgs = Vec::new();
    if (chunks.len() - 1) % 2 != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "unpaired message delimiter",
        ));
    }
    for pair in chunks[1..].chunks_exact(2) {
        let head = pair[0];
        let body = pair[1];
        let mut m = Message {
            label: None,
            id: None,
            attribute: Vec::new(),
            style: None,
            parts: Vec::new(),
        };
        for l in head.lines() {
            if let Some(x) = l.strip_prefix("label: ") {
                m.label = Some(x.into())
            } else if let Some(x) = l.strip_prefix("id: ") {
                m.id = x.parse().ok()
            } else if let Some(x) = l.strip_prefix("styleIndex: ") {
                m.style = x.parse().ok()
            } else if let Some(x) = l.strip_prefix("attribute: ") {
                m.attribute = (0..x.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&x[i..i + 2], 16))
                    .collect::<Result<_, _>>()
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad attribute hex"))?
            }
        }
        let body = body
            .strip_suffix('\n')
            .unwrap_or(body)
            .replace("{{{{---}}}}", "{{---}}")
            .replace("{{---}}", "---");
        m.parts = parse_parts(&body, template.header.endian)?;
        if let Some(original) = template.messages.get(msgs.len()) {
            preserve_tag_suffixes(&mut m.parts, &original.parts);
        }
        msgs.push(m)
    }
    out.messages = msgs;
    Ok(out)
}
fn parse_parts(s: &str, endian: crate::parser::binary::Endian) -> io::Result<Vec<TextPart>> {
    let mut v = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("{{") {
        if i > 0 {
            v.push(TextPart::Text(rest[..i].into()))
        }
        let e = rest[i + 2..]
            .find("}}")
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "unterminated tag"))?
            + i
            + 2;
        let tag = &rest[i + 2..e];
        let a: Vec<&str> = tag.split(':').collect();
        match a.as_slice() {
            ["tag", g, k, h] => {
                if h.len() % 2 != 0 {
                    return Err(io::Error::new(ErrorKind::InvalidData, "odd tag hex"));
                }
                let args = (0..h.len())
                    .step_by(2)
                    .map(|j| u8::from_str_radix(&h[j..j + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad tag hex"))?;
                v.push(TextPart::Start {
                    group: g
                        .parse()
                        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad tag group"))?,
                    kind: k
                        .parse()
                        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad tag type"))?,
                    args,
                })
            }
            ["end", g, k] => v.push(TextPart::End {
                group: g
                    .parse()
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad tag group"))?,
                kind: k
                    .parse()
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "bad tag type"))?,
            }),
            _ => {
                if let Some(name) = tag.strip_prefix('/') {
                    let name = name.trim();
                    let (group, kind) = if let Some((group, kind)) = parse_numeric_id(name) {
                        (group, kind)
                    } else {
                        super::tag::parse_end(name)?
                    };
                    v.push(TextPart::End { group, kind });
                } else if let Some((group, kind, args)) = parse_default_tag(tag) {
                    v.push(TextPart::Start { group, kind, args });
                } else if let Some((name, values)) = parse_named_tag(tag) {
                    let (group, kind, args) = super::tag::parse_start(name, &values, endian)?;
                    v.push(TextPart::Start { group, kind, args });
                } else {
                    v.push(TextPart::Text(rest[i..e + 2].into()))
                }
            }
        }
        rest = &rest[e + 2..]
    }
    if !rest.is_empty() {
        v.push(TextPart::Text(rest.into()))
    }
    Ok(v)
}

fn parse_numeric_id(value: &str) -> Option<(u16, u16)> {
    let (group, kind) = value.split_once(':')?;
    Some((group.parse().ok()?, kind.parse().ok()?))
}

fn parse_default_tag(value: &str) -> Option<(u16, u16, Vec<u8>)> {
    let split = value.find(char::is_whitespace).unwrap_or(value.len());
    let (group, kind) = parse_numeric_id(&value[..split])?;
    let rest = value[split..].trim();
    if rest.is_empty() {
        return Some((group, kind, Vec::new()));
    }
    let hex = rest.strip_prefix("arg=\"0x")?.strip_suffix('"')?;
    if hex.len() % 2 != 0 {
        return None;
    }
    let args = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    Some((group, kind, args))
}

fn parse_named_tag(tag: &str) -> Option<(&str, std::collections::HashMap<String, String>)> {
    let split = tag.find(char::is_whitespace).unwrap_or(tag.len());
    let name = &tag[..split];
    if name.is_empty() || name.contains(':') {
        return None;
    }
    let mut values = std::collections::HashMap::new();
    let mut rest = tag[split..].trim();
    while !rest.is_empty() {
        let equals = rest.find('=')?;
        let key = rest[..equals].trim().to_string();
        rest = rest[equals + 1..].trim_start();
        let quoted = rest.strip_prefix('"')?;
        let mut value = String::new();
        let mut escaped = false;
        let mut end = None;
        for (index, ch) in quoted.char_indices() {
            if escaped {
                value.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                end = Some(index);
                break;
            } else {
                value.push(ch);
            }
        }
        let end = end?;
        values.insert(key, value);
        rest = quoted[end + 1..].trim_start();
    }
    Some((name, values))
}

fn preserve_tag_suffixes(parts: &mut [TextPart], original: &[TextPart]) {
    let mut originals = original.iter().filter_map(|part| match part {
        TextPart::Start { group, kind, args } => Some((*group, *kind, args)),
        _ => None,
    });
    for part in parts {
        let TextPart::Start { group, kind, args } = part else {
            continue;
        };
        let Some((old_group, old_kind, old_args)) = originals.next() else {
            break;
        };
        if *group == old_group && *kind == old_kind && old_args.starts_with(args.as_slice()) {
            args.extend_from_slice(&old_args[args.len()..]);
        }
    }
}

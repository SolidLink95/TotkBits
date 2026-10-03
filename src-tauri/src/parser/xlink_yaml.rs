//! Lossless bridge between the modern XLink text representation (the
//! `Metadata { … }` language produced by `xlink2_bindings`) and YAML.
//!
//! The pipeline is `text -> serde_yaml::Value -> YAML string -> Value -> text`
//! and the final text is byte-identical to the input for anything the native
//! converter prints.
//!
//! Node shape:
//! - `Head {` … `}`           -> `Head:` mapping (the key is the whole head, so
//!   `Execute = Asset {` becomes the key `Execute = Asset`).
//! - `Key = value`            -> `Key: value`.
//! - bare lines               -> sequence items; `(1.0, 2.0)` tuples become
//!   nested number sequences.
//! - A block mixing bare lines with entries, or repeating a key, becomes a
//!   sequence whose entries are single-key mappings.
//!
//! Scalars:
//! - `true` / `false`, integers and floats are native YAML scalars, but only
//!   when printing them back reproduces the source token exactly.
//! - `"quoted"` strings are plain YAML strings (the content is kept verbatim,
//!   escapes included).
//! - `0x…` / `0b…` literals are YAML strings holding the literal. They must
//!   stay quoted in the YAML, otherwise YAML reads them as plain integers.
//! - A quoted string that itself looks like such a literal is tagged `!str`.
//! - Every other bare token (enum names, `Local::…` properties, asset
//!   references, numbers in a non-canonical spelling) is tagged `!raw`.

use std::{collections::HashSet, io};

use serde_yaml::{
    value::{Tag, TaggedValue},
    Mapping, Number, Value,
};

const INDENT: &str = "  ";
const RAW_TAG: &str = "raw";
const STR_TAG: &str = "str";

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub fn text_to_yaml(text: &str) -> io::Result<String> {
    nodes_to_yaml(&text_to_nodes(text)?)
}

pub fn yaml_to_text(yaml: &str) -> io::Result<String> {
    nodes_to_text(&yaml_to_nodes(yaml)?)
}

pub fn nodes_to_yaml(nodes: &Value) -> io::Result<String> {
    serde_yaml::to_string(nodes).map_err(|error| invalid(format!("XLink YAML: {error}")))
}

pub fn yaml_to_nodes(yaml: &str) -> io::Result<Value> {
    serde_yaml::from_str(yaml).map_err(|error| invalid(format!("XLink YAML: {error}")))
}

enum Item {
    Entry(String, Value),
    Bare(Value),
}

pub fn text_to_nodes(text: &str) -> io::Result<Value> {
    // (head of the open block, its items); the first frame is the document.
    let mut stack: Vec<(String, Vec<Item>)> = vec![(String::new(), Vec::new())];
    for (index, line) in text.split('\n').enumerate() {
        let line = line
            .strip_suffix('\r')
            .unwrap_or(line)
            .trim_start_matches(' ');
        if line.is_empty() {
            continue;
        }
        if line == "}" {
            let (head, items) = stack.pop().unwrap();
            let Some((_, parent)) = stack.last_mut() else {
                return Err(invalid(format!(
                    "XLink text line {}: unmatched '}}'",
                    index + 1
                )));
            };
            parent.push(Item::Entry(head, build_block(items)));
        } else if let Some(head) = line.strip_suffix(" {") {
            stack.push((head.to_string(), Vec::new()));
        } else if let Some((key, value)) = line.split_once(" = ") {
            let item = Item::Entry(key.to_string(), parse_value(value));
            stack.last_mut().unwrap().1.push(item);
        } else {
            stack
                .last_mut()
                .unwrap()
                .1
                .push(Item::Bare(parse_bare(line)));
        }
    }
    if stack.len() != 1 {
        return Err(invalid(format!(
            "XLink text: block '{}' is never closed",
            stack.last().unwrap().0
        )));
    }
    Ok(build_block(stack.pop().unwrap().1))
}

fn build_block(items: Vec<Item>) -> Value {
    let mut keys = HashSet::new();
    let is_mapping = items.iter().all(|item| match item {
        Item::Entry(key, _) => keys.insert(key.as_str()),
        Item::Bare(_) => false,
    });
    drop(keys);
    if is_mapping {
        let mut mapping = Mapping::with_capacity(items.len());
        for item in items {
            if let Item::Entry(key, value) = item {
                mapping.insert(Value::String(key), value);
            }
        }
        return Value::Mapping(mapping);
    }
    Value::Sequence(
        items
            .into_iter()
            .map(|item| match item {
                Item::Entry(key, value) => {
                    let mut mapping = Mapping::with_capacity(1);
                    mapping.insert(Value::String(key), value);
                    Value::Mapping(mapping)
                }
                Item::Bare(value) => value,
            })
            .collect(),
    )
}

fn tagged(tag: &str, value: &str) -> Value {
    Value::Tagged(Box::new(TaggedValue {
        tag: Tag::new(tag),
        value: Value::String(value.to_string()),
    }))
}

fn is_hex_or_bin(token: &str) -> bool {
    if let Some(digits) = token.strip_prefix("0x") {
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit())
    } else if let Some(digits) = token.strip_prefix("0b") {
        !digits.is_empty() && digits.bytes().all(|b| matches!(b, b'0' | b'1'))
    } else {
        false
    }
}

/// A number node, but only when `format_number` gives the token back.
fn parse_number(token: &str) -> Option<Number> {
    let first = token.bytes().next()?;
    if !(first.is_ascii_digit() || first == b'-') {
        return None;
    }
    let number = if let Ok(value) = token.parse::<i64>() {
        Number::from(value)
    } else if let Ok(value) = token.parse::<u64>() {
        Number::from(value)
    } else {
        let value = token.parse::<f64>().ok()?;
        if !value.is_finite() || (value == 0.0 && value.is_sign_negative()) {
            return None;
        }
        Number::from(value)
    };
    (format_number(&number) == token).then_some(number)
}

fn format_number(number: &Number) -> String {
    if let Some(value) = number.as_i64() {
        value.to_string()
    } else if let Some(value) = number.as_u64() {
        value.to_string()
    } else {
        format!("{:?}", number.as_f64().unwrap_or_default())
    }
}

fn parse_value(token: &str) -> Value {
    match token {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    if let Some(number) = parse_number(token) {
        return Value::Number(number);
    }
    if is_hex_or_bin(token) {
        return Value::String(token.to_string());
    }
    if token.len() >= 2 && token.starts_with('"') && token.ends_with('"') {
        let inner = &token[1..token.len() - 1];
        return if is_hex_or_bin(inner) {
            tagged(STR_TAG, inner)
        } else {
            Value::String(inner.to_string())
        };
    }
    tagged(RAW_TAG, token)
}

/// Bare lines are kept verbatim, except `(a, b, …)` tuples of numbers.
fn parse_bare(line: &str) -> Value {
    if let Some(inner) = line.strip_prefix('(').and_then(|l| l.strip_suffix(')')) {
        let numbers: Option<Vec<Value>> = inner
            .split(", ")
            .map(|part| parse_number(part).map(Value::Number))
            .collect();
        if let Some(numbers) = numbers {
            return Value::Sequence(numbers);
        }
    }
    Value::String(line.to_string())
}

pub fn nodes_to_text(nodes: &Value) -> io::Result<String> {
    let mut out = String::new();
    write_block(&mut out, nodes, 0)?;
    Ok(out)
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

fn write_block(out: &mut String, block: &Value, depth: usize) -> io::Result<()> {
    match block {
        Value::Mapping(mapping) => write_entries(out, mapping, depth),
        Value::Sequence(items) => {
            for item in items {
                match item {
                    Value::Mapping(mapping) => write_entries(out, mapping, depth)?,
                    Value::Sequence(tuple) => {
                        push_indent(out, depth);
                        out.push('(');
                        for (index, element) in tuple.iter().enumerate() {
                            if index > 0 {
                                out.push_str(", ");
                            }
                            out.push_str(&plain_scalar(element)?);
                        }
                        out.push_str(")\n");
                    }
                    scalar => {
                        push_indent(out, depth);
                        out.push_str(&plain_scalar(scalar)?);
                        out.push('\n');
                    }
                }
            }
            Ok(())
        }
        other => Err(invalid(format!(
            "XLink YAML: expected a mapping or a sequence, found {other:?}"
        ))),
    }
}

fn write_entries(out: &mut String, mapping: &Mapping, depth: usize) -> io::Result<()> {
    for (key, value) in mapping {
        push_indent(out, depth);
        out.push_str(&plain_scalar(key)?);
        match value {
            Value::Mapping(_) | Value::Sequence(_) => {
                out.push_str(" {\n");
                write_block(out, value, depth + 1)?;
                push_indent(out, depth);
                out.push_str("}\n");
            }
            scalar => {
                out.push_str(" = ");
                out.push_str(&format_value(scalar)?);
                out.push('\n');
            }
        }
    }
    Ok(())
}

/// Keys, bare lines and tuple elements: printed without quoting.
fn plain_scalar(value: &Value) -> io::Result<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(format_number(number)),
        Value::Bool(flag) => Ok(flag.to_string()),
        Value::Tagged(tagged) => plain_scalar(&tagged.value),
        other => Err(invalid(format!(
            "XLink YAML: expected a scalar, found {other:?}"
        ))),
    }
}

/// The right-hand side of `Key = value`.
fn format_value(value: &Value) -> io::Result<String> {
    match value {
        Value::String(text) if is_hex_or_bin(text) => Ok(text.clone()),
        Value::String(text) => Ok(format!("\"{text}\"")),
        Value::Tagged(tagged) if tagged.tag == RAW_TAG => plain_scalar(&tagged.value),
        Value::Tagged(tagged) if tagged.tag == STR_TAG => {
            Ok(format!("\"{}\"", plain_scalar(&tagged.value)?))
        }
        Value::Tagged(tagged) => Err(invalid(format!(
            "XLink YAML: unknown tag {} (expected !raw or !str)",
            tagged.tag
        ))),
        Value::Null => Err(invalid("XLink YAML: a key has no value")),
        other => plain_scalar(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> String {
        let nodes = text_to_nodes(text).unwrap();
        let yaml = nodes_to_yaml(&nodes).unwrap();
        let reparsed = yaml_to_nodes(&yaml).unwrap();
        assert_eq!(nodes, reparsed, "YAML string changed the nodes");
        let rebuilt = nodes_to_text(&reparsed).unwrap();
        if rebuilt != text {
            let line = rebuilt
                .lines()
                .zip(text.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| rebuilt.lines().count().min(text.lines().count()));
            panic!(
                "XLink text differs at line {}:\n  expected: {:?}\n  rebuilt:  {:?}",
                line + 1,
                text.lines().nth(line),
                rebuilt.lines().nth(line)
            );
        }
        yaml
    }

    #[test]
    fn every_construct_round_trips_exactly() {
        let text = r#"Metadata {
  ModuleType = ELink
}
ParamDefines {
  SystemUserParams {
  }
  CustomUserParams {
    leafType = 0
    Ratio = 0.850000024
    Tiny = 1e-05
    NegZero = -0.0
    Big = 18446744073709551615
    Padded = 007
    Enabled = true
    AssetName = ""
    Tricky = "0x10"
    Word = "true"
    Number = "12"
    Clip = 0x0
    BitFlag = 0b10
    Empty =
  }
}
Users {
  FireFruit {
    LocalProperties {
      カメラとの距離
      "Quoted Name"
    }
    AssetCallTables {
      速度分岐[0x7424c466] {
        @Unknown = -1
        Execute = Switch (Local::速度) {
          (<value> > 0.100000001) => 鳥[0xb4206570] {
            Execute = Asset {
              Scale = CURVE {
                Type = Standard
                Property = Local::カメラとの距離
                Points = {
                  (20.0, 1.0)
                  (0.400000006, 0.0)
                  (1e-05, x)
                }
              }
              Duration = RANDOM {
                Min = 30.0
              }
            }
          }
          (_) => それ以外[0x00000001] {
            Asset = Down[0x84f82a12]
            OverwriteParams =  {
              Scale = 1.20000005
            }
          }
        }
        Execute = Random {
          1.0 => 落雷[0xf8e7dab8] {
          }
        }
      }
      "Skl[0]" {
        0x0411816c {
          End = 2147483647
        }
      }
    }
    Properties {
      Local::アクター気温 {
        if <value> < 5.0 => 0x4a9b10a4 {
          Lazy = false
        }
        Mixed
        if <value> < 5.0 => 0x4a9b10a4 {
          Lazy = true
        }
      }
    }
  }
}
"#;
        let yaml = round_trip(text);
        assert!(yaml.contains("ModuleType: !raw ELink"));
        assert!(yaml.contains("Tricky: !str"));
        assert!(yaml.contains("leafType: 0\n"));
    }

    #[test]
    fn unbalanced_blocks_are_rejected() {
        assert!(text_to_nodes("Users {\n").is_err());
        assert!(text_to_nodes("}\n").is_err());
    }

    /// Full vanilla file from the configured TOTK dump. Set
    /// `XLINK_YAML_DUMP_DIR` to keep the text and the YAML for inspection.
    #[test]
    #[cfg(windows)]
    fn vanilla_elink_text_round_trips_through_yaml() {
        use crate::{file_format::Xlink::Xlink_rs, TotkConfig::TotkConfig, Zstd::TotkZstd};
        use std::{path::Path, sync::Arc};

        let Ok(mut config) = TotkConfig::safe_new(false) else {
            return;
        };
        let path = Path::new(&config.romfs).join("ELink2/elink2.Product.110.belnk.zs");
        if !path.is_file() {
            return;
        }
        config.xlink_format = "modern".into();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ZSTD dictionaries"),
        );
        let text = Xlink_rs::new(zstd)
            .unwrap()
            .binary_to_yaml(&std::fs::read(&path).unwrap())
            .expect("convert ELink to text");
        let yaml = round_trip(&text);
        if let Ok(dir) = std::env::var("XLINK_YAML_DUMP_DIR") {
            std::fs::write(Path::new(&dir).join("elink.txt"), &text).unwrap();
            std::fs::write(Path::new(&dir).join("elink.yaml"), &yaml).unwrap();
        }
    }
}

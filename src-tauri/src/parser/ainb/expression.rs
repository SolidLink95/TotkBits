use super::common::AinbWriter;
use crate::parser::binary::BinaryReader;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExpressionModule {
    #[serde(rename = "Version")]
    pub version: u32,
    #[serde(rename = "Expressions")]
    pub expressions: Vec<Expression>,
    #[serde(rename = "Parameter Table Size", default)]
    pub parameter_table_size: Option<u32>,
    #[serde(rename = "Parameter Table Bytes", default)]
    pub parameter_table_bytes: String,
    #[serde(rename = "Operand Storage", default)]
    pub operand_storage: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Expression {
    #[serde(rename = "Expression Index")]
    pub index: u32,
    #[serde(rename = "Input Type")]
    pub input_type: String,
    #[serde(rename = "Output Type")]
    pub output_type: String,
    #[serde(rename = "Setup", default, skip_serializing_if = "Vec::is_empty")]
    pub setup: Vec<String>,
    #[serde(rename = "Main")]
    pub main: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum DataType {
    None = 0,
    Imm = 1,
    Bool = 2,
    Int = 3,
    UInt = 4,
    Float = 5,
    String = 6,
    Vector3F = 7,
}

impl DataType {
    fn read(value: u8, version: u32) -> io::Result<Self> {
        let value = if value < 4 || version >= 3 {
            value
        } else {
            value + 1
        };
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Imm),
            2 => Ok(Self::Bool),
            3 => Ok(Self::Int),
            4 => Ok(Self::UInt),
            5 => Ok(Self::Float),
            6 => Ok(Self::String),
            7 => Ok(Self::Vector3F),
            _ => Err(invalid(format!("unknown expression datatype {value}"))),
        }
    }

    fn write(self, version: u32) -> io::Result<u8> {
        let value = self as u8;
        if value < 4 || version >= 3 {
            Ok(value)
        } else if self == Self::UInt {
            Err(invalid("UInt expressions require EXB version 3"))
        } else {
            Ok(value - 1)
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Imm => "IMM",
            Self::Bool => "BOOL",
            Self::Int => "INT",
            Self::UInt => "UINT",
            Self::Float => "FLOAT",
            Self::String => "STRING",
            Self::Vector3F => "VECTOR3F",
        }
    }

    fn from_name(value: &str) -> io::Result<Self> {
        match value {
            "NONE" => Ok(Self::None),
            "IMM" => Ok(Self::Imm),
            "BOOL" => Ok(Self::Bool),
            "INT" => Ok(Self::Int),
            "UINT" => Ok(Self::UInt),
            "FLOAT" => Ok(Self::Float),
            "STRING" => Ok(Self::String),
            "VECTOR3F" => Ok(Self::Vector3F),
            _ => Err(invalid(format!("unknown expression datatype {value}"))),
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::None | Self::Imm => "",
            Self::Bool => "bool ",
            Self::Int => "int ",
            Self::UInt => "uint ",
            Self::Float => "float ",
            Self::String => "str ",
            Self::Vector3F => "vec3f ",
        }
    }

    fn size(self) -> u32 {
        match self {
            Self::String => 8,
            Self::Vector3F => 12,
            _ => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum OperandType {
    Immediate = 0,
    ImmediateString = 1,
    GlobalMemory = 2,
    ParamTable = 3,
    ParamTableString = 4,
    ExpressionOutput = 5,
    ExpressionInput = 6,
    LocalMemory32 = 7,
    LocalMemory64 = 8,
    Output = 9,
    Input = 10,
}

impl OperandType {
    fn read(value: u8) -> io::Result<Self> {
        match value {
            0 => Ok(Self::Immediate),
            1 => Ok(Self::ImmediateString),
            2 => Ok(Self::GlobalMemory),
            3 => Ok(Self::ParamTable),
            4 => Ok(Self::ParamTableString),
            5 => Ok(Self::ExpressionOutput),
            6 => Ok(Self::ExpressionInput),
            7 => Ok(Self::LocalMemory32),
            8 => Ok(Self::LocalMemory64),
            9 => Ok(Self::Output),
            10 => Ok(Self::Input),
            _ => Err(invalid(format!("unknown expression operand type {value}"))),
        }
    }

    fn prefix(self) -> Option<&'static str> {
        match self {
            Self::GlobalMemory => Some("GMem"),
            Self::ExpressionInput => Some("In"),
            Self::ExpressionOutput => Some("Out"),
            Self::LocalMemory32 => Some("LMem32"),
            Self::LocalMemory64 => Some("LMem64"),
            Self::Output => Some("UserOut"),
            Self::Input => Some("UserIn"),
            _ => None,
        }
    }

    fn from_prefix(value: &str) -> Option<Self> {
        match value {
            "GMem" => Some(Self::GlobalMemory),
            "In" => Some(Self::ExpressionInput),
            "Out" => Some(Self::ExpressionOutput),
            "LMem32" => Some(Self::LocalMemory32),
            "LMem64" => Some(Self::LocalMemory64),
            "UserOut" => Some(Self::Output),
            "UserIn" => Some(Self::Input),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum OperandValue {
    Bool(bool),
    Int(i32),
    UInt(u32),
    Float(f32),
    String(String),
    Vector([f32; 3]),
    Offset(u16),
}

#[derive(Clone, Debug)]
struct Operand {
    kind: OperandType,
    datatype: DataType,
    value: OperandValue,
    vector_offset: Option<u16>,
}

#[derive(Clone, Debug)]
enum Instruction {
    End,
    Single(u8, Operand),
    Dual(u8, Operand, Operand),
    Call {
        datatype: DataType,
        args_offset: u16,
        signature: String,
    },
    JumpZero {
        condition: Operand,
        address: u16,
    },
    Jump(u16),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExpressionSizes {
    pub global: u32,
    pub local32: u16,
    pub local64: u16,
    pub io: u16,
}

struct ReadContext<'a> {
    reader: BinaryReader<'a>,
    version: u32,
    pool: usize,
    param_table: usize,
    signatures: Vec<String>,
    operand_storage: Vec<u8>,
}

impl ExpressionModule {
    pub fn from_bytes(data: &[u8]) -> io::Result<Self> {
        let mut reader = BinaryReader::new(data);
        if reader.read_bytes(4)? != b"EXB " {
            return Err(invalid("invalid EXB magic"));
        }
        let version = reader.read_u32()?;
        if !(1..=3).contains(&version) {
            return Err(invalid(format!("unsupported EXB version {version}")));
        }
        reader.skip(16)?;
        let expression_offset = reader.read_u32()? as usize;
        let instruction_offset = reader.read_u32()? as usize;
        let signature_offset = reader.read_u32()? as usize;
        let param_table = reader.read_u32()? as usize;
        let pool = reader.read_u32()? as usize;
        reader.seek(signature_offset)?;
        let signature_count = reader.read_u32()?;
        let signatures = (0..signature_count)
            .map(|_| read_string(&mut reader, pool))
            .collect::<io::Result<Vec<_>>>()?;
        let mut context = ReadContext {
            reader,
            version,
            pool,
            param_table,
            signatures,
            operand_storage: Vec::new(),
        };
        context.reader.seek(instruction_offset)?;
        let instruction_count = context.reader.read_u32()?;
        let instructions = (0..instruction_count)
            .map(|_| read_instruction(&mut context))
            .collect::<io::Result<Vec<_>>>()?;
        context.reader.seek(expression_offset)?;
        let expression_count = context.reader.read_u32()?;
        let mut expressions = Vec::new();
        for index in 0..expression_count {
            let setup_base = context.reader.read_i32()?;
            let setup_count = if version > 1 {
                context.reader.read_u32()? as usize
            } else {
                count_until_end(&instructions, setup_base)
            };
            let main_base = context.reader.read_i32()?;
            let main_count = if version > 1 {
                context.reader.read_u32()? as usize
            } else {
                count_until_end(&instructions, main_base)
            };
            context.reader.read_u32()?;
            context.reader.read_u16()?;
            context.reader.read_u16()?;
            let output_type = DataType::read(context.reader.read_u16()? as u8, version)?;
            let input_type = DataType::read(context.reader.read_u16()? as u8, version)?;
            expressions.push(Expression {
                index,
                input_type: input_type.name().to_owned(),
                output_type: output_type.name().to_owned(),
                setup: instruction_strings(&instructions, setup_base, setup_count)?,
                main: instruction_strings(&instructions, main_base, main_count)?,
            });
        }
        Ok(Self {
            version,
            expressions,
            parameter_table_size: Some((pool - param_table) as u32),
            parameter_table_bytes: data
                .get(param_table..pool)
                .ok_or_else(|| invalid("EXB parameter table lies outside the file"))?
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            operand_storage: context.operand_storage,
        })
    }

    pub fn to_bytes(&self, instance_count: u32) -> io::Result<(Vec<u8>, Vec<ExpressionSizes>)> {
        let mut parsed = self
            .expressions
            .iter()
            .map(|expression| {
                Ok((
                    expression
                        .setup
                        .iter()
                        .map(|value| parse_instruction(value))
                        .collect::<io::Result<Vec<_>>>()?,
                    expression
                        .main
                        .iter()
                        .map(|value| parse_instruction(value))
                        .collect::<io::Result<Vec<_>>>()?,
                ))
            })
            .collect::<io::Result<Vec<_>>>()?;
        apply_operand_storage(&mut parsed, &self.operand_storage)?;
        let mut context = WriteExpressionContext::new(self.version);
        let mut sizes = Vec::new();
        let mut base_setup = Vec::new();
        let mut base_main = Vec::new();
        for (setup, main) in &parsed {
            base_main.push(context.instruction_count);
            let main_sizes = preprocess_instructions(main, &mut context)?;
            base_setup.push(if setup.is_empty() {
                -1
            } else {
                context.instruction_count as i32
            });
            let setup_sizes = preprocess_instructions(setup, &mut context)?;
            sizes.push(ExpressionSizes {
                global: main_sizes.global.max(setup_sizes.global),
                local32: main_sizes.local32.max(setup_sizes.local32),
                local64: main_sizes.local64.max(setup_sizes.local64),
                io: main_sizes.io.max(setup_sizes.io),
            });
        }
        let mut writer = std::mem::take(&mut context.writer);
        writer.write_bytes(b"EXB ");
        writer.write_u32(self.version);
        writer.write_u32(sizes.iter().map(|size| size.global).max().unwrap_or(0));
        writer.write_u32(instance_count);
        writer.write_u32(sizes.iter().map(|size| size.local32 as u32).sum());
        writer.write_u32(sizes.iter().map(|size| size.local64 as u32).sum());
        let expression_offset = 0x2c;
        writer.write_u32(expression_offset);
        let expression_record_size = if self.version > 1 { 0x1c } else { 0x14 };
        let instruction_offset =
            expression_offset + 4 + expression_record_size * self.expressions.len() as u32;
        writer.write_u32(instruction_offset);
        let signature_offset = instruction_offset + 4 + context.instruction_count * 8;
        writer.write_u32(signature_offset);
        let signature_table_offset = signature_offset + 4 + context.signatures.len() as u32 * 4;
        writer.write_u32(signature_table_offset);
        let parameter_table_size = self
            .parameter_table_size
            .unwrap_or(context.param_table_size);
        writer.write_u32(signature_table_offset + parameter_table_size);
        writer.write_u32(self.expressions.len() as u32);
        for (index, expression) in self.expressions.iter().enumerate() {
            writer.write_i32(base_setup[index]);
            if self.version > 1 {
                writer.write_u32(parsed[index].0.len() as u32);
            }
            writer.write_i32(base_main[index] as i32);
            if self.version > 1 {
                writer.write_u32(parsed[index].1.len() as u32);
            }
            writer.write_u32(sizes[index].global);
            writer.write_u16(sizes[index].local32);
            writer.write_u16(sizes[index].local64);
            writer.write_u16(
                DataType::from_name(&expression.output_type)?.write(self.version)? as u16,
            );
            writer.write_u16(
                DataType::from_name(&expression.input_type)?.write(self.version)? as u16,
            );
        }
        writer.write_u32(context.instruction_count);
        for (setup, main) in &parsed {
            for instruction in setup.iter().chain(main) {
                write_instruction(&mut writer, instruction, &context)?;
            }
        }
        writer.write_u32(context.signatures.len() as u32);
        for signature in &context.signatures {
            writer.write_u32(writer.string_offset(signature)?);
        }
        let parameter_table_start = writer.position();
        for parameter in &context.parameters {
            match parameter {
                OperandValue::Bool(value) => writer.write_u32(*value as u32),
                OperandValue::Int(value) => writer.write_i32(*value),
                OperandValue::UInt(value) => writer.write_u32(*value),
                OperandValue::Float(value) => writer.write_f32(*value),
                OperandValue::String(value) => writer.write_string_offset(value),
                OperandValue::Vector(value) => writer.write_vec3(*value),
                OperandValue::Offset(_) => {
                    return Err(invalid("invalid EXB parameter-table value"))
                }
            }
        }
        let parameter_table_end = parameter_table_start + parameter_table_size as usize;
        if writer.position() > parameter_table_end {
            writer.truncate(parameter_table_end);
        } else if writer.position() < parameter_table_end {
            let required = parameter_table_end - writer.position();
            let raw = decode_hex(&self.parameter_table_bytes)?;
            if raw.len() >= required {
                writer.write_bytes(&raw[raw.len() - required..]);
            } else {
                writer.write_bytes(&raw);
                writer.write_bytes(&vec![0; required - raw.len()]);
            }
        }
        writer.write_string_pool();
        Ok((writer.into_inner(), sizes))
    }
}

fn decode_hex(value: &str) -> io::Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(invalid("EXB parameter table has an odd hex length"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| invalid("EXB parameter table contains invalid hex"))
        })
        .collect()
}

fn apply_operand_storage(
    expressions: &mut [(Vec<Instruction>, Vec<Instruction>)],
    storage: &[u8],
) -> io::Result<()> {
    let mut kinds = storage.iter().copied();
    for instruction in expressions
        .iter_mut()
        .flat_map(|(setup, main)| setup.iter_mut().chain(main.iter_mut()))
    {
        let mut apply = |operand: &mut Operand| -> io::Result<()> {
            if let Some(raw) = kinds.next() {
                operand.kind = OperandType::read(raw)?;
            }
            Ok(())
        };
        match instruction {
            Instruction::Single(_, operand) => apply(operand)?,
            Instruction::Dual(_, first, second) => {
                apply(first)?;
                apply(second)?;
            }
            Instruction::JumpZero { condition, .. } => apply(condition)?,
            Instruction::End | Instruction::Call { .. } | Instruction::Jump(_) => {}
        }
    }
    Ok(())
}

fn count_until_end(instructions: &[Instruction], base: i32) -> usize {
    if base < 0 {
        return 0;
    }
    let Some(tail) = instructions.get(base as usize..) else {
        return 0;
    };
    tail.iter()
        .position(|instruction| matches!(instruction, Instruction::End))
        .map_or(tail.len(), |index| index + 1)
}

fn instruction_strings(
    instructions: &[Instruction],
    base: i32,
    count: usize,
) -> io::Result<Vec<String>> {
    if base < 0 {
        return Ok(Vec::new());
    }
    instructions
        .get(base as usize..base as usize + count)
        .ok_or_else(|| invalid("expression instruction range exceeds table"))?
        .iter()
        .enumerate()
        .map(|(index, instruction)| {
            Ok(format!(
                "{:#06x}    {}",
                index * 8,
                format_instruction(instruction)?
            ))
        })
        .collect()
}

fn read_instruction(context: &mut ReadContext<'_>) -> io::Result<Instruction> {
    let opcode = context.reader.read_u8()?;
    match opcode {
        1 => {
            context.reader.skip(7)?;
            Ok(Instruction::End)
        }
        2 | 5..=9 | 12..=26 => {
            let datatype = DataType::read(context.reader.read_u8()?, context.version)?;
            let first_raw = context.reader.read_u8()?;
            let second_raw = context.reader.read_u8()?;
            context.operand_storage.extend([first_raw, second_raw]);
            let first_type = OperandType::read(first_raw)?;
            let second_type = OperandType::read(second_raw)?;
            let first = read_operand(context, datatype, first_type)?;
            let second = read_operand(context, datatype, second_type)?;
            Ok(Instruction::Dual(opcode, first, second))
        }
        3 | 4 | 10 | 11 => {
            let datatype = DataType::read(context.reader.read_u8()?, context.version)?;
            let operand_raw = context.reader.read_u8()?;
            context.operand_storage.push(operand_raw);
            let operand_type = OperandType::read(operand_raw)?;
            context.reader.read_u8()?;
            let operand = read_operand(context, datatype, operand_type)?;
            context.reader.read_u16()?;
            Ok(Instruction::Single(opcode, operand))
        }
        27 => {
            let datatype = DataType::read(context.reader.read_u8()?, context.version)?;
            let args_offset = context.reader.read_u16()?;
            let index = context.reader.read_u32()? as usize;
            Ok(Instruction::Call {
                datatype,
                args_offset,
                signature: context
                    .signatures
                    .get(index)
                    .cloned()
                    .ok_or_else(|| invalid("EXB signature index exceeds table"))?,
            })
        }
        28 => {
            let datatype = DataType::read(context.reader.read_u8()?, context.version)?;
            let operand_raw = context.reader.read_u8()?;
            context.operand_storage.push(operand_raw);
            let operand_type = OperandType::read(operand_raw)?;
            context.reader.read_u8()?;
            let condition = read_operand(context, datatype, operand_type)?;
            let address = context.reader.read_u16()? * 8;
            Ok(Instruction::JumpZero { condition, address })
        }
        29 => {
            context.reader.skip(5)?;
            Ok(Instruction::Jump(context.reader.read_u16()? * 8))
        }
        _ => Err(invalid(format!("unsupported expression opcode {opcode}"))),
    }
}

fn read_operand(
    context: &mut ReadContext<'_>,
    datatype: DataType,
    kind: OperandType,
) -> io::Result<Operand> {
    let raw = context.reader.read_u16()?;
    let position = context.reader.position();
    let value = match kind {
        OperandType::Immediate => match datatype {
            DataType::Bool => OperandValue::Bool(raw != 0),
            DataType::Int => OperandValue::Int(raw as i32),
            DataType::UInt => OperandValue::UInt(raw as u32),
            DataType::Float => OperandValue::Float(raw as f32),
            DataType::None => OperandValue::Offset(raw),
            _ => return Err(invalid("invalid immediate expression datatype")),
        },
        OperandType::ImmediateString => OperandValue::String(
            context
                .reader
                .read_c_string_at(context.pool + raw as usize)?,
        ),
        OperandType::ParamTable | OperandType::ParamTableString => {
            context.reader.seek(context.param_table + raw as usize)?;
            let value = match datatype {
                DataType::Bool => OperandValue::Bool(context.reader.read_u32()? != 0),
                DataType::Int => OperandValue::Int(context.reader.read_i32()?),
                DataType::UInt => OperandValue::UInt(context.reader.read_u32()?),
                DataType::Float => OperandValue::Float(context.reader.read_f32()?),
                DataType::String => {
                    OperandValue::String(read_string(&mut context.reader, context.pool)?)
                }
                DataType::Vector3F => OperandValue::Vector([
                    context.reader.read_f32()?,
                    context.reader.read_f32()?,
                    context.reader.read_f32()?,
                ]),
                _ => return Err(invalid("invalid parameter-table datatype")),
            };
            context.reader.seek(position)?;
            value
        }
        _ => OperandValue::Offset(raw & if raw >> 15 != 0 { 0xff } else { 0xffff }),
    };
    let vector_offset = if matches!(kind, OperandType::Input | OperandType::Output)
        && datatype == DataType::Float
        && raw >> 15 != 0
    {
        Some((raw & 0x7f00) >> 8)
    } else {
        None
    };
    Ok(Operand {
        kind,
        datatype,
        value,
        vector_offset,
    })
}

fn format_instruction(instruction: &Instruction) -> io::Result<String> {
    Ok(match instruction {
        Instruction::End => "END".to_owned(),
        Instruction::Single(opcode, operand) => {
            format!("{} {}", opcode_name(*opcode)?, format_operand(operand)?)
        }
        Instruction::Dual(opcode, first, second) => format!(
            "{} {}, {}",
            opcode_name(*opcode)?,
            format_operand(first)?,
            format_operand(second)?
        ),
        Instruction::Call {
            args_offset,
            signature,
            ..
        } => format!("CFN {signature}, GMem[{args_offset:#x}]"),
        Instruction::JumpZero { condition, address } => {
            format!("JZE {}, {address:#x}", format_operand(condition)?)
        }
        Instruction::Jump(address) => format!("JMP {address:#x}"),
    })
}

fn format_operand(operand: &Operand) -> io::Result<String> {
    let prefix = operand.datatype.prefix();
    Ok(match &operand.value {
        OperandValue::Bool(value) => format!("{prefix}{}", if *value { "True" } else { "False" }),
        OperandValue::Int(value) => format!("{prefix}{value}"),
        OperandValue::UInt(value) => format!("{prefix}{value}"),
        OperandValue::Float(value) => format!("{prefix}{value}"),
        OperandValue::String(value) => format!("{prefix}\"{value}\""),
        OperandValue::Vector(value) => {
            format!("{prefix}({}, {}, {})", value[0], value[1], value[2])
        }
        OperandValue::Offset(value) => {
            let source = operand
                .kind
                .prefix()
                .ok_or_else(|| invalid("offset operand has no memory prefix"))?;
            let component = match operand.vector_offset {
                None => "",
                Some(0) => ".x",
                Some(4) => ".y",
                Some(8) => ".z",
                Some(_) => return Err(invalid("invalid vector component offset")),
            };
            format!("{prefix}{source}[{value:#x}]{component}")
        }
    })
}

fn parse_instruction(value: &str) -> io::Result<Instruction> {
    let text = value
        .split_once("    ")
        .map(|(_, instruction)| instruction)
        .unwrap_or(value)
        .trim();
    let (opcode_name, arguments) = text.split_once(' ').unwrap_or((text, ""));
    let opcode = opcode_value(opcode_name)?;
    match opcode {
        1 => Ok(Instruction::End),
        3 | 4 | 10 | 11 => Ok(Instruction::Single(opcode, parse_operand(arguments)?)),
        2 | 5..=9 | 12..=26 => {
            let (first, second) = split_arguments(arguments)?;
            Ok(Instruction::Dual(
                opcode,
                parse_operand(first)?,
                parse_operand(second)?,
            ))
        }
        27 => {
            let marker = ", GMem[";
            let split = arguments
                .rfind(marker)
                .ok_or_else(|| invalid("invalid CFN instruction"))?;
            let signature = arguments[..split].to_owned();
            let args_offset = parse_u16(arguments[split + marker.len()..].trim_end_matches(']'))?;
            let return_name = signature
                .rfind('(')
                .and_then(|start| {
                    signature[start + 1..]
                        .split(|character: char| character.is_whitespace() || character == ',')
                        .find(|value| !value.is_empty())
                })
                .ok_or_else(|| invalid("CFN signature has no return datatype"))?;
            Ok(Instruction::Call {
                datatype: DataType::from_name(&return_name.to_ascii_uppercase())?,
                args_offset,
                signature,
            })
        }
        28 => {
            let (condition, address) = split_arguments(arguments)?;
            Ok(Instruction::JumpZero {
                condition: parse_operand(condition)?,
                address: parse_u16(address)?,
            })
        }
        29 => Ok(Instruction::Jump(parse_u16(arguments)?)),
        _ => Err(invalid("unsupported expression instruction")),
    }
}

fn parse_operand(text: &str) -> io::Result<Operand> {
    let text = text.trim();
    let (datatype, argument) = [
        ("bool ", DataType::Bool),
        ("int ", DataType::Int),
        ("uint ", DataType::UInt),
        ("float ", DataType::Float),
        ("str ", DataType::String),
        ("vec3f ", DataType::Vector3F),
    ]
    .into_iter()
    .find_map(|(prefix, datatype)| text.strip_prefix(prefix).map(|value| (datatype, value)))
    .unwrap_or((DataType::None, text));
    if let Some(bracket) = argument.find('[') {
        let source = &argument[..bracket];
        let end = argument[bracket + 1..]
            .find(']')
            .map(|index| bracket + 1 + index)
            .ok_or_else(|| invalid("unterminated memory operand"))?;
        let offset = parse_u16(&argument[bracket + 1..end])?;
        let vector_offset = match argument.get(end + 1..) {
            Some(".x") => Some(0),
            Some(".y") => Some(4),
            Some(".z") => Some(8),
            _ => None,
        };
        return Ok(Operand {
            kind: OperandType::from_prefix(source)
                .ok_or_else(|| invalid(format!("unknown memory source {source}")))?,
            datatype,
            value: OperandValue::Offset(offset),
            vector_offset,
        });
    }
    let quoted = argument
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'));
    let vector = argument
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'));
    let (datatype, value) = if let Some(text) = quoted {
        (DataType::String, OperandValue::String(text.to_owned()))
    } else if let Some(text) = vector {
        let values = text
            .split(',')
            .map(|value| {
                value
                    .trim()
                    .parse::<f32>()
                    .map_err(|error| invalid(error.to_string()))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if values.len() != 3 {
            return Err(invalid("vector operand must have three values"));
        }
        (
            DataType::Vector3F,
            OperandValue::Vector([values[0], values[1], values[2]]),
        )
    } else {
        match datatype {
            DataType::Bool => (
                datatype,
                OperandValue::Bool(argument.eq_ignore_ascii_case("true")),
            ),
            DataType::Int => (datatype, OperandValue::Int(parse_i32(argument)?)),
            DataType::UInt => (datatype, OperandValue::UInt(parse_u32(argument)?)),
            DataType::Float => (
                datatype,
                OperandValue::Float(
                    argument
                        .parse()
                        .map_err(|error| invalid(format!("invalid float: {error}")))?,
                ),
            ),
            _ => return Err(invalid(format!("invalid immediate operand {argument}"))),
        }
    };
    Ok(Operand {
        kind: OperandType::Immediate,
        datatype,
        value,
        vector_offset: None,
    })
}

struct WriteExpressionContext {
    version: u32,
    writer: AinbWriter,
    instruction_count: u32,
    parameters: Vec<OperandValue>,
    param_offsets: Vec<u32>,
    param_table_size: u32,
    signatures: Vec<String>,
}

impl WriteExpressionContext {
    fn new(version: u32) -> Self {
        Self {
            version,
            writer: AinbWriter::new(),
            instruction_count: 0,
            parameters: Vec::new(),
            param_offsets: Vec::new(),
            param_table_size: 0,
            signatures: Vec::new(),
        }
    }

    fn prepare_operand(&mut self, operand: &Operand) -> io::Result<()> {
        if matches!(
            operand.kind,
            OperandType::ParamTable | OperandType::ParamTableString
        ) {
            self.add_parameter(operand.value.clone());
            return Ok(());
        }
        match &operand.value {
            OperandValue::String(value) => {
                let offset = self.writer.add_string(value);
                if offset > u16::MAX as u32 {
                    self.add_parameter(operand.value.clone());
                }
            }
            OperandValue::Vector(_) => self.add_parameter(operand.value.clone()),
            OperandValue::Int(value) if *value > u16::MAX as i32 => {
                self.add_parameter(operand.value.clone())
            }
            OperandValue::UInt(value) if *value > u16::MAX as u32 => {
                self.add_parameter(operand.value.clone())
            }
            OperandValue::Float(value)
                if *value > 65535.0 || *value < 0.0 || value.fract() != 0.0 || self.version < 2 =>
            {
                self.add_parameter(operand.value.clone())
            }
            _ => {}
        }
        Ok(())
    }

    fn add_parameter(&mut self, value: OperandValue) {
        if self.parameters.contains(&value) {
            return;
        }
        self.param_offsets.push(self.param_table_size);
        self.param_table_size += match value {
            OperandValue::Vector(_) => 12,
            _ => 4,
        };
        self.parameters.push(value);
    }

    fn parameter_offset(&self, value: &OperandValue) -> io::Result<u16> {
        let index = self
            .parameters
            .iter()
            .position(|candidate| candidate == value)
            .ok_or_else(|| invalid("missing EXB parameter value"))?;
        u16::try_from(self.param_offsets[index])
            .map_err(|_| invalid("EXB parameter table offset exceeds u16"))
    }
}

fn preprocess_instructions(
    instructions: &[Instruction],
    context: &mut WriteExpressionContext,
) -> io::Result<ExpressionSizes> {
    let mut sizes = ExpressionSizes::default();
    for instruction in instructions {
        context.instruction_count += 1;
        match instruction {
            Instruction::Single(_, operand) => {
                update_size(&mut sizes, operand);
                context.prepare_operand(operand)?;
            }
            Instruction::Dual(opcode, first, second) => {
                update_size(&mut sizes, first);
                update_size(&mut sizes, second);
                if matches!(opcode, 12 | 13) {
                    update_size_with(&mut sizes, first, 4);
                }
                context.prepare_operand(first)?;
                context.prepare_operand(second)?;
            }
            Instruction::Call { signature, .. } => {
                if !context.signatures.contains(signature) {
                    context.writer.add_string(signature);
                    context.signatures.push(signature.clone());
                }
            }
            Instruction::JumpZero { condition, .. } => {
                update_size_with(&mut sizes, condition, 1);
                context.prepare_operand(condition)?;
            }
            Instruction::End | Instruction::Jump(_) => {}
        }
    }
    Ok(sizes)
}

fn update_size(sizes: &mut ExpressionSizes, operand: &Operand) {
    update_size_with(sizes, operand, operand.datatype.size());
}

fn update_size_with(sizes: &mut ExpressionSizes, operand: &Operand, size: u32) {
    let OperandValue::Offset(offset) = operand.value else {
        return;
    };
    let end = offset as u32 + size;
    match operand.kind {
        OperandType::ExpressionInput | OperandType::ExpressionOutput => {
            sizes.io = sizes.io.max(end as u16)
        }
        OperandType::GlobalMemory => sizes.global = sizes.global.max(end),
        OperandType::LocalMemory32 => sizes.local32 = sizes.local32.max(end as u16),
        OperandType::LocalMemory64 => sizes.local64 = sizes.local64.max(end as u16),
        _ => {}
    }
}

fn write_instruction(
    writer: &mut AinbWriter,
    instruction: &Instruction,
    context: &WriteExpressionContext,
) -> io::Result<()> {
    match instruction {
        Instruction::End => {
            writer.write_u8(1);
            writer.write_bytes(&[0; 7]);
        }
        Instruction::Single(opcode, operand) => {
            writer.write_u8(*opcode);
            writer.write_u8(operand.datatype.write(context.version)?);
            writer.write_u8(write_operand_type(operand, context)? as u8);
            writer.write_u8(0);
            writer.write_u16(write_operand_value(writer, operand, context)?);
            writer.write_u16(0);
        }
        Instruction::Dual(opcode, first, second) => {
            writer.write_u8(*opcode);
            writer.write_u8(first.datatype.write(context.version)?);
            writer.write_u8(write_operand_type(first, context)? as u8);
            writer.write_u8(write_operand_type(second, context)? as u8);
            writer.write_u16(write_operand_value(writer, first, context)?);
            writer.write_u16(write_operand_value(writer, second, context)?);
        }
        Instruction::Call {
            datatype,
            args_offset,
            signature,
        } => {
            writer.write_u8(27);
            writer.write_u8(datatype.write(context.version)?);
            writer.write_u16(*args_offset);
            writer.write_u32(
                context
                    .signatures
                    .iter()
                    .position(|value| value == signature)
                    .ok_or_else(|| invalid("missing EXB signature"))? as u32,
            );
        }
        Instruction::JumpZero { condition, address } => {
            writer.write_u8(28);
            writer.write_u8(condition.datatype.write(context.version)?);
            writer.write_u8(write_operand_type(condition, context)? as u8);
            writer.write_u8(0);
            writer.write_u16(write_operand_value(writer, condition, context)?);
            writer.write_u16(address / 8);
        }
        Instruction::Jump(address) => {
            writer.write_u8(29);
            writer.write_bytes(&[0; 5]);
            writer.write_u16(address / 8);
        }
    }
    Ok(())
}

fn write_operand_type(
    operand: &Operand,
    context: &WriteExpressionContext,
) -> io::Result<OperandType> {
    if matches!(
        operand.kind,
        OperandType::ParamTable | OperandType::ParamTableString
    ) {
        return Ok(operand.kind);
    }
    if !matches!(
        operand.kind,
        OperandType::Immediate
            | OperandType::ImmediateString
            | OperandType::ParamTable
            | OperandType::ParamTableString
    ) {
        return Ok(operand.kind);
    }
    Ok(match &operand.value {
        OperandValue::String(value) if context.writer.string_offset(value)? <= u16::MAX as u32 => {
            OperandType::ImmediateString
        }
        OperandValue::String(_) => OperandType::ParamTableString,
        OperandValue::Vector(_) => OperandType::ParamTable,
        OperandValue::Int(value) if *value > u16::MAX as i32 => OperandType::ParamTable,
        OperandValue::UInt(value) if *value > u16::MAX as u32 => OperandType::ParamTable,
        OperandValue::Float(value)
            if *value > 65535.0 || *value < 0.0 || value.fract() != 0.0 || context.version < 2 =>
        {
            OperandType::ParamTable
        }
        _ => OperandType::Immediate,
    })
}

fn write_operand_value(
    writer: &AinbWriter,
    operand: &Operand,
    context: &WriteExpressionContext,
) -> io::Result<u16> {
    let kind = write_operand_type(operand, context)?;
    if matches!(
        kind,
        OperandType::ParamTable | OperandType::ParamTableString
    ) {
        return context.parameter_offset(&operand.value);
    }
    Ok(match &operand.value {
        OperandValue::Bool(value) => *value as u16,
        OperandValue::Int(value) => *value as u16,
        OperandValue::UInt(value) => *value as u16,
        OperandValue::Float(value) => *value as u16,
        OperandValue::String(value) => writer.string_offset(value)? as u16,
        OperandValue::Offset(value) => {
            *value
                | operand
                    .vector_offset
                    .map(|offset| 0x8000 | offset << 8)
                    .unwrap_or(0)
        }
        OperandValue::Vector(_) => return Err(invalid("vector must use EXB parameter table")),
    })
}

fn opcode_name(value: u8) -> io::Result<&'static str> {
    OPCODES
        .get(value as usize)
        .and_then(|value| *value)
        .ok_or_else(|| invalid(format!("unknown expression opcode {value}")))
}

fn opcode_value(name: &str) -> io::Result<u8> {
    OPCODES
        .iter()
        .position(|candidate| candidate.is_some_and(|value| value == name))
        .map(|value| value as u8)
        .ok_or_else(|| invalid(format!("unknown expression opcode {name}")))
}

const OPCODES: [Option<&str>; 30] = [
    None,
    Some("END"),
    Some("STR"),
    Some("NEG"),
    Some("NOT"),
    Some("ADD"),
    Some("SUB"),
    Some("MUL"),
    Some("DIV"),
    Some("MOD"),
    Some("INC"),
    Some("DEC"),
    Some("VMS"),
    Some("VDS"),
    Some("LSH"),
    Some("RSH"),
    Some("LST"),
    Some("LTE"),
    Some("GRT"),
    Some("GTE"),
    Some("EQL"),
    Some("NEQ"),
    Some("AND"),
    Some("XOR"),
    Some("ORR"),
    Some("LAN"),
    Some("LOR"),
    Some("CFN"),
    Some("JZE"),
    Some("JMP"),
];

fn split_arguments(value: &str) -> io::Result<(&str, &str)> {
    let mut depth = 0;
    let mut quoted = false;
    for (index, character) in value.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                return Ok((value[..index].trim(), value[index + 1..].trim()))
            }
            _ => {}
        }
    }
    Err(invalid("instruction requires two arguments"))
}

fn parse_u16(value: &str) -> io::Result<u16> {
    u16::try_from(parse_u32(value)?).map_err(|_| invalid("value exceeds u16"))
}

fn parse_u32(value: &str) -> io::Result<u32> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).map_err(|error| invalid(error.to_string()))
    } else if let Some(binary) = value.strip_prefix("0b") {
        u32::from_str_radix(binary, 2).map_err(|error| invalid(error.to_string()))
    } else {
        value.parse().map_err(|error| invalid(format!("{error}")))
    }
}

fn parse_i32(value: &str) -> io::Result<i32> {
    let value = value.trim();
    let (negative, value) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let parsed = parse_u32(value)?;
    let parsed = i32::try_from(parsed).map_err(|_| invalid("value exceeds i32"))?;
    Ok(if negative { -parsed } else { parsed })
}

fn read_string(reader: &mut BinaryReader<'_>, pool: usize) -> io::Result<String> {
    let offset = reader.read_u32()? as usize;
    reader.read_c_string_at(pool + offset)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

//! "Add ELink" tool: clones a user of `ELink2/elink2.Product.*.belnk.zs` under
//! a new name with edited asset-call parameters (position, rotation, scale,
//! color, emission, bone, emitter set), clones the user's own emitter-set file
//! `Effect/<user>.Nin_NX_NVN.esetb.byml.zs` under the new name and refreshes
//! the output mod's RSTB.
//!
//! The ELink text produced by the native converter is a brace tree: a line
//! ending in `{` opens a node, a lone `}` closes it and everything else is a
//! leaf (`Key = Value` or a bare property name). Users sit at indent 2 under
//! `Users {`; their asset calls are the `Execute = Asset {` nodes below
//! `AssetCallTables`. The converter re-hashes and re-sorts users when the text
//! is turned back into binary, so a renamed copy of a block is a new user.

use crate::{
    file_format::Xlink::Xlink_rs, tools::items_creator::rstb::ModRstbProcessor, Zstd::TotkZstd,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};

/// File name suffix of a user's emitter-set file below `Effect/`.
pub const ESETB_SUFFIX: &str = ".Nin_NX_NVN.esetb.byml.zs";
/// Width of the file-name slot in the PTCL header (`VFXB` + 0x20).
const PTCL_NAME_SLOT: usize = 32;
const PTCL_NAME_OFFSET: usize = 0x20;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamKind {
    Float,
    Text,
}

/// An editable asset-call parameter with its ELink default (the value the
/// converter omits from the text).
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: ParamKind,
    pub default: &'static str,
}

const fn float(name: &'static str, default: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        kind: ParamKind::Float,
        default,
    }
}

const fn text(name: &'static str) -> ParamSpec {
    ParamSpec {
        name,
        kind: ParamKind::Text,
        default: "",
    }
}

/// Parameters the tool exposes, in `SystemAssetParams` order.
pub const EDITABLE_PARAMS: &[ParamSpec] = &[
    text("RuntimeAssetName"),
    text("EffectGroup"),
    text("EffectPauseGroup"),
    float("Delay", "0.0"),
    float("Duration", "0.0"),
    text("Bone"),
    float("Scale", "1.0"),
    float("PositionX", "0.0"),
    float("PositionY", "0.0"),
    float("PositionZ", "0.0"),
    float("RotationX", "0.0"),
    float("RotationY", "0.0"),
    float("RotationZ", "0.0"),
    float("Red", "1.0"),
    float("Green", "1.0"),
    float("Blue", "1.0"),
    float("Alpha", "1.0"),
    float("EmissionRate", "1.0"),
    float("EmissionScale", "1.0"),
    float("EmissionInterval", "1.0"),
    float("DirectionalVel", "1.0"),
    float("LifeScale", "1.0"),
];

fn param_spec(name: &str) -> Option<&'static ParamSpec> {
    EDITABLE_PARAMS.iter().find(|spec| spec.name == name)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

// ---------------------------------------------------------------------------
// Text tree
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Node {
    /// Opening line without the trailing `{`.
    label: String,
    indent: usize,
    /// Index of the opening line.
    start: usize,
    /// Index of the closing `}` line.
    end: usize,
    children: Vec<Node>,
    /// Indices of the leaf lines directly inside this node.
    leaves: Vec<usize>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn is_opener(trimmed: &str) -> bool {
    trimmed.ends_with('{') && !trimmed.ends_with("\"{")
}

fn open_node(lines: &[&str], index: usize) -> Node {
    let line = lines[index];
    let trimmed = line.trim();
    Node {
        label: trimmed[..trimmed.len() - 1].trim_end().to_owned(),
        indent: indent_of(line),
        start: index,
        end: index,
        children: Vec::new(),
        leaves: Vec::new(),
    }
}

/// Parses the block whose opening line is `lines[start]`.
fn parse_block(lines: &[&str], start: usize) -> io::Result<Node> {
    if !is_opener(lines[start].trim()) {
        return Err(invalid(format!(
            "ELink line {} does not open a block",
            start + 1
        )));
    }
    let mut stack = vec![open_node(lines, start)];
    let mut index = start + 1;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed == "}" {
            let mut node = stack.pop().expect("stack holds the block being closed");
            node.end = index;
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => return Ok(node),
            }
        } else if is_opener(trimmed) {
            stack.push(open_node(lines, index));
        } else if !trimmed.is_empty() {
            stack
                .last_mut()
                .expect("stack is never empty inside a block")
                .leaves
                .push(index);
        }
        index += 1;
    }
    Err(invalid(format!(
        "ELink block opened on line {} is never closed",
        start + 1
    )))
}

/// Line index of `Users {` at indent 0.
fn users_start(lines: &[&str]) -> io::Result<usize> {
    lines
        .iter()
        .position(|line| *line == "Users {")
        .ok_or_else(|| invalid("ELink text has no Users block"))
}

/// Line index of the opening line of `user` inside `Users`.
fn user_start(lines: &[&str], user: &str) -> Option<usize> {
    let users = users_start(lines).ok()?;
    let wanted = format!("  {user} {{");
    lines[users + 1..]
        .iter()
        .position(|line| *line == wanted)
        .map(|offset| users + 1 + offset)
}

/// Names of every user of the ELink text, in file order.
pub fn list_users(text: &str) -> io::Result<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    let users = users_start(&lines)?;
    let mut names = Vec::new();
    for line in &lines[users + 1..] {
        if *line == "}" {
            break;
        }
        if indent_of(line) == 2 {
            if let Some(name) = line.trim().strip_suffix(" {") {
                names.push(name.to_owned());
            }
        }
    }
    Ok(names)
}

// ---------------------------------------------------------------------------
// Asset entries
// ---------------------------------------------------------------------------

/// One `Execute = Asset` call of a user, in depth-first order.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntry {
    /// Position in the user's depth-first asset list; the edit key.
    pub id: usize,
    /// Top-level asset-call-table entry the call belongs to (e.g. `Blade`).
    pub table: String,
    /// Human readable location: table, blend/switch branches, asset name.
    pub path: String,
    pub asset_name: String,
    /// Current values of the editable parameters that the text spells out
    /// (strings unquoted, floats as written, `CURVE` for curve-driven ones).
    pub params: BTreeMap<String, String>,
}

fn strip_hash(label: &str) -> &str {
    match label.rfind('[') {
        Some(open) if label.ends_with(']') && label[open + 1..].starts_with("0x") => &label[..open],
        _ => label,
    }
}

/// Display name of a node between `AssetCallTables` and an asset call, or
/// `None` for structural nodes (`Execute = Blend`, `Execute = Switch …`).
fn display_label(label: &str, switch_variable: Option<&str>) -> Option<String> {
    if label.starts_with("Execute = ") {
        return None;
    }
    if let Some((condition, name)) = label.split_once(" => ") {
        let name = strip_hash(name.trim());
        let condition = condition.trim();
        let condition = condition
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix(')'))
            .unwrap_or(condition);
        let condition = if condition == "_" {
            "otherwise".to_owned()
        } else {
            condition.replace("<value>", switch_variable.unwrap_or("value"))
        };
        return Some(format!("{name} ⟨{condition}⟩"));
    }
    Some(strip_hash(label).to_owned())
}

fn switch_variable(label: &str) -> Option<&str> {
    let inner = label
        .strip_prefix("Execute = Switch (")?
        .strip_suffix(')')?;
    Some(inner.rsplit("::").next().unwrap_or(inner))
}

fn leaf_key_value(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let (key, value) = trimmed.split_once(" = ")?;
    Some((key.trim(), value.trim()))
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

struct AssetNode<'a> {
    node: &'a Node,
    table: String,
    path: String,
    asset_name: String,
}

fn collect_assets<'a>(
    node: &'a Node,
    lines: &[&str],
    trail: &mut Vec<String>,
    switch: Option<&str>,
    out: &mut Vec<AssetNode<'a>>,
) {
    for child in &node.children {
        if child.label == "Execute = Asset" {
            let asset_name = child
                .leaves
                .iter()
                .filter_map(|&index| leaf_key_value(lines[index]))
                .find(|(key, _)| *key == "AssetName")
                .map(|(_, value)| unquote(value).to_owned())
                .unwrap_or_default();
            let mut path = trail.clone();
            let repeats_name = path.last().map_or(false, |last| {
                last == &asset_name || last.starts_with(&format!("{asset_name} ⟨"))
            });
            if !repeats_name {
                path.push(asset_name.clone());
            }
            out.push(AssetNode {
                node: child,
                table: trail.first().cloned().unwrap_or_default(),
                path: path.join(" › "),
                asset_name,
            });
            continue;
        }
        let variable = switch_variable(&child.label).or(switch);
        let label = display_label(&child.label, switch);
        let pushed = label.is_some();
        if let Some(label) = label {
            trail.push(label);
        }
        collect_assets(child, lines, trail, variable, out);
        if pushed {
            trail.pop();
        }
    }
}

fn user_asset_nodes<'a>(user_block: &'a Node, lines: &[&str]) -> Vec<AssetNode<'a>> {
    let mut out = Vec::new();
    if let Some(tables) = user_block
        .children
        .iter()
        .find(|child| child.label == "AssetCallTables")
    {
        collect_assets(tables, lines, &mut Vec::new(), None, &mut out);
    }
    out
}

fn entry_params(node: &Node, lines: &[&str]) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    for &index in &node.leaves {
        if let Some((key, value)) = leaf_key_value(lines[index]) {
            if let Some(spec) = param_spec(key) {
                let value = match spec.kind {
                    ParamKind::Text => unquote(value).to_owned(),
                    ParamKind::Float => value.to_owned(),
                };
                params.insert(key.to_owned(), value);
            }
        }
    }
    params
}

/// The asset calls of `user` with their current editable parameters.
pub fn user_assets(text: &str, user: &str) -> io::Result<Vec<AssetEntry>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = user_start(&lines, user).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("ELink user {user} not found"),
        )
    })?;
    let block = parse_block(&lines, start)?;
    Ok(user_asset_nodes(&block, &lines)
        .into_iter()
        .enumerate()
        .map(|(id, asset)| AssetEntry {
            id,
            table: asset.table,
            path: asset.path,
            asset_name: asset.asset_name,
            params: entry_params(asset.node, &lines),
        })
        .collect())
}

/// Number of asset calls of every user (for the picker).
pub fn asset_counts(text: &str) -> io::Result<BTreeMap<String, usize>> {
    let lines: Vec<&str> = text.lines().collect();
    let users = users_start(&lines)?;
    let mut counts = BTreeMap::new();
    let mut index = users + 1;
    while index < lines.len() && lines[index] != "}" {
        if indent_of(lines[index]) == 2 && is_opener(lines[index].trim()) {
            let block = parse_block(&lines, index)?;
            let name = block.label.clone();
            counts.insert(name, user_asset_nodes(&block, &lines).len());
            index = block.end + 1;
        } else {
            index += 1;
        }
    }
    Ok(counts)
}

// ---------------------------------------------------------------------------
// Editing
// ---------------------------------------------------------------------------

/// Parameter changes of one asset call: `None` restores the ELink default.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntryEdit {
    pub id: usize,
    #[serde(default)]
    pub params: BTreeMap<String, Option<String>>,
}

fn format_float(value: &str, key: &str) -> io::Result<String> {
    let parsed: f64 = value
        .trim()
        .parse()
        .map_err(|_| invalid(format!("{key}: '{value}' is not a number")))?;
    if !parsed.is_finite() {
        return Err(invalid(format!("{key}: '{value}' is not a finite number")));
    }
    let rounded = parsed as f32 as f64;
    if rounded.fract() == 0.0 && rounded.abs() < 1e15 {
        Ok(format!("{rounded:.1}"))
    } else {
        Ok(format!("{rounded}"))
    }
}

/// The text of a leaf value for `spec`, or `None` when the value is the
/// default and the line should be dropped.
fn leaf_value(spec: &ParamSpec, value: Option<&str>) -> io::Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    match spec.kind {
        ParamKind::Float => {
            if value.is_empty() {
                return Ok(None);
            }
            let formatted = format_float(value, spec.name)?;
            if formatted == spec.default {
                return Ok(None);
            }
            Ok(Some(formatted))
        }
        ParamKind::Text => {
            if value.contains('"') || value.contains('\n') {
                return Err(invalid(format!(
                    "{}: quotes and line breaks are not allowed",
                    spec.name
                )));
            }
            if value.is_empty() {
                return Ok(None);
            }
            Ok(Some(format!("\"{value}\"")))
        }
    }
}

/// Validates a new ELink user / emitter-set file name.
pub fn validate_new_name(name: &str) -> io::Result<()> {
    if name.is_empty() {
        return Err(invalid("enter a name for the new effect"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(invalid(
            "the new name may only contain ASCII letters, digits and underscores",
        ));
    }
    if name.len() >= PTCL_NAME_SLOT {
        return Err(invalid(format!(
            "the new name must be shorter than {PTCL_NAME_SLOT} characters (the emitter-set file name slot)"
        )));
    }
    Ok(())
}

/// The block of `base_user` renamed to `new_name` with `edits` applied,
/// as text lines.
pub fn build_custom_user(
    text: &str,
    base_user: &str,
    new_name: &str,
    edits: &[EntryEdit],
) -> io::Result<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = user_start(&lines, base_user).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("ELink user {base_user} not found"),
        )
    })?;
    let block = parse_block(&lines, start)?;
    let assets = user_asset_nodes(&block, &lines);

    // line index -> replacement (None deletes); node end -> inserted lines
    let mut replacements: HashMap<usize, Option<String>> = HashMap::new();
    let mut insertions: HashMap<usize, Vec<String>> = HashMap::new();
    for edit in edits {
        let asset = assets
            .get(edit.id)
            .ok_or_else(|| invalid(format!("asset entry {} does not exist", edit.id)))?;
        let node = asset.node;
        let leaf_indent = node
            .leaves
            .first()
            .map(|&index| indent_of(lines[index]))
            .unwrap_or(node.indent + 2);
        for (key, value) in &edit.params {
            let spec = param_spec(key)
                .ok_or_else(|| invalid(format!("{key} is not an editable parameter")))?;
            let new_value = leaf_value(spec, value.as_deref())?;
            let existing =
                node.leaves.iter().copied().find(|&index| {
                    leaf_key_value(lines[index]).map(|(k, _)| k) == Some(key.as_str())
                });
            match (existing, new_value) {
                (Some(index), Some(value)) => {
                    replacements.insert(
                        index,
                        Some(format!("{}{key} = {value}", " ".repeat(leaf_indent))),
                    );
                }
                (Some(index), None) => {
                    replacements.insert(index, None);
                }
                (None, Some(value)) => {
                    insertions
                        .entry(node.end)
                        .or_default()
                        .push(format!("{}{key} = {value}", " ".repeat(leaf_indent)));
                }
                (None, None) => {}
            }
        }
    }

    let mut out = Vec::with_capacity(block.end - block.start + 1);
    for index in block.start..=block.end {
        if let Some(inserted) = insertions.get(&index) {
            out.extend(inserted.iter().cloned());
        }
        match replacements.get(&index) {
            Some(Some(replacement)) => out.push(replacement.clone()),
            Some(None) => {}
            None => out.push(lines[index].to_owned()),
        }
    }
    out[0] = format!("  {new_name} {{");
    Ok(out)
}

/// `text` with `block` added as a user: it replaces an existing block of the
/// same name (a previous run of the tool) or follows `base_user`.
pub fn splice_user(text: &str, base_user: &str, block: &[String]) -> io::Result<String> {
    let new_name = block[0]
        .trim()
        .strip_suffix(" {")
        .ok_or_else(|| invalid("custom user block has no opening line"))?
        .to_owned();
    let lines: Vec<&str> = text.lines().collect();
    let (start, end) = match user_start(&lines, &new_name) {
        Some(start) => {
            let existing = parse_block(&lines, start)?;
            (start, existing.end + 1)
        }
        None => {
            let start = user_start(&lines, base_user).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("ELink user {base_user} not found"),
                )
            })?;
            let base = parse_block(&lines, start)?;
            (base.end + 1, base.end + 1)
        }
    };
    let mut out =
        String::with_capacity(text.len() + block.iter().map(|l| l.len() + 1).sum::<usize>());
    for line in &lines[..start] {
        out.push_str(line);
        out.push('\n');
    }
    for line in block {
        out.push_str(line);
        out.push('\n');
    }
    for line in &lines[end..] {
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// RomFS-relative path of the versioned ELink file (`ELink2/elink2.Product.NNN.belnk.zs`).
pub fn elink_relative_path(romfs: &Path) -> io::Result<String> {
    let dir = romfs.join("ELink2");
    let mut best: Option<(u32, String)> = None;
    for entry in fs::read_dir(&dir)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", dir.display())))?
    {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let Some(version) = name
            .strip_prefix("elink2.Product.")
            .and_then(|rest| rest.strip_suffix(".belnk.zs"))
            .and_then(|version| version.parse::<u32>().ok())
        else {
            continue;
        };
        if best.as_ref().map_or(true, |(v, _)| version > *v) {
            best = Some((version, name));
        }
    }
    best.map(|(_, name)| format!("ELink2/{name}"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no ELink2/elink2.Product.*.belnk.zs in the RomFS",
            )
        })
}

type TextCache = Mutex<HashMap<PathBuf, (SystemTime, Arc<String>)>>;

fn text_cache() -> &'static TextCache {
    static CACHE: OnceLock<TextCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The ELink file at `path` as text, cached by modification time (the
/// conversion of the 3 MB binary takes seconds).
pub fn elink_text(path: &Path, zstd: Arc<TotkZstd<'_>>) -> io::Result<Arc<String>> {
    let modified = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    if let Some((stamp, text)) = text_cache().lock().unwrap().get(path) {
        if *stamp == modified {
            return Ok(text.clone());
        }
    }
    let data = fs::read(path)?;
    let text = Arc::new(Xlink_rs::new(zstd)?.binary_to_yaml(&data)?);
    text_cache()
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), (modified, text.clone()));
    Ok(text)
}

/// The vanilla ELink text of `romfs`.
pub fn vanilla_text(romfs: &Path, zstd: Arc<TotkZstd<'_>>) -> io::Result<Arc<String>> {
    let relative = elink_relative_path(romfs)?;
    elink_text(&romfs.join(relative), zstd)
}

pub fn esetb_path(romfs: &Path, user: &str) -> PathBuf {
    romfs.join("Effect").join(format!("{user}{ESETB_SUFFIX}"))
}

/// A copy of the decompressed emitter-set file `data` whose PTCL header names
/// `new_name` instead of `base_user`.
pub fn rename_esetb(data: &[u8], base_user: &str, new_name: &str) -> io::Result<Vec<u8>> {
    let magic = data
        .windows(4)
        .position(|window| window == b"VFXB")
        .ok_or_else(|| invalid("emitter-set file has no VFXB header"))?;
    let slot = magic + PTCL_NAME_OFFSET;
    if slot + PTCL_NAME_SLOT > data.len() {
        return Err(invalid("emitter-set file is truncated"));
    }
    let current = &data[slot..slot + PTCL_NAME_SLOT];
    let current_name = current
        .iter()
        .position(|&b| b == 0)
        .map(|end| &current[..end])
        .unwrap_or(current);
    if current_name != base_user.as_bytes() {
        return Err(invalid(format!(
            "emitter-set file names '{}' instead of {base_user}",
            String::from_utf8_lossy(current_name)
        )));
    }
    if new_name.len() >= PTCL_NAME_SLOT {
        return Err(invalid(format!(
            "{new_name} does not fit the {PTCL_NAME_SLOT}-byte emitter-set name slot"
        )));
    }
    let mut out = data.to_vec();
    out[slot..slot + PTCL_NAME_SLOT].fill(0);
    out[slot..slot + new_name.len()].copy_from_slice(new_name.as_bytes());
    Ok(out)
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// One custom effect: the UI sends it in camelCase, an items creator spec
/// file may also spell the keys in snake_case like the other entries.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElinkRequest {
    #[serde(alias = "base_user")]
    pub base_user: String,
    #[serde(alias = "new_name")]
    pub new_name: String,
    #[serde(default)]
    pub entries: Vec<EntryEdit>,
    /// Copy `Effect/<base>.Nin_NX_NVN.esetb.byml.zs` under the new name.
    #[serde(default = "default_true", alias = "clone_esetb")]
    pub clone_esetb: bool,
    /// Rebuild the mod's ResourceSizeTable afterwards (ignored inside an
    /// items creator mod, whose single RSTB pass covers the effect).
    #[serde(default = "default_true", alias = "update_rstb")]
    pub update_rstb: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElinkReport {
    pub output_romfs: PathBuf,
    pub elink: PathBuf,
    pub esetb: Option<PathBuf>,
    pub rstb_entries: Option<usize>,
    pub edited_entries: usize,
    pub notes: Vec<String>,
    /// The custom user written (the request's trimmed `new_name`).
    pub new_name: String,
    /// The vanilla user it was copied from.
    pub base_user: String,
}

/// Writes the mod files for `request` below `output_romfs`.
pub fn generate(
    request: &ElinkRequest,
    clean_romfs: &Path,
    output_romfs: &Path,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<ElinkReport> {
    let base_user = request.base_user.trim();
    let new_name = request.new_name.trim();
    validate_new_name(new_name)?;
    if new_name == base_user {
        return Err(invalid("the new name must differ from the base effect"));
    }
    let mut notes = Vec::new();

    let relative = elink_relative_path(clean_romfs)?;
    let vanilla = vanilla_text(clean_romfs, zstd.clone())?;
    let vanilla_users = list_users(&vanilla)?;
    if !vanilla_users.iter().any(|user| user == base_user) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{base_user} is not an ELink user of the RomFS"),
        ));
    }
    if vanilla_users.iter().any(|user| user == new_name) {
        return Err(invalid(format!(
            "{new_name} already exists in the vanilla ELink; choose another name"
        )));
    }

    // Build on the mod's own copy when a previous run left one, so several
    // custom effects can share one mod.
    let destination = output_romfs.join(&relative);
    let source_text = if destination.is_file() {
        notes.push(format!(
            "Added to the existing mod ELink {}",
            destination.display()
        ));
        elink_text(&destination, zstd.clone())?
    } else {
        vanilla.clone()
    };
    let block = build_custom_user(&source_text, base_user, new_name, &request.entries)?;
    let text = splice_user(&source_text, base_user, &block)?;
    let binary = Xlink_rs::new(zstd.clone())?.yaml_to_binary(&text)?;
    let compressed = zstd.compress_zs(&binary)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, compressed).map_err(|error| {
        io::Error::new(error.kind(), format!("{}: {error}", destination.display()))
    })?;
    text_cache().lock().unwrap().remove(&destination);

    let mut esetb = None;
    if request.clone_esetb {
        let source = esetb_path(clean_romfs, base_user);
        if source.is_file() {
            let data = zstd.try_decompress(&fs::read(&source)?)?;
            let renamed = rename_esetb(&data, base_user, new_name)?;
            let target = esetb_path(output_romfs, new_name);
            fs::create_dir_all(target.parent().expect("Effect folder"))?;
            fs::write(&target, zstd.compress_zs(&renamed)?).map_err(|error| {
                io::Error::new(error.kind(), format!("{}: {error}", target.display()))
            })?;
            esetb = Some(target);
        } else {
            notes.push(format!(
                "{base_user} has no emitter-set file of its own (it only uses shared effects from Effect/static), so none was cloned"
            ));
        }
    }

    let mut rstb_entries = None;
    if request.update_rstb {
        let report = ModRstbProcessor::new(clean_romfs, output_romfs, zstd).generate()?;
        rstb_entries = Some(report.entries.len());
    }

    Ok(ElinkReport {
        output_romfs: output_romfs.to_path_buf(),
        elink: destination,
        esetb,
        rstb_entries,
        new_name: new_name.to_owned(),
        base_user: base_user.to_owned(),
        edited_entries: request
            .entries
            .iter()
            .filter(|edit| !edit.params.is_empty())
            .count(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Metadata {\n  ModuleType = ELink\n}\nUsers {\n  Other {\n    Unknown = 0\n  }\n  Item_Weapon_01 {\n    Unknown = 0\n    LocalProperties {\n      Attachment_IsAttached\n    }\n    AssetCallTables {\n      Blade[0x6e007fe2] {\n        EmitCount = 1\n        Execute = Blend {\n          鏃[0xd9e18d9d] {\n            Execute = Asset {\n              AssetName = \"鏃\"\n              RuntimeAssetName = \"Wpn_AncientArrow_Blade\"\n              Scale = 1.60000002\n              PositionX = -0.109999999\n              Bone = \"Root\"\n            }\n          }\n        }\n      }\n      OnPouch[0x7eca75ba] {\n        Execute = Switch (Local::Attachment_IsAttached) {\n          (<value> == 1) => Held[0xdf69978e] {\n            Execute = Asset {\n              AssetName = \"Held\"\n              RuntimeAssetName = \"Wpn_AncientArrow_Blade_OnPouch\"\n              Red = CURVE\n            }\n          }\n        }\n      }\n    }\n  }\n}\n";

    #[test]
    fn lists_users_and_assets() {
        assert_eq!(list_users(SAMPLE).unwrap(), vec!["Other", "Item_Weapon_01"]);
        let assets = user_assets(SAMPLE, "Item_Weapon_01").unwrap();
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].table, "Blade");
        assert_eq!(assets[0].path, "Blade › 鏃");
        assert_eq!(assets[0].params["Scale"], "1.60000002");
        assert_eq!(assets[0].params["Bone"], "Root");
        assert_eq!(
            assets[1].path,
            "OnPouch › Held ⟨Attachment_IsAttached == 1⟩"
        );
        assert_eq!(assets[1].params["Red"], "CURVE");
        assert_eq!(asset_counts(SAMPLE).unwrap()["Item_Weapon_01"], 2);
    }

    #[test]
    fn builds_renamed_block_with_edits() {
        let mut params = BTreeMap::new();
        params.insert("Scale".to_owned(), Some("2".to_owned()));
        params.insert("PositionX".to_owned(), None);
        params.insert("Red".to_owned(), Some("0.25".to_owned()));
        params.insert("Alpha".to_owned(), Some("1".to_owned()));
        params.insert("Bone".to_owned(), Some(String::new()));
        let edits = vec![EntryEdit { id: 0, params }];
        let block =
            build_custom_user(SAMPLE, "Item_Weapon_01", "Item_Weapon_01_custom", &edits).unwrap();
        let text = block.join("\n");
        assert!(text.starts_with("  Item_Weapon_01_custom {"));
        assert!(text.contains("              Scale = 2.0\n"));
        assert!(!text.contains("PositionX"));
        assert!(!text.contains("Bone"));
        assert!(!text.contains("Alpha"));
        assert!(text.contains("              Red = 0.25\n"));
        let spliced = splice_user(SAMPLE, "Item_Weapon_01", &block).unwrap();
        assert_eq!(
            list_users(&spliced).unwrap(),
            vec!["Other", "Item_Weapon_01", "Item_Weapon_01_custom"]
        );
        // A second run replaces the previous custom block instead of adding another.
        let again = splice_user(&spliced, "Item_Weapon_01", &block).unwrap();
        assert_eq!(again, spliced);
    }

    /// Full pipeline against the configured TOTK RomFS; output in tmp/_CLAUDE/elink/e2e.
    #[test]
    #[ignore]
    fn generates_custom_item_weapon_01_from_romfs() {
        let config = crate::TotkConfig::TotkConfig::safe_new(false).unwrap();
        let romfs = PathBuf::from(&config.romfs);
        let zstd = Arc::new(TotkZstd::new(Arc::new(config), 16).unwrap());
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tmp/_CLAUDE/elink/e2e/romfs");
        let _ = fs::remove_dir_all(out.parent().unwrap());
        let mut params = BTreeMap::new();
        params.insert("PositionX".to_owned(), Some("0.25".to_owned()));
        params.insert("Scale".to_owned(), Some("2".to_owned()));
        params.insert("Red".to_owned(), Some("0.2".to_owned()));
        params.insert("Green".to_owned(), Some("1".to_owned()));
        params.insert("Blue".to_owned(), Some("0.3".to_owned()));
        let request = ElinkRequest {
            base_user: "Item_Weapon_01".into(),
            new_name: "Item_Weapon_01_custom".into(),
            entries: vec![EntryEdit { id: 7, params }],
            clone_esetb: true,
            update_rstb: true,
        };
        let vanilla = vanilla_text(&romfs, zstd.clone()).unwrap();
        let assets = user_assets(&vanilla, "Item_Weapon_01").unwrap();
        for asset in &assets {
            println!(
                "{:>2} {} <- {}",
                asset.id,
                asset.path,
                asset
                    .params
                    .get("RuntimeAssetName")
                    .map(String::as_str)
                    .unwrap_or("")
            );
        }
        assert_eq!(assets[7].path, "Blade › 鏃");
        let report = generate(&request, &romfs, &out, zstd.clone()).unwrap();
        println!("{report:#?}");
        assert!(report.esetb.as_ref().unwrap().is_file());
        assert!(report.rstb_entries.unwrap() >= 2);
        let text = elink_text(&report.elink, zstd.clone()).unwrap();
        assert!(list_users(&text)
            .unwrap()
            .iter()
            .any(|u| u == "Item_Weapon_01_custom"));
        let custom = user_assets(&text, "Item_Weapon_01_custom").unwrap();
        assert_eq!(custom.len(), assets.len());
        let value = |key: &str| custom[7].params[key].parse::<f32>().unwrap();
        assert!((value("PositionX") - 0.25).abs() < 1e-6);
        assert!((value("Scale") - 2.0).abs() < 1e-6);
        assert!((value("Red") - 0.2).abs() < 1e-6);
        assert!((value("Blue") - 0.3).abs() < 1e-6);
        // Green = 1.0 is the ELink default, so the converter leaves it out.
        assert!(!custom[7].params.contains_key("Green"));
        assert_eq!(custom[8].params["PositionX"], assets[8].params["PositionX"]);
        let esetb = zstd
            .try_decompress(&fs::read(report.esetb.unwrap()).unwrap())
            .unwrap();
        assert!(esetb.windows(22).any(|w| w == b"Item_Weapon_01_custom "));
        // Second run: the custom block is replaced, not duplicated.
        let report = generate(&request, &romfs, &out, zstd.clone()).unwrap();
        let text = elink_text(&report.elink, zstd).unwrap();
        assert_eq!(
            list_users(&text)
                .unwrap()
                .iter()
                .filter(|u| *u == "Item_Weapon_01_custom")
                .count(),
            1
        );
    }

    #[test]
    fn renames_ptcl_header() {
        let mut data = vec![0u8; 0x10];
        data.extend_from_slice(b"VFXB    ");
        data.resize(0x10 + 0x20, 0);
        data.extend_from_slice(b"Item_Weapon_01");
        data.resize(0x10 + 0x40 + 8, 0);
        let renamed = rename_esetb(&data, "Item_Weapon_01", "Item_Weapon_01_custom").unwrap();
        assert_eq!(&renamed[0x30..0x30 + 21], b"Item_Weapon_01_custom");
        assert_eq!(renamed[0x30 + 21], 0);
        assert!(rename_esetb(&data, "Wrong", "X").is_err());
        assert!(validate_new_name("a-b").is_err());
        assert!(validate_new_name(&"a".repeat(32)).is_err());
        assert!(validate_new_name("Item_Weapon_01_custom").is_ok());
    }
}

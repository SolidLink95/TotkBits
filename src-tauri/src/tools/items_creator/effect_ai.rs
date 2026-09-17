//! Spawn-time effect AI ("advanced effect").
//!
//! Vanilla weapons and armor have no root AI, so nothing of theirs ever
//! *emits* an ELink effect by itself: their ELink user only reacts to
//! animation and reaction triggers. Mods that want a permanent glow, gloom
//! or trail on an item give it a tiny root AI that runs once at spawn and
//! searches-and-emits named entries of the item's ELink user:
//!
//! - `AI/AIInfo/<Actor>_Effect.engine__actor__AIInfo.bgyml` with the
//!   `RootAIRef`,
//! - `AI/<Actor>_Effect.root.ainb`, one `OneShotXLinkSearchAndEmit` node per
//!   XLink key (several keys sit under an `Element_Simultaneous` root),
//! - `AIInfoRef` in the ActorParam that binds the AIInfo file.
//!
//! The keys are asset-call-table entries of the ELink user the item ends up
//! with (the effect donor's, e.g. `Item_Weapon_01` → `古代矢完成`, or
//! `Player` → `Miasma_Status_In`), as listed in
//! `ELink2/elink2.Product.*.belnk.zs`. A looping entry keeps running after
//! the one-shot emit, which is what makes the effect permanent.

use super::actor_pack::InjectedPackEntry;
use crate::parser::ainb::AinbDocument;
use roead::byml::Byml;
use std::io::{self, Cursor};

/// The files and the ActorParam reference of one item's effect AI.
#[derive(Clone, Debug)]
pub(super) struct EffectAi {
    /// `?AI/AIInfo/<name>.engine__actor__AIInfo.bgyml`, the `AIInfoRef` value.
    pub(super) ai_info_ref: String,
    /// The AIInfo BYML and the root AINB, as pack entries.
    pub(super) entries: Vec<InjectedPackEntry>,
}

/// Pack folder of the AI files; a template's own AI is superseded by the
/// generated one.
pub(super) const ENTRY_PREFIX: &str = "AI/";

/// Trimmed, non-empty, de-duplicated keys in their original order.
pub(super) fn normalized_keys(keys: &[String]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for key in keys {
        let key = key.trim();
        if !key.is_empty() && !result.iter().any(|known| known == key) {
            result.push(key.to_owned());
        }
    }
    result
}

/// The effect AI of `actor_name` for `keys`, or `None` when no usable key
/// is given (the item then keeps the template's AI, usually none).
pub(super) fn build_effect_ai(actor_name: &str, keys: &[String]) -> io::Result<Option<EffectAi>> {
    let keys = normalized_keys(keys);
    if keys.is_empty() {
        return Ok(None);
    }
    build_named_effect_ai(&format!("{actor_name}_Effect"), &keys).map(Some)
}

/// Builds the AI files under `name` (`AI/<name>.root.ainb`).
pub(super) fn build_named_effect_ai(name: &str, keys: &[String]) -> io::Result<EffectAi> {
    for key in keys {
        if key.trim().is_empty() {
            return Err(invalid("effect AI: an XLink key is blank"));
        }
    }
    let ainb = AinbDocument::from_yaml(&ainb_yaml(name, keys))
        .map_err(|error| invalid(format!("effect AI: generated AINB is invalid: {error}")))?
        .to_bytes()?;
    let mut ai_info = roead::byml::Map::default();
    ai_info.insert(
        "RootAIRef".into(),
        Byml::String(format!("Work/AI/Root/{name}.root.ain").into()),
    );
    let ai_info_path = format!("AI/AIInfo/{name}.engine__actor__AIInfo.bgyml");
    Ok(EffectAi {
        ai_info_ref: format!("?{ai_info_path}"),
        entries: vec![
            InjectedPackEntry {
                path: ai_info_path,
                data: byml_v7_bytes(&Byml::Map(ai_info))?,
            },
            InjectedPackEntry {
                path: format!("AI/{name}.root.ainb"),
                data: ainb,
            },
        ],
    })
}

/// A little-endian BYML with the version 7 header vanilla `.bgyml` files carry.
pub(super) fn byml_v7_bytes(value: &Byml) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    value
        .write(&mut Cursor::new(&mut data), roead::Endian::Little, 4)
        .map_err(|error| invalid(format!("BYML serialization failed: {error}")))?;
    if data.len() < 4 {
        return Err(invalid("serialized BYML header is truncated"));
    }
    data[2..4].copy_from_slice(&7u16.to_le_bytes());
    Ok(data)
}

/// The root AI as the AINB YAML the native writer reads. One emit node per
/// key; with several keys they are the children of an `Element_Simultaneous`
/// root, exactly like the hand-made mods this reproduces.
fn ainb_yaml(name: &str, keys: &[String]) -> String {
    let filename = format!("{name}.root");
    let root_index = if keys.len() == 1 { 0 } else { keys.len() };
    let mut yaml = String::new();
    yaml.push_str("Version: 1031\n");
    yaml.push_str(&format!("Filename: {}\n", yaml_string(&filename)));
    yaml.push_str("Category: AI\n");
    yaml.push_str(&format!(
        "Blackboard ID: {}\n",
        fnv1a_32(filename.as_bytes())
    ));
    yaml.push_str("Parent Blackboard ID: 0\n");
    yaml.push_str("Commands:\n");
    yaml.push_str("- Name: Root\n");
    yaml.push_str(&format!("  GUID: {}\n", guid(&filename, u32::MAX)));
    yaml.push_str(&format!("  Root Node Index: {root_index}\n"));
    yaml.push_str("Nodes:\n");
    for (index, key) in keys.iter().enumerate() {
        let is_root = index == root_index;
        yaml.push_str("- Node Type: UserDefined\n");
        yaml.push_str(&format!("  Node Index: {index}\n"));
        yaml.push_str("  Name: OneShotXLinkSearchAndEmit\n");
        yaml.push_str(&format!("  GUID: {}\n", guid(&filename, index as u32)));
        yaml.push_str(if is_root {
            "  Flags:\n  - Is Root Node\n"
        } else {
            "  Flags: []\n"
        });
        yaml.push_str("  Queries: []\n");
        yaml.push_str("  Attachments: []\n");
        yaml.push_str("  Properties:\n");
        yaml.push_str("    Bool:\n");
        yaml.push_str("    - Name: ExecuteEveryFrame\n");
        yaml.push_str("      Default Value: false\n");
        yaml.push_str("      Flags: []\n");
        yaml.push_str("  Parameters:\n");
        yaml.push_str("    Inputs:\n");
        yaml.push_str("      Bool:\n");
        yaml.push_str("      - Name: ELinkCall\n");
        yaml.push_str("        Default Value: true\n");
        yaml.push_str("        Node Index: -1\n");
        yaml.push_str("        Output Index: 0\n");
        yaml.push_str("        Flags:\n");
        yaml.push_str("        - Uses Default\n");
        yaml.push_str("        - Is Output\n");
        yaml.push_str("      - Name: SLinkCall\n");
        yaml.push_str("        Default Value: false\n");
        yaml.push_str("        Node Index: -1\n");
        yaml.push_str("        Output Index: 0\n");
        yaml.push_str("        Flags:\n");
        yaml.push_str("        - Uses Default\n");
        yaml.push_str("      String:\n");
        yaml.push_str("      - Name: XLinkKeyName\n");
        yaml.push_str(&format!("        Default Value: {}\n", yaml_string(key)));
        yaml.push_str("        Node Index: -1\n");
        yaml.push_str("        Output Index: 0\n");
        yaml.push_str("        Flags: []\n");
        yaml.push_str("    Outputs:\n");
        yaml.push_str("      Pointer:\n");
        yaml.push_str("      - Name: HandleSet\n");
        yaml.push_str("        Classname: xlink2::HandleSet\n");
        yaml.push_str("        Is Output: false\n");
        yaml.push_str("  XLink Actions: []\n");
        yaml.push_str("  Plugs: {}\n");
    }
    if keys.len() > 1 {
        yaml.push_str("- Node Type: Element_Simultaneous\n");
        yaml.push_str(&format!("  Node Index: {root_index}\n"));
        yaml.push_str("  Name: ''\n");
        yaml.push_str(&format!("  GUID: {}\n", guid(&filename, root_index as u32)));
        yaml.push_str("  Flags:\n  - Is Root Node\n");
        yaml.push_str("  Queries: []\n");
        yaml.push_str("  Attachments: []\n");
        yaml.push_str("  Properties:\n");
        yaml.push_str("    Int:\n");
        yaml.push_str("    - Name: EndPolicy\n");
        yaml.push_str("      Default Value: 0\n");
        yaml.push_str("      Flags: []\n");
        yaml.push_str("    - Name: ResultPolicy\n");
        yaml.push_str("      Default Value: 0\n");
        yaml.push_str("      Flags: []\n");
        yaml.push_str("  Parameters:\n");
        yaml.push_str("    Inputs: {}\n");
        yaml.push_str("    Outputs: {}\n");
        yaml.push_str("  XLink Actions: []\n");
        yaml.push_str("  Plugs:\n");
        yaml.push_str("    Child:\n");
        for index in 0..keys.len() {
            yaml.push_str(&format!("    - Node Index: {index}\n"));
            yaml.push_str("      Name: ''\n");
        }
    }
    yaml.push_str("Blackboard: {}\n");
    yaml.push_str("Expressions: {}\n");
    yaml.push_str("Replacement Table: []\n");
    yaml.push_str("Modules: []\n");
    yaml.push_str("Unknown Section 0x58: {}\n");
    yaml.push_str("Has Section 0x6C: true\n");
    yaml
}

/// A YAML double-quoted scalar (JSON string syntax is valid YAML), so keys
/// with spaces, colons or non-ASCII text survive.
fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

/// Deterministic node GUIDs: the same spec always writes the same bytes.
fn guid(seed: &str, index: u32) -> String {
    let a = fnv1a_64(format!("{seed}#{index}#a").as_bytes());
    let b = fnv1a_64(format!("{seed}#{index}#b").as_bytes());
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a as u16) & 0x0fff,
        0x8000 | ((b >> 48) as u16 & 0x3fff),
        b & 0xffff_ffff_ffff
    )
}

fn fnv1a_32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emit_keys(document: &AinbDocument) -> Vec<String> {
        document
            .nodes
            .iter()
            .filter(|node| node.name == "OneShotXLinkSearchAndEmit")
            .map(|node| {
                node.parameters.inputs["String"]
                    .iter()
                    .find(|input| input.name == "XLinkKeyName")
                    .and_then(|input| input.default_value.as_str().map(str::to_owned))
                    .expect("XLinkKeyName input")
            })
            .collect()
    }

    #[test]
    fn keys_are_trimmed_and_deduplicated() {
        assert_eq!(
            normalized_keys(&[" 軌跡 ".into(), "".into(), "軌跡".into(), "射撃".into()]),
            vec!["軌跡".to_owned(), "射撃".to_owned()]
        );
        assert!(build_effect_ai("Weapon_Sword_900", &["  ".into()])
            .unwrap()
            .is_none());
    }

    #[test]
    fn single_key_is_a_lone_root_node() {
        let ai = build_effect_ai("Armor_900_Upper", &["Miasma_Status_In".into()])
            .unwrap()
            .unwrap();
        assert_eq!(
            ai.ai_info_ref,
            "?AI/AIInfo/Armor_900_Upper_Effect.engine__actor__AIInfo.bgyml"
        );
        assert_eq!(
            ai.entries[0].path,
            "AI/AIInfo/Armor_900_Upper_Effect.engine__actor__AIInfo.bgyml"
        );
        assert_eq!(ai.entries[1].path, "AI/Armor_900_Upper_Effect.root.ainb");
        let ai_info = Byml::from_binary(&ai.entries[0].data).unwrap();
        assert_eq!(
            ai_info.as_map().unwrap()["RootAIRef"].as_string().unwrap(),
            "Work/AI/Root/Armor_900_Upper_Effect.root.ain"
        );
        assert_eq!(&ai.entries[0].data[..4], b"YB\x07\x00");
        let document = AinbDocument::from_bytes(&ai.entries[1].data).unwrap();
        assert_eq!(document.filename, "Armor_900_Upper_Effect.root");
        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.commands[0].root_node_index, 0);
        assert_eq!(emit_keys(&document), vec!["Miasma_Status_In".to_owned()]);
        // Writing the same spec twice yields identical bytes.
        let again = build_effect_ai("Armor_900_Upper", &["Miasma_Status_In".into()])
            .unwrap()
            .unwrap();
        assert_eq!(again.entries[1].data, ai.entries[1].data);
    }

    #[test]
    fn several_keys_hang_under_a_simultaneous_root() {
        let keys = ["古代矢完成", "軌跡", "射撃"].map(String::from);
        let ai = build_effect_ai("Weapon_Sword_900", &keys).unwrap().unwrap();
        let document = AinbDocument::from_bytes(&ai.entries[1].data).unwrap();
        assert_eq!(document.nodes.len(), 4);
        assert_eq!(document.commands[0].root_node_index, 3);
        let root = &document.nodes[3];
        assert_eq!(root.node_type, "Element_Simultaneous");
        assert!(root.node_flags.iter().any(|flag| flag == "Is Root Node"));
        let children: Vec<i64> = root.plugs["Child"]
            .iter()
            .map(|plug| plug["Node Index"].as_i64().unwrap())
            .collect();
        assert_eq!(children, vec![0, 1, 2]);
        assert_eq!(emit_keys(&document), keys.to_vec());
        for node in &document.nodes[..3] {
            assert!(!node.node_flags.iter().any(|flag| flag == "Is Root Node"));
        }
        // The YAML round trip is stable.
        let yaml = document.to_yaml().unwrap();
        let reparsed = AinbDocument::from_yaml(&yaml).unwrap().to_bytes().unwrap();
        assert_eq!(reparsed, ai.entries[1].data);
    }
}

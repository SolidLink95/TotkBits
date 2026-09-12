//! English MALS/MSBT generation for custom weapons.

use crate::{
    file_format::Pack::PackFile,
    parser::msbt::{document::Message, token::TextPart, Msbt},
    Zstd::TotkZstd,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path, sync::Arc};

const US_ENGLISH_MALS_PREFIX: &str = "USen.Product.";
const US_ENGLISH_MALS_SUFFIX: &str = ".sarc.zs";
const POUCH_CONTENT: &str = "ActorMsg/PouchContent.msbt";
const ATTACHMENT: &str = "ActorMsg/Attachment.msbt";
const PICTURE_BOOK: &str = "ActorMsg/PictureBook.msbt";

/// JSON/form-friendly text used to add one weapon to the US English MALS archive.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeaponMessageRequest {
    /// Custom actor identifier. `name` is accepted for compatibility with actor-pack input.
    #[serde(alias = "name")]
    pub actor_name: String,
    /// Inventory name. Also used for the compendium unless overridden.
    #[serde(default)]
    pub display_name: String,
    /// Inventory description. Also used for the compendium unless overridden.
    #[serde(default)]
    pub description: String,
    /// Short noun used by the inventory UI, such as `Bat` or `Longsword`.
    /// Defaults to `display_name`.
    #[serde(default)]
    pub base_name: Option<String>,
    /// Fusion/attachment adjective. Defaults to `display_name`.
    #[serde(default, alias = "attachment_name")]
    pub attachment_adjective: Option<String>,
    #[serde(default)]
    pub picture_book_name: Option<String>,
    #[serde(default, alias = "picture_book_caption")]
    pub picture_book_description: Option<String>,
}

impl WeaponMessageRequest {
    pub fn from_json(text: &str) -> io::Result<Self> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Discover the clean ROMFS product version and emit the matching filename in mod ROMFS.
    pub fn generate_to_mod_romfs(
        &self,
        clean_romfs: &Path,
        output_romfs: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<std::path::PathBuf> {
        let (version, _) = super::version::discover_product_file(
            &clean_romfs.join("Mals"),
            US_ENGLISH_MALS_PREFIX,
            US_ENGLISH_MALS_SUFFIX,
        )?;
        let name =
            super::version::product_name(US_ENGLISH_MALS_PREFIX, &version, US_ENGLISH_MALS_SUFFIX)?;
        let output = output_romfs.join("Mals").join(name);
        self.generate_us_english_mals(clean_romfs, &output, zstd)?;
        Ok(output)
    }

    /// Clone the clean US English MALS, add or replace this weapon's labels, and save it.
    pub fn generate_us_english_mals(
        &self,
        clean_romfs: &Path,
        output: &Path,
        zstd: Arc<TotkZstd<'_>>,
    ) -> io::Result<()> {
        validate_actor_name(&self.actor_name)?;
        ensure_output_outside_romfs(clean_romfs, output)?;

        let display_name = self.text_or_placeholder(&self.display_name, "weapon");
        let description = self.text_or_placeholder(&self.description, "description");
        let picture_name = self
            .picture_book_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&display_name);
        let picture_description = self
            .picture_book_description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&description);

        let (_, clean_source) = super::version::discover_product_file(
            &clean_romfs.join("Mals"),
            US_ENGLISH_MALS_PREFIX,
            US_ENGLISH_MALS_SUFFIX,
        )?;
        let source = if output.is_file() {
            output.to_path_buf()
        } else {
            clean_source
        };
        let compressed = fs::read(&source)?;
        let pack = PackFile::from_binary(&compressed, zstd.clone())?;
        let mut replacements = BTreeMap::new();

        // The game reads every label below for a pouch item, so omitted texts
        // fall back to the display name instead of leaving the label out.
        let base_name = self
            .base_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&display_name);
        let attachment_adjective = self
            .attachment_adjective
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&display_name);
        let pouch_entries = [
            ("Name", Some(display_name.as_str())),
            ("Caption", Some(description.as_str())),
            ("BaseName", Some(base_name)),
        ];
        edit_msbt(
            &pack,
            &mut replacements,
            POUCH_CONTENT,
            &self.actor_name,
            &pouch_entries,
        )?;

        let attachment_entries = [("Adjective", Some(attachment_adjective))];
        edit_msbt(
            &pack,
            &mut replacements,
            ATTACHMENT,
            &self.actor_name,
            &attachment_entries,
        )?;

        let picture_entries = [
            ("Name", Some(picture_name)),
            ("Caption", Some(picture_description)),
        ];
        edit_msbt(
            &pack,
            &mut replacements,
            PICTURE_BOOK,
            &self.actor_name,
            &picture_entries,
        )?;

        if replacements.is_empty() {
            if source != output {
                if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(source, output)?;
            }
            return Ok(());
        }

        let output_bytes = pack.rebuild_replacing_entries(replacements)?;
        let verification = PackFile::from_binary(&output_bytes, zstd)?;
        validate_generated_mals(&verification, self)?;
        if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, output_bytes)
    }

    fn text_or_placeholder(&self, value: &str, kind: &str) -> String {
        if value.trim().is_empty() {
            format!("{} {kind}", self.actor_name)
        } else {
            value.to_owned()
        }
    }

    fn optional_text_or_placeholder(&self, value: &Option<String>, kind: &str) -> String {
        self.text_or_placeholder(value.as_deref().unwrap_or_default(), kind)
    }
}

/// Adds or replaces only the `<actor>_Name` / `<actor>_Caption` labels of
/// `ActorMsg/PouchContent.msbt` — everything a pouch-only item such as armor
/// needs — and writes the versioned US English MALS into the mod ROMFS.
pub(super) fn generate_pouch_labels(
    clean_romfs: &Path,
    output_romfs: &Path,
    actor_name: &str,
    display_name: &str,
    description: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<std::path::PathBuf> {
    validate_actor_name(actor_name)?;
    require_text(display_name, "display_name")?;
    require_text(description, "description")?;
    let (version, clean_source) = super::version::discover_product_file(
        &clean_romfs.join("Mals"),
        US_ENGLISH_MALS_PREFIX,
        US_ENGLISH_MALS_SUFFIX,
    )?;
    let name =
        super::version::product_name(US_ENGLISH_MALS_PREFIX, &version, US_ENGLISH_MALS_SUFFIX)?;
    let output = output_romfs.join("Mals").join(name);
    ensure_output_outside_romfs(clean_romfs, &output)?;
    let source = if output.is_file() {
        output.clone()
    } else {
        clean_source
    };
    let compressed = fs::read(&source)?;
    let pack = PackFile::from_binary(&compressed, zstd.clone())?;
    let mut replacements = BTreeMap::new();
    let entries = [("Name", Some(display_name)), ("Caption", Some(description))];
    edit_msbt(
        &pack,
        &mut replacements,
        POUCH_CONTENT,
        actor_name,
        &entries,
    )?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    if replacements.is_empty() {
        if source != output {
            fs::copy(&source, &output)?;
        }
        return Ok(output);
    }
    let output_bytes = pack.rebuild_replacing_entries(replacements)?;
    let verification = PackFile::from_binary(&output_bytes, zstd)?;
    let data = verification
        .sarc
        .get_data(POUCH_CONTENT)
        .ok_or_else(|| invalid_data("generated MALS lost PouchContent.msbt"))?;
    let msbt = Msbt::from_bytes(data)?;
    for suffix in ["Name", "Caption"] {
        let label = format!("{actor_name}_{suffix}");
        if !msbt
            .messages
            .iter()
            .any(|message| message.label.as_deref() == Some(&label))
        {
            return Err(invalid_data(format!("generated label is missing: {label}")));
        }
    }
    fs::write(&output, output_bytes)?;
    Ok(output)
}

fn edit_msbt(
    pack: &PackFile<'_>,
    replacements: &mut BTreeMap<String, Vec<u8>>,
    path: &str,
    actor_name: &str,
    entries: &[(&str, Option<&str>)],
) -> io::Result<()> {
    if entries.iter().all(|(_, value)| value.is_none()) {
        return Ok(());
    }
    let data = pack
        .sarc
        .get_data(path)
        .ok_or_else(|| invalid_data(format!("required MALS entry is missing: {path}")))?;
    let mut msbt = Msbt::from_bytes(data)
        .map_err(|error| invalid_data(format!("failed to parse {path}: {error}")))?;
    let original_message_count = msbt.messages.len();
    let mut changed = false;
    for (suffix, value) in entries {
        if let Some(value) = value {
            require_text(value, suffix)?;
            changed |=
                upsert_plain_message(&mut msbt, &format!("{actor_name}_{suffix}"), value, suffix);
        }
    }
    if !changed {
        return Ok(());
    }
    if msbt.messages.len() != original_message_count {
        // Match the game-valid working MALS layout. A single LBL1 bucket avoids
        // relying on bucket hashes for labels newly appended to ROMFS MSBTs.
        msbt.label_groups = 1;
    }
    let rebuilt = msbt
        .to_bytes_preserving_layout()
        .map_err(|error| invalid_data(format!("failed to rebuild {path}: {error}")))?;
    Msbt::from_bytes(&rebuilt)
        .map_err(|error| invalid_data(format!("rebuilt {path} is invalid: {error}")))?;
    replacements.insert(path.to_owned(), rebuilt);
    Ok(())
}

fn upsert_plain_message(msbt: &mut Msbt, label: &str, value: &str, suffix: &str) -> bool {
    if let Some(message) = msbt
        .messages
        .iter_mut()
        .find(|message| message.label.as_deref() == Some(label))
    {
        let parts = vec![TextPart::Text(value.to_owned())];
        if message.parts == parts {
            return false;
        }
        message.parts = parts;
        return true;
    }

    // Match an existing entry of the same semantic kind so ATR1/TSY1 metadata is retained.
    let template = msbt
        .messages
        .iter()
        .find(|message| {
            message
                .label
                .as_deref()
                .is_some_and(|name| name.ends_with(&format!("_{suffix}")))
        })
        .cloned();
    let mut message = template.unwrap_or(Message {
        label: None,
        id: None,
        attribute: Vec::new(),
        style: None,
        parts: Vec::new(),
    });
    message.label = Some(label.to_owned());
    message.id = None;
    message.parts = vec![TextPart::Text(value.to_owned())];
    msbt.messages.push(message);
    true
}

fn validate_generated_mals(pack: &PackFile<'_>, request: &WeaponMessageRequest) -> io::Result<()> {
    for (path, suffixes) in [
        (POUCH_CONTENT, vec!["Name", "Caption", "BaseName"]),
        (ATTACHMENT, vec!["Adjective"]),
        (PICTURE_BOOK, vec!["Name", "Caption"]),
    ] {
        let data = pack
            .sarc
            .get_data(path)
            .ok_or_else(|| invalid_data(format!("generated MALS entry is missing: {path}")))?;
        let msbt = Msbt::from_bytes(data)?;
        for suffix in suffixes {
            let label = format!("{}_{suffix}", request.actor_name);
            if !msbt
                .messages
                .iter()
                .any(|message| message.label.as_deref() == Some(&label))
            {
                return Err(invalid_data(format!("generated label is missing: {label}")));
            }
        }
    }
    Ok(())
}

fn validate_actor_name(actor: &str) -> io::Result<()> {
    if actor.is_empty()
        || !actor
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid custom actor name: {actor}"),
        ));
    }
    Ok(())
}

fn require_text(value: &str, field: &str) -> io::Result<()> {
    if value.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}

fn ensure_output_outside_romfs(clean_romfs: &Path, output: &Path) -> io::Result<()> {
    let clean = clean_romfs.canonicalize()?;
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()?.join(output)
    };
    if output.starts_with(clean) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output MALS must be outside the clean ROMFS",
        ));
    }
    Ok(())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::binary::Endian;
    use crate::parser::msbt::{header::Header, section::Section};
    use crate::{TotkConfig::TotkConfig, Zstd::TOTK_ZSTD_COMPRESSION_LEVEL};

    fn synthetic_msbt() -> Msbt {
        Msbt {
            header: Header {
                endian: Endian::Little,
                unknown: 0,
                encoding: 1,
                version: 3,
                section_count: 2,
                reserved: 0,
                file_size: 0,
                padding: [0; 10],
            },
            sections: vec![
                Section {
                    magic: *b"LBL1",
                    reserved: [0; 8],
                    data: Vec::new(),
                    padding: Vec::new(),
                },
                Section {
                    magic: *b"TXT2",
                    reserved: [0; 8],
                    data: Vec::new(),
                    padding: Vec::new(),
                },
            ],
            messages: vec![Message {
                label: Some("Weapon_Lsword_001_Name".into()),
                id: None,
                attribute: Vec::new(),
                style: None,
                parts: vec![TextPart::Text("Traveler's Claymore".into())],
            }],
            label_groups: 101,
            attribute_offsets: Vec::new(),
            attribute_string_pool: Vec::new(),
        }
    }

    #[test]
    fn message_upsert_is_idempotent_and_round_trips() {
        let mut msbt = synthetic_msbt();
        upsert_plain_message(&mut msbt, "Weapon_Lsword_900_Name", "Test Blade", "Name");
        upsert_plain_message(&mut msbt, "Weapon_Lsword_900_Name", "Final Blade", "Name");
        assert_eq!(
            msbt.messages
                .iter()
                .filter(|message| message.label.as_deref() == Some("Weapon_Lsword_900_Name"))
                .count(),
            1
        );
        let reparsed = Msbt::from_bytes(&msbt.to_bytes().unwrap()).unwrap();
        let message = reparsed
            .messages
            .iter()
            .find(|message| message.label.as_deref() == Some("Weapon_Lsword_900_Name"))
            .unwrap();
        assert_eq!(message.parts, [TextPart::Text("Final Blade".into())]);
    }

    #[test]
    fn request_requires_only_actor_name_and_generates_placeholders() {
        assert!(WeaponMessageRequest::from_json(
            r#"{"display_name":"Blade","description":"Description"}"#
        )
        .is_err());
        let request = WeaponMessageRequest::from_json(r#"{"name":"Weapon_Lsword_900"}"#).unwrap();
        assert_eq!(request.actor_name, "Weapon_Lsword_900");
        assert_eq!(
            request.text_or_placeholder(&request.display_name, "weapon"),
            "Weapon_Lsword_900 weapon"
        );
        assert_eq!(
            request.text_or_placeholder(&request.description, "description"),
            "Weapon_Lsword_900 description"
        );
        assert_eq!(
            request.optional_text_or_placeholder(&request.attachment_adjective, "attachment"),
            "Weapon_Lsword_900 attachment"
        );
        assert_eq!(request.base_name, None);
        assert_eq!(request.attachment_adjective, None);
    }

    #[test]
    fn supplied_message_text_wins_over_placeholders() {
        let request = WeaponMessageRequest::from_json(
            r#"{"name":"Weapon_Lsword_900","display_name":"Blade","base_name":"Claymore"}"#,
        )
        .unwrap();
        assert_eq!(
            request.text_or_placeholder(&request.display_name, "weapon"),
            "Blade"
        );
        assert_eq!(
            request.optional_text_or_placeholder(&request.base_name, "weapon"),
            "Claymore"
        );
        assert_eq!(
            request.text_or_placeholder(&request.description, "description"),
            "Weapon_Lsword_900 description"
        );
    }

    #[test]
    #[ignore = "requires a configured clean ROMFS"]
    fn generates_reopenable_real_us_english_mals() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        if super::super::version::discover_product_file(
            &clean_romfs.join("Mals"),
            US_ENGLISH_MALS_PREFIX,
            US_ENGLISH_MALS_SUFFIX,
        )
        .is_err()
        {
            return;
        }
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let request = WeaponMessageRequest {
            actor_name: "Weapon_Lsword_900".into(),
            display_name: "Test Blade".into(),
            description: "A generated test weapon.".into(),
            base_name: Some("Blade".into()),
            attachment_adjective: Some("Test-Blade".into()),
            picture_book_name: None,
            picture_book_description: None,
        };
        let (version, _) = super::super::version::discover_product_file(
            &clean_romfs.join("Mals"),
            US_ENGLISH_MALS_PREFIX,
            US_ENGLISH_MALS_SUFFIX,
        )
        .unwrap();
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../tmp/USen.Product.{version}.generated.sarc.zs"));
        request
            .generate_us_english_mals(clean_romfs, &output, zstd.clone())
            .unwrap();
        let pack = PackFile::from_binary(&fs::read(&output).unwrap(), zstd).unwrap();
        for (path, labels) in [
            (
                POUCH_CONTENT,
                vec![
                    "Weapon_Lsword_900_Name",
                    "Weapon_Lsword_900_Caption",
                    "Weapon_Lsword_900_BaseName",
                ],
            ),
            (ATTACHMENT, vec!["Weapon_Lsword_900_Adjective"]),
            (
                PICTURE_BOOK,
                vec!["Weapon_Lsword_900_Name", "Weapon_Lsword_900_Caption"],
            ),
        ] {
            let msbt = Msbt::from_bytes(pack.sarc.get_data(path).unwrap()).unwrap();
            for label in labels {
                assert!(msbt
                    .messages
                    .iter()
                    .any(|message| message.label.as_deref() == Some(label)));
            }
        }
        fs::remove_file(output).unwrap();
    }

    #[test]
    #[ignore = "regenerates only tmp/test_sic's US English MALS archive"]
    fn regenerates_test_sic_us_english_mals() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let input = fs::read_to_string(root.join("test_sic_items_creator_input.json")).unwrap();
        let request = WeaponMessageRequest::from_json(&input).unwrap();
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        request
            .generate_to_mod_romfs(clean_romfs, &root.join("test_sic/romfs"), zstd)
            .unwrap();
    }

    #[test]
    #[ignore = "diagnostic comparison with tmp/works MALS"]
    fn compares_generated_and_working_mals_layout() {
        let clean_romfs = Path::new("E:/TOTK_modding/0100F2C0115B6000/romfs");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let mut config = TotkConfig::default();
        config.romfs = clean_romfs.to_string_lossy().into_owned();
        let zstd = Arc::new(
            TotkZstd::new(Arc::new(config), TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("load ROMFS dictionaries"),
        );
        let open =
            |path: &Path| PackFile::from_binary(&fs::read(path).unwrap(), zstd.clone()).unwrap();
        let generated = open(&root.join("test_sic/romfs/Mals/USen.Product.121.sarc.zs"));
        let working = open(&root.join("works/USen.Product.121.sarc.zs"));
        println!(
            "RAW generated={} working={} endian={:?}/{:?} compression={:?}/{:?} files={}/{}",
            generated.data.len(),
            working.data.len(),
            generated.endian,
            working.endian,
            generated.compression,
            working.compression,
            generated.sarc.files().count(),
            working.sarc.files().count()
        );
        let generated_names = generated
            .sarc
            .files()
            .filter_map(|file| file.name().map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        let working_names = working
            .sarc
            .files()
            .filter_map(|file| file.name().map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        println!(
            "ONLY GENERATED={:?}\nONLY WORKING={:?}",
            generated_names
                .difference(&working_names)
                .collect::<Vec<_>>(),
            working_names
                .difference(&generated_names)
                .collect::<Vec<_>>()
        );
        for path in [POUCH_CONTENT, ATTACHMENT, PICTURE_BOOK] {
            let generated_data = generated.sarc.get_data(path).unwrap();
            let working_data = working.sarc.get_data(path).unwrap();
            let generated_msbt = Msbt::from_bytes(generated_data).unwrap();
            let working_msbt = Msbt::from_bytes(working_data).unwrap();
            let generated_labels = generated_msbt
                .messages
                .iter()
                .filter_map(|message| message.label.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let working_labels = working_msbt
                .messages
                .iter()
                .filter_map(|message| message.label.clone())
                .collect::<std::collections::BTreeSet<_>>();
            println!(
                "{path}: bytes={}/{} messages={}/{} groups={}/{} sections={:?}/{:?}",
                generated_data.len(),
                working_data.len(),
                generated_msbt.messages.len(),
                working_msbt.messages.len(),
                generated_msbt.label_groups,
                working_msbt.label_groups,
                generated_msbt
                    .sections
                    .iter()
                    .map(|section| (section.name(), section.data.len(), section.padding.len()))
                    .collect::<Vec<_>>(),
                working_msbt
                    .sections
                    .iter()
                    .map(|section| (section.name(), section.data.len(), section.padding.len()))
                    .collect::<Vec<_>>()
            );
            println!(
                "{path} ONLY WORKING LABELS={:?}",
                working_labels
                    .difference(&generated_labels)
                    .take(120)
                    .collect::<Vec<_>>()
            );
            for message in working_msbt.messages.iter().filter(|message| {
                message
                    .label
                    .as_deref()
                    .is_some_and(|label| label.starts_with("Weapon_Lsword_005_"))
            }) {
                println!("{path} WORKING {:?}={:?}", message.label, message.parts);
            }
        }
    }
}

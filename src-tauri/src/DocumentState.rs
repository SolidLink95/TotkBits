use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex};

use crate::{
    Open_and_Save::SendData,
    TotkApp::{SaveData, TotkBitsApp},
    TotkConfig::TotkConfig,
    Zstd::{TotkFileType, TotkZstd, TOTK_ZSTD_COMPRESSION_LEVEL},
};
use std::sync::Arc;

#[derive(Default)]
pub struct DocumentState {
    documents: Mutex<HashMap<String, TotkBitsApp<'static>>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBphclDocument {
    pub document_id: String,
    pub label: String,
    pub path: String,
    pub location: String,
    pub cloth_count: usize,
    pub collidable_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BphclSelectableNode {
    pub document_id: String,
    pub node_id: String,
    pub kind: String,
    pub index: usize,
    pub name: String,
    pub item_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenHkclDocument {
    pub document_id: String,
    pub label: String,
    pub path: String,
    pub location: String,
    pub skeleton_count: usize,
    pub cloth_count: usize,
    pub constraint_count: usize,
    pub collidable_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBphhbDocument {
    pub document_id: String,
    pub label: String,
    pub path: String,
    pub location: String,
    pub bone_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSidecarDocument {
    pub document_id: String,
    pub label: String,
    pub path: String,
    pub location: String,
    pub format: String,
    pub bone_count: usize,
    pub group_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarMergeResult {
    pub status_text: String,
    pub format: String,
    pub group_count: usize,
    pub bone_count: usize,
    pub sarc_paths: crate::file_format::Pack::SarcPaths,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HkclSelectableNode {
    pub document_id: String,
    pub node_id: String,
    pub kind: String,
    pub index: usize,
    pub name: String,
    pub section_index: usize,
    pub data_offset: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BphclMergeResult {
    pub selected_count: usize,
    pub imported_count: usize,
    pub imported_selection_count: usize,
    pub skipped_count: usize,
    pub added_cloth_count: usize,
    pub added_collidable_count: usize,
    pub cloth_count: usize,
    pub collidable_count: usize,
    pub imported: Vec<String>,
    pub skipped: Vec<BphclMergeSkip>,
    pub sarc_paths: crate::file_format::Pack::SarcPaths,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BphclMergeSkip {
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BphclMutationResult {
    pub status_text: String,
    pub sarc_paths: crate::file_format::Pack::SarcPaths,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PhysicsMergeFormat {
    Hkcl,
    Bphcl,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsMergeRequest {
    pub target_document_id: String,
    pub source_document_id: String,
    pub target_format: PhysicsMergeFormat,
    pub source_format: PhysicsMergeFormat,
    pub node_ids: Vec<String>,
    pub template_cloth_index: Option<usize>,
    pub helper_document_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsMergeValidation {
    pub valid: bool,
    pub issues: Vec<String>,
    pub requires_template: bool,
    pub supports_collidables: bool,
    pub helper_used: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsGraphMergeResult {
    pub imported: Vec<String>,
    pub graph: crate::parser::physics_graph::FormatNeutralPhysicsGraph,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuiltPhysicsDocument {
    pub document_id: String,
    pub format: PhysicsMergeFormat,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsDocumentUpdateResult {
    pub status_text: String,
    pub format: PhysicsMergeFormat,
    pub byte_count: usize,
    pub sarc_paths: crate::file_format::Pack::SarcPaths,
}

impl DocumentState {
    pub fn update_runtime_config(&self, config: TotkConfig) -> String {
        let config = Arc::new(config);
        let (zstd, status) = match TotkZstd::new(config.clone(), TOTK_ZSTD_COMPRESSION_LEVEL) {
            Ok(zstd) => (
                Arc::new(zstd),
                "Settings saved and applied. ZSTD dictionaries are available.".to_string(),
            ),
            Err(error) => (
                Arc::new(TotkZstd::dictionaryless(
                    config,
                    TOTK_ZSTD_COMPRESSION_LEVEL,
                )),
                format!(
                    "Settings saved and applied. Dictionary-backed ZSTD is unavailable: {error}"
                ),
            ),
        };
        let mut documents = self.documents();
        for app in documents.values_mut() {
            app.zstd = zstd.clone();
            if let Some(pack) = &mut app.pack {
                pack.set_zstd(zstd.clone());
            }
            if let Some(byml) = &mut app.opened_file.byml {
                byml.zstd = zstd.clone();
            }
            if let Some(tag) = &mut app.opened_file.tag {
                tag.byml.zstd = zstd.clone();
            }
            if let Some(restbl) = &mut app.opened_file.restbl {
                restbl.zstd = zstd.clone();
            }
            if let Some(esetb) = &mut app.opened_file.esetb {
                esetb.byml.zstd = zstd.clone();
            }
            if let Some(internal) = &mut app.internal_file {
                if let Some(byml) = &mut internal.byml {
                    byml.zstd = zstd.clone();
                }
                if let Some(esetb) = &mut internal.esetb {
                    esetb.byml.zstd = zstd.clone();
                }
                if let Some(tag) = &mut internal.tag {
                    tag.byml.zstd = zstd.clone();
                }
            }
        }
        status
    }

    pub fn commit_rebuilt_physics_document(
        &self,
        request: RebuiltPhysicsDocument,
    ) -> Result<PhysicsDocumentUpdateResult, String> {
        if request.bytes.is_empty() {
            return Err("Rebuilt physics document is empty".into());
        }
        enum Parsed {
            Hkcl(crate::parser::hkcl::HkclDocument),
            Bphcl(crate::parser::bphcl::BphclDocument),
        }
        let parsed = match request.format {
            PhysicsMergeFormat::Hkcl => {
                let document = crate::parser::hkcl::HkclDocument::parse(&request.bytes)
                    .map_err(|error| format!("Rebuilt HKCL did not parse: {error}"))?;
                document
                    .validate()
                    .map_err(|error| format!("Rebuilt HKCL is invalid: {error}"))?;
                Parsed::Hkcl(document)
            }
            PhysicsMergeFormat::Bphcl => {
                let document = crate::parser::bphcl::BphclDocument::parse(&request.bytes)
                    .map_err(|error| format!("Rebuilt BPHCL did not parse: {error}"))?;
                document
                    .validate()
                    .map_err(|error| format!("Rebuilt BPHCL is invalid: {error}"))?;
                Parsed::Bphcl(document)
            }
        };

        let mut documents = self.documents();
        let target = documents
            .get(&request.document_id)
            .ok_or_else(|| format!("Document '{}' is not open", request.document_id))?;
        match request.format {
            PhysicsMergeFormat::Hkcl if target.opened_file.hkcl.is_none() => {
                return Err(format!("Document '{}' is not HKCL", request.document_id))
            }
            PhysicsMergeFormat::Bphcl if target.opened_file.bphcl.is_none() => {
                return Err(format!("Document '{}' is not BPHCL", request.document_id))
            }
            _ => {}
        }
        let parent_link = target.internal_parent.clone();
        let source_path = match request.format {
            PhysicsMergeFormat::Hkcl => target
                .opened_file
                .hkcl
                .as_ref()
                .and_then(|file| file.source_path.clone()),
            PhysicsMergeFormat::Bphcl => target
                .opened_file
                .bphcl
                .as_ref()
                .and_then(|file| file.source_path.clone()),
        };
        if let Some(link) = &parent_link {
            documents
                .get_mut(&link.document_id)
                .ok_or_else(|| format!("Parent document '{}' is not open", link.document_id))?
                .update_child_entry(
                    link.outer_path.as_deref(),
                    &link.inner_path,
                    request.bytes.clone(),
                )?;
        }
        let target = documents.get_mut(&request.document_id).ok_or_else(|| {
            format!(
                "Document '{}' was closed during update",
                request.document_id
            )
        })?;
        let root_name = if target.opened_file.path.name.is_empty() {
            match request.format {
                PhysicsMergeFormat::Hkcl => "rebuilt.hkcl",
                PhysicsMergeFormat::Bphcl => "rebuilt.bphcl",
            }
            .to_string()
        } else {
            target.opened_file.path.name.clone()
        };
        let leaves = match parsed {
            Parsed::Hkcl(document) => {
                let leaves = document
                    .leaves()
                    .map_err(|error| format!("Failed to refresh rebuilt HKCL tree: {error}"))?;
                target.opened_file.hkcl = Some(crate::file_format::hkcl::HkclFile {
                    source_path,
                    document,
                });
                leaves
            }
            Parsed::Bphcl(document) => {
                let file = crate::file_format::bphcl::BphclFile {
                    source_path,
                    document,
                };
                let leaves = file
                    .leaves()
                    .map_err(|error| format!("Failed to refresh rebuilt BPHCL tree: {error}"))?
                    .into_iter()
                    .map(|leaf| crate::parser::hkcl::HkclLeaf {
                        path: leaf.path,
                        yaml: leaf.yaml,
                        viewer_type: leaf.viewer_type,
                        read_only: leaf.read_only,
                    })
                    .collect();
                target.opened_file.bphcl = Some(file);
                leaves
            }
        };
        let mut sarc_paths = crate::file_format::Pack::SarcPaths::default();
        sarc_paths.read_only = true;
        sarc_paths.paths = leaves
            .into_iter()
            .map(|leaf| format!("{root_name}/{}", leaf.path))
            .collect();
        Ok(PhysicsDocumentUpdateResult {
            status_text: format!(
                "Updated {} document with {} rebuilt bytes",
                match request.format {
                    PhysicsMergeFormat::Hkcl => "HKCL",
                    PhysicsMergeFormat::Bphcl => "BPHCL",
                },
                request.bytes.len()
            ),
            format: request.format,
            byte_count: request.bytes.len(),
            sarc_paths,
        })
    }

    pub fn open_bphhb_documents(&self) -> Vec<OpenBphhbDocument> {
        let documents = self.documents();
        let mut result: Vec<_> = documents
            .iter()
            .filter_map(|(document_id, app)| {
                let bphhb = app.opened_file.bphhb.as_ref()?;
                let path = app.opened_file.path.full_path.clone();
                let label = if app.opened_file.path.name.is_empty() {
                    document_id.clone()
                } else {
                    app.opened_file.path.name.clone()
                };
                let location = match app
                    .internal_parent
                    .as_ref()
                    .and_then(|link| link.outer_path.as_ref())
                {
                    Some(_) => "nested-archive",
                    None if app.internal_parent.is_some() => "archive",
                    None => "disk",
                };
                Some(OpenBphhbDocument {
                    document_id: document_id.clone(),
                    label,
                    path,
                    location: location.into(),
                    bone_count: bphhb.document.bones.len(),
                })
            })
            .collect();
        result.sort_by(|left, right| left.document_id.cmp(&right.document_id));
        result
    }

    pub fn validate_physics_merge_request(
        &self,
        request: &PhysicsMergeRequest,
    ) -> PhysicsMergeValidation {
        let documents = self.documents();
        validate_physics_merge_request(&documents, request)
    }

    pub fn build_physics_merge_graph(
        &self,
        request: &PhysicsMergeRequest,
    ) -> Result<PhysicsGraphMergeResult, String> {
        let documents = self.documents();
        let validation = validate_physics_merge_request(&documents, request);
        if !validation.valid {
            return Err(validation.issues.join("; "));
        }
        let source_app = &documents[&request.source_document_id];
        let target_app = &documents[&request.target_document_id];
        let source = neutral_graph(source_app, request.source_format)?;
        let mut merged = neutral_graph(target_app, request.target_format)?;
        let helper = request
            .helper_document_id
            .as_ref()
            .and_then(|id| documents.get(id))
            .and_then(|app| app.opened_file.bphhb.as_ref())
            .map(|file| &file.document);
        let template = request.template_cloth_index.unwrap_or(0);
        let mut imported = Vec::new();

        for node_id in &request.node_ids {
            let (kind, index) = parse_physics_node_id(node_id)?;
            merged = match (request.target_format, request.source_format, kind) {
                (PhysicsMergeFormat::Hkcl, PhysicsMergeFormat::Hkcl, "cloth") => {
                    crate::parser::hkcl_merge::merge_complete_hkcl_cloth(&merged, &source, index)
                }
                (PhysicsMergeFormat::Hkcl, PhysicsMergeFormat::Hkcl, "collidable") => {
                    crate::parser::hkcl_merge::merge_standalone_hkcl_collidable(
                        &merged, &source, index,
                    )
                }
                (PhysicsMergeFormat::Bphcl, PhysicsMergeFormat::Hkcl, "cloth") => {
                    crate::parser::hkcl_to_bphcl::merge_hkcl_cloth_into_bphcl(
                        &source, &merged, index, template,
                    )
                }
                (PhysicsMergeFormat::Hkcl, PhysicsMergeFormat::Bphcl, "cloth") => {
                    if let Some(helper) = helper {
                        crate::parser::bphcl_to_hkcl::merge_bphcl_cloth_into_hkcl_with_bphhb(
                            &source, &merged, index, template, helper,
                        )
                    } else {
                        crate::parser::bphcl_to_hkcl::merge_bphcl_cloth_into_hkcl(
                            &source, &merged, index, template,
                        )
                    }
                }
                (PhysicsMergeFormat::Bphcl, PhysicsMergeFormat::Bphcl, _) => {
                    return Err("Use merge_bphcl_nodes for native BPHCL document mutation".into())
                }
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cross-format standalone collidable merging is not supported",
                )),
            }
            .map_err(|error| format!("Failed to merge '{node_id}': {error}"))?;
            imported.push(node_id.clone());
        }
        Ok(PhysicsGraphMergeResult {
            imported,
            graph: merged,
        })
    }

    pub fn merge_hkcl_nodes_into_bphcl(
        &self,
        request: &PhysicsMergeRequest,
    ) -> Result<BphclMergeResult, String> {
        if request.source_format != PhysicsMergeFormat::Hkcl
            || request.target_format != PhysicsMergeFormat::Bphcl
        {
            return Err(
                "Native cross-format saving requires an HKCL source and BPHCL target".into(),
            );
        }
        let mut documents = self.documents();
        let validation = validate_physics_merge_request(&documents, request);
        if !validation.valid {
            return Err(validation.issues.join("; "));
        }
        let source = documents
            .get(&request.source_document_id)
            .ok_or_else(|| format!("Document '{}' is not open", request.source_document_id))?
            .opened_file
            .hkcl
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "Document '{}' is not an HKCL document",
                    request.source_document_id
                )
            })?
            .document
            .neutral_physics_graph();
        let target_app = documents
            .get(&request.target_document_id)
            .ok_or_else(|| format!("Document '{}' is not open", request.target_document_id))?;
        let target_file = target_app.opened_file.bphcl.as_ref().ok_or_else(|| {
            format!(
                "Document '{}' is not a BPHCL document",
                request.target_document_id
            )
        })?;
        let target_source_path = target_file.source_path.clone();
        let parent_link = target_app.internal_parent.clone();
        let original_cloth_count = target_file.document.cloth.len();
        let original_collidable_count = target_file.document.collidables.len();
        let template = request
            .template_cloth_index
            .ok_or_else(|| "HKCL to BPHCL import requires a template cloth".to_owned())?;
        let mut merged = target_file.document.clone();
        let mut imported = Vec::new();
        let mut skipped = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for node_id in &request.node_ids {
            if !seen.insert(node_id.as_str()) {
                skipped.push(BphclMergeSkip {
                    name: node_id.clone(),
                    reason: "Duplicate selection".into(),
                });
                continue;
            }
            let (kind, index) = parse_physics_node_id(node_id)?;
            if kind != "cloth" {
                return Err(format!(
                    "HKCL to BPHCL saving does not support standalone {kind} nodes"
                ));
            }
            let name = source
                .cloths
                .get(index)
                .and_then(|cloth| cloth.name.clone())
                .unwrap_or_else(|| format!("HKCL cloth {index}"));
            let bytes = merged
                .import_hkcl_cloth(&source, index, template)
                .map_err(|error| format!("Failed to import '{node_id}': {error}"))?;
            merged = crate::parser::bphcl::BphclDocument::parse(&bytes)
                .map_err(|error| format!("Imported BPHCL did not reparse: {error}"))?;
            imported.push(format!("Cloth: {name}"));
        }

        let final_bytes = merged.raw.clone();
        if let Some(link) = &parent_link {
            documents
                .get_mut(&link.document_id)
                .ok_or_else(|| format!("Parent document '{}' is not open", link.document_id))?
                .update_child_entry(link.outer_path.as_deref(), &link.inner_path, final_bytes)?;
        }
        let cloth_count = merged.cloth.len();
        let collidable_count = merged.collidables.len();
        let added_cloth_count = cloth_count.saturating_sub(original_cloth_count);
        let added_collidable_count = collidable_count.saturating_sub(original_collidable_count);
        let target = documents
            .get_mut(&request.target_document_id)
            .ok_or_else(|| {
                format!(
                    "Document '{}' was closed during merge",
                    request.target_document_id
                )
            })?;
        target.opened_file.bphcl = Some(crate::file_format::bphcl::BphclFile {
            source_path: target_source_path,
            document: merged,
        });
        let root_name = if target.opened_file.path.name.is_empty() {
            "merged.bphcl".to_string()
        } else {
            target.opened_file.path.name.clone()
        };
        let refreshed = target
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| "Merged BPHCL state was lost".to_owned())?;
        let sarc_paths = refreshed
            .send_data(
                std::path::Path::new(&root_name),
                "Refreshed HKCL-imported BPHCL".into(),
            )
            .map_err(|error| format!("Failed to refresh merged BPHCL tree: {error}"))?
            .sarc_paths;

        Ok(BphclMergeResult {
            selected_count: request.node_ids.len(),
            imported_count: added_cloth_count + added_collidable_count,
            imported_selection_count: imported.len(),
            skipped_count: skipped.len(),
            added_cloth_count,
            added_collidable_count,
            cloth_count,
            collidable_count,
            imported,
            skipped,
            sarc_paths,
        })
    }

    pub fn remove_bphcl_node(
        &self,
        document_id: &str,
        path: &str,
    ) -> Result<BphclMutationResult, String> {
        let normalized = path.replace('\\', "/");
        let parts: Vec<_> = normalized.split('/').collect();
        let (kind, file_name) = parts
            .windows(2)
            .find_map(|parts| match parts[0] {
                "Cloth" => Some(("cloth", parts[1])),
                "Collidables" => Some(("collidable", parts[1])),
                _ => None,
            })
            .ok_or_else(|| "Only cloth and collidable nodes can be removed".to_string())?;
        let index = file_name
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| format!("Cannot determine BPHCL node index from '{file_name}'"))?;

        let mut documents = self.documents();
        let app = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))?;
        let file = app
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| format!("Document '{document_id}' is not a BPHCL document"))?;
        let source_path = file.source_path.clone();
        let parent_link = app.internal_parent.clone();
        let display_name = if kind == "cloth" {
            file.document
                .cloth
                .get(index)
                .map(|node| node.name.clone())
                .ok_or_else(|| format!("Cloth {index} no longer exists"))?
        } else {
            file.document
                .collidables
                .get(index)
                .map(|node| node.name.clone())
                .ok_or_else(|| format!("Collidable {index} no longer exists"))?
        };
        let bytes = if kind == "cloth" {
            file.document.remove_cloth(index)
        } else {
            file.document.remove_collidable(index)
        }
        .map_err(|error| error.to_string())?;
        let rebuilt = crate::parser::bphcl::BphclDocument::parse(&bytes)
            .map_err(|error| format!("Removed BPHCL did not reparse: {error}"))?;

        if let Some(link) = &parent_link {
            documents
                .get_mut(&link.document_id)
                .ok_or_else(|| format!("Parent document '{}' is not open", link.document_id))?
                .update_child_entry(link.outer_path.as_deref(), &link.inner_path, bytes)?;
        }
        let target = documents
            .get_mut(document_id)
            .ok_or_else(|| format!("Document '{document_id}' was closed during removal"))?;
        target.opened_file.bphcl = Some(crate::file_format::bphcl::BphclFile {
            source_path,
            document: rebuilt,
        });
        let root_name = if target.opened_file.path.name.is_empty() {
            "modified.bphcl".to_string()
        } else {
            target.opened_file.path.name.clone()
        };
        let mut sarc_paths = crate::file_format::Pack::SarcPaths::default();
        sarc_paths.read_only = true;
        let refreshed = target
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| "Modified BPHCL state was lost".to_owned())?;
        sarc_paths.paths = refreshed
            .leaves()
            .map_err(|error| format!("Failed to refresh BPHCL tree: {error}"))?
            .into_iter()
            .map(|leaf| format!("{root_name}/{}", leaf.path))
            .collect();
        Ok(BphclMutationResult {
            status_text: format!("Removed {kind} '{display_name}'"),
            sarc_paths,
        })
    }

    pub fn merge_bphcl_nodes(
        &self,
        target_document_id: &str,
        source_document_id: &str,
        node_ids: &[String],
    ) -> Result<BphclMergeResult, String> {
        if target_document_id == source_document_id {
            return Err("Source and target BPHCL documents must be different".into());
        }
        if node_ids.is_empty() {
            return Err("Select at least one cloth or collidable to merge".into());
        }

        let mut documents = self.documents();
        let source = documents
            .get(source_document_id)
            .ok_or_else(|| format!("Document '{source_document_id}' is not open"))?
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| format!("Document '{source_document_id}' is not a BPHCL document"))?
            .document
            .clone();
        let target_app = documents
            .get(target_document_id)
            .ok_or_else(|| format!("Document '{target_document_id}' is not open"))?;
        let target_file =
            target_app.opened_file.bphcl.as_ref().ok_or_else(|| {
                format!("Document '{target_document_id}' is not a BPHCL document")
            })?;
        let target_source_path = target_file.source_path.clone();
        let original_cloth_count = target_file.document.cloth.len();
        let original_collidable_count = target_file.document.collidables.len();
        let original_collidable_names: std::collections::HashSet<String> = target_file
            .document
            .collidables
            .iter()
            .map(|collidable| collidable.name.clone())
            .collect();
        let mut merged = target_file.document.clone();
        let mut imported = Vec::new();
        let mut skipped = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for node_id in node_ids {
            if !seen.insert(node_id.as_str()) {
                skipped.push(BphclMergeSkip {
                    name: node_id.clone(),
                    reason: "Duplicate selection".into(),
                });
                continue;
            }
            let (kind, index) = node_id
                .split_once(':')
                .ok_or_else(|| format!("Invalid BPHCL node ID '{node_id}'"))?;
            let index: usize = index
                .parse()
                .map_err(|_| format!("Invalid BPHCL node ID '{node_id}'"))?;
            let before = merged.raw.clone();
            let (display_name, existed_in_original_target, bytes) = match kind {
                "cloth" => {
                    let position = source
                        .cloth
                        .iter()
                        .position(|cloth| cloth.index == index)
                        .ok_or_else(|| format!("Source cloth '{node_id}' no longer exists"))?;
                    let name = source.cloth[position].name.clone();
                    (
                        format!("Cloth: {name}"),
                        false,
                        merged.merge_complete_cloth(&source, position),
                    )
                }
                "collidable" => {
                    let position = source
                        .collidables
                        .iter()
                        .position(|collidable| collidable.index == index)
                        .ok_or_else(|| format!("Source collidable '{node_id}' no longer exists"))?;
                    let name = source.collidables[position].name.clone();
                    (
                        format!("Collidable: {name}"),
                        original_collidable_names.contains(&name),
                        merged.merge_collidable(&source, position),
                    )
                }
                _ => (
                    node_id.clone(),
                    false,
                    Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Unsupported BPHCL node kind '{kind}'"),
                    )),
                ),
            };
            let bytes = bytes.map_err(|error| format!("Failed to merge '{node_id}': {error}"))?;
            if bytes == before {
                if kind == "collidable" && !existed_in_original_target {
                    // A previously processed cloth imported this selected collidable as a
                    // dependency. The selection was fulfilled by this merge, not skipped.
                    imported.push(display_name);
                } else {
                    skipped.push(BphclMergeSkip {
                        name: display_name,
                        reason: if kind == "cloth" {
                            "Target already contains a cloth with this name"
                        } else {
                            "Target already contained a collidable with this exact name before the merge"
                        }
                        .into(),
                    });
                }
            } else {
                imported.push(display_name);
                merged = crate::parser::bphcl::BphclDocument::parse(&bytes)
                    .map_err(|error| format!("Merged BPHCL did not reparse: {error}"))?;
            }
        }

        let cloth_count = merged.cloth.len();
        let collidable_count = merged.collidables.len();
        let added_cloth_count = cloth_count.saturating_sub(original_cloth_count);
        let added_collidable_count = collidable_count.saturating_sub(original_collidable_count);
        let target = documents
            .get_mut(target_document_id)
            .ok_or_else(|| format!("Document '{target_document_id}' was closed during merge"))?;
        target.opened_file.bphcl = Some(crate::file_format::bphcl::BphclFile {
            source_path: target_source_path,
            document: merged,
        });
        let root_name = if target.opened_file.path.name.is_empty() {
            "merged.bphcl".to_string()
        } else {
            target.opened_file.path.name.clone()
        };
        let refreshed = target
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| "Merged BPHCL state was lost".to_owned())?;
        let sarc_paths = refreshed
            .send_data(
                std::path::Path::new(&root_name),
                "Refreshed merged BPHCL".into(),
            )
            .map_err(|error| format!("Failed to refresh merged BPHCL tree: {error}"))?
            .sarc_paths;

        Ok(BphclMergeResult {
            selected_count: node_ids.len(),
            imported_count: added_cloth_count + added_collidable_count,
            imported_selection_count: imported.len(),
            skipped_count: skipped.len(),
            added_cloth_count,
            added_collidable_count,
            cloth_count,
            collidable_count,
            imported,
            skipped,
            sarc_paths,
        })
    }

    pub fn open_bphcl_documents(&self) -> Vec<OpenBphclDocument> {
        let documents = self.documents();
        let mut result: Vec<_> = documents
            .iter()
            .filter_map(|(document_id, app)| {
                let bphcl = app.opened_file.bphcl.as_ref()?;
                let path = app.opened_file.path.full_path.clone();
                let label = if app.opened_file.path.name.is_empty() {
                    document_id.clone()
                } else {
                    app.opened_file.path.name.clone()
                };
                let location = match app
                    .internal_parent
                    .as_ref()
                    .and_then(|link| link.outer_path.as_ref())
                {
                    Some(_) => "nested-archive",
                    None if app.internal_parent.is_some() => "archive",
                    None => "disk",
                };
                Some(OpenBphclDocument {
                    document_id: document_id.clone(),
                    label,
                    path,
                    location: location.into(),
                    cloth_count: bphcl.document.cloth.len(),
                    collidable_count: bphcl.document.collidables.len(),
                })
            })
            .collect();
        result.sort_by(|left, right| left.document_id.cmp(&right.document_id));
        result
    }

    pub fn bphcl_selectable_nodes(
        &self,
        document_id: &str,
    ) -> Result<Vec<BphclSelectableNode>, String> {
        let documents = self.documents();
        let app = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))?;
        let bphcl = app
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| format!("Document '{document_id}' is not a BPHCL document"))?;
        let mut nodes =
            Vec::with_capacity(bphcl.document.cloth.len() + bphcl.document.collidables.len());
        nodes.extend(
            bphcl
                .document
                .cloth
                .iter()
                .map(|cloth| BphclSelectableNode {
                    document_id: document_id.into(),
                    node_id: format!("cloth:{}", cloth.index),
                    kind: "cloth".into(),
                    index: cloth.index,
                    name: cloth.name.clone(),
                    item_index: cloth.item_index,
                }),
        );
        nodes.extend(
            bphcl
                .document
                .collidables
                .iter()
                .map(|collidable| BphclSelectableNode {
                    document_id: document_id.into(),
                    node_id: format!("collidable:{}", collidable.index),
                    kind: "collidable".into(),
                    index: collidable.index,
                    name: collidable.name.clone(),
                    item_index: collidable.item_index,
                }),
        );
        Ok(nodes)
    }

    pub fn open_hkcl_documents(&self) -> Vec<OpenHkclDocument> {
        let documents = self.documents();
        let mut result: Vec<_> = documents
            .iter()
            .filter_map(|(document_id, app)| {
                let hkcl = app.opened_file.hkcl.as_ref()?;
                let path = app.opened_file.path.full_path.clone();
                let label = if app.opened_file.path.name.is_empty() {
                    document_id.clone()
                } else {
                    app.opened_file.path.name.clone()
                };
                let location = match app
                    .internal_parent
                    .as_ref()
                    .and_then(|link| link.outer_path.as_ref())
                {
                    Some(_) => "nested-archive",
                    None if app.internal_parent.is_some() => "archive",
                    None => "disk",
                };
                Some(OpenHkclDocument {
                    document_id: document_id.clone(),
                    label,
                    path,
                    location: location.into(),
                    skeleton_count: hkcl.document.physics.skeletons.len(),
                    cloth_count: hkcl.document.physics.cloths.len(),
                    constraint_count: hkcl.document.physics.constraints.len(),
                    collidable_count: hkcl.document.physics.collidables.len(),
                })
            })
            .collect();
        result.sort_by(|left, right| left.document_id.cmp(&right.document_id));
        result
    }

    pub fn hkcl_selectable_nodes(
        &self,
        document_id: &str,
    ) -> Result<Vec<HkclSelectableNode>, String> {
        let documents = self.documents();
        let app = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))?;
        let hkcl = app
            .opened_file
            .hkcl
            .as_ref()
            .ok_or_else(|| format!("Document '{document_id}' is not an HKCL document"))?;
        let mut nodes = Vec::with_capacity(
            hkcl.document.physics.cloths.len() + hkcl.document.physics.collidables.len(),
        );
        nodes.extend(
            hkcl.document
                .physics
                .cloths
                .iter()
                .enumerate()
                .map(|(index, cloth)| HkclSelectableNode {
                    document_id: document_id.into(),
                    node_id: format!("cloth:{index}"),
                    kind: "cloth".into(),
                    index,
                    name: cloth
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Cloth {index}")),
                    section_index: cloth.key.section_index,
                    data_offset: cloth.key.offset,
                }),
        );
        nodes.extend(hkcl.document.physics.collidables.iter().enumerate().map(
            |(index, collidable)| {
                HkclSelectableNode {
                    document_id: document_id.into(),
                    node_id: format!("collidable:{index}"),
                    kind: "collidable".into(),
                    index,
                    name: collidable
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Collidable {index}")),
                    section_index: collidable.key.section_index,
                    data_offset: collidable.key.offset,
                }
            },
        ));
        Ok(nodes)
    }
    pub fn open_bphcl_leaf(
        &self,
        parent_id: &str,
        child_id: &str,
        path: String,
    ) -> Option<SendData> {
        let mut documents = self.documents();
        let parent = documents.get(parent_id)?;
        let (leaf, format_name) = if let Some(file) = parent.opened_file.bphcl.as_ref() {
            let leaf = file.leaf(&path).ok()?;
            (
                crate::parser::hkcl::HkclLeaf {
                    path: leaf.path,
                    yaml: leaf.yaml,
                    viewer_type: leaf.viewer_type,
                    read_only: leaf.read_only,
                },
                "BPHCL",
            )
        } else if let Some(file) = parent.opened_file.hkcl.as_ref() {
            (file.leaf(&path).ok()?, "HKCL")
        } else if let Some(file) = parent.opened_file.bphhb.as_ref() {
            (file.leaf(&path).ok()?, "BPHHB")
        } else if let Some(file) = parent.opened_file.bphyssb.as_ref() {
            (file.leaf(&path).ok()?, "BPHYSSB")
        } else if let Some(file) = parent.opened_file.hkrg.as_ref() {
            (file.leaf(&path).ok()?, "HKRG")
        } else {
            return None;
        };
        let editable_bphcl_aamp = format_name == "BPHCL" && leaf.viewer_type == "AAMP";
        let child = documents.entry(child_id.to_owned()).or_default();
        if editable_bphcl_aamp {
            child.opened_file = crate::file_format::BinTextFile::OpenedFile::from_path(
                path.clone(),
                crate::Zstd::TotkFileType::Aamp,
            );
            let mut internal = crate::InternalFile::InternalFile::new(path.clone());
            internal.file_type = crate::Zstd::TotkFileType::Aamp;
            child.internal_file = Some(internal);
            child.internal_parent = Some(crate::TotkApp::InternalParentLink {
                document_id: parent_id.to_owned(),
                outer_path: None,
                inner_path: path.clone(),
            });
        }
        let mut data = SendData::default();
        data.text = leaf.yaml;
        data.tab = "YAML".into();
        data.lang = "yaml".into();
        let file_name = path.replace('\\', "/").rsplit('/').next()?.to_owned();
        data.path = crate::Settings::Pathlib::new(&file_name);
        let mode = if editable_bphcl_aamp {
            "Editable"
        } else {
            "ReadOnly"
        };
        data.file_label = format!(
            "{file_name} [{format_name}] [{}] [{mode}]",
            leaf.viewer_type
        );
        data.file_metadata = format!("[{format_name}] [{}] [{mode}]", leaf.viewer_type);
        data.file_type = if editable_bphcl_aamp {
            crate::Zstd::TotkFileType::Aamp
        } else {
            match format_name {
                "BPHCL" => crate::Zstd::TotkFileType::Bphcl,
                "HKCL" => crate::Zstd::TotkFileType::Hkcl,
                "BPHHB" => crate::Zstd::TotkFileType::Bphhb,
                "BPHYSSB" => crate::Zstd::TotkFileType::Bphyssb,
                "HKRG" => crate::Zstd::TotkFileType::Hkrg,
                _ => crate::Zstd::TotkFileType::Other,
            }
        };
        data.status_text = format!("Opened {} {format_name} leaf: {path}", mode.to_lowercase());
        data.read_only = !editable_bphcl_aamp;
        Some(data)
    }

    pub fn open_amta_node(
        &self,
        parent_id: &str,
        child_id: &str,
        path: String,
    ) -> Result<SendData, String> {
        let mut documents = self.documents();
        let parent = documents
            .get(parent_id)
            .ok_or_else(|| "parent document is not open".to_owned())?;
        let bytes = parent
            .archive
            .as_ref()
            .and_then(|archive| archive.get(&path))
            .ok_or_else(|| format!("archive entry not found: {path}"))?;
        let amta = crate::parser::amta::AmtaFile::parse(bytes)?;
        let yaml = serde_yaml::to_string(&amta).map_err(|error| error.to_string())?;
        let file_name = path
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or("metadata.amta")
            .to_owned();

        documents.entry(child_id.to_owned()).or_default();
        let mut data = SendData::default();
        data.text = yaml;
        data.tab = "YAML".into();
        data.lang = "yaml".into();
        data.path = crate::Settings::Pathlib::new(&file_name);
        data.file_label = format!("{file_name} [AMTA] [ReadOnly]");
        data.file_metadata = "[AMTA] [ReadOnly]".into();
        data.file_type = crate::Zstd::TotkFileType::Other;
        data.status_text = format!("Opened read-only AMTA metadata: {path}");
        data.read_only = true;
        Ok(data)
    }
    fn documents(&self) -> std::sync::MutexGuard<'_, HashMap<String, TotkBitsApp<'static>>> {
        match self.documents.lock() {
            Ok(documents) => documents,
            Err(poisoned) => {
                // A panic may have interrupted a multi-document update. Discard the
                // potentially partial graph rather than exposing inconsistent state.
                let mut documents = poisoned.into_inner();
                documents.clear();
                self.documents.clear_poison();
                documents
            }
        }
    }

    pub fn open_internal_child(
        &self,
        parent_id: &str,
        child_id: &str,
        outer_path: Option<String>,
        path: String,
    ) -> Option<SendData> {
        let mut documents = self.documents();
        let bytes = documents
            .get_mut(parent_id)?
            .internal_entry_bytes(outer_path.as_deref(), &path)?;
        documents
            .entry(child_id.to_owned())
            .or_default()
            .open_internal_child(parent_id.to_owned(), outer_path, path, bytes)
    }

    pub fn save_document(&self, id: &str, save_data: SaveData) -> Option<SendData> {
        let mut documents = self.documents();
        let link = documents.get(id)?.internal_parent.clone();
        let Some(link) = link else {
            return documents.get_mut(id)?.save(save_data);
        };
        if !documents.contains_key(&link.document_id) {
            let child = documents.get_mut(id)?;
            let result = child.save_as(save_data);
            if result.is_some() {
                if let Some(internal) = &child.internal_file {
                    child.opened_file.file_type = internal.file_type;
                    child.opened_file.endian = internal.endian;
                }
                child.internal_parent = None;
                child.internal_file = None;
            }
            return result;
        }
        let is_bphcl_aamp = documents
            .get(id)
            .is_some_and(|child| child.opened_file.file_type == crate::Zstd::TotkFileType::Aamp)
            && documents
                .get(&link.document_id)
                .is_some_and(|parent| parent.opened_file.bphcl.is_some())
            && link.inner_path.ends_with("Section.aamp");
        if is_bphcl_aamp {
            let parent = documents.get_mut(&link.document_id)?;
            let bphcl = parent.opened_file.bphcl.as_mut()?;
            if let Err(error) = bphcl.replace_aamp_yaml(&save_data.text) {
                let mut data = SendData::default();
                data.tab = "ERROR".into();
                data.status_text = format!("Error: failed to update BPHCL Section.aamp: {error}");
                return Some(data);
            }
            let source_path = bphcl
                .source_path
                .clone()
                .unwrap_or_else(|| parent.opened_file.path.full_path.clone());
            let mut data = bphcl
                .send_data(
                    std::path::Path::new(&source_path),
                    "Updated Section.aamp in BPHCL; save the BPHCL to persist it".into(),
                )
                .ok()?;
            data.tab = "YAML".into();
            return Some(data);
        }
        let bytes = documents
            .get_mut(id)?
            .internal_binary(&save_data.text, None)?;
        let parent_data = documents
            .get_mut(&link.document_id)?
            .update_child_entry(link.outer_path.as_deref(), &link.inner_path, bytes)
            .ok()?;
        let child = documents.get(id)?;
        if let Some(bphcl) = &child.opened_file.bphcl {
            let mut data = bphcl
                .send_data(
                    std::path::Path::new(&link.inner_path),
                    format!("Saved {} in archive", link.inner_path),
                )
                .ok()?;
            data.parent_modded_path = Some(link.inner_path);
            return Some(data);
        }
        Some(parent_data)
    }

    pub fn compare_internal_file(
        &self,
        id: &str,
        internal_path: String,
        is_from_sarc: bool,
    ) -> Option<SendData> {
        let mut documents = self.documents();
        let msbt_parent = documents.get(id).and_then(|child| {
            let link = child.internal_parent.as_ref()?;
            let internal = child.internal_file.as_ref()?;
            (!is_from_sarc && link.outer_path.is_none() && internal.file_type == TotkFileType::Msbt)
                .then(|| (link.document_id.clone(), link.inner_path.clone()))
        });
        if let Some((parent_id, path)) = msbt_parent {
            if let Some(parent) = documents.get_mut(&parent_id) {
                return parent.compare_internal_file_with_original(path, true);
            }
        }
        documents
            .get_mut(id)?
            .compare_internal_file_with_original(internal_path, is_from_sarc)
    }
    pub fn with_mut<T>(
        &self,
        id: &str,
        operation: impl FnOnce(&mut TotkBitsApp<'static>) -> T,
    ) -> T {
        let mut documents = self.documents();
        operation(documents.entry(id.to_owned()).or_default())
    }

    pub fn with<T>(&self, id: &str, operation: impl FnOnce(&TotkBitsApp<'static>) -> T) -> T {
        let mut documents = self.documents();
        operation(documents.entry(id.to_owned()).or_default())
    }

    pub fn close(&self, id: &str) -> bool {
        self.documents().remove(id).is_some()
    }

    pub fn close_all(&self) {
        self.documents().clear();
    }
}

fn validate_physics_merge_request(
    documents: &HashMap<String, TotkBitsApp<'static>>,
    request: &PhysicsMergeRequest,
) -> PhysicsMergeValidation {
    let cross_format = request.target_format != request.source_format;
    let mut issues = Vec::new();
    if request.target_document_id == request.source_document_id {
        issues.push("Source and target documents must be different".into());
    }
    if request.node_ids.is_empty() {
        issues.push("Select at least one cloth or collidable".into());
    }
    let source = documents.get(&request.source_document_id);
    let target = documents.get(&request.target_document_id);
    if source.is_none() {
        issues.push(format!(
            "Source document '{}' is not open",
            request.source_document_id
        ));
    }
    if target.is_none() {
        issues.push(format!(
            "Target document '{}' is not open",
            request.target_document_id
        ));
    }
    if let Some(source) = source {
        if neutral_graph(source, request.source_format).is_err() {
            issues.push("Selected source format does not match the source document".into());
        }
    }
    if let Some(target) = target {
        if neutral_graph(target, request.target_format).is_err() {
            issues.push("Selected target format does not match the target document".into());
        }
    }
    for node_id in &request.node_ids {
        match parse_physics_node_id(node_id) {
            Ok(("collidable", _)) if cross_format => issues.push(format!(
                "Cross-format merge does not support standalone collidable '{node_id}'"
            )),
            Ok(_) => {}
            Err(error) => issues.push(error),
        }
    }
    if cross_format && request.template_cloth_index.is_none() {
        issues.push("Cross-format cloth merging requires a target template cloth".into());
    }
    let helper_used = request.helper_document_id.is_some();
    if let Some(helper_id) = &request.helper_document_id {
        if request.target_format != PhysicsMergeFormat::Hkcl
            || request.source_format != PhysicsMergeFormat::Bphcl
        {
            issues.push("BPHHB assistance is only valid for BPHCL to HKCL merging".into());
        }
        if !documents
            .get(helper_id)
            .is_some_and(|app| app.opened_file.bphhb.is_some())
        {
            issues.push(format!(
                "Helper document '{helper_id}' is not an open BPHHB document"
            ));
        }
    }
    if cross_format {
        if let (Some(source_app), Some(target_app), Some(template)) =
            (source, target, request.template_cloth_index)
        {
            if let (Ok(source_graph), Ok(target_graph)) = (
                neutral_graph(source_app, request.source_format),
                neutral_graph(target_app, request.target_format),
            ) {
                let helper = request
                    .helper_document_id
                    .as_ref()
                    .and_then(|id| documents.get(id))
                    .and_then(|app| app.opened_file.bphhb.as_ref())
                    .map(|file| &file.document);
                for node_id in &request.node_ids {
                    let Ok(("cloth", cloth)) = parse_physics_node_id(node_id) else {
                        continue;
                    };
                    let compatibility = match (request.target_format, request.source_format) {
                        (PhysicsMergeFormat::Bphcl, PhysicsMergeFormat::Hkcl) => {
                            crate::parser::hkcl_to_bphcl::analyze_hkcl_to_bphcl(
                                &source_graph,
                                &target_graph,
                                cloth,
                                template,
                            )
                        }
                        (PhysicsMergeFormat::Hkcl, PhysicsMergeFormat::Bphcl) => {
                            if let Some(helper) = helper {
                                crate::parser::bphcl_to_hkcl::analyze_bphcl_to_hkcl_with_bphhb(
                                    &source_graph,
                                    &target_graph,
                                    cloth,
                                    template,
                                    helper,
                                )
                            } else {
                                crate::parser::bphcl_to_hkcl::analyze_bphcl_to_hkcl(
                                    &source_graph,
                                    &target_graph,
                                    cloth,
                                    template,
                                )
                            }
                        }
                        _ => continue,
                    };
                    issues.extend(
                        compatibility
                            .issues
                            .into_iter()
                            .map(|issue| format!("{node_id}: {}", issue.message)),
                    );
                }
            }
        }
    }
    if request.target_format == PhysicsMergeFormat::Bphcl
        && request.source_format == PhysicsMergeFormat::Bphcl
    {
        issues.push("Use the native merge_bphcl_nodes API for BPHCL to BPHCL merging".into());
    }
    PhysicsMergeValidation {
        valid: issues.is_empty(),
        issues,
        requires_template: cross_format,
        supports_collidables: !cross_format,
        helper_used,
    }
}

fn neutral_graph(
    app: &TotkBitsApp<'static>,
    format: PhysicsMergeFormat,
) -> Result<crate::parser::physics_graph::FormatNeutralPhysicsGraph, String> {
    match format {
        PhysicsMergeFormat::Hkcl => app
            .opened_file
            .hkcl
            .as_ref()
            .map(|file| file.document.neutral_physics_graph())
            .ok_or_else(|| "document is not HKCL".into()),
        PhysicsMergeFormat::Bphcl => app
            .opened_file
            .bphcl
            .as_ref()
            .map(|file| file.document.neutral_physics_graph())
            .ok_or_else(|| "document is not BPHCL".into()),
    }
}

fn parse_physics_node_id(node_id: &str) -> Result<(&str, usize), String> {
    let (kind, index) = node_id
        .split_once(':')
        .ok_or_else(|| format!("Invalid physics node ID '{node_id}'"))?;
    if kind != "cloth" && kind != "collidable" {
        return Err(format!("Unsupported physics node kind '{kind}'"));
    }
    let index = index
        .parse()
        .map_err(|_| format!("Invalid physics node ID '{node_id}'"))?;
    Ok((kind, index))
}

/// Helper-bone (BPHHB) and support-bone (BPHYSSB) documents share one
/// driver-group surface; only same-format merges are meaningful.
impl DocumentState {
    fn sidecar_bytes(app: &crate::TotkApp::TotkBitsApp<'_>) -> Option<(&'static str, Vec<u8>)> {
        if let Some(file) = app.opened_file.bphhb.as_ref() {
            return Some(("bphhb", file.document.raw.clone()));
        }
        if let Some(file) = app.opened_file.bphyssb.as_ref() {
            return Some(("bphyssb", file.document.bytes.clone()));
        }
        None
    }

    fn sidecar_groups(
        format: &str,
        bytes: &[u8],
    ) -> Result<(Vec<String>, Vec<crate::parser::physics::SidecarDriverGroup>), String> {
        let result = match format {
            "bphhb" => crate::parser::physics::HelperBoneDocument::parse(bytes)
                .and_then(|document| Ok((document.bone_names(), document.driver_groups()?))),
            _ => crate::parser::physics::SupportBoneDocument::parse(bytes)
                .and_then(|document| Ok((document.bone_names(), document.driver_groups()?))),
        };
        result.map_err(|error| error.to_string())
    }

    fn document_location(app: &crate::TotkApp::TotkBitsApp<'_>) -> &'static str {
        match app
            .internal_parent
            .as_ref()
            .and_then(|link| link.outer_path.as_ref())
        {
            Some(_) => "nested-archive",
            None if app.internal_parent.is_some() => "archive",
            None => "disk",
        }
    }

    pub fn open_sidecar_documents(&self) -> Vec<OpenSidecarDocument> {
        let documents = self.documents();
        let mut result: Vec<_> = documents
            .iter()
            .filter_map(|(document_id, app)| {
                let (format, bytes) = Self::sidecar_bytes(app)?;
                let (bones, groups) = Self::sidecar_groups(format, &bytes).ok()?;
                let label = if app.opened_file.path.name.is_empty() {
                    document_id.clone()
                } else {
                    app.opened_file.path.name.clone()
                };
                Some(OpenSidecarDocument {
                    document_id: document_id.clone(),
                    label,
                    path: app.opened_file.path.full_path.clone(),
                    location: Self::document_location(app).into(),
                    format: format.into(),
                    bone_count: bones.len(),
                    group_count: groups.len(),
                })
            })
            .collect();
        result.sort_by(|left, right| left.document_id.cmp(&right.document_id));
        result
    }

    pub fn sidecar_driver_groups(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::parser::physics::SidecarDriverGroup>, String> {
        let documents = self.documents();
        let app = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))?;
        let (format, bytes) = Self::sidecar_bytes(app).ok_or_else(|| {
            format!("Document '{document_id}' is not a BPHHB or BPHYSSB document")
        })?;
        Ok(Self::sidecar_groups(format, &bytes)?.1)
    }

    /// Imports one driver group from a same-format donor into the target and
    /// refreshes the target's tree.
    pub fn merge_sidecar_driver_group(
        &self,
        target_document_id: &str,
        source_document_id: &str,
        group_index: usize,
    ) -> Result<SidecarMergeResult, String> {
        if target_document_id == source_document_id {
            return Err("Source and target documents must be different".into());
        }
        let mut documents = self.documents();
        let (source_format, source_bytes) = documents
            .get(source_document_id)
            .ok_or_else(|| format!("Document '{source_document_id}' is not open"))
            .and_then(|app| {
                Self::sidecar_bytes(app).ok_or_else(|| {
                    format!("Document '{source_document_id}' is not a BPHHB or BPHYSSB document")
                })
            })?;
        let (target_format, target_bytes) = documents
            .get(target_document_id)
            .ok_or_else(|| format!("Document '{target_document_id}' is not open"))
            .and_then(|app| {
                Self::sidecar_bytes(app).ok_or_else(|| {
                    format!("Document '{target_document_id}' is not a BPHHB or BPHYSSB document")
                })
            })?;
        if source_format != target_format {
            return Err(
                "Driver-group merging requires both documents to use the same sidecar format. \
                 Convert the donor first when moving between BPHHB and BPHYSSB."
                    .into(),
            );
        }
        let merged = match target_format {
            "bphhb" => {
                let target = crate::parser::physics::HelperBoneDocument::parse(&target_bytes);
                let source = crate::parser::physics::HelperBoneDocument::parse(&source_bytes);
                target
                    .and_then(|target| Ok(target.merge_driver_group(&source?, group_index)?.bytes))
            }
            _ => {
                let target = crate::parser::physics::SupportBoneDocument::parse(&target_bytes);
                let source = crate::parser::physics::SupportBoneDocument::parse(&source_bytes);
                target
                    .and_then(|target| Ok(target.merge_driver_group(&source?, group_index)?.bytes))
            }
        }
        .map_err(|error| format!("Failed to merge driver group {group_index}: {error}"))?;
        let (bones, groups) = Self::sidecar_groups(target_format, &merged)?;
        let sarc_paths = Self::replace_sidecar_document(
            &mut documents,
            target_document_id,
            target_format,
            merged,
        )?;
        Ok(SidecarMergeResult {
            status_text: format!(
                "Merged driver group {group_index}: {} groups, {} bones",
                groups.len(),
                bones.len()
            ),
            format: target_format.into(),
            group_count: groups.len(),
            bone_count: bones.len(),
            sarc_paths,
        })
    }

    /// Reflects one driver group across X, swapping L/R name tokens.
    pub fn mirror_sidecar_driver_group(
        &self,
        document_id: &str,
        group_index: usize,
    ) -> Result<SidecarMergeResult, String> {
        let mut documents = self.documents();
        let (format, bytes) = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))
            .and_then(|app| {
                Self::sidecar_bytes(app).ok_or_else(|| {
                    format!("Document '{document_id}' is not a BPHHB or BPHYSSB document")
                })
            })?;
        let mirrored = match format {
            "bphhb" => crate::parser::physics::HelperBoneDocument::parse(&bytes)
                .and_then(|document| Ok(document.mirror_driver_group_across_x(group_index)?.bytes)),
            _ => crate::parser::physics::SupportBoneDocument::parse(&bytes)
                .and_then(|document| Ok(document.mirror_driver_group_across_x(group_index)?.bytes)),
        }
        .map_err(|error| format!("Failed to mirror driver group {group_index}: {error}"))?;
        let (bones, groups) = Self::sidecar_groups(format, &mirrored)?;
        let sarc_paths =
            Self::replace_sidecar_document(&mut documents, document_id, format, mirrored)?;
        Ok(SidecarMergeResult {
            status_text: format!("Mirrored driver group {group_index} across X"),
            format: format.into(),
            group_count: groups.len(),
            bone_count: bones.len(),
            sarc_paths,
        })
    }

    fn replace_sidecar_document(
        documents: &mut std::collections::HashMap<String, crate::TotkApp::TotkBitsApp<'static>>,
        document_id: &str,
        format: &str,
        bytes: Vec<u8>,
    ) -> Result<crate::file_format::Pack::SarcPaths, String> {
        let app = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))?;
        let parent_link = app.internal_parent.clone();
        let source_path = match format {
            "bphhb" => app
                .opened_file
                .bphhb
                .as_ref()
                .and_then(|file| file.source_path.clone()),
            _ => app
                .opened_file
                .bphyssb
                .as_ref()
                .and_then(|file| file.source_path.clone()),
        };
        if let Some(link) = &parent_link {
            documents
                .get_mut(&link.document_id)
                .ok_or_else(|| format!("Parent document '{}' is not open", link.document_id))?
                .update_child_entry(link.outer_path.as_deref(), &link.inner_path, bytes.clone())?;
        }
        let target = documents
            .get_mut(document_id)
            .ok_or_else(|| format!("Document '{document_id}' was closed during update"))?;
        let root_name = if target.opened_file.path.name.is_empty() {
            format!("merged.{format}")
        } else {
            target.opened_file.path.name.clone()
        };
        let path = source_path.as_deref().map(std::path::Path::new);
        let sarc_paths = match format {
            "bphhb" => {
                let file = crate::file_format::bphhb::BphhbFile::from_binary(&bytes, path)
                    .map_err(|error| format!("Rebuilt BPHHB did not reparse: {error}"))?;
                let paths = file
                    .send_data(std::path::Path::new(&root_name), "Refreshed BPHHB".into())
                    .map_err(|error| format!("Failed to refresh BPHHB tree: {error}"))?
                    .sarc_paths;
                target.opened_file.bphhb = Some(file);
                paths
            }
            _ => {
                let file = crate::file_format::bphyssb::BphyssbFile::from_binary(&bytes, path)
                    .map_err(|error| format!("Rebuilt BPHYSSB did not reparse: {error}"))?;
                let paths = file
                    .send_data(std::path::Path::new(&root_name), "Refreshed BPHYSSB".into())
                    .map_err(|error| format!("Failed to refresh BPHYSSB tree: {error}"))?
                    .sarc_paths;
                target.opened_file.bphyssb = Some(file);
                paths
            }
        };
        Ok(sarc_paths)
    }

    /// Drops every unreachable DATA allocation from an open BPHCL, the way
    /// PhysicsTool's save-without-cloth path does after a removal.
    pub fn compact_bphcl_document(&self, document_id: &str) -> Result<BphclMutationResult, String> {
        let mut documents = self.documents();
        let app = documents
            .get(document_id)
            .ok_or_else(|| format!("Document '{document_id}' is not open"))?;
        let file = app
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| format!("Document '{document_id}' is not a BPHCL document"))?;
        let source_path = file.source_path.clone();
        let parent_link = app.internal_parent.clone();
        let before = file.document.raw.len();
        let bytes = crate::parser::physics::compact(&file.document)
            .map_err(|error| format!("BPHCL compaction failed: {error}"))?;
        let rebuilt = crate::parser::bphcl::BphclDocument::parse(&bytes)
            .map_err(|error| format!("Compacted BPHCL did not reparse: {error}"))?;
        rebuilt
            .validate_item_graph()
            .map_err(|error| format!("Compacted BPHCL is invalid: {error}"))?;
        if let Some(link) = &parent_link {
            documents
                .get_mut(&link.document_id)
                .ok_or_else(|| format!("Parent document '{}' is not open", link.document_id))?
                .update_child_entry(link.outer_path.as_deref(), &link.inner_path, bytes.clone())?;
        }
        let target = documents
            .get_mut(document_id)
            .ok_or_else(|| format!("Document '{document_id}' was closed during compaction"))?;
        target.opened_file.bphcl = Some(crate::file_format::bphcl::BphclFile {
            source_path,
            document: rebuilt,
        });
        let root_name = if target.opened_file.path.name.is_empty() {
            "compacted.bphcl".to_string()
        } else {
            target.opened_file.path.name.clone()
        };
        let mut sarc_paths = crate::file_format::Pack::SarcPaths::default();
        sarc_paths.read_only = true;
        sarc_paths.paths = target
            .opened_file
            .bphcl
            .as_ref()
            .ok_or_else(|| "Compacted BPHCL state was lost".to_owned())?
            .leaves()
            .map_err(|error| format!("Failed to refresh BPHCL tree: {error}"))?
            .into_iter()
            .map(|leaf| format!("{root_name}/{}", leaf.path))
            .collect();
        Ok(BphclMutationResult {
            status_text: format!("Compacted BPHCL from {before} to {} bytes", bytes.len()),
            sarc_paths,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentState, HkclSelectableNode, PhysicsMergeFormat, PhysicsMergeRequest,
        RebuiltPhysicsDocument,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn documents_are_created_lazily_and_closed_independently() {
        let state = DocumentState::default();
        state.with_mut("doc-a", |app| app.opened_file.path.name = "a".into());
        state.with_mut("doc-b", |app| app.opened_file.path.name = "b".into());
        assert_eq!(
            state.with("doc-a", |app| app.opened_file.path.name.clone()),
            "a"
        );
        assert!(state.close("doc-a"));
        assert_eq!(
            state.with("doc-b", |app| app.opened_file.path.name.clone()),
            "b"
        );
    }

    #[test]
    fn hkcl_selectable_node_contract_is_camel_case() {
        let node = HkclSelectableNode {
            document_id: "document".into(),
            node_id: "cloth:0".into(),
            kind: "cloth".into(),
            index: 0,
            name: "Cloth".into(),
            section_index: 2,
            data_offset: 0x40,
        };
        let value = serde_json::to_value(node).unwrap();
        assert_eq!(value["documentId"], "document");
        assert_eq!(value["nodeId"], "cloth:0");
        assert_eq!(value["sectionIndex"], 2);
        assert_eq!(value["dataOffset"], 0x40);
    }

    #[test]
    fn physics_merge_request_deserializes_explicit_formats() {
        let request: PhysicsMergeRequest = serde_json::from_value(serde_json::json!({
            "targetDocumentId": "target",
            "sourceDocumentId": "source",
            "targetFormat": "hkcl",
            "sourceFormat": "bphcl",
            "nodeIds": ["cloth:2"],
            "templateClothIndex": 1,
            "helperDocumentId": "helper"
        }))
        .unwrap();
        assert_eq!(request.target_format, PhysicsMergeFormat::Hkcl);
        assert_eq!(request.source_format, PhysicsMergeFormat::Bphcl);
        assert_eq!(request.template_cloth_index, Some(1));
    }

    #[test]
    fn physics_merge_validation_reports_request_and_document_errors() {
        let state = DocumentState::default();
        let request = PhysicsMergeRequest {
            target_document_id: "target".into(),
            source_document_id: "source".into(),
            target_format: PhysicsMergeFormat::Bphcl,
            source_format: PhysicsMergeFormat::Hkcl,
            node_ids: vec!["collidable:0".into()],
            template_cloth_index: None,
            helper_document_id: None,
        };
        let validation = state.validate_physics_merge_request(&request);
        assert!(!validation.valid);
        assert!(validation.requires_template);
        assert!(!validation.supports_collidables);
        assert!(validation
            .issues
            .iter()
            .any(|issue| issue.contains("Target document")));
        assert!(validation
            .issues
            .iter()
            .any(|issue| issue.contains("template cloth")));
        assert!(validation
            .issues
            .iter()
            .any(|issue| issue.contains("standalone collidable")));
    }

    #[test]
    fn commits_reparsed_hkcl_and_bphcl_bytes_to_open_documents() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cases = [
            (
                PhysicsMergeFormat::Hkcl,
                manifest.join("../tmp/hkcl/Armor_001_Head_A.hkcl"),
            ),
            (
                PhysicsMergeFormat::Bphcl,
                manifest.join("../tmp/_bphcl/Animal_Donkey.bphcl"),
            ),
        ];
        for (format, path) in cases {
            let bytes = fs::read(&path).unwrap();
            let state = DocumentState::default();
            state.with_mut("document", |app| {
                app.opened_file.path = crate::Settings::Pathlib::new(&path);
                match format {
                    PhysicsMergeFormat::Hkcl => {
                        app.opened_file.hkcl = Some(crate::file_format::hkcl::HkclFile {
                            source_path: Some(path.to_string_lossy().into_owned()),
                            document: crate::parser::hkcl::HkclDocument::parse(&bytes).unwrap(),
                        });
                    }
                    PhysicsMergeFormat::Bphcl => {
                        app.opened_file.bphcl = Some(crate::file_format::bphcl::BphclFile {
                            source_path: Some(path.to_string_lossy().into_owned()),
                            document: crate::parser::bphcl::BphclDocument::parse(&bytes).unwrap(),
                        });
                    }
                }
            });
            let result = state
                .commit_rebuilt_physics_document(RebuiltPhysicsDocument {
                    document_id: "document".into(),
                    format,
                    bytes: bytes.clone(),
                })
                .unwrap();
            assert_eq!(result.byte_count, bytes.len());
            assert!(!result.sarc_paths.paths.is_empty());
        }
    }

    #[test]
    fn rejects_invalid_rebuilt_bytes_before_document_lookup_or_mutation() {
        let state = DocumentState::default();
        let error = state
            .commit_rebuilt_physics_document(RebuiltPhysicsDocument {
                document_id: "missing".into(),
                format: PhysicsMergeFormat::Hkcl,
                bytes: b"not hkcl".to_vec(),
            })
            .unwrap_err();
        assert!(error.contains("did not parse"));
    }
}

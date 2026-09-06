//! Physics editing ported from PhysicsTool beyond cloth merging: BPHCL DATA
//! compaction, BotW ragdolls (HKRG), BotW support-bone sidecars (BPHYSSB),
//! and driver-group merging for both helper-bone sidecar formats.

pub mod aamp_tree;
pub mod bphcl;
pub mod bphcl_to_hkcl;
pub mod bphhb;
pub mod bphhb_mapping;
pub mod bphyssb;
pub mod compactor;
pub mod hkcl;
pub mod hkcl_merge;
pub mod hkcl_to_bphcl;
pub mod hkrg;
#[cfg(test)]
mod physics_corpus_tests;
pub mod physics_graph;
pub mod sidecar;
#[cfg(test)]
mod sidecar_tests;

pub use aamp_tree::{AampList, AampObject, AampParameter, AampTree};
pub use bphhb::{HelperBoneDocument, HelperBoneGraph};
pub use bphyssb::{SupportBoneDocument, SupportBoneGraph, SupportBoneSummary};
pub use compactor::{compact, compact_preserving_item_indices};
pub use hkrg::HkrgDocument;
pub use sidecar::SidecarDriverGroup;

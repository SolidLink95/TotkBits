use super::{
    AampRegistrationMerger, BphclBuilder, BphclDocument, Collidable, Patch, ReferenceArray,
};
use std::{
    collections::{HashMap, HashSet},
    io::{self, ErrorKind},
};

impl BphclDocument {
    /// Removes a cloth and its paired skeleton from the container arrays.
    pub fn remove_cloth(&self, cloth_index: usize) -> io::Result<Vec<u8>> {
        if self.cloth.len() <= 1 {
            return Err(invalid("a BPHCL must retain at least one cloth"));
        }
        if self.collidables.is_empty() {
            return Err(invalid("a BPHCL must retain at least one collidable"));
        }
        let cloth = self
            .cloth
            .get(cloth_index)
            .ok_or_else(|| invalid("cloth index is out of range"))?;
        let skeleton = self
            .paired_skeleton(cloth_index)
            .ok_or_else(|| invalid("cloth has no paired skeleton"))?;
        let cloth_array = cloth_container_array(self, 40)?;
        let skeleton_array = container_array(self, "hkaAnimationContainer", 24)?;
        let mut builder = BphclBuilder::new(self)?;
        builder.replace_reference_array(
            &cloth_array,
            self.reference_item_indices(cloth_array.field_offset)?
                .into_iter()
                .filter(|item| *item != cloth.item_index),
        )?;
        builder.replace_reference_array(
            &skeleton_array,
            self.reference_item_indices(skeleton_array.field_offset)?
                .into_iter()
                .filter(|item| *item != skeleton.item_index),
        )?;
        builder.replace_aamp(AampRegistrationMerger::remove_cloth(self, &cloth.name)?);
        let removed = validate_rebuild(builder.build()?)?;
        // Colliders only the removed cloth used leave with it, along with
        // their AAMP registrations. DATA allocations stay where they are.
        let pruned = BphclDocument::parse(&removed)?.prune_unreferenced_collidables()?;
        let result = BphclDocument::parse(&pruned)?;
        result.validate_item_graph()?;
        if result.cloth.len() != self.cloth.len() - 1 {
            return Err(invalid(&format!(
                "cloth removal produced {} cloths; expected {}",
                result.cloth.len(),
                self.cloth.len() - 1
            )));
        }
        let expected_skeletons = self.skeletons.len().saturating_sub(1);
        if result.skeletons.len() != expected_skeletons {
            return Err(invalid(&format!(
                "cloth removal produced {} skeletons; expected {expected_skeletons}",
                result.skeletons.len()
            )));
        }
        if result.aamp.is_some()
            && AampRegistrationMerger::cloth_entry_names(&result)?
                .iter()
                .any(|entry| entry == &cloth.name)
        {
            return Err(invalid(&format!(
                "cloth removal left '{}' registered in AAMP metadata",
                cloth.name
            )));
        }
        let referenced = result.referenced_collidable_items();
        if result
            .collidables
            .iter()
            .any(|collider| !referenced.contains(&collider.item_index))
        {
            return Err(invalid(
                "cloth removal left unreferenced colliders in the collider array",
            ));
        }
        Ok(pruned)
    }

    /// ITEM indices of every collider some simulation still references.
    pub fn referenced_collidable_items(&self) -> HashSet<usize> {
        self.cloth
            .iter()
            .flat_map(|cloth| &cloth.simulations)
            .flat_map(|simulation| &simulation.collidable_item_indices)
            .copied()
            .collect()
    }

    /// Drops colliders no simulation references from the container array and
    /// from the AAMP collidable list, leaving DATA allocations untouched.
    pub fn prune_unreferenced_collidables(&self) -> io::Result<Vec<u8>> {
        let referenced = self.referenced_collidable_items();
        let retained: Vec<&Collidable> = self
            .collidables
            .iter()
            .filter(|collider| referenced.contains(&collider.item_index))
            .collect();
        if retained.len() == self.collidables.len() {
            return Ok(self.raw.clone());
        }
        let array = cloth_container_array(self, 24)?;
        let mut builder = BphclBuilder::new(self)?;
        builder
            .replace_reference_array(&array, retained.iter().map(|collider| collider.item_index))?;
        builder.replace_aamp(AampRegistrationMerger::keep_collidables(
            self,
            retained.iter().map(|collider| collider.name.as_str()),
        )?);
        validate_rebuild(builder.build()?)
    }

    /// Removes a standalone collidable from the cloth-container root array.
    pub fn remove_collidable(&self, collidable_index: usize) -> io::Result<Vec<u8>> {
        if self.collidables.len() <= 1 {
            return Err(invalid("a BPHCL must retain at least one collidable"));
        }
        if self.cloth.is_empty() {
            return Err(invalid("a BPHCL must retain at least one cloth"));
        }
        let collidable = self
            .collidables
            .get(collidable_index)
            .ok_or_else(|| invalid("collidable index is out of range"))?;
        let array = cloth_container_array(self, 24)?;
        let mut builder = BphclBuilder::new(self)?;
        builder.replace_reference_array(
            &array,
            self.reference_item_indices(array.field_offset)?
                .into_iter()
                .filter(|item| *item != collidable.item_index),
        )?;
        builder.replace_aamp(AampRegistrationMerger::remove_collidable(
            self,
            &collidable.name,
        )?);
        validate_rebuild(builder.build()?)
    }

    /// Imports one collidable and its complete reachable graph without a cloth.
    pub fn merge_collidable(
        &self,
        source: &BphclDocument,
        collidable_index: usize,
    ) -> io::Result<Vec<u8>> {
        let collider = source
            .collidables
            .get(collidable_index)
            .ok_or_else(|| invalid("source collidable index is out of range"))?;
        match collidable_policy(&self.collidables, collider) {
            CollidablePolicy::Skip => return Ok(self.raw.clone()),
            CollidablePolicy::Import => {}
        }
        let closure = source.collect_item_closure([collider.item_index])?;
        let source_patches = source.patches_for_items(closure.iter().copied())?;
        let mut required_types: Vec<_> = closure
            .iter()
            .map(|index| source.items[*index].type_index)
            .chain(source_patches.iter().map(|patch| patch.type_index))
            .collect();
        if let Ok(array) = cloth_container_array(source, 24) {
            required_types.push(array.storage_item.type_index);
            required_types.push(array.entry_patch_type_index);
        }
        let type_merge = self
            .type_table
            .merge_required(&source.type_table, required_types)?;
        let aamp = AampRegistrationMerger::append_collidables(
            AampRegistrationMerger::original(self)?,
            source,
            [collider.name.as_str()],
        )?;

        let mut builder = BphclBuilder::new(self)?;
        let copied = source.copy_item_closure(closure.iter().copied(), builder.data)?;
        builder.data = copied.data;
        let mut imported = HashMap::new();
        for source_index in &closure {
            let item = &source.items[*source_index];
            let range = copied
                .ranges_by_old_start
                .get(&item.data_offset)
                .ok_or_else(|| invalid("copied ITEM range is missing"))?;
            let type_index = mapped(&type_merge.source_to_merged, item.type_index)?;
            imported.insert(*source_index, builder.items.len());
            builder.items.push(super::Item {
                flags: (item.flags & 0xff00_0000) | type_index,
                type_index,
                data_offset: range.new_start + item.data_offset - range.old_start,
                count: item.count,
            });
        }
        source.relocate_copied_pointers(
            &mut builder.data,
            &source_patches,
            &copied.ranges_by_old_start,
            &imported,
            &HashMap::new(),
        )?;
        append_imported_patches(
            &mut builder.patches,
            &source_patches,
            &copied.ranges_by_old_start,
            &type_merge.source_to_merged,
        )?;

        let root = self
            .variants
            .iter()
            .find(|variant| variant.class_name == "hclClothContainer")
            .ok_or_else(|| invalid("target has no hclClothContainer"))?;
        let base = self.items[root.item_index].data_offset;
        let source_root = source
            .variants
            .iter()
            .find(|variant| variant.class_name == "hclClothContainer")
            .ok_or_else(|| invalid("source has no hclClothContainer"))?;
        let source_base = source.items[source_root.item_index].data_offset;
        let array = reference_array_for_append(
            self,
            base + 24,
            source,
            source_base + 24,
            &type_merge.source_to_merged,
        )?;
        builder.append_reference_array(&array, [imported[&collider.item_index]])?;
        if let Some(section) = type_merge.replacement_section {
            builder.replace_type(section);
        }
        builder.replace_aamp(aamp);
        let bytes = builder.build()?;
        let rebuilt = BphclDocument::parse(&bytes)?;
        rebuilt.validate_item_graph()?;
        Ok(bytes)
    }

    /// Imports a cloth, its paired skeleton, and its complete reachable ITEM graph.
    /// An existing cloth name is a no-op so callers can safely batch donors.
    pub fn merge_complete_cloth(
        &self,
        source: &BphclDocument,
        cloth_index: usize,
    ) -> io::Result<Vec<u8>> {
        let cloth = source
            .cloth
            .get(cloth_index)
            .ok_or_else(|| invalid("source cloth index is out of range"))?;
        if has_cloth_name(&self.cloth, &cloth.name) {
            return Ok(self.raw.clone());
        }
        let skeleton = source
            .paired_skeleton(cloth_index)
            .ok_or_else(|| invalid("source cloth has no paired skeleton"))?;
        let closure = source.collect_item_closure([cloth.item_index, skeleton.item_index])?;
        let source_patches = source.patches_for_items(closure.iter().copied())?;
        let mut required_types: Vec<_> = closure
            .iter()
            .map(|index| source.items[*index].type_index)
            .chain(source_patches.iter().map(|patch| patch.type_index))
            .collect();
        if let Ok(array) = cloth_container_array(source, 24) {
            required_types.push(array.storage_item.type_index);
            required_types.push(array.entry_patch_type_index);
        }
        for array in [
            container_array(source, "hclClothContainer", 40),
            container_array(source, "hkaAnimationContainer", 24),
        ]
        .into_iter()
        .flatten()
        {
            required_types.push(array.storage_item.type_index);
            required_types.push(array.entry_patch_type_index);
        }
        let type_merge = self
            .type_table
            .merge_required(&source.type_table, required_types)?;

        let live: HashSet<_> = closure.iter().copied().collect();
        let reused_colliders = reusable_colliders(self, source, &live);
        let mut aamp = AampRegistrationMerger::append_cloth(self, source, &cloth.name)?;
        let collider_names: Vec<_> = source
            .collidables
            .iter()
            .filter(|collider| {
                live.contains(&collider.item_index)
                    && !reused_colliders.contains_key(&collider.item_index)
            })
            .map(|collider| collider.name.as_str())
            .collect();
        aamp = AampRegistrationMerger::append_collidables(aamp, source, collider_names)?;

        let mut builder = BphclBuilder::new(self)?;
        let copied = source.copy_item_closure(closure.iter().copied(), builder.data)?;
        builder.data = copied.data;
        let mut imported = HashMap::new();
        for source_index in &closure {
            let item = &source.items[*source_index];
            let range = copied
                .ranges_by_old_start
                .get(&item.data_offset)
                .ok_or_else(|| invalid("copied ITEM range is missing"))?;
            let type_index = mapped(&type_merge.source_to_merged, item.type_index)?;
            let target_index = builder.items.len();
            imported.insert(*source_index, target_index);
            builder.items.push(super::Item {
                flags: (item.flags & 0xff00_0000) | type_index,
                type_index,
                data_offset: range.new_start + item.data_offset - range.old_start,
                count: item.count,
            });
        }
        source.relocate_copied_pointers(
            &mut builder.data,
            &source_patches,
            &copied.ranges_by_old_start,
            &imported,
            &reused_colliders,
        )?;
        for patch in &source_patches {
            let type_index = mapped(&type_merge.source_to_merged, patch.type_index)?;
            let group = builder
                .patches
                .iter_mut()
                .find(|group| group.type_index == type_index);
            let offsets: Vec<u32> = patch
                .offsets
                .iter()
                .map(|offset| {
                    copied
                        .ranges_by_old_start
                        .values()
                        .find_map(|range| range.relocate(*offset))
                        .ok_or_else(|| invalid("patch lies outside copied graph"))
                })
                .collect::<io::Result<_>>()?;
            if let Some(group) = group {
                group.offsets.extend(offsets);
            } else {
                builder.patches.push(Patch {
                    type_index,
                    offsets,
                });
            }
        }

        let cloth_root = self
            .variants
            .iter()
            .find(|variant| variant.class_name == "hclClothContainer")
            .ok_or_else(|| invalid("target has no hclClothContainer"))?;
        let animation_root = self
            .variants
            .iter()
            .find(|variant| variant.class_name == "hkaAnimationContainer")
            .ok_or_else(|| invalid("target has no hkaAnimationContainer"))?;
        let source_cloth_root = source
            .variants
            .iter()
            .find(|variant| variant.class_name == "hclClothContainer")
            .ok_or_else(|| invalid("source has no hclClothContainer"))?;
        let source_animation_root = source
            .variants
            .iter()
            .find(|variant| variant.class_name == "hkaAnimationContainer")
            .ok_or_else(|| invalid("source has no hkaAnimationContainer"))?;
        let cloth_base = self.items[cloth_root.item_index].data_offset;
        let animation_base = self.items[animation_root.item_index].data_offset;
        let new_colliders: Vec<_> = source
            .collidables
            .iter()
            .filter(|collider| {
                live.contains(&collider.item_index)
                    && !reused_colliders.contains_key(&collider.item_index)
            })
            .map(|collider| imported[&collider.item_index])
            .collect();
        if !new_colliders.is_empty() {
            let source_cloth_base = source.items[source_cloth_root.item_index].data_offset;
            let array = reference_array_for_append(
                self,
                cloth_base + 24,
                source,
                source_cloth_base + 24,
                &type_merge.source_to_merged,
            )?;
            builder.append_reference_array(&array, new_colliders)?;
        }
        let source_cloth_base = source.items[source_cloth_root.item_index].data_offset;
        let source_animation_base = source.items[source_animation_root.item_index].data_offset;
        let cloth_array = reference_array_for_append(
            self,
            cloth_base + 40,
            source,
            source_cloth_base + 40,
            &type_merge.source_to_merged,
        )?;
        builder.append_reference_array(&cloth_array, [imported[&cloth.item_index]])?;
        let skeleton_array = reference_array_for_append(
            self,
            animation_base + 24,
            source,
            source_animation_base + 24,
            &type_merge.source_to_merged,
        )?;
        builder.append_reference_array(&skeleton_array, [imported[&skeleton.item_index]])?;
        if let Some(section) = type_merge.replacement_section {
            builder.replace_type(section);
        }
        builder.replace_aamp(aamp);
        let bytes = builder.build()?;
        let rebuilt = BphclDocument::parse(&bytes)?;
        rebuilt.validate_item_graph()?;
        Ok(bytes)
    }
}

fn reference_array_for_append(
    target: &BphclDocument,
    target_field: u32,
    source: &BphclDocument,
    source_field: u32,
    source_types: &HashMap<u32, u32>,
) -> io::Result<ReferenceArray> {
    if let Ok(array) = target.reference_array_metadata(target_field) {
        return Ok(array);
    }
    let mut array = source.reference_array_metadata(source_field)?;
    let type_index = mapped(source_types, array.storage_item.type_index)?;
    array.field_offset = target_field;
    array.storage_item_index = usize::MAX;
    array.storage_item.flags = (array.storage_item.flags & 0xff00_0000) | type_index;
    array.storage_item.type_index = type_index;
    array.entry_patch_type_index = mapped(source_types, array.entry_patch_type_index)?;
    Ok(array)
}

fn cloth_container_array(
    document: &BphclDocument,
    relative_field: u32,
) -> io::Result<ReferenceArray> {
    container_array(document, "hclClothContainer", relative_field)
}

fn container_array(
    document: &BphclDocument,
    class_name: &str,
    relative_field: u32,
) -> io::Result<ReferenceArray> {
    let root = document
        .variants
        .iter()
        .find(|variant| variant.class_name == class_name)
        .ok_or_else(|| invalid(&format!("document has no {class_name}")))?;
    let base = document.items[root.item_index].data_offset;
    document.reference_array_metadata(base + relative_field)
}

fn append_imported_patches(
    target: &mut Vec<Patch>,
    source: &[Patch],
    ranges: &HashMap<u32, super::ImportedRange>,
    type_map: &HashMap<u32, u32>,
) -> io::Result<()> {
    for patch in source {
        let type_index = mapped(type_map, patch.type_index)?;
        let offsets = patch
            .offsets
            .iter()
            .map(|offset| {
                ranges
                    .values()
                    .find_map(|range| range.relocate(*offset))
                    .ok_or_else(|| invalid("patch lies outside copied graph"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if let Some(group) = target
            .iter_mut()
            .find(|group| group.type_index == type_index)
        {
            group.offsets.extend(offsets);
        } else {
            target.push(Patch {
                type_index,
                offsets,
            });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollidablePolicy {
    Import,
    Skip,
}
fn collidable_policy(target: &[Collidable], source: &Collidable) -> CollidablePolicy {
    if target.iter().any(|item| item.name == source.name) {
        CollidablePolicy::Skip
    } else {
        CollidablePolicy::Import
    }
}

fn reusable_colliders(
    target: &BphclDocument,
    source: &BphclDocument,
    live: &HashSet<usize>,
) -> HashMap<usize, usize> {
    let mut result = HashMap::new();
    let mut claimed = HashSet::new();
    for source_collider in source
        .collidables
        .iter()
        .filter(|collider| live.contains(&collider.item_index))
    {
        let matches: Vec<_> = target
            .collidables
            .iter()
            .filter(|target_collider| {
                !claimed.contains(&target_collider.item_index)
                    && colliders_match(source_collider, target_collider)
            })
            .collect();
        if matches.len() == 1 {
            result.insert(source_collider.item_index, matches[0].item_index);
            claimed.insert(matches[0].item_index);
        }
    }
    result
}
/// A target collider can stand in for a donor's only when everything the
/// simulation reads from it agrees: identity, shape class, pinch detection,
/// enabled state and placement.
fn colliders_match(source: &Collidable, target: &Collidable) -> bool {
    source.name == target.name
        && source.shape_class_name == target.shape_class_name
        && source.shape_kind == target.shape_kind
        && source.pinch_detection_enabled == target.pinch_detection_enabled
        && source.pinch_detection_priority == target.pinch_detection_priority
        && source.enabled == target.enabled
        && approximately_equal(source.pinch_detection_radius, target.pinch_detection_radius)
        && approximately_equal(source.translation.x, target.translation.x)
        && approximately_equal(source.translation.y, target.translation.y)
        && approximately_equal(source.translation.z, target.translation.z)
}

fn approximately_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.00001
}

fn validate_rebuild(bytes: Vec<u8>) -> io::Result<Vec<u8>> {
    let rebuilt = BphclDocument::parse(&bytes)?;
    rebuilt.validate_item_graph()?;
    Ok(bytes)
}
fn has_cloth_name(cloths: &[super::Cloth], name: &str) -> bool {
    cloths.iter().any(|cloth| cloth.name == name)
}
fn mapped(map: &HashMap<u32, u32>, index: u32) -> io::Result<u32> {
    map.get(&index)
        .copied()
        .ok_or_else(|| invalid(&format!("TYPE {index} is unmapped")))
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::super::CollidableShape;
    use super::*;

    #[test]
    fn duplicate_cloth_names_are_detected_exactly() {
        let cloths = vec![super::super::Cloth {
            index: 0,
            name: "Cape".into(),
            item_index: 4,
            simulations: vec![],
        }];
        assert!(has_cloth_name(&cloths, "Cape"));
        assert!(!has_cloth_name(&cloths, "cape"));
        assert!(!has_cloth_name(&cloths, "Cape_2"));
    }

    fn collider(name: &str, x: f32) -> Collidable {
        Collidable {
            index: 0,
            name: name.into(),
            item_index: 1,
            class_name: "hknp".into(),
            translation: super::super::Vector4 {
                x,
                ..Default::default()
            },
            axis_x: Default::default(),
            axis_y: Default::default(),
            axis_z: Default::default(),
            enabled: true,
            shape: CollidableShape::Sphere {
                center: Default::default(),
                radius: 1.0,
            },
            shape_class_name: "hclSphereShape".into(),
            shape_kind: 1,
            pinch_detection_radius: 0.0,
            pinch_detection_priority: 0,
            pinch_detection_enabled: false,
            virtual_collision_point_collision_enabled: false,
        }
    }

    #[test]
    fn collider_reuse_requires_matching_shape_pinch_and_placement() {
        let target = collider("Body", 1.0);
        assert!(colliders_match(&collider("Body", 1.0), &target));
        assert!(colliders_match(&collider("Body", 1.000001), &target));
        assert!(!colliders_match(&collider("Body", 1.5), &target));
        assert!(!colliders_match(&collider("body", 1.0), &target));
        let mut pinching = collider("Body", 1.0);
        pinching.pinch_detection_enabled = true;
        assert!(!colliders_match(&pinching, &target));
        let mut wider = collider("Body", 1.0);
        wider.pinch_detection_radius = 0.01;
        assert!(!colliders_match(&wider, &target));
        let mut capsule = collider("Body", 1.0);
        capsule.shape_class_name = "hclCapsuleShape".into();
        assert!(!colliders_match(&capsule, &target));
        let mut disabled = collider("Body", 1.0);
        disabled.enabled = false;
        assert!(!colliders_match(&disabled, &target));
    }

    #[test]
    fn standalone_collidable_policy_skips_only_exact_case_sensitive_names() {
        let target = vec![collider("Body", 1.0)];
        assert_eq!(
            collidable_policy(&target, &collider("Body", 1.0)),
            CollidablePolicy::Skip
        );
        assert_eq!(
            collidable_policy(&target, &collider("Body", 2.0)),
            CollidablePolicy::Skip
        );
        assert_eq!(
            collidable_policy(&target, &collider("Other", 1.0)),
            CollidablePolicy::Import
        );
        assert_eq!(
            collidable_policy(&target, &collider("body", 1.0)),
            CollidablePolicy::Import
        );
        assert_eq!(
            collidable_policy(&target, &collider("Link:Body", 1.0)),
            CollidablePolicy::Import
        );
    }
}

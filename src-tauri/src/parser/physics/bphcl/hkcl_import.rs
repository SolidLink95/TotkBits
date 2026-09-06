use super::{write_constraint_elements, AampRegistrationMerger, BphclBuilder, BphclDocument, Item};
use crate::parser::{
    binary::{BinaryPatcher, BinaryReader, BinaryWriter, Endian},
    physics::{
        hkcl_to_bphcl::convert_hkcl_cloth_to_bphcl_template,
        physics_graph::{
            FormatNeutralPhysicsGraph, PhysicsCloth, PhysicsCollidable, PhysicsSkeleton, SourceRef,
        },
    },
};
use std::{
    collections::BTreeSet,
    io::{self, ErrorKind},
};

impl BphclDocument {
    /// Materializes an HKCL cloth through an existing BPHCL template, then
    /// imports the resulting native ITEM graph as a complete writable cloth.
    pub fn import_hkcl_cloth(
        &self,
        source: &FormatNeutralPhysicsGraph,
        source_cloth_index: usize,
        template_cloth_index: usize,
    ) -> io::Result<Vec<u8>> {
        let target = self.neutral_physics_graph();
        let converted = convert_hkcl_cloth_to_bphcl_template(
            source,
            &target,
            source_cloth_index,
            template_cloth_index,
        )?;
        let template = target
            .cloths
            .get(template_cloth_index)
            .ok_or_else(|| invalid("BPHCL template cloth index is out of range"))?;
        let cloth = converted
            .cloths
            .get(template_cloth_index)
            .ok_or_else(|| invalid("converted BPHCL cloth is missing"))?;
        let cloth_name = required_name(cloth.name.as_deref(), "HKCL cloth")?;
        if self
            .cloth
            .iter()
            .any(|existing| existing.name == cloth_name)
        {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                format!("target already contains a cloth named '{cloth_name}'"),
            ));
        }

        let mut builder = BphclBuilder::new(self)?;
        materialize_cloth(&mut builder, self, &converted, cloth)?;
        let skeleton = paired_skeleton(&converted, cloth)?;
        materialize_skeleton(&mut builder, self, skeleton)?;

        let referenced_collidables: BTreeSet<_> = cloth
            .simulations
            .iter()
            .flat_map(|simulation| simulation.collidables.iter())
            .collect();
        let mut collidable_renames = Vec::new();
        for collidable in converted
            .collidables
            .iter()
            .filter(|collidable| referenced_collidables.contains(&collidable.id))
        {
            if let Some(rename) = materialize_collidable(&mut builder, self, collidable)? {
                collidable_renames.push(rename);
            }
        }

        let template_name = required_name(template.name.as_deref(), "BPHCL template cloth")?;
        builder.replace_aamp(AampRegistrationMerger::rename_template_entries(
            self,
            (template_name, cloth_name),
            collidable_renames,
        )?);
        let donor_bytes = builder.build()?;
        let donor = BphclDocument::parse(&donor_bytes)?;
        donor.validate_item_graph()?;
        if donor
            .cloth
            .get(template_cloth_index)
            .is_none_or(|value| value.name != cloth_name)
        {
            return Err(invalid("materialized HKCL cloth name did not reparse"));
        }

        let merged = self.merge_complete_cloth(&donor, template_cloth_index)?;
        let rebuilt = BphclDocument::parse(&merged)?;
        rebuilt.validate_item_graph()?;
        if !rebuilt.cloth.iter().any(|value| value.name == cloth_name) {
            return Err(invalid(
                "HKCL cloth was not present after native BPHCL merge",
            ));
        }
        Ok(merged)
    }
}

fn materialize_cloth(
    builder: &mut BphclBuilder<'_>,
    document: &BphclDocument,
    graph: &FormatNeutralPhysicsGraph,
    cloth: &PhysicsCloth,
) -> io::Result<()> {
    let cloth_item = source_item(&cloth.source)?;
    let cloth_offset = item(document, cloth_item)?.data_offset;
    if let Some(name) = cloth.name.as_deref() {
        repoint_string(builder, document, checked_add(cloth_offset, 24)?, name)?;
    }
    for simulation in &cloth.simulations {
        let simulation_item = source_item(&simulation.source)?;
        let simulation_offset = item(document, simulation_item)?.data_offset;
        if let Some(name) = simulation.name.as_deref() {
            repoint_string(builder, document, checked_add(simulation_offset, 24)?, name)?;
        }
        write_particles(builder, document, simulation_offset, &simulation.particles)?;
        for constraint_id in &simulation.constraints {
            let constraint = graph
                .constraints
                .iter()
                .find(|constraint| &constraint.id == constraint_id)
                .ok_or_else(|| invalid("converted constraint is missing"))?;
            let constraint_item = source_item(&constraint.source)?;
            if let Some(name) = constraint.name.as_deref() {
                repoint_string(
                    builder,
                    document,
                    checked_add(item(document, constraint_item)?.data_offset, 24)?,
                    name,
                )?;
            }
            write_constraint_elements(builder, document, constraint_item, &constraint.elements)?;
        }
    }
    Ok(())
}

fn write_particles(
    builder: &mut BphclBuilder<'_>,
    document: &BphclDocument,
    simulation_offset: u32,
    particles: &[crate::parser::physics::physics_graph::PhysicsParticle],
) -> io::Result<()> {
    let physics = array_item(document, checked_add(simulation_offset, 64)?)?;
    if physics.count as usize != particles.len() {
        return Err(invalid("HKCL particle count differs from BPHCL template"));
    }
    let poses = reference_item_indices(document, checked_add(simulation_offset, 120)?)?;
    let pose = poses
        .first()
        .copied()
        .ok_or_else(|| invalid("BPHCL template has no simulation pose"))?;
    let positions = array_item(
        document,
        checked_add(item(document, pose)?.data_offset, 32)?,
    )?;
    if positions.count as usize != particles.len() {
        return Err(invalid(
            "BPHCL simulation pose count differs from HKCL particles",
        ));
    }
    for (index, particle) in particles.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| invalid("particle index exceeds u32"))?;
        let physics_offset = checked_add(
            physics.data_offset,
            index
                .checked_mul(16)
                .ok_or_else(|| invalid("particle offset overflow"))?,
        )?;
        write_f32(&mut builder.data, physics_offset, particle.mass)?;
        write_f32(
            &mut builder.data,
            checked_add(physics_offset, 4)?,
            particle.inverse_mass,
        )?;
        write_f32(
            &mut builder.data,
            checked_add(physics_offset, 8)?,
            particle.radius,
        )?;
        write_f32(
            &mut builder.data,
            checked_add(physics_offset, 12)?,
            particle.friction,
        )?;
        let position = particle
            .position
            .ok_or_else(|| invalid("HKCL particle has no position"))?;
        write_vec4(
            &mut builder.data,
            checked_add(
                positions.data_offset,
                index
                    .checked_mul(16)
                    .ok_or_else(|| invalid("pose offset overflow"))?,
            )?,
            position,
        )?;
    }
    let fixed = particles
        .iter()
        .enumerate()
        .filter(|(_, particle)| particle.fixed)
        .map(|(index, _)| {
            u16::try_from(index).map_err(|_| invalid("fixed particle index exceeds u16"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    replace_u16_array(
        builder,
        document,
        checked_add(simulation_offset, 80)?,
        &fixed,
    )
}

fn materialize_skeleton(
    builder: &mut BphclBuilder<'_>,
    document: &BphclDocument,
    skeleton: &PhysicsSkeleton,
) -> io::Result<()> {
    let skeleton_item = source_item(&skeleton.source)?;
    let skeleton_offset = item(document, skeleton_item)?.data_offset;
    if let Some(name) = skeleton.name.as_deref() {
        repoint_string(builder, document, checked_add(skeleton_offset, 24)?, name)?;
    }
    let parents = array_item(document, checked_add(skeleton_offset, 32)?)?;
    let bones = array_item(document, checked_add(skeleton_offset, 48)?)?;
    let poses = array_item(document, checked_add(skeleton_offset, 64)?)?;
    if [parents.count, bones.count, poses.count]
        .into_iter()
        .any(|count| count as usize != skeleton.bones.len())
    {
        return Err(invalid("HKCL skeleton layout differs from BPHCL template"));
    }
    for (index, bone) in skeleton.bones.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| invalid("bone index exceeds u32"))?;
        let parent_offset = checked_add(
            parents.data_offset,
            index
                .checked_mul(2)
                .ok_or_else(|| invalid("parent offset overflow"))?,
        )?;
        write_u16(
            &mut builder.data,
            parent_offset,
            bone.parent_index
                .map(|parent| {
                    u16::try_from(parent).map_err(|_| invalid("bone parent index exceeds u16"))
                })
                .transpose()?
                .unwrap_or(u16::MAX),
        )?;
        let bone_offset = checked_add(
            bones.data_offset,
            index
                .checked_mul(16)
                .ok_or_else(|| invalid("bone entry offset overflow"))?,
        )?;
        if let Some(name) = bone.name.as_deref() {
            repoint_string(builder, document, bone_offset, name)?;
        }
        write_bytes(
            &mut builder.data,
            checked_add(bone_offset, 8)?,
            &[u8::from(bone.lock_translation)],
        )?;
        if let Some(transform) = bone.transform {
            let pose_offset = checked_add(
                poses.data_offset,
                index
                    .checked_mul(48)
                    .ok_or_else(|| invalid("bone pose offset overflow"))?,
            )?;
            write_vec4(&mut builder.data, pose_offset, transform.translation)?;
            write_vec4(
                &mut builder.data,
                checked_add(pose_offset, 16)?,
                transform.rotation,
            )?;
            write_vec4(
                &mut builder.data,
                checked_add(pose_offset, 32)?,
                transform.scale,
            )?;
        }
    }
    Ok(())
}

fn materialize_collidable(
    builder: &mut BphclBuilder<'_>,
    document: &BphclDocument,
    collidable: &PhysicsCollidable,
) -> io::Result<Option<(String, String)>> {
    let collidable_item = source_item(&collidable.source)?;
    let offset = item(document, collidable_item)?.data_offset;
    let old_name = document
        .collidables
        .iter()
        .find(|value| value.item_index == collidable_item)
        .map(|value| value.name.clone())
        .ok_or_else(|| invalid("BPHCL template collidable is missing"))?;
    let new_name = collidable.name.clone().unwrap_or_else(|| old_name.clone());
    let renamed = new_name != old_name;
    if renamed {
        repoint_string(builder, document, checked_add(offset, 144)?, &new_name)?;
    }
    if let Some(transform) = collidable.transform {
        write_vec4(
            &mut builder.data,
            checked_add(offset, 32)?,
            [transform[0], transform[1], transform[2], transform[3]],
        )?;
        write_vec4(
            &mut builder.data,
            checked_add(offset, 48)?,
            [transform[4], transform[5], transform[6], transform[7]],
        )?;
        write_vec4(
            &mut builder.data,
            checked_add(offset, 64)?,
            [transform[8], transform[9], transform[10], transform[11]],
        )?;
        write_vec4(
            &mut builder.data,
            checked_add(offset, 80)?,
            [transform[12], transform[13], transform[14], transform[15]],
        )?;
    }
    write_bytes(
        &mut builder.data,
        checked_add(offset, 159)?,
        &[u8::from(collidable.enabled)],
    )?;
    // Pinch settings sit just before the enabled flag: radius at +152, then
    // priority and the enabled byte. A disabled source only clears the flag
    // and leaves the template's radius and priority in place.
    match collidable.pinch_detection {
        Some((priority, radius)) => {
            write_f32(&mut builder.data, checked_add(offset, 152)?, radius)?;
            write_bytes(
                &mut builder.data,
                checked_add(offset, 156)?,
                &[priority as u8, 1],
            )?;
        }
        None => write_bytes(&mut builder.data, checked_add(offset, 157)?, &[0])?,
    }
    Ok(renamed.then_some((old_name, new_name)))
}

fn paired_skeleton<'a>(
    graph: &'a FormatNeutralPhysicsGraph,
    cloth: &PhysicsCloth,
) -> io::Result<&'a PhysicsSkeleton> {
    let binding = graph
        .skeleton_bindings
        .iter()
        .find(|binding| binding.cloth == cloth.id)
        .ok_or_else(|| invalid("converted BPHCL cloth has no skeleton binding"))?;
    graph
        .skeletons
        .iter()
        .find(|skeleton| skeleton.id == binding.skeleton)
        .ok_or_else(|| invalid("converted BPHCL skeleton is missing"))
}

fn source_item(source: &SourceRef) -> io::Result<usize> {
    match source {
        SourceRef::Bphcl { item } => Ok(*item),
        SourceRef::Hkcl { .. } => Err(invalid("materialized node lacks a BPHCL template ITEM")),
    }
}

fn item(document: &BphclDocument, index: usize) -> io::Result<&Item> {
    document
        .items
        .get(index)
        .ok_or_else(|| invalid("BPHCL ITEM is missing"))
}

fn referenced(document: &BphclDocument, field: u32) -> io::Result<usize> {
    if !document
        .patches
        .iter()
        .any(|patch| patch.offsets.contains(&field))
    {
        return Err(invalid("BPHCL pointer has no relocation patch"));
    }
    let index = read_u32(data(document)?, field)? as usize;
    if index >= document.items.len() {
        return Err(invalid("BPHCL pointer references an invalid ITEM"));
    }
    Ok(index)
}

fn array_item(document: &BphclDocument, field: u32) -> io::Result<&Item> {
    item(document, referenced(document, field)?)
}

fn reference_item_indices(document: &BphclDocument, field: u32) -> io::Result<Vec<usize>> {
    let storage = array_item(document, field)?;
    (0..storage.count)
        .map(|index| {
            referenced(
                document,
                checked_add(
                    storage.data_offset,
                    index
                        .checked_mul(8)
                        .ok_or_else(|| invalid("reference array offset overflow"))?,
                )?,
            )
        })
        .collect()
}

fn repoint_string(
    builder: &mut BphclBuilder<'_>,
    document: &BphclDocument,
    field: u32,
    value: &str,
) -> io::Result<()> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid("BPHCL name is empty or contains NUL"));
    }
    let old = item(document, referenced(document, field)?)?.clone();
    align_data(&mut builder.data, 8);
    let data_offset =
        u32::try_from(builder.data.len()).map_err(|_| invalid("BPHCL DATA exceeds u32"))?;
    builder.data.extend_from_slice(value.as_bytes());
    builder.data.push(0);
    let item_index =
        u32::try_from(builder.items.len()).map_err(|_| invalid("BPHCL ITEM index exceeds u32"))?;
    let mut string = old;
    string.data_offset = data_offset;
    string.count = u32::try_from(value.len() + 1).map_err(|_| invalid("BPHCL name is too long"))?;
    builder.items.push(string);
    write_u32(&mut builder.data, field, item_index)
}

fn replace_u16_array(
    builder: &mut BphclBuilder<'_>,
    document: &BphclDocument,
    field: u32,
    values: &[u16],
) -> io::Result<()> {
    let mut storage = item(document, referenced(document, field)?)?.clone();
    align_data(&mut builder.data, 8);
    storage.data_offset =
        u32::try_from(builder.data.len()).map_err(|_| invalid("BPHCL DATA exceeds u32"))?;
    storage.count =
        u32::try_from(values.len()).map_err(|_| invalid("BPHCL array count exceeds u32"))?;
    let mut writer = BinaryWriter::appending(std::mem::take(&mut builder.data), Endian::Little);
    for value in values {
        writer.write_u16(*value);
    }
    builder.data = writer.into_inner();
    let item_index =
        u32::try_from(builder.items.len()).map_err(|_| invalid("BPHCL ITEM index exceeds u32"))?;
    builder.items.push(storage);
    write_u32(&mut builder.data, field, item_index)?;
    let count =
        u32::try_from(values.len()).map_err(|_| invalid("BPHCL array count exceeds u32"))?;
    write_u32(&mut builder.data, checked_add(field, 8)?, count)?;
    let capacity_offset = checked_add(field, 12)?;
    let capacity = read_u32(&builder.data, capacity_offset)?;
    write_u32(
        &mut builder.data,
        capacity_offset,
        (capacity & 0xc000_0000) | count,
    )
}

fn data(document: &BphclDocument) -> io::Result<&[u8]> {
    let section = document
        .tag
        .find("DATA")
        .ok_or_else(|| invalid("BPHCL has no DATA section"))?;
    Ok(&document.raw[section.payload_offset..section.payload_end()])
}

fn required_name<'a>(name: Option<&'a str>, description: &str) -> io::Result<&'a str> {
    name.filter(|name| !name.is_empty())
        .ok_or_else(|| invalid(&format!("{description} has no name")))
}

fn write_vec4(data: &mut [u8], offset: u32, value: [f32; 4]) -> io::Result<()> {
    for (index, value) in value.into_iter().enumerate() {
        write_f32(
            data,
            checked_add(
                offset,
                u32::try_from(index * 4).map_err(|_| invalid("vector offset exceeds u32"))?,
            )?,
            value,
        )?;
    }
    Ok(())
}

fn write_f32(data: &mut [u8], offset: u32, value: f32) -> io::Result<()> {
    if !value.is_finite() {
        return Err(invalid("physics value is not finite"));
    }
    BinaryPatcher::new(data)
        .write_f32_at(offset as usize, value)
        .map_err(write_error)
}

fn write_u16(data: &mut [u8], offset: u32, value: u16) -> io::Result<()> {
    BinaryPatcher::new(data)
        .write_u16_at(offset as usize, value)
        .map_err(write_error)
}

fn read_u32(data: &[u8], offset: u32) -> io::Result<u32> {
    BinaryReader::new(data)
        .read_u32_at(offset as usize)
        .map_err(|_| invalid("BPHCL read exceeds DATA"))
}

fn write_u32(data: &mut [u8], offset: u32, value: u32) -> io::Result<()> {
    BinaryPatcher::new(data)
        .write_u32_at(offset as usize, value)
        .map_err(write_error)
}

fn write_bytes(data: &mut [u8], offset: u32, bytes: &[u8]) -> io::Result<()> {
    BinaryPatcher::new(data)
        .write_bytes_at(offset as usize, bytes)
        .map_err(write_error)
}

fn write_error(_: io::Error) -> io::Error {
    invalid("BPHCL write exceeds DATA")
}

fn checked_add(value: u32, addition: u32) -> io::Result<u32> {
    value
        .checked_add(addition)
        .ok_or_else(|| invalid("DATA offset overflow"))
}

fn align_data(data: &mut Vec<u8>, alignment: usize) {
    while data.len() % alignment != 0 {
        data.push(0);
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

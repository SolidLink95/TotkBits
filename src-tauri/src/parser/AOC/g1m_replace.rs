use crate::parser::{
    binary::{BinaryPatcher, BinaryReader, Endian},
    fbx::import::{import_for_g1m, ImportedFbx, ImportedMesh},
    AOC::g1m::{bone_index, G1mFile},
};
use std::{
    collections::{HashMap, HashSet},
    io,
};

#[derive(Clone)]
struct Section<'a> {
    kind: u32,
    count: usize,
    bytes: &'a [u8],
}
#[derive(Clone)]
struct Attr {
    buffer: usize,
    offset: usize,
    kind: u8,
    dummy: u8,
    semantic: u8,
    layer: u8,
}
#[derive(Clone)]
struct AttrSet {
    buffers: Vec<usize>,
    attrs: Vec<Attr>,
}
#[derive(Clone)]
struct VertexBuffer {
    unknown1: u32,
    unknown2: Option<u32>,
    stride: usize,
    data: Vec<u8>,
}
#[derive(Clone)]
struct IndexBuffer {
    unknown: Option<u32>,
    bits: u32,
    data: Vec<u8>,
    count: usize,
}
#[derive(Clone)]
struct Palette {
    raw: Vec<[u32; 3]>,
    joints: Vec<u32>,
}
#[derive(Clone, Copy)]
struct SupplementalJoint {
    raw: [u32; 3],
    matrix: Option<[f32; 16]>,
}
#[derive(Clone)]
struct Submesh {
    values: [u32; 14],
}
struct Geometry<'a> {
    signature: [u8; 4],
    version: u32,
    platform: [u8; 4],
    reserved: u32,
    bounds: [f32; 6],
    sections: Vec<Section<'a>>,
    vertex_buffers: Vec<VertexBuffer>,
    attrs: Vec<AttrSet>,
    index_buffers: Vec<IndexBuffer>,
    palettes: Vec<Palette>,
    submeshes: Vec<Submesh>,
    visible_submeshes: HashSet<usize>,
    cloth_submeshes: HashSet<usize>,
}
struct Chunk<'a> {
    signature: [u8; 4],
    version: [u8; 4],
    bytes: &'a [u8],
}

pub fn replace_meshes_from_fbx(g1m: &[u8], fbx: &[u8], name: &str) -> io::Result<Vec<u8>> {
    replace_imported_meshes(g1m, import_for_g1m(fbx)?, name)
}

fn replace_imported_meshes(
    g1m: &[u8],
    mut imported: ImportedFbx,
    name: &str,
) -> io::Result<Vec<u8>> {
    let endian = g1m_endian(g1m)?;
    let chunks = parse_chunks(g1m, endian)?;
    let geometry_chunk = chunks
        .iter()
        .find(|c| matches!(&c.signature, b"G1MG" | b"GM1G"))
        .ok_or_else(|| invalid("G1M has no G1MG chunk"))?;
    let geometry = parse_geometry(geometry_chunk.bytes, endian)?;
    let matrix_chunk = chunks
        .iter()
        .find(|chunk| matches!(&chunk.signature, b"G1MM" | b"MM1G"))
        .ok_or_else(|| invalid("G1M has no G1MM chunk"))?;
    let matrix_count = matrix_chunk_count(matrix_chunk.bytes, endian)?;
    let original = G1mFile::parse_for_export(g1m, name)?;
    let (replacements, supplemental_joints) =
        validate_and_order(&original, &geometry, &mut imported, name, matrix_count)?;
    let new_geometry = build_geometry(&geometry, &replacements, &supplemental_joints, endian)?;
    let mut supplemental_matrices = vec![
        [0.0; 16];
        supplemental_joints
            .values()
            .filter(|joint| joint.matrix.is_some())
            .count()
    ];
    for joint in supplemental_joints.values() {
        if let Some(matrix) = joint.matrix {
            supplemental_matrices[joint.raw[0] as usize - matrix_count] = matrix;
        }
    }
    let new_matrix_count = matrix_count + supplemental_matrices.len();
    let new_matrices = rebuild_matrix_chunk(matrix_chunk.bytes, &supplemental_matrices, endian)?;
    let new_g1mf = rebuild_g1mf(
        chunks
            .iter()
            .find(|c| matches!(&c.signature, b"G1MF" | b"FM1G"))
            .map(|c| c.bytes),
        &geometry,
        &new_geometry,
        new_matrix_count,
        endian,
    )?;
    let mut body = Vec::new();
    for chunk in &chunks {
        if matches!(&chunk.signature, b"G1MG" | b"GM1G") {
            body.extend_from_slice(&new_geometry);
        } else if matches!(&chunk.signature, b"G1MM" | b"MM1G") {
            body.extend_from_slice(&new_matrices);
        } else if matches!(&chunk.signature, b"G1MF" | b"FM1G") {
            body.extend_from_slice(&new_g1mf);
        } else {
            body.extend_from_slice(chunk.bytes);
        }
    }
    let chunk_offset = read_u32(g1m, 12, endian)? as usize;
    let mut output = g1m[..chunk_offset].to_vec();
    output.extend_from_slice(&body);
    let output_len = output.len() as u32;
    put_u32(&mut output, 8, output_len, endian)?;
    // Reparse the result before it can reach disk.
    G1mFile::parse_for_export(&output, name)
        .map_err(|e| invalid(format!("rebuilt G1M failed validation: {e}")))?;
    Ok(output)
}

fn validate_and_order(
    original: &G1mFile,
    geometry: &Geometry<'_>,
    imported: &mut ImportedFbx,
    name: &str,
    matrix_count: usize,
) -> io::Result<(Vec<Option<ImportedMesh>>, HashMap<u32, SupplementalJoint>)> {
    if geometry.submeshes.len() != original.render.meshes.len() {
        return Err(invalid("G1M mesh metadata is inconsistent"));
    }
    if geometry.submeshes.is_empty() {
        return Err(invalid("G1M contains no meshes"));
    }
    for (bone, parent) in &imported.bones {
        let generated_end = parent
            .as_deref()
            .is_some_and(|parent| bone == &format!("{parent}_end"));
        if bone_index(bone).is_none() && !generated_end {
            return Err(invalid(format!(
                "FBX armature contains unknown bone {bone}"
            )));
        }
        if !generated_end {
            let global = bone_index(bone)
                .ok_or_else(|| invalid(format!("FBX armature contains unknown bone {bone}")))?;
            if original
                .global_to_local_bones
                .get(global)
                .is_none_or(|local| *local == u16::MAX)
            {
                return Err(invalid(format!(
                    "FBX armature bone {bone} is not present in the G1M skeleton"
                )));
            }
        }
    }
    let original_names: HashSet<_> = original
        .render
        .bones
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    if original_names.len() != original.render.bones.len() {
        return Err(invalid("G1M bone names are not unique"));
    }
    let imported_names: HashSet<_> = imported
        .bones
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    if imported_names.len() != imported.bones.len() {
        return Err(invalid("FBX bone names are not unique"));
    }
    let imported_by_name: HashMap<_, _> = imported
        .bones
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| original.render.bones.iter().any(|bone| bone.name == *name))
        .map(|(i, (n, p))| (n.as_str(), (i, p.as_deref())))
        .collect();
    for (index, bone) in original.render.bones.iter().enumerate() {
        let mut parent_index = bone.parent_index;
        let expected_parent = loop {
            if parent_index < 0 {
                break None;
            }
            let candidate = &original.render.bones[parent_index as usize];
            if imported_by_name.contains_key(candidate.name.as_str()) {
                break Some(candidate.name.as_str());
            }
            parent_index = candidate.parent_index;
        };
        let Some((_, parent)) = imported_by_name.get(bone.name.as_str()) else {
            continue;
        };
        if *parent != expected_parent {
            return Err(invalid(format!(
                "FBX bone {} has a different parent",
                bone.name
            )));
        }
        if index > u16::MAX as usize {
            return Err(invalid("armature exceeds G1M bone index range"));
        }
    }
    let mut ordered: Vec<Option<ImportedMesh>> =
        (0..geometry.submeshes.len()).map(|_| None).collect();
    for mesh in std::mem::take(&mut imported.meshes) {
        let index = mesh_index(&mesh.name).ok_or_else(|| {
            invalid(format!(
                "mesh name {} does not identify a G1M mesh",
                mesh.name
            ))
        })?;
        if index >= geometry.submeshes.len() {
            return Err(invalid(format!(
                "mesh name {} exceeds G1M mesh count {}",
                mesh.name,
                geometry.submeshes.len()
            )));
        }
        if ordered[index].is_some() {
            return Err(invalid(format!("mesh name {} is duplicated", mesh.name)));
        }
        let material_index = geometry.submeshes[index].values[6] as usize;
        let accepted = [
            format!("Material {material_index}"),
            format!("{name}_{material_index}"),
        ];
        // The FBX exporter emits one material per UV routing (`_UV1`/`_UV2`);
        // both variants still identify the same G1M material.
        let material_name = mesh
            .material
            .rsplit_once("_UV")
            .filter(|(_, suffix)| suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty())
            .map(|(base, _)| base)
            .unwrap_or(&mesh.material);
        if !accepted
            .iter()
            .any(|value| value.eq_ignore_ascii_case(material_name))
        {
            return Err(invalid(format!(
                "mesh {} material {} does not match G1M material {}",
                mesh.name, mesh.material, accepted[0]
            )));
        }
        ordered[index] = Some(mesh);
    }
    for (index, mesh) in ordered
        .iter()
        .enumerate()
        .filter_map(|(index, mesh)| mesh.as_ref().map(|mesh| (index, mesh)))
    {
        let (attributes, _) = source_layout(geometry, index)?;
        if attributes.iter().any(|attribute| attribute.semantic == 3)
            && mesh.normals.len() != mesh.positions.len()
        {
            return Err(invalid(format!(
                "mesh {} does not provide one normal per vertex",
                mesh.name
            )));
        }
        let required_uv_maps = original.render.meshes[index].uv_maps.len();
        if mesh.uv_maps.len() != required_uv_maps
            || mesh
                .uv_maps
                .iter()
                .any(|uvs| uvs.len() != mesh.positions.len())
        {
            return Err(invalid(format!(
                "mesh {} UV map count/layout differs: G1M requires {}, FBX has {}",
                mesh.name,
                required_uv_maps,
                mesh.uv_maps.len()
            )));
        }
    }
    // Convert FBX armature indices to the local joint IDs stored by G1M palettes.
    let remap: Vec<u16> = imported
        .bones
        .iter()
        .map(|(n, _)| {
            let global = bone_index(n).unwrap_or(0);
            let local = original
                .global_to_local_bones
                .get(global)
                .copied()
                .filter(|local| *local != u16::MAX)
                .unwrap_or(0);
            local
        })
        .collect();
    for mesh in ordered.iter_mut().filter_map(Option::as_mut) {
        for joints in &mut mesh.bone_indices {
            for joint in joints {
                *joint = remap[*joint as usize];
            }
        }
        for joint in &mut mesh.palette_bones {
            *joint = remap[*joint as usize];
        }
    }
    let existing_joints: HashSet<_> = geometry
        .palettes
        .iter()
        .flat_map(|palette| palette.joints.iter().copied())
        .collect();
    let mut supplemental_joints = HashMap::new();
    for joint in ordered
        .iter()
        .filter_map(Option::as_ref)
        .flat_map(|mesh| mesh.palette_bones.iter().copied())
        .map(u32::from)
    {
        if existing_joints.contains(&joint) || supplemental_joints.contains_key(&joint) {
            continue;
        }
        let local = joint as usize;
        original
            .render
            .bones
            .get(local)
            .ok_or_else(|| invalid(format!("bone {joint} is outside the G1M skeleton")))?;
        let matrix_index = matrix_count + supplemental_joints.len();
        let inverse = crate::parser::fbx::inverse_affine_matrix(
            crate::parser::fbx::bone_world_matrix(&original.render.bones, local),
        );
        let mut matrix = [0.0; 16];
        for row in 0..4 {
            for column in 0..4 {
                matrix[column * 4 + row] = inverse[row * 4 + column] as f32;
            }
        }
        supplemental_joints.insert(
            joint,
            SupplementalJoint {
                raw: [matrix_index as u32, 0, joint],
                matrix: Some(matrix),
            },
        );
    }
    Ok((ordered, supplemental_joints))
}

fn mesh_index(name: &str) -> Option<usize> {
    name.strip_prefix("Mesh ")?.parse().ok()
}

fn parse_chunks(data: &[u8], endian: Endian) -> io::Result<Vec<Chunk<'_>>> {
    let mut reader = BinaryReader::with_endian(data, endian);
    let file_size = reader.read_u32_at(8)? as usize;
    if file_size > data.len() {
        return Err(invalid("G1M file size exceeds input"));
    }
    let offset = reader.read_u32_at(12)? as usize;
    let count = reader.read_u32_at(20)? as usize;
    reader.seek(offset)?;
    let mut chunks = Vec::with_capacity(count);
    for _ in 0..count {
        let start = reader.position();
        let mut signature = [0; 4];
        signature.copy_from_slice(reader.read_bytes(4)?);
        let mut version = [0; 4];
        version.copy_from_slice(reader.read_bytes(4)?);
        let size = reader.read_u32()? as usize;
        if size < 12 {
            return Err(invalid("invalid G1M chunk size"));
        }
        let end = start
            .checked_add(size)
            .ok_or_else(|| invalid("G1M chunk overflow"))?;
        let bytes = reader.slice(start, end)?;
        chunks.push(Chunk {
            signature,
            version,
            bytes,
        });
        reader.seek(end)?;
    }
    Ok(chunks)
}

fn parse_geometry(data: &[u8], endian: Endian) -> io::Result<Geometry<'_>> {
    let mut reader = BinaryReader::with_endian(data, endian);
    let mut signature = [0; 4];
    signature.copy_from_slice(reader.read_bytes(4)?);
    let version = reader.read_u32()?;
    let size = reader.read_u32()? as usize;
    if size > data.len() {
        return Err(invalid("G1MG size exceeds input"));
    }
    let mut platform = [0; 4];
    platform.copy_from_slice(reader.read_bytes(4)?);
    let reserved = reader.read_u32()?;
    let mut bounds = [0.0; 6];
    for value in &mut bounds {
        *value = reader.read_f32()?;
    }
    let count = reader.read_u32()? as usize;
    let mut sections = Vec::with_capacity(count);
    let mut vertex_buffers = Vec::new();
    let mut attrs = Vec::new();
    let mut index_buffers = Vec::new();
    let mut palettes = Vec::new();
    let mut submeshes = Vec::new();
    let mut visible_submeshes = HashSet::new();
    let mut cloth_submeshes = HashSet::new();
    for _ in 0..count {
        let start = reader.position();
        let kind = reader.read_u32()?;
        let section_size = reader.read_u32()? as usize;
        let item_count = reader.read_u32()? as usize;
        let end = start
            .checked_add(section_size)
            .ok_or_else(|| invalid("G1MG section overflow"))?;
        let bytes = reader.slice(start, end)?;
        match kind {
            0x0001_0004 => {
                for _ in 0..item_count {
                    let unknown1 = reader.read_u32()?;
                    let stride = reader.read_u32()? as usize;
                    let n = reader.read_u32()? as usize;
                    let unknown2 = (version > 0x3030_3430)
                        .then(|| reader.read_u32())
                        .transpose()?;
                    let byte_count = stride
                        .checked_mul(n)
                        .ok_or_else(|| invalid("vertex buffer overflow"))?;
                    let data = reader.read_bytes(byte_count)?.to_vec();
                    vertex_buffers.push(VertexBuffer {
                        unknown1,
                        unknown2,
                        stride,
                        data,
                    });
                }
            }
            0x0001_0005 => {
                for _ in 0..item_count {
                    let n = reader.read_u32()? as usize;
                    let buffers = (0..n)
                        .map(|_| reader.read_u32().map(|v| v as usize))
                        .collect::<io::Result<_>>()?;
                    let n = reader.read_u32()? as usize;
                    let mut list = Vec::with_capacity(n);
                    for _ in 0..n {
                        list.push(Attr {
                            buffer: reader.read_u16()? as usize,
                            offset: reader.read_u16()? as usize,
                            kind: reader.read_u8()?,
                            dummy: reader.read_u8()?,
                            semantic: reader.read_u8()?,
                            layer: reader.read_u8()?,
                        });
                    }
                    attrs.push(AttrSet {
                        buffers,
                        attrs: list,
                    });
                }
            }
            0x0001_0006 => {
                for _ in 0..item_count {
                    let n = reader.read_u32()? as usize;
                    let mut raw = Vec::with_capacity(n);
                    let mut joints = Vec::with_capacity(n);
                    for _ in 0..n {
                        let v = [reader.read_u32()?, reader.read_u32()?, reader.read_u32()?];
                        joints.push(v[2] & 0x7fff_ffff);
                        raw.push(v);
                    }
                    palettes.push(Palette { raw, joints });
                }
            }
            0x0001_0007 => {
                for _ in 0..item_count {
                    let n = reader.read_u32()? as usize;
                    let bits = reader.read_u32()?;
                    let unknown = (version > 0x3030_3430)
                        .then(|| reader.read_u32())
                        .transpose()?;
                    let byte_count = n
                        .checked_mul(bits as usize / 8)
                        .ok_or_else(|| invalid("index buffer overflow"))?;
                    let data = reader.read_bytes(byte_count)?.to_vec();
                    reader.align(4)?;
                    index_buffers.push(IndexBuffer {
                        unknown,
                        bits,
                        data,
                        count: n,
                    });
                }
            }
            0x0001_0008 => {
                for _ in 0..item_count {
                    let mut values = [0; 14];
                    for v in &mut values {
                        *v = reader.read_u32()?;
                    }
                    submeshes.push(Submesh { values });
                }
            }
            0x0001_0009 => {
                for group_index in 0..item_count {
                    let (first, second) = if version > 0x3030_3330 {
                        reader.skip(12)?;
                        let first = reader.read_u32()? as usize;
                        let second = reader.read_u32()? as usize;
                        if version > 0x3030_3430 {
                            reader.skip(16)?;
                        }
                        (first, second)
                    } else {
                        reader.skip(4)?;
                        (reader.read_u32()? as usize, reader.read_u32()? as usize)
                    };
                    for _ in 0..first.saturating_add(second) {
                        reader.skip(16)?;
                        let cloth_id = reader.read_u16()?;
                        reader.skip(2)?;
                        reader.skip(4)?;
                        let n = reader.read_u32()? as usize;
                        if n == 0 {
                            reader.skip(4)?;
                        } else {
                            for _ in 0..n {
                                let index = reader.read_u32()? as usize;
                                if group_index == 0 {
                                    visible_submeshes.insert(index);
                                    if cloth_id != 0 {
                                        cloth_submeshes.insert(index);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        reader.seek(end)?;
        sections.push(Section {
            kind,
            count: item_count,
            bytes,
        });
    }
    Ok(Geometry {
        signature,
        version,
        platform,
        reserved,
        bounds,
        sections,
        vertex_buffers,
        attrs,
        index_buffers,
        palettes,
        submeshes,
        visible_submeshes,
        cloth_submeshes,
    })
}

fn build_geometry(
    source: &Geometry<'_>,
    replacements: &[Option<ImportedMesh>],
    supplemental_joints: &HashMap<u32, SupplementalJoint>,
    endian: Endian,
) -> io::Result<Vec<u8>> {
    let mut built_sections = Vec::with_capacity(source.sections.len());
    for section in &source.sections {
        let rebuilt = match section.kind {
            0x0001_0004 => build_vertex_section(source, replacements, supplemental_joints, endian)?,
            0x0001_0005 => build_attribute_section(source, endian)?,
            0x0001_0006 => {
                build_palette_section(source, replacements, supplemental_joints, endian)?
            }
            0x0001_0007 => build_index_section(source, replacements, source.version, endian)?,
            0x0001_0008 => build_submesh_section(source, replacements, endian)?,
            _ => section.bytes.to_vec(),
        };
        built_sections.push(rebuilt);
    }
    let positions = replacements
        .iter()
        .filter_map(Option::as_ref)
        .flat_map(|mesh| mesh.positions.iter());
    let mut min = [source.bounds[0], source.bounds[1], source.bounds[2]];
    let mut max = [source.bounds[3], source.bounds[4], source.bounds[5]];
    for point in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let mut output = Vec::new();
    output.extend_from_slice(&source.signature);
    push_u32(&mut output, source.version, endian);
    push_u32(&mut output, 0, endian);
    output.extend_from_slice(&source.platform);
    push_u32(&mut output, source.reserved, endian);
    for value in min.into_iter().chain(max) {
        push_f32(&mut output, value, endian);
    }
    push_u32(&mut output, built_sections.len() as u32, endian);
    for section in built_sections {
        output.extend_from_slice(&section);
    }
    let size = output.len() as u32;
    put_u32(&mut output, 8, size, endian)?;
    Ok(output)
}

fn source_layout(source: &Geometry<'_>, mesh_index: usize) -> io::Result<(Vec<Attr>, usize)> {
    let sub = source
        .submeshes
        .get(mesh_index)
        .ok_or_else(|| invalid("missing submesh"))?;
    let set = source
        .attrs
        .get(sub.values[1] as usize)
        .ok_or_else(|| invalid("submesh vertex attribute set is out of range"))?;
    let mut attrs = Vec::with_capacity(set.attrs.len());
    let mut bases = Vec::with_capacity(set.buffers.len());
    let mut stride = 0;
    for &buffer in &set.buffers {
        bases.push(stride);
        stride += source
            .vertex_buffers
            .get(buffer)
            .ok_or_else(|| invalid("vertex buffer is out of range"))?
            .stride;
    }
    for attr in &set.attrs {
        let mut attr = attr.clone();
        attr.offset += *bases
            .get(attr.buffer)
            .ok_or_else(|| invalid("attribute buffer slot is out of range"))?;
        attr.buffer = 0;
        attrs.push(attr);
    }
    Ok((attrs, stride))
}

fn build_vertex_section(
    source: &Geometry<'_>,
    replacements: &[Option<ImportedMesh>],
    supplemental_joints: &HashMap<u32, SupplementalJoint>,
    endian: Endian,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    let palettes = replacement_palettes(source, replacements, supplemental_joints)?;
    for buffer_index in 0..source.vertex_buffers.len() {
        let mesh_indices: Vec<_> = source
            .submeshes
            .iter()
            .enumerate()
            .filter_map(|(index, sub)| (sub.values[1] as usize == buffer_index).then_some(index))
            .collect();
        if !mesh_indices
            .iter()
            .any(|index| replacements[*index].is_some())
        {
            let old = &source.vertex_buffers[buffer_index];
            push_u32(&mut body, old.unknown1, endian);
            push_u32(&mut body, old.stride as u32, endian);
            push_u32(&mut body, (old.data.len() / old.stride) as u32, endian);
            if let Some(value) = old.unknown2 {
                push_u32(&mut body, value, endian);
            }
            body.extend_from_slice(&old.data);
            continue;
        }
        let (attrs, stride) = source_layout(source, mesh_indices[0])?;
        let old = source
            .vertex_buffers
            .get(buffer_index)
            .ok_or_else(|| invalid("missing source vertex buffer"))?;
        push_u32(&mut body, old.unknown1, endian);
        push_u32(&mut body, stride as u32, endian);
        push_u32(
            &mut body,
            mesh_indices
                .iter()
                .map(|index| {
                    replacements[*index]
                        .as_ref()
                        .map_or(source.submeshes[*index].values[11], |mesh| {
                            mesh.positions.len() as u32
                        })
                })
                .sum(),
            endian,
        );
        if let Some(v) = old.unknown2 {
            push_u32(&mut body, v, endian);
        }
        for index in mesh_indices {
            let Some(mesh) = replacements[index].as_ref() else {
                let submesh = &source.submeshes[index];
                let start = submesh.values[10] as usize * old.stride;
                let size = submesh.values[11] as usize * old.stride;
                let end = start
                    .checked_add(size)
                    .ok_or_else(|| invalid("source vertex range overflow"))?;
                body.extend_from_slice(
                    old.data
                        .get(start..end)
                        .ok_or_else(|| invalid("source vertex range is out of bounds"))?,
                );
                continue;
            };
            let palette_index = source.submeshes[index].values[2] as usize;
            let palette = palettes
                .get(palette_index)
                .ok_or_else(|| invalid("mesh joint palette is out of range"))?;
            for vertex in 0..mesh.positions.len() {
                let start = body.len();
                body.resize(start + stride, 0);
                let source_vertex =
                    source.submeshes[index].values[10] as usize + mesh.source_vertices[vertex];
                let source_start = source_vertex.saturating_mul(old.stride);
                if old.stride == stride && source_start + stride <= old.data.len() {
                    body[start..start + stride]
                        .copy_from_slice(&old.data[source_start..source_start + stride]);
                }
                for attr in &attrs {
                    if matches!(attr.semantic, 4 | 7 | 8 | 9 | 10 | 11 | 12 | 13)
                        || (attr.semantic == 5 && attr.layer > 2)
                    {
                        continue;
                    }
                    write_attribute(
                        &mut body[start + attr.offset..],
                        attr,
                        mesh,
                        vertex,
                        Some(palette),
                        endian,
                    )?;
                }
            }
        }
    }
    section(0x0001_0004, source.vertex_buffers.len(), body, endian)
}

fn write_attribute(
    dst: &mut [u8],
    attr: &Attr,
    mesh: &ImportedMesh,
    vertex: usize,
    palette: Option<&Palette>,
    endian: Endian,
) -> io::Result<()> {
    let mut value = [0.0; 4];
    match attr.semantic {
        0 => {
            let p = mesh.positions[vertex];
            value = [p[0], p[1], p[2], 1.0];
        }
        1 => {
            let start = attr.layer as usize * 4;
            value.copy_from_slice(&mesh.bone_weights[vertex][start..start + 4]);
        }
        2 => {
            let joints = mesh.bone_indices[vertex];
            let palette = palette.ok_or_else(|| invalid("skinned mesh has no joint palette"))?;
            for i in 0..4 {
                let influence = attr.layer as usize * 4 + i;
                if mesh.bone_weights[vertex][influence] <= 0.0 {
                    value[i] = 0.0;
                    continue;
                }
                let local = palette
                    .joints
                    .iter()
                    .position(|v| *v == joints[influence] as u32)
                    .ok_or_else(|| {
                        invalid(format!(
                            "mesh {} uses bone {} outside its original joint palette {:?}",
                            mesh.name, joints[influence], palette.joints
                        ))
                    })?;
                value[i] = (local * 3) as f32;
            }
        }
        3 => {
            let n = mesh.normals.get(vertex).copied().unwrap_or([0.0, 0.0, 1.0]);
            value = [n[0], n[1], n[2], 0.0];
        }
        5 => {
            if matches!(attr.kind, 0x03 | 0x0b) {
                let uv0 = mesh
                    .uv_maps
                    .first()
                    .and_then(|uvs| uvs.get(vertex))
                    .copied()
                    .unwrap_or([0.0; 2]);
                let uv1 = mesh
                    .uv_maps
                    .get(1)
                    .and_then(|uvs| uvs.get(vertex))
                    .copied()
                    .unwrap_or([0.0; 2]);
                value = [uv0[0], uv0[1], uv1[0], uv1[1]];
            } else {
                let uv = mesh
                    .uv_maps
                    .get(attr.layer as usize)
                    .and_then(|uvs| uvs.get(vertex))
                    .copied()
                    .unwrap_or([0.0; 2]);
                value = [uv[0], uv[1], 0.0, 0.0];
            }
        }
        6 => value = mesh.tangents[vertex],
        10 => {
            value = mesh
                .colors
                .get(attr.layer as usize)
                .and_then(|values| values.get(vertex))
                .copied()
                .unwrap_or([1.0; 4]);
        }
        _ => {}
    }
    encode_value(dst, attr.kind, value, endian)
}

fn encode_value(dst: &mut [u8], kind: u8, v: [f32; 4], endian: Endian) -> io::Result<()> {
    let need = match kind {
        0x00 => 4,
        0x01 => 8,
        0x02 => 12,
        0x03 => 16,
        0x05 | 0x0d => 4,
        0x07 | 0x0b => 8,
        0x09 => 16,
        0x0a => 4,
        0xff => 0,
        _ => {
            return Err(invalid(format!(
                "unsupported G1M vertex data type 0x{kind:02X}"
            )))
        }
    };
    if dst.len() < need {
        return Err(invalid("vertex attribute exceeds stride"));
    }
    match kind {
        0x00..=0x03 => {
            for (i, value) in v.iter().take((kind + 1) as usize).enumerate() {
                write_f32(&mut dst[i * 4..], *value, endian)?;
            }
        }
        0x05 => {
            for i in 0..4 {
                dst[i] = v[i].round().clamp(0.0, 255.0) as u8;
            }
        }
        0x07 => {
            for i in 0..4 {
                write_u16(
                    &mut dst[i * 2..],
                    v[i].round().clamp(0.0, 65535.0) as u16,
                    endian,
                )?;
            }
        }
        0x09 => {
            for i in 0..4 {
                write_u32(&mut dst[i * 4..], v[i].round().max(0.0) as u32, endian)?;
            }
        }
        0x0a => {
            for i in 0..2 {
                write_u16(&mut dst[i * 2..], f32_to_half(v[i]), endian)?;
            }
        }
        0x0b => {
            for i in 0..4 {
                write_u16(&mut dst[i * 2..], f32_to_half(v[i]), endian)?;
            }
        }
        0x0d => {
            for i in 0..4 {
                dst[i] = (v[i].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
        _ => {}
    }
    Ok(())
}

fn build_attribute_section(source: &Geometry<'_>, endian: Endian) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for buffer_index in 0..source.vertex_buffers.len() {
        let mesh_index = source
            .submeshes
            .iter()
            .position(|sub| sub.values[1] as usize == buffer_index)
            .ok_or_else(|| invalid("vertex buffer has no submesh"))?;
        let (attrs, _) = source_layout(source, mesh_index)?;
        push_u32(&mut body, 1, endian);
        push_u32(&mut body, buffer_index as u32, endian);
        push_u32(&mut body, attrs.len() as u32, endian);
        for attribute in attrs {
            push_u16(&mut body, 0, endian);
            push_u16(&mut body, attribute.offset as u16, endian);
            body.extend_from_slice(&[
                attribute.kind,
                attribute.dummy,
                attribute.semantic,
                attribute.layer,
            ]);
        }
    }
    section(0x0001_0005, source.vertex_buffers.len(), body, endian)
}
fn replacement_palettes(
    source: &Geometry<'_>,
    replacements: &[Option<ImportedMesh>],
    supplemental_joints: &HashMap<u32, SupplementalJoint>,
) -> io::Result<Vec<Palette>> {
    let mut global_entries = HashMap::<u32, [u32; 3]>::new();
    for entry in source
        .palettes
        .iter()
        .flat_map(|palette| palette.raw.iter().copied())
    {
        let joint = entry[2] & 0x7fff_ffff;
        let current = global_entries.entry(joint).or_insert(entry);
        if current[1] != 0 && entry[1] == 0 {
            *current = entry;
        }
    }
    let mut palettes = source.palettes.clone();
    for (index, mesh) in replacements
        .iter()
        .enumerate()
        .filter_map(|(index, mesh)| mesh.as_ref().map(|mesh| (index, mesh)))
    {
        let palette_index = source.submeshes[index].values[2] as usize;
        let palette = palettes
            .get_mut(palette_index)
            .ok_or_else(|| invalid("mesh joint palette is out of range"))?;
        for &joint in &mesh.palette_bones {
            let joint = joint as u32;
            if palette.joints.contains(&joint) {
                continue;
            }
            let entry = global_entries
                .get(&joint)
                .copied()
                .or_else(|| supplemental_joints.get(&joint).map(|joint| joint.raw))
                .ok_or_else(|| invalid(format!("bone {joint} has no G1M joint metadata")))?;
            palette.raw.push(entry);
            palette.joints.push(joint);
        }
    }
    Ok(palettes)
}
fn build_palette_section(
    source: &Geometry<'_>,
    replacements: &[Option<ImportedMesh>],
    supplemental_joints: &HashMap<u32, SupplementalJoint>,
    endian: Endian,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    let palettes = replacement_palettes(source, replacements, supplemental_joints)?;
    for palette in &palettes {
        push_u32(&mut body, palette.raw.len() as u32, endian);
        for entry in &palette.raw {
            for value in entry {
                push_u32(&mut body, *value, endian);
            }
        }
    }
    section(0x0001_0006, palettes.len(), body, endian)
}
fn build_index_section(
    source: &Geometry<'_>,
    replacements: &[Option<ImportedMesh>],
    version: u32,
    endian: Endian,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for buffer_index in 0..source.index_buffers.len() {
        let mesh_indices: Vec<_> = source
            .submeshes
            .iter()
            .enumerate()
            .filter_map(|(index, sub)| (sub.values[7] as usize == buffer_index).then_some(index))
            .collect();
        if !mesh_indices
            .iter()
            .any(|index| replacements[*index].is_some())
        {
            let old = &source.index_buffers[buffer_index];
            push_u32(&mut body, old.count as u32, endian);
            push_u32(&mut body, old.bits, endian);
            if version > 0x3030_3430 {
                push_u32(&mut body, old.unknown.unwrap_or(0), endian);
            }
            body.extend_from_slice(&old.data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            continue;
        }
        let vertex_count: usize = mesh_indices
            .iter()
            .map(|index| {
                replacements[*index]
                    .as_ref()
                    .map_or(source.submeshes[*index].values[11] as usize, |mesh| {
                        mesh.positions.len()
                    })
            })
            .sum();
        let index_count: usize = mesh_indices
            .iter()
            .map(|index| {
                replacements[*index]
                    .as_ref()
                    .map_or(source.submeshes[*index].values[13] as usize, |mesh| {
                        mesh.indices.len()
                    })
            })
            .sum();
        let old = &source.index_buffers[buffer_index];
        let use32 = old.bits == 32 || vertex_count > u16::MAX as usize;
        push_u32(&mut body, index_count as u32, endian);
        push_u32(&mut body, if use32 { 32 } else { 16 }, endian);
        if version > 0x3030_3430 {
            push_u32(&mut body, old.unknown.unwrap_or(0), endian);
        }
        let mut vertex_offset = 0u32;
        for mesh_index in mesh_indices {
            let submesh = &source.submeshes[mesh_index];
            let indices = if let Some(mesh) = replacements[mesh_index].as_ref() {
                mesh.indices.clone()
            } else {
                let reader = BinaryReader::with_endian(&old.data, endian);
                let stride = old.bits as usize / 8;
                (submesh.values[12] as usize
                    ..submesh.values[12].saturating_add(submesh.values[13]) as usize)
                    .map(|index| match stride {
                        2 => reader.read_u16_at(index * stride).map(u32::from),
                        4 => reader.read_u32_at(index * stride),
                        _ => Err(invalid("unsupported source index width")),
                    })
                    .collect::<io::Result<Vec<_>>>()?
                    .into_iter()
                    .map(|index| index.saturating_sub(submesh.values[10]))
                    .collect()
            };
            for index in indices {
                let index = index + vertex_offset;
                if use32 {
                    push_u32(&mut body, index, endian)
                } else {
                    push_u16(&mut body, index as u16, endian)
                }
            }
            vertex_offset += replacements[mesh_index]
                .as_ref()
                .map_or(submesh.values[11], |mesh| mesh.positions.len() as u32);
        }
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    section(0x0001_0007, source.index_buffers.len(), body, endian)
}
fn build_submesh_section(
    source: &Geometry<'_>,
    replacements: &[Option<ImportedMesh>],
    endian: Endian,
) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for i in 0..source.submeshes.len() {
        let mut v = source.submeshes[i].values;
        if let Some(mesh) = replacements.get(i).and_then(Option::as_ref) {
            v[9] = 3;
            v[10] = source.submeshes[..i]
                .iter()
                .enumerate()
                .filter(|(_, sub)| sub.values[1] == v[1])
                .map(|(prior, submesh)| {
                    replacements[prior]
                        .as_ref()
                        .map_or(submesh.values[11], |mesh| mesh.positions.len() as u32)
                })
                .sum();
            v[11] = mesh.positions.len() as u32;
            v[12] = source.submeshes[..i]
                .iter()
                .enumerate()
                .filter(|(_, sub)| sub.values[7] == v[7])
                .map(|(prior, submesh)| {
                    replacements[prior]
                        .as_ref()
                        .map_or(submesh.values[13], |mesh| mesh.indices.len() as u32)
                })
                .sum();
            v[13] = mesh.indices.len() as u32;
        }
        for x in v {
            push_u32(&mut body, x, endian);
        }
    }
    section(0x0001_0008, source.submeshes.len(), body, endian)
}
fn section(kind: u32, count: usize, body: Vec<u8>, endian: Endian) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    push_u32(&mut out, kind, endian);
    push_u32(&mut out, (body.len() + 12) as u32, endian);
    push_u32(&mut out, count as u32, endian);
    out.extend(body);
    Ok(out)
}

fn rebuild_g1mf(
    old: Option<&[u8]>,
    old_geometry: &Geometry<'_>,
    new_geometry: &[u8],
    new_matrix_count: usize,
    endian: Endian,
) -> io::Result<Vec<u8>> {
    let Some(old) = old else {
        return Ok(Vec::new());
    };
    if old.len() < 96 {
        return Err(invalid("G1MF chunk is truncated"));
    }
    let new = parse_geometry(new_geometry, endian)?;
    let mut out = old.to_vec();
    put_u32(&mut out, 24, new_matrix_count as u32, endian)?;
    let counts = |g: &Geometry<'_>, kind| {
        g.sections
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.count as u32)
            .unwrap_or(0)
    };
    // G1MF's first four counters describe non-geometry data and remain byte-for-byte intact.
    let fields = [
        counts(&new, 0x0001_0001),
        counts(&new, 0x0001_0002),
        counts(&new, 0x0001_0003),
        read_u32(old, 40, endian)?,
        read_u32(old, 44, endian)?,
        read_u32(old, 48, endian)?,
        counts(&new, 0x0001_0004),
        counts(&new, 0x0001_0005),
        new.attrs.iter().map(|a| a.buffers.len() as u32).sum(),
        counts(&new, 0x0001_0006),
        new.palettes.iter().map(|p| p.raw.len() as u32).sum(),
        counts(&new, 0x0001_0007),
        counts(&new, 0x0001_0008),
        counts(&new, 0x0001_0008),
        counts(&new, 0x0001_0009),
        read_u32(old, 88, endian)?,
        read_u32(old, 92, endian)?,
    ];
    for (i, value) in fields.into_iter().enumerate() {
        put_u32(&mut out, 28 + i * 4, value, endian)?;
    }
    // The chunk length itself is fixed by the format and the retained tail.
    let out_len = out.len() as u32;
    put_u32(&mut out, 8, out_len, endian)?;
    let _ = old_geometry;
    Ok(out)
}

fn matrix_chunk_count(data: &[u8], endian: Endian) -> io::Result<usize> {
    if data.len() < 16 {
        return Err(invalid("G1MM chunk is truncated"));
    }
    let count = read_u32(data, 12, endian)? as usize;
    let expected = 16usize
        .checked_add(
            count
                .checked_mul(64)
                .ok_or_else(|| invalid("G1MM matrix count overflow"))?,
        )
        .ok_or_else(|| invalid("G1MM size overflow"))?;
    if expected > data.len() {
        return Err(invalid("G1MM matrices exceed chunk size"));
    }
    Ok(count)
}

fn rebuild_matrix_chunk(old: &[u8], matrices: &[[f32; 16]], endian: Endian) -> io::Result<Vec<u8>> {
    let old_count = matrix_chunk_count(old, endian)?;
    let mut out = old.to_vec();
    for matrix in matrices {
        for value in matrix {
            push_f32(&mut out, *value, endian);
        }
    }
    let size = out.len() as u32;
    put_u32(&mut out, 8, size, endian)?;
    put_u32(&mut out, 12, (old_count + matrices.len()) as u32, endian)?;
    Ok(out)
}

fn g1m_endian(data: &[u8]) -> io::Result<Endian> {
    match data.get(..4) {
        Some(b"_M1G") => Ok(Endian::Little),
        Some(b"G1M_") => Ok(Endian::Big),
        _ => Err(invalid("not a G1M model")),
    }
}
fn read_u32(data: &[u8], offset: usize, endian: Endian) -> io::Result<u32> {
    BinaryReader::with_endian(data, endian).read_u32_at(offset)
}
fn push_u16(out: &mut Vec<u8>, v: u16, e: Endian) {
    out.extend_from_slice(&e.u16_to_bytes(v));
}
fn push_u32(out: &mut Vec<u8>, v: u32, e: Endian) {
    out.extend_from_slice(&e.u32_to_bytes(v));
}
fn push_f32(out: &mut Vec<u8>, v: f32, e: Endian) {
    push_u32(out, v.to_bits(), e)
}
fn write_u16(out: &mut [u8], v: u16, e: Endian) -> io::Result<()> {
    put_bounded(BinaryPatcher::with_endian(out, e).write_u16_at(0, v))
}
fn write_u32(out: &mut [u8], v: u32, e: Endian) -> io::Result<()> {
    put_bounded(BinaryPatcher::with_endian(out, e).write_u32_at(0, v))
}
fn write_f32(out: &mut [u8], v: f32, e: Endian) -> io::Result<()> {
    write_u32(out, v.to_bits(), e)
}
fn put_u32(out: &mut [u8], offset: usize, v: u32, e: Endian) -> io::Result<()> {
    put_bounded(BinaryPatcher::with_endian(out, e).write_u32_at(offset, v))
}
fn put_bounded(result: io::Result<()>) -> io::Result<()> {
    result.map_err(|_| invalid("write exceeds output"))
}
fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7fffff;
    if exponent <= 0 {
        return sign;
    }
    if exponent >= 31 {
        return sign | 0x7c00;
    }
    sign | ((exponent as u16) << 10) | ((mantissa >> 13) as u16)
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[test]
    fn replaces_supplied_original_with_external_fbx() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let g1m = root.join("2/f23c0538.g1m");
        let fbx = root.join("1/[Impa cutscene] f23c0538.fbx");
        if !g1m.is_file() || !fbx.is_file() {
            return;
        }
        let fbx_bytes = std::fs::read(&fbx).unwrap();
        let mut imported = import_for_g1m(&fbx_bytes).unwrap();
        imported
            .meshes
            .sort_by_key(|mesh| mesh_index(&mesh.name).unwrap());
        let original_bytes = std::fs::read(g1m).unwrap();
        let original = G1mFile::parse_for_export(&original_bytes, "f23c0538").unwrap();
        let result = replace_meshes_from_fbx(&original_bytes, &fbx_bytes, "f23c0538").unwrap();
        let parsed = G1mFile::parse_for_export(&result, "f23c0538").unwrap();
        let expected = G1mFile::parse_for_export(
            &std::fs::read(root.join("1/f23c0538.g1m")).unwrap(),
            "f23c0538",
        )
        .unwrap();
        assert_eq!(parsed.render.meshes.len(), original.render.meshes.len());
        assert_eq!(
            parsed
                .render
                .meshes
                .iter()
                .take(imported.meshes.len())
                .map(|mesh| mesh.positions.len())
                .collect::<Vec<_>>(),
            expected
                .render
                .meshes
                .iter()
                .take(imported.meshes.len())
                .map(|mesh| mesh.positions.len())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            parsed
                .render
                .meshes
                .iter()
                .take(10)
                .map(|m| m.indices.len())
                .collect::<Vec<_>>(),
            expected
                .render
                .meshes
                .iter()
                .take(10)
                .map(|m| m.indices.len())
                .collect::<Vec<_>>()
        );
        let bounds = |model: &G1mFile| {
            model
                .render
                .meshes
                .iter()
                .take(10)
                .flat_map(|mesh| &mesh.positions)
                .fold(
                    ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
                    |(mut min, mut max), point| {
                        for axis in 0..3 {
                            min[axis] = min[axis].min(point[axis]);
                            max[axis] = max[axis].max(point[axis]);
                        }
                        (min, max)
                    },
                )
        };
        let (actual_min, actual_max) = bounds(&parsed);
        let (expected_min, expected_max) = bounds(&expected);
        for axis in 0..3 {
            assert!((actual_min[axis] - expected_min[axis]).abs() < 0.01, "axis {axis}: actual {actual_min:?}..{actual_max:?}, expected {expected_min:?}..{expected_max:?}");
            assert!((actual_max[axis] - expected_max[axis]).abs() < 0.01, "axis {axis}: actual {actual_min:?}..{actual_max:?}, expected {expected_min:?}..{expected_max:?}");
        }
        for (mesh_index, (actual, source)) in parsed
            .render
            .meshes
            .iter()
            .zip(&imported.meshes)
            .enumerate()
        {
            assert_eq!(
                actual.normals.len(),
                source.normals.len(),
                "mesh {mesh_index}"
            );
            assert_eq!(
                actual.uv_maps.len(),
                source.uv_maps.len(),
                "mesh {mesh_index}"
            );
            for (vertex, (actual_normal, source_normal)) in
                actual.normals.iter().zip(&source.normals).enumerate()
            {
                for axis in 0..3 {
                    assert!(
                        (actual_normal[axis] - source_normal[axis]).abs() < 0.002,
                        "mesh {mesh_index} vertex {vertex} normal axis {axis}: {actual_normal:?} != {source_normal:?}"
                    );
                }
            }
            for (layer, (actual_uvs, source_uvs)) in
                actual.uv_maps.iter().zip(&source.uv_maps).enumerate()
            {
                assert_eq!(
                    actual_uvs.len(),
                    source_uvs.len(),
                    "mesh {mesh_index} UV {layer}"
                );
                for (vertex, (actual_uv, source_uv)) in
                    actual_uvs.iter().zip(source_uvs).enumerate()
                {
                    for axis in 0..2 {
                        assert!(
                            (actual_uv[axis] - source_uv[axis]).abs() < 0.002,
                            "mesh {mesh_index} UV {layer} vertex {vertex} axis {axis}: {actual_uv:?} != {source_uv:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn replaces_only_named_fbx_meshes_and_preserves_the_rest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let g1m = root.join("2/f23c0538.g1m");
        let fbx = root.join("1/[Impa cutscene] f23c0538.fbx");
        if !g1m.is_file() || !fbx.is_file() {
            return;
        }
        let original_bytes = std::fs::read(g1m).unwrap();
        let original = G1mFile::parse_for_export(&original_bytes, "f23c0538").unwrap();
        let mut imported = import_for_g1m(&std::fs::read(fbx).unwrap()).unwrap();
        imported.meshes.retain(|mesh| mesh.name == "Mesh 3");
        let replacement_vertex_count = imported.meshes[0].positions.len();
        let rebuilt = replace_imported_meshes(&original_bytes, imported, "f23c0538").unwrap();
        let parsed = G1mFile::parse_for_export(&rebuilt, "f23c0538").unwrap();

        assert_eq!(parsed.render.meshes.len(), original.render.meshes.len());
        assert_eq!(
            parsed.render.meshes[3].positions.len(),
            replacement_vertex_count
        );
        for index in 0..original.render.meshes.len() {
            if index != 3 {
                assert_eq!(
                    parsed.render.meshes[index].positions, original.render.meshes[index].positions,
                    "mesh {index} changed"
                );
            }
        }
    }

    #[test]
    fn rejects_out_of_range_mesh_name_without_panicking() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let g1m = root.join("2/f23c0538.g1m");
        let fbx = root.join("1/[Impa cutscene] f23c0538.fbx");
        if !g1m.is_file() || !fbx.is_file() {
            return;
        }
        let original_bytes = std::fs::read(g1m).unwrap();
        let mut imported = import_for_g1m(&std::fs::read(fbx).unwrap()).unwrap();
        imported.meshes.truncate(1);
        imported.meshes[0].name = "Mesh 9999".into();
        let error = replace_imported_meshes(&original_bytes, imported, "f23c0538").unwrap_err();
        assert!(error.to_string().contains("exceeds G1M mesh count"));
    }

    #[test]
    fn playable_impa_preserves_palette_metadata() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
        let original_path = root.join("clean/231ccec8.g1m");
        let fbx_path = root.join("1/[Impa playable] 231ccec8.fbx");
        let expected_path = root.join("moddedworking/231ccec8.g1m");
        if [&original_path, &fbx_path, &expected_path]
            .iter()
            .any(|path| !path.is_file())
        {
            return;
        }
        let original = std::fs::read(original_path).unwrap();
        let expected = std::fs::read(expected_path).unwrap();
        let result =
            replace_meshes_from_fbx(&original, &std::fs::read(fbx_path).unwrap(), "231ccec8")
                .unwrap();
        assert!(result.len().abs_diff(expected.len()) < 1024);

        fn geometry(data: &[u8]) -> Geometry<'_> {
            let endian = g1m_endian(data).unwrap();
            let chunks = parse_chunks(data, endian).unwrap();
            let chunk = chunks
                .iter()
                .find(|chunk| matches!(&chunk.signature, b"G1MG" | b"GM1G"))
                .unwrap();
            parse_geometry(chunk.bytes, endian).unwrap()
        }
        let original_geometry = geometry(&original);
        let result_geometry = geometry(&result);
        let valid_entries: HashSet<_> = original_geometry
            .palettes
            .iter()
            .flat_map(|palette| palette.raw.iter().copied())
            .collect();
        assert!(result_geometry
            .palettes
            .iter()
            .flat_map(|palette| &palette.raw)
            .all(|entry| valid_entries.contains(entry)));

        let actual = G1mFile::parse_for_export(&result, "231ccec8").unwrap();
        let expected = G1mFile::parse_for_export(&expected, "231ccec8").unwrap();
        assert_eq!(actual.render.meshes.len(), expected.render.meshes.len());
        for (actual, expected) in actual.render.meshes.iter().zip(&expected.render.meshes) {
            assert_eq!(actual.positions.len(), expected.positions.len());
            assert_eq!(actual.indices.len(), expected.indices.len());
            for vertex in 0..actual.bone_weights.len() {
                for influence in 0..4 {
                    if actual.bone_weights[vertex][influence] > 0.001
                        || expected.bone_weights[vertex][influence] > 0.001
                    {
                        assert_eq!(
                            actual.bone_indices[vertex][influence],
                            expected.bone_indices[vertex][influence]
                        );
                    }
                }
            }
        }
    }
}

//! Loader for little-endian BFRES version 10 files (vanilla TOTK models with
//! external strings and external GPU buffers, as well as files previously
//! written by Toolbox). Mirrors what Syroot's `ResFileSwitchLoader` keeps in
//! memory so the saver can reproduce Toolbox's output.

use super::super::BfresError;
use super::model::*;
use super::strings::{read_f32, read_res_string, read_u16, read_u32, read_u64, ExternalStrings};

const DEFAULT_VALUE: &str = "<Default Value>";

struct Loader<'a> {
    data: &'a [u8],
    ext: &'a ExternalStrings,
    /// Absolute offset of the GPU buffer region (`BufferInfo.BufferOffset`).
    buffer_offset: usize,
}

impl<'a> Loader<'a> {
    fn string(&self, pointer: u64) -> Result<String, BfresError> {
        if pointer == 0 {
            return Ok(String::new());
        }
        if let Some(value) = self.ext.map.get(&pointer) {
            return Ok(value.clone());
        }
        let offset = usize::try_from(pointer)
            .ok()
            .filter(|offset| *offset < self.data.len())
            .ok_or_else(|| {
                BfresError::new(
                    0,
                    format!("string key {pointer:#x} is not in the external string table"),
                )
            })?;
        read_res_string(self.data, offset)
    }

    fn string_at(&self, field: usize) -> Result<String, BfresError> {
        self.string(read_u64(self.data, field)?)
    }

    fn strings_array(&self, pointer: usize, count: usize) -> Result<Vec<String>, BfresError> {
        (0..count)
            .map(|index| self.string_at(pointer + index * 8))
            .collect()
    }

    fn dict_keys(&self, pointer: usize) -> Result<Vec<String>, BfresError> {
        if pointer == 0 {
            return Ok(Vec::new());
        }
        let count = read_u32(self.data, pointer + 4)? as usize;
        (0..count)
            .map(|index| self.string_at(pointer + 8 + (index + 1) * 16 + 8))
            .collect()
    }

    fn bytes(&self, offset: usize, len: usize) -> Result<Vec<u8>, BfresError> {
        self.data
            .get(offset..offset + len)
            .map(|slice| slice.to_vec())
            .ok_or_else(|| BfresError::new(offset, format!("truncated block of {len} bytes")))
    }

    /// Reads `count` v10 user data records (32 bytes each).
    fn user_data(&self, array: usize, count: usize) -> Result<Vec<UserData>, BfresError> {
        let d = self.data;
        let mut list = Vec::with_capacity(count);
        if array == 0 {
            return Ok(list);
        }
        for index in 0..count {
            let record = array + index * 32;
            let data = read_u64(d, record + 8)? as usize;
            let length = read_u32(d, record + 16)? as usize;
            let kind = d[record + 20];
            let mut entry = UserData {
                name: self.string_at(record)?,
                kind,
                ..Default::default()
            };
            match kind {
                0 => {
                    for i in 0..length {
                        entry.ints.push(read_u32(d, data + i * 4)? as i32);
                    }
                }
                1 => {
                    for i in 0..length {
                        entry.floats.push(read_f32(d, data + i * 4)?);
                    }
                }
                2 => entry.strings = self.strings_array(data, length)?,
                4 => entry.bytes = self.bytes(data, length)?,
                other => {
                    return Err(BfresError::new(
                        record + 20,
                        format!("user data type {other} is not supported"),
                    ))
                }
            }
            list.push(entry);
        }
        Ok(list)
    }

    fn u16_array(&self, offset: usize, count: usize) -> Result<Vec<u16>, BfresError> {
        (0..count)
            .map(|index| read_u16(self.data, offset + index * 2))
            .collect()
    }

    fn model(&self, header: usize) -> Result<Model, BfresError> {
        let d = self.data;
        if d.get(header..header + 4) != Some(b"FMDL") {
            return Err(BfresError::new(header, "missing FMDL signature"));
        }
        let vertex_count = read_u16(d, header + 104)? as usize;
        let shape_count = read_u16(d, header + 106)? as usize;
        let material_count = read_u16(d, header + 108)? as usize;
        let vertex_array = read_u64(d, header + 32)? as usize;
        let shape_array = read_u64(d, header + 40)? as usize;
        let material_array = read_u64(d, header + 56)? as usize;

        let mut vertex_buffers = Vec::with_capacity(vertex_count);
        let mut vertex_positions = Vec::with_capacity(vertex_count);
        for index in 0..vertex_count {
            let offset = vertex_array + index * 88;
            vertex_positions.push(offset);
            vertex_buffers.push(self.vertex_buffer(offset)?);
        }
        let mut shapes = Vec::with_capacity(shape_count);
        for index in 0..shape_count {
            shapes.push(self.shape(shape_array + index * 96, &vertex_positions)?);
        }
        let mut materials = Vec::with_capacity(material_count);
        for index in 0..material_count {
            materials.push(self.material(material_array + index * 176)?);
        }
        Ok(Model {
            flags: read_u32(d, header + 4)?,
            name: self.string_at(header + 8)?,
            path: self.string_at(header + 16)?,
            skeleton: self.skeleton(read_u64(d, header + 24)? as usize)?,
            vertex_buffers,
            shapes,
            materials,
            user_data: self.user_data(
                read_u64(d, header + 80)? as usize,
                read_u16(d, header + 112)? as usize,
            )?,
        })
    }

    fn skeleton(&self, header: usize) -> Result<Skeleton, BfresError> {
        let d = self.data;
        if d.get(header..header + 4) != Some(b"FSKL") {
            return Err(BfresError::new(header, "missing FSKL signature"));
        }
        let bone_array = read_u64(d, header + 16)? as usize;
        let matrix_to_bone = read_u64(d, header + 24)? as usize;
        let inverse = read_u64(d, header + 32)? as usize;
        let mirror = read_u64(d, header + 48)? as usize;
        let bone_count = read_u16(d, header + 56)? as usize;
        let smooth_count = read_u16(d, header + 58)? as usize;
        let rigid_count = read_u16(d, header + 60)? as usize;
        let mut bones = Vec::with_capacity(bone_count);
        for index in 0..bone_count {
            let b = bone_array + index * 88;
            let f = |o: usize| read_f32(d, b + o);
            bones.push(Bone {
                name: self.string_at(b)?,
                parent_index: read_u16(d, b + 34)? as i16,
                smooth_matrix_index: read_u16(d, b + 36)? as i16,
                rigid_matrix_index: read_u16(d, b + 38)? as i16,
                billboard_index: read_u16(d, b + 40)? as i16,
                flags: read_u32(d, b + 44)?,
                scale: [f(48)?, f(52)?, f(56)?],
                rotation: [f(60)?, f(64)?, f(68)?, f(72)?],
                position: [f(76)?, f(80)?, f(84)?],
                user_data: self
                    .user_data(read_u64(d, b + 8)? as usize, read_u16(d, b + 42)? as usize)?,
            });
        }
        let mut inverse_matrices = Vec::with_capacity(smooth_count);
        if inverse != 0 {
            for index in 0..smooth_count {
                let base = inverse + index * 48;
                let mut matrix = [0f32; 12];
                for (slot, value) in matrix.iter_mut().enumerate() {
                    *value = read_f32(d, base + slot * 4)?;
                }
                inverse_matrices.push(matrix);
            }
        }
        Ok(Skeleton {
            flags: read_u32(d, header + 4)?,
            bones,
            matrix_to_bone: if matrix_to_bone != 0 {
                self.u16_array(matrix_to_bone, smooth_count + rigid_count)?
            } else {
                Vec::new()
            },
            inverse_matrices,
            mirrored_bones: if mirror != 0 {
                self.u16_array(mirror, bone_count)?
            } else {
                Vec::new()
            },
        })
    }

    fn vertex_buffer(&self, header: usize) -> Result<VertexBuffer, BfresError> {
        let d = self.data;
        if d.get(header..header + 4) != Some(b"FVTX") {
            return Err(BfresError::new(header, "missing FVTX signature"));
        }
        let attribute_array = read_u64(d, header + 8)? as usize;
        let sizes = read_u64(d, header + 48)? as usize;
        let strides = read_u64(d, header + 56)? as usize;
        let buffer_offset = read_u32(d, header + 72)? as usize;
        let attribute_count = d[header + 76] as usize;
        let buffer_count = d[header + 77] as usize;
        let gpu_alignment = read_u16(d, header + 86)?;
        let mut attributes = Vec::with_capacity(attribute_count);
        for index in 0..attribute_count {
            let a = attribute_array + index * 16;
            attributes.push(VertexAttrib {
                name: self.string_at(a)?,
                format: [d[a + 8], d[a + 9]],
                offset: read_u16(d, a + 12)?,
                buffer_index: read_u16(d, a + 14)?,
            });
        }
        let alignment = usize::from(gpu_alignment.max(1));
        let mut position = self.buffer_offset + buffer_offset;
        let mut buffers = Vec::with_capacity(buffer_count);
        for index in 0..buffer_count {
            let size = read_u32(d, sizes + index * 16)? as usize;
            let stride = read_u32(d, strides + index * 16)?;
            position = position.div_ceil(alignment) * alignment;
            buffers.push(VertexData {
                data: self.bytes(position, size)?,
                stride,
            });
            position += size;
        }
        Ok(VertexBuffer {
            flags: read_u32(d, header + 4)?,
            attributes,
            buffers,
            vertex_count: read_u32(d, header + 80)?,
            vertex_skin_count: read_u16(d, header + 84)?,
            gpu_alignment,
        })
    }

    fn shape(&self, header: usize, vertex_positions: &[usize]) -> Result<Shape, BfresError> {
        let d = self.data;
        if d.get(header..header + 4) != Some(b"FSHP") {
            return Err(BfresError::new(header, "missing FSHP signature"));
        }
        let vertex_pointer = read_u64(d, header + 16)? as usize;
        let mesh_array = read_u64(d, header + 24)? as usize;
        let skin_pointer = read_u64(d, header + 32)? as usize;
        let bounds_pointer = read_u64(d, header + 56)? as usize;
        let radius_pointer = read_u64(d, header + 64)? as usize;
        let skin_count = read_u16(d, header + 88)? as usize;
        let mesh_count = d[header + 91] as usize;
        let key_count = d[header + 92] as usize;
        if key_count != 0 {
            return Err(BfresError::new(header + 92, "key shapes are not supported"));
        }
        let vertex_buffer_index = vertex_positions
            .iter()
            .position(|position| *position == vertex_pointer)
            .map(|index| index as u16)
            .unwrap_or(read_u16(d, header + 86)?);
        let mut meshes = Vec::with_capacity(mesh_count);
        for index in 0..mesh_count {
            let m = mesh_array + index * 56;
            let submesh_array = read_u64(d, m)? as usize;
            let size_record = read_u64(d, m + 24)? as usize;
            let face_offset = read_u32(d, m + 32)? as usize;
            let submesh_count = read_u16(d, m + 52)? as usize;
            let size = read_u32(d, size_record)? as usize;
            let mut submeshes = Vec::with_capacity(submesh_count);
            for sub in 0..submesh_count {
                submeshes.push((
                    read_u32(d, submesh_array + sub * 8)?,
                    read_u32(d, submesh_array + sub * 8 + 4)?,
                ));
            }
            meshes.push(Mesh {
                submeshes,
                buffer_flag: read_u32(d, size_record + 4)?,
                data: self.bytes(self.buffer_offset + face_offset, size)?,
                primitive_type: read_u32(d, m + 36)?,
                index_format: read_u32(d, m + 40)?,
                index_count: read_u32(d, m + 44)?,
                first_vertex: read_u32(d, m + 48)?,
            });
        }
        let bounding_count: usize = meshes.iter().map(|mesh| mesh.submeshes.len() + 1).sum();
        let mut boundings = Vec::with_capacity(bounding_count);
        if bounds_pointer != 0 {
            for index in 0..bounding_count {
                let base = bounds_pointer + index * 24;
                let mut b = [0f32; 6];
                for (slot, value) in b.iter_mut().enumerate() {
                    *value = read_f32(d, base + slot * 4)?;
                }
                boundings.push(b);
            }
        }
        let radius_count = if skin_count == 0 {
            mesh_count
        } else {
            skin_count
        };
        let mut radius_list = Vec::with_capacity(radius_count);
        if radius_pointer != 0 && mesh_count > 0 {
            for index in 0..radius_count {
                let base = radius_pointer + index * 16;
                radius_list.push([
                    read_f32(d, base)?,
                    read_f32(d, base + 4)?,
                    read_f32(d, base + 8)?,
                    read_f32(d, base + 12)?,
                ]);
            }
        }
        Ok(Shape {
            flags: read_u32(d, header + 4)?,
            name: self.string_at(header + 8)?,
            vertex_buffer_index,
            meshes,
            skin_bone_indices: if skin_count != 0 && skin_pointer != 0 {
                self.u16_array(skin_pointer, skin_count)?
            } else {
                Vec::new()
            },
            boundings,
            radius_list,
            material_index: read_u16(d, header + 82)?,
            bone_index: read_u16(d, header + 84)?,
            vertex_skin_count: d[header + 90],
            target_attrib_count: d[header + 93],
        })
    }

    fn material(&self, header: usize) -> Result<Material, BfresError> {
        let d = self.data;
        if d.get(header..header + 4) != Some(b"FMAT") {
            return Err(BfresError::new(header, "missing FMAT signature"));
        }
        let shader_info = read_u64(d, header + 0x10)? as usize;
        let texture_names = read_u64(d, header + 0x20)? as usize;
        let sampler_array = read_u64(d, header + 0x30)? as usize;
        let sampler_dict = read_u64(d, header + 0x38)? as usize;
        let ri_data = read_u64(d, header + 0x40)? as usize;
        let ri_counts = read_u64(d, header + 0x48)? as usize;
        let ri_offsets = read_u64(d, header + 0x50)? as usize;
        let param_data = read_u64(d, header + 0x58)? as usize;
        let param_indices = read_u64(d, header + 0x60)? as usize;
        let user_data_count = read_u16(d, header + 0xa6)? as usize;
        let user_data = self.user_data(read_u64(d, header + 0x70)? as usize, user_data_count)?;
        let slots_a = read_u64(d, header + 0x90)? as usize;
        let slots_b = read_u64(d, header + 0x98)? as usize;
        let sampler_count = d[header + 0xa2] as usize;
        let texture_count = d[header + 0xa3] as usize;
        let render_info_size = read_u16(d, header + 0xa8)?;

        if shader_info == 0 {
            return Err(BfresError::new(
                header + 0x10,
                "material without shader info",
            ));
        }
        let assign = read_u64(d, shader_info)? as usize;
        let attr_strings = read_u64(d, shader_info + 8)? as usize;
        let attr_indices = read_u64(d, shader_info + 0x10)? as usize;
        let samp_strings = read_u64(d, shader_info + 0x18)? as usize;
        let samp_indices = read_u64(d, shader_info + 0x20)? as usize;
        let opt_toggles = read_u64(d, shader_info + 0x28)? as usize;
        let opt_strings = read_u64(d, shader_info + 0x30)? as usize;
        let opt_indices = read_u64(d, shader_info + 0x38)? as usize;
        let attr_count = d[shader_info + 0x44] as usize;
        let samp_count = d[shader_info + 0x45] as usize;
        let bool_count = read_u16(d, shader_info + 0x46)? as usize;
        let choice_count = read_u16(d, shader_info + 0x48)? as usize;

        let ri_list = read_u64(d, assign + 0x10)? as usize;
        let param_list = read_u64(d, assign + 0x20)? as usize;
        let attr_dict = self.dict_keys(read_u64(d, assign + 0x30)? as usize)?;
        let samp_dict = self.dict_keys(read_u64(d, assign + 0x38)? as usize)?;
        let opt_dict = self.dict_keys(read_u64(d, assign + 0x40)? as usize)?;
        let ri_count = read_u16(d, assign + 0x48)? as usize;
        let param_count = read_u16(d, assign + 0x4a)? as usize;
        let param_size = read_u16(d, assign + 0x4c)? as usize;

        let mut render_infos = Vec::with_capacity(ri_count);
        for index in 0..ri_count {
            let record = ri_list + index * 16;
            let name = self.string_at(record)?;
            let kind = d[record + 8];
            let count = read_u16(d, ri_counts + index * 2)? as usize;
            let offset = read_u16(d, ri_offsets + index * 2)? as usize;
            let base = ri_data + offset;
            let mut info = RenderInfo {
                name,
                kind,
                ..Default::default()
            };
            match kind {
                0 => {
                    for value in 0..count {
                        info.ints.push(read_u32(d, base + value * 4)? as i32);
                    }
                }
                1 => {
                    for value in 0..count {
                        info.floats.push(read_f32(d, base + value * 4)?);
                    }
                }
                _ => {
                    info.strings = self.strings_array(base, count)?;
                }
            }
            render_infos.push(info);
        }

        let mut shader_params = Vec::with_capacity(param_count);
        for index in 0..param_count {
            let record = param_list + index * 24;
            shader_params.push(ShaderParam {
                name: self.string_at(record + 8)?,
                data_offset: read_u16(d, record + 16)?,
                param_type: read_u16(d, record + 18)?,
            });
        }

        let attr_values = self.strings_array(attr_strings, attr_count)?;
        let samp_values = self.strings_array(samp_strings, samp_count)?;
        let attr_index_list = if attr_indices != 0 {
            Some(self.sbyte_indices(attr_indices, attr_count, attr_dict.len())?)
        } else {
            None
        };
        let samp_index_list = if samp_indices != 0 {
            Some(self.sbyte_indices(samp_indices, samp_count, samp_dict.len())?)
        } else {
            None
        };
        let mut toggles = Vec::with_capacity(bool_count);
        if opt_toggles != 0 {
            let flag_count = 1 + bool_count / 64;
            let mut flags = Vec::with_capacity(flag_count);
            for index in 0..flag_count {
                flags.push(read_u64(d, opt_toggles + index * 8)?);
            }
            for index in 0..bool_count {
                toggles.push(flags[index / 64] & (1u64 << (index % 64)) != 0);
            }
        }
        let opt_values = if opt_strings != 0 {
            self.strings_array(opt_strings, choice_count.saturating_sub(bool_count))?
        } else {
            Vec::new()
        };
        let opt_index_list = if opt_indices != 0 {
            Some(self.short_indices(opt_indices, choice_count, opt_dict.len())?)
        } else {
            None
        };

        let assign_pairs = |keys: &[String], values: &[String], indices: &Option<Vec<i64>>| {
            keys.iter()
                .enumerate()
                .map(|(i, key)| {
                    let idx = match indices {
                        Some(list) if !list.is_empty() => list[i],
                        _ => i as i64,
                    };
                    let value = if idx == -1 {
                        DEFAULT_VALUE.to_string()
                    } else {
                        values.get(idx as usize).cloned().unwrap_or_default()
                    };
                    (key.clone(), value)
                })
                .collect::<Vec<_>>()
        };
        let attrib_assign = assign_pairs(&attr_dict, &attr_values, &attr_index_list);
        let sampler_assign = assign_pairs(&samp_dict, &samp_values, &samp_index_list);
        let mut choices: Vec<String> = toggles
            .iter()
            .map(|t| {
                if *t {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            })
            .collect();
        choices.extend(opt_values);
        let options = assign_pairs(&opt_dict, &choices, &opt_index_list);

        let sampler_names = self.dict_keys(sampler_dict)?;
        let mut samplers = Vec::with_capacity(sampler_count);
        for index in 0..sampler_count {
            let raw = self.bytes(sampler_array + index * 32, 32)?;
            samplers.push(Sampler {
                name: sampler_names.get(index).cloned().unwrap_or_default(),
                raw: raw.try_into().unwrap(),
            });
        }

        let read_slots = |pointer: usize, count: usize| -> Result<Option<Vec<i64>>, BfresError> {
            if pointer == 0 {
                return Ok(None);
            }
            (0..count)
                .map(|index| read_u64(d, pointer + index * 8).map(|v| v as i64))
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        };

        Ok(Material {
            flags: read_u32(d, header + 4)?,
            name: self.string_at(header + 8)?,
            shader_archive: self.string_at(assign)?,
            shading_model: self.string_at(assign + 8)?,
            render_infos,
            shader_params,
            param_data: if param_data != 0 {
                self.bytes(param_data, param_size)?
            } else {
                Vec::new()
            },
            param_indices: if param_indices != 0 {
                Some(
                    (0..param_count)
                        .map(|index| read_u32(d, param_indices + index * 4).map(|v| v as i32))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            } else {
                None
            },
            attrib_assign,
            sampler_assign,
            options,
            texture_refs: if texture_names != 0 {
                self.strings_array(texture_names, texture_count)?
            } else {
                Vec::new()
            },
            samplers,
            // Syroot loads the +0x90 array as `TextureSlotArray` and the
            // +0x98 array as `SamplerSlotArray`.
            texture_slots: read_slots(slots_a, texture_count)?,
            sampler_slots: read_slots(slots_b, sampler_count)?,
            render_info_size,
            user_data,
        })
    }

    fn sbyte_indices(
        &self,
        pointer: usize,
        used: usize,
        total: usize,
    ) -> Result<Vec<i64>, BfresError> {
        let base = pointer + used;
        (0..total)
            .map(|index| {
                self.data
                    .get(base + index)
                    .map(|v| i64::from(*v as i8))
                    .ok_or_else(|| BfresError::new(base + index, "truncated index table"))
            })
            .collect()
    }

    fn short_indices(
        &self,
        pointer: usize,
        used: usize,
        total: usize,
    ) -> Result<Vec<i64>, BfresError> {
        let base = pointer + used * 2;
        (0..total)
            .map(|index| read_u16(self.data, base + index * 2).map(|v| i64::from(v as i16)))
            .collect()
    }
}

/// Parses a BFRES v10 file. `ext` resolves the 64-bit string keys of vanilla
/// TOTK models; pass an empty table for files whose strings are all local.
pub fn load(data: &[u8], ext: &ExternalStrings) -> Result<ResFile, BfresError> {
    if data.get(..4) != Some(b"FRES") {
        return Err(BfresError::new(0, "missing FRES signature"));
    }
    if data.get(10).copied() != Some(10) {
        return Err(BfresError::new(
            8,
            "Toolbox-compatible saving supports BFRES version 10 only",
        ));
    }
    if data.get(0xc..0xe) != Some(&[0xff, 0xfe]) {
        return Err(BfresError::new(0xc, "big-endian BFRES is not supported"));
    }
    let file_size = read_u32(data, 0x1c)?;
    let buffer_info = read_u64(data, 0xb0)? as usize;
    let external_flag = data[0xee];
    let buffer_offset = if buffer_info != 0 {
        read_u64(data, buffer_info + 8)? as usize
    } else if external_flag & 0x08 != 0 {
        file_size as usize + 288
    } else {
        0
    };
    let loader = Loader {
        data,
        ext,
        buffer_offset,
    };

    let block = read_u16(data, 0x16)? as usize;
    if data.get(block..block + 4) != Some(b"_STR") {
        return Err(BfresError::new(block, "missing _STR block"));
    }
    let string_count = read_u32(data, block + 0x10)? as usize;
    let mut original_strings = Vec::with_capacity(string_count + 1);
    let mut cursor = block + 0x14;
    for _ in 0..=string_count {
        let len = read_u16(data, cursor)? as usize;
        original_strings.push(read_res_string(data, cursor)?);
        cursor = (cursor + 2 + len + 1 + 1) & !1;
    }

    let model_count = read_u16(data, 0xdc)? as usize;
    let model_array = read_u64(data, 0x28)? as usize;
    let mut models = Vec::with_capacity(model_count);
    for index in 0..model_count {
        models.push(loader.model(model_array + index * 120)?);
    }

    let external_count = read_u16(data, 0xec)? as usize;
    let external_array = read_u64(data, 0xb8)? as usize;
    let external_names = loader.dict_keys(read_u64(data, 0xc0)? as usize)?;
    let mut external_files = Vec::with_capacity(external_count);
    for index in 0..external_count {
        let entry = external_array + index * 16;
        let offset = read_u64(data, entry)? as usize;
        let size = read_u64(data, entry + 8)? as usize;
        external_files.push(ExternalFile {
            name: external_names.get(index).cloned().unwrap_or_default(),
            data: if size != 0 {
                loader.bytes(offset, size)?
            } else {
                Vec::new()
            },
        });
    }

    Ok(ResFile {
        version: read_u32(data, 8)?,
        alignment: data[0xe],
        target_address_size: data[0xf],
        flag: read_u16(data, 0x14)?,
        name: loader.string_at(0x20)?,
        models,
        original_strings,
        external_flag,
        reserve10: data[0xef],
        source_file_size: file_size,
        has_buffer_info: buffer_info != 0 || external_flag & 0x08 != 0,
        external_files,
    })
}

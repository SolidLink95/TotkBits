//! Port of Syroot's `ResFileSwitchSaver` (NSW.Bfres 1.2.3 as bundled with
//! Switch Toolbox) restricted to what a BFRES version 10 model file needs:
//! models, skeletons, shapes, vertex buffers, v10 materials, embedded GPU
//! buffers, the string pool and the relocation table.
//!
//! The saver keeps the same deferred "item" queue, string pool ordering and
//! relocation bookkeeping as the C# implementation so the output is byte for
//! byte what Toolbox writes.

use super::super::BfresError;
use super::model::*;
use super::patricia::build_nodes;
use super::prepare::{prepare, Prepared, PreparedModel, ShaderInfoV10};
use std::collections::HashMap;

const SECTION_COUNT: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
enum DictId {
    Model,
    ModelUserData(usize),
    BoneUserData(usize, usize),
    MaterialUserData(usize, usize),
    Shape(usize),
    Material(usize),
    Bone(usize),
    Attribute(usize, usize),
    Sampler(usize, usize),
    RenderInfo(usize, usize),
    Param(usize, usize),
    AttribAssign(usize, usize),
    SamplerAssign(usize, usize),
    Options(usize, usize),
    ExternalFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CustomId {
    TexArr(usize, usize),
    TexNames(usize, usize),
    SampArr(usize, usize),
    RiData(usize, usize),
    RiCounts(usize, usize),
    RiOffsets(usize, usize),
    ParamData(usize, usize),
    ParamIdx(usize, usize),
    Volatile(usize, usize),
    SampSlots(usize, usize),
    TexSlots(usize, usize),
    AttrStrs(usize, usize),
    AttrIdx(usize, usize),
    SampStrs(usize, usize),
    SampIdx(usize, usize),
    OptToggles(usize, usize),
    OptStrs(usize, usize),
    OptIdx(usize, usize),
    RiList(usize, usize),
    ParamList(usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Obj {
    ShaderAssign(usize, usize),
    ShaderInfo(usize, usize),
    Sampler(usize, usize, usize),
    Dict(DictId),
    Custom(CustomId),
}

#[derive(Clone, Debug)]
struct Item {
    obj: Obj,
    offsets: Vec<usize>,
    target: Option<usize>,
    index: i32,
}

#[derive(Clone, Debug)]
struct StringEntry {
    value: String,
    offsets: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
struct RelocEntry {
    position: u32,
    offset_count: u32,
    struct_count: u32,
    padding_count: u32,
}

struct RelocSection {
    position: u32,
    size: u32,
    entry_index: u32,
    entries: Vec<RelocEntry>,
}

struct Saver<'a> {
    p: &'a Prepared,
    out: Vec<u8>,
    pos: usize,
    items: Vec<Item>,
    strings: Vec<StringEntry>,
    header_blocks: Vec<usize>,
    memory_pool_pointers: Vec<usize>,
    sections: [Vec<RelocEntry>; SECTION_COUNT],
    current_index: i32,
    file_name: String,
    ofs_file_size: usize,
    ofs_file_name: usize,
    ofs_string_pool: usize,
    ofs_total_buffer_size: usize,
    ofs_index_buffer: usize,
    ofs_vertex_buffer: usize,
    ofs_relocation_table: usize,
    ofs_end_of_string_table: usize,
    ofs_end_of_block: usize,
    ofs_file_name_string: usize,
    ofs_external_file_block: usize,
    buffer_info_offset: usize,
    memory_pool_offset: usize,
    // Offsets recorded by the Save methods (the `Pos*` fields in C#).
    model_offset: usize,
    model_dict_offset: usize,
    buffer_info_field: usize,
    external_file_offset: usize,
    external_file_dict_offset: usize,
    external_blocks: Vec<(usize, Vec<usize>)>,
    models: Vec<ModelPositions>,
    render_info_offsets: HashMap<(usize, usize), Vec<u16>>,
}

#[derive(Clone, Debug, Default)]
struct ModelPositions {
    skeleton: usize,
    vertex_buffers: usize,
    shapes: usize,
    shape_dict: usize,
    materials: usize,
    material_dict: usize,
    user_data: usize,
    user_data_dict: usize,
    material_user_data: Vec<(usize, usize)>,
    bone_user_data: Vec<(usize, usize)>,
    user_data_records: Vec<usize>,
    vertex: Vec<VertexPositions>,
    shape: Vec<ShapePositions>,
    skl: SkeletonPositions,
}

#[derive(Clone, Debug, Default)]
struct VertexPositions {
    position: usize,
    attributes: usize,
    attribute_dict: usize,
    unk: usize,
    unk2: usize,
    sizes: usize,
    strides: usize,
}

#[derive(Clone, Debug, Default)]
struct ShapePositions {
    meshes: usize,
    skin: usize,
    boundings: usize,
    radius: usize,
    mesh: Vec<(usize, usize, usize)>,
}

#[derive(Clone, Debug, Default)]
struct SkeletonPositions {
    bone_dict: usize,
    bones: usize,
    matrix_to_bone: usize,
    inverse: usize,
    user_pointer: usize,
}

pub fn save(file: &ResFile) -> Result<Vec<u8>, BfresError> {
    let prepared = prepare(file)?;
    let mut saver = Saver::new(&prepared);
    saver.execute()?;
    Ok(saver.out)
}

impl<'a> Saver<'a> {
    fn new(p: &'a Prepared) -> Self {
        Saver {
            p,
            out: Vec::new(),
            pos: 0,
            items: Vec::new(),
            strings: Vec::new(),
            header_blocks: Vec::new(),
            memory_pool_pointers: Vec::new(),
            sections: Default::default(),
            current_index: 0,
            file_name: p.file.name.clone(),
            ofs_file_size: 0,
            ofs_file_name: 0,
            ofs_string_pool: 0,
            ofs_total_buffer_size: 0,
            ofs_index_buffer: 0,
            ofs_vertex_buffer: 0,
            ofs_relocation_table: 0,
            ofs_end_of_string_table: 0,
            ofs_end_of_block: 0,
            ofs_file_name_string: 0,
            ofs_external_file_block: 0,
            buffer_info_offset: 0,
            memory_pool_offset: 0,
            model_offset: 0,
            model_dict_offset: 0,
            buffer_info_field: 0,
            external_file_offset: 0,
            external_file_dict_offset: 0,
            external_blocks: Vec::new(),
            models: vec![ModelPositions::default(); p.models.len()],
            render_info_offsets: HashMap::new(),
        }
    }

    // ---- low level writer --------------------------------------------------

    fn write(&mut self, bytes: &[u8]) {
        let end = self.pos + bytes.len();
        if self.out.len() < end {
            self.out.resize(end, 0);
        }
        self.out[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
    }
    fn u8(&mut self, v: u8) {
        self.write(&[v]);
    }
    fn u16(&mut self, v: u16) {
        self.write(&v.to_le_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.write(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.write(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.write(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.write(&v.to_le_bytes());
    }
    fn zeros(&mut self, count: usize) {
        let end = self.pos + count;
        if self.out.len() < end {
            self.out.resize(end, 0);
        }
        self.pos = end;
    }
    fn seek(&mut self, count: usize) {
        self.pos += count;
    }
    fn align(&mut self, alignment: usize) {
        if alignment > 1 {
            self.pos = self.pos.div_ceil(alignment) * alignment;
        }
    }
    fn write_u64_at(&mut self, offset: usize, value: u64) {
        let saved = self.pos;
        self.pos = offset;
        self.u64(value);
        self.pos = saved;
    }
    fn write_u32_at(&mut self, offset: usize, value: u32) {
        let saved = self.pos;
        self.pos = offset;
        self.u32(value);
        self.pos = saved;
    }
    fn write_u16_at(&mut self, offset: usize, value: u16) {
        let saved = self.pos;
        self.pos = offset;
        self.u16(value);
        self.pos = saved;
    }
    fn length(&self) -> usize {
        self.out.len().max(self.pos)
    }

    // ---- saver primitives ---------------------------------------------------

    /// `WriteOffset`: stores the current position into the pointer field.
    fn write_offset(&mut self, field: usize) {
        let target = self.pos as u64;
        self.write_u64_at(field, target);
    }

    fn save_offset(&mut self) -> usize {
        let field = self.pos;
        self.u64(0);
        field
    }

    fn reloc(
        &mut self,
        position: usize,
        offset_count: u32,
        struct_count: u32,
        padding: u32,
        section: usize,
    ) {
        if offset_count > 255 {
            self.reloc(position, 255, struct_count, padding, section);
            self.reloc(
                position + 255 * 8,
                offset_count - 255,
                struct_count,
                padding,
                section,
            );
            return;
        }
        self.sections[section - 1].push(RelocEntry {
            position: position as u32,
            offset_count,
            struct_count,
            padding_count: padding,
        });
    }

    fn save_string(&mut self, value: &str) {
        let field = self.pos;
        match self.strings.iter_mut().find(|entry| entry.value == value) {
            Some(entry) => entry.offsets.push(field),
            None => self.strings.push(StringEntry {
                value: value.to_owned(),
                offsets: vec![field],
            }),
        }
        self.u64(0);
    }

    fn save_strings_relocated(&mut self, values: &[String]) {
        self.reloc(self.pos, values.len() as u32, 1, 0, 1);
        for value in values {
            self.save_string(value);
        }
    }

    fn save_memory_pool_pointer(&mut self) {
        self.memory_pool_pointers.push(self.pos);
        self.u64(0);
    }

    fn save_header_block(&mut self, binary: bool) {
        self.header_blocks.push(self.pos);
        if binary {
            self.u16(0);
        } else {
            self.u32(0);
            self.u64(0);
        }
    }

    fn find_item(&mut self, obj: &Obj) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.obj == *obj)
    }

    /// `Save(IResData)` for an object that is queued once and referenced by
    /// every later pointer.
    fn save_ref(&mut self, obj: Obj) {
        let field = self.pos;
        match self.find_item(&obj) {
            Some(item) => {
                item.offsets.push(field);
                item.index = -1;
            }
            None => self.items.push(Item {
                obj,
                offsets: vec![field],
                target: None,
                index: -1,
            }),
        }
        self.u64(0);
    }

    /// `SaveList` for `count` elements created by `make`.
    fn save_list(&mut self, count: usize, make: impl Fn(usize) -> Obj) {
        if count == 0 {
            self.u64(0);
            return;
        }
        let first = make(0);
        let field = self.pos;
        if let Some(item) = self.find_item(&first) {
            item.offsets.push(field);
            item.index = 0;
        } else {
            for index in 0..count {
                self.items.push(Item {
                    obj: make(index),
                    offsets: if index == 0 { vec![field] } else { Vec::new() },
                    target: None,
                    index: index as i32,
                });
            }
        }
        self.u64(0);
    }

    fn save_dict(&mut self, id: DictId, count: usize) {
        if count == 0 {
            self.u64(0);
            return;
        }
        let field = self.pos;
        let obj = Obj::Dict(id);
        match self.find_item(&obj) {
            Some(item) => item.offsets.push(field),
            None => self.items.push(Item {
                obj,
                offsets: vec![field],
                target: None,
                index: -1,
            }),
        }
        self.u64(0);
    }

    fn save_custom(&mut self, id: CustomId) {
        let field = self.pos;
        let obj = Obj::Custom(id);
        match self.find_item(&obj) {
            Some(item) => item.offsets.push(field),
            None => self.items.push(Item {
                obj,
                offsets: vec![field],
                target: None,
                index: -1,
            }),
        }
        self.u64(0);
    }

    fn save_entries(&mut self) -> Result<(), BfresError> {
        let mut i = 0;
        while i < self.items.len() {
            if self.items[i].target.is_some() {
                i += 1;
                continue;
            }
            self.align(8);
            let target = self.pos;
            self.items[i].target = Some(target);
            self.current_index = self.items[i].index;
            let obj = self.items[i].obj.clone();
            match obj {
                Obj::ShaderAssign(m, rep) => self.write_shader_assign(m, rep)?,
                Obj::ShaderInfo(m, mat) => self.write_shader_info(m, mat)?,
                Obj::Sampler(m, mat, index) => {
                    let raw = self.p.models[m].model.materials[mat].samplers[index].raw;
                    self.write(&raw);
                }
                Obj::Dict(id) => self.write_dict(&id)?,
                Obj::Custom(id) => self.write_custom(&id)?,
            }
            i += 1;
        }
        self.write_offsets();
        self.items.clear();
        Ok(())
    }

    fn write_offsets(&mut self) {
        let patches: Vec<(usize, usize)> = self
            .items
            .iter()
            .filter_map(|item| item.target.map(|target| (target, item.offsets.clone())))
            .flat_map(|(target, offsets)| offsets.into_iter().map(move |offset| (offset, target)))
            .collect();
        for (offset, target) in patches {
            self.write_u64_at(offset, target as u64);
        }
    }

    // ---- dictionaries --------------------------------------------------------

    fn dict_keys(&self, id: &DictId) -> Vec<String> {
        let p = self.p;
        match id {
            DictId::Model => p.models.iter().map(|m| m.model.name.clone()).collect(),
            DictId::ModelUserData(m) => p.models[*m]
                .model
                .user_data
                .iter()
                .map(|u| u.name.clone())
                .collect(),
            DictId::BoneUserData(m, b) => p.models[*m].model.skeleton.bones[*b]
                .user_data
                .iter()
                .map(|u| u.name.clone())
                .collect(),
            DictId::MaterialUserData(m, mat) => p.models[*m].model.materials[*mat]
                .user_data
                .iter()
                .map(|u| u.name.clone())
                .collect(),
            DictId::Shape(m) => p.models[*m]
                .model
                .shapes
                .iter()
                .map(|s| s.name.clone())
                .collect(),
            DictId::Material(m) => p.models[*m]
                .model
                .materials
                .iter()
                .map(|s| s.name.clone())
                .collect(),
            DictId::Bone(m) => p.models[*m]
                .model
                .skeleton
                .bones
                .iter()
                .map(|b| b.name.clone())
                .collect(),
            DictId::Attribute(m, v) => p.models[*m].model.vertex_buffers[*v]
                .attributes
                .iter()
                .map(|a| a.name.clone())
                .collect(),
            DictId::Sampler(m, mat) => p.models[*m].model.materials[*mat]
                .samplers
                .iter()
                .map(|s| s.name.clone())
                .collect(),
            DictId::RenderInfo(m, mat) => p.models[*m].model.materials[*mat]
                .render_infos
                .iter()
                .map(|r| r.name.clone())
                .collect(),
            DictId::Param(m, mat) => p.models[*m].model.materials[*mat]
                .shader_params
                .iter()
                .map(|r| r.name.clone())
                .collect(),
            DictId::AttribAssign(m, mat) => p.models[*m].model.materials[*mat]
                .attrib_assign
                .iter()
                .map(|(k, _)| k.clone())
                .collect(),
            DictId::SamplerAssign(m, mat) => p.models[*m].model.materials[*mat]
                .sampler_assign
                .iter()
                .map(|(k, _)| k.clone())
                .collect(),
            DictId::Options(m, mat) => p.models[*m].model.materials[*mat]
                .options
                .iter()
                .map(|(k, _)| k.clone())
                .collect(),
            DictId::ExternalFile => p
                .file
                .external_files
                .iter()
                .map(|f| f.name.clone())
                .collect(),
        }
    }

    /// `ResDict.Save` (Switch): rebuilds the Patricia trie then writes nodes.
    fn write_dict(&mut self, id: &DictId) -> Result<(), BfresError> {
        let keys = self.dict_keys(id);
        let nodes = build_nodes(&keys);
        self.u32(0);
        self.u32(keys.len() as u32);
        for (index, node) in nodes.iter().enumerate() {
            self.u32(node.reference);
            self.u16(node.left);
            self.u16(node.right);
            if index == 0 {
                self.reloc(self.pos, 1, nodes.len() as u32, 1, 1);
                self.save_string("");
            } else {
                let key = node.key.clone().unwrap_or_default();
                self.save_string(&key);
            }
        }
        Ok(())
    }

    // ---- ResFile ---------------------------------------------------------------

    fn execute(&mut self) -> Result<(), BfresError> {
        let file = &self.p.file;
        let has_memory_pool = !self.p.models.is_empty();
        let has_buffer_info = file.has_buffer_info;

        // ResFile.Save
        self.write(b"FRES");
        self.u32(0x20202020);
        self.u32(file.version);
        self.write(&[0xff, 0xfe]);
        self.u8(file.alignment);
        self.u8(file.target_address_size);
        self.ofs_file_name = self.pos;
        self.u32(0);
        self.u16(file.flag);
        self.save_header_block(true);
        self.ofs_relocation_table = self.pos;
        self.u32(0);
        self.ofs_file_size = self.pos;
        self.u32(0);
        self.reloc(self.pos, 17, 1, 0, 1);
        let name = file.name.clone();
        self.save_string(&name);
        self.model_offset = self.save_offset();
        self.model_dict_offset = self.save_offset();
        self.zeros(32);
        for _ in 0..10 {
            self.save_offset();
        }
        if has_memory_pool {
            self.reloc(self.pos, 1, 1, 0, 4);
        }
        self.save_memory_pool_pointer();
        if has_buffer_info {
            self.reloc(self.pos, 1, 1, 0, 1);
        }
        self.buffer_info_field = self.save_offset();
        if !file.external_files.is_empty() {
            self.reloc(self.pos, 2, 1, 0, 1);
        }
        self.external_file_offset = self.save_offset();
        self.external_file_dict_offset = self.save_offset();
        self.u64(0);
        self.reloc(self.pos, 1, 1, 0, 1);
        self.ofs_string_pool = self.pos;
        self.u64(0);
        self.u32(0);
        self.u16(self.p.models.len() as u16);
        self.u16(0);
        self.u16(0);
        for _ in 0..5 {
            self.u16(0);
        }
        self.u16(file.external_files.len() as u16);
        self.u8(0);
        self.u8(1);
        if self.pos != 0xf0 {
            return Err(BfresError::new(self.pos, "unexpected BFRES header size"));
        }

        // Model headers.
        if !self.p.models.is_empty() {
            self.write_offset(self.model_offset);
            for m in 0..self.p.models.len() {
                self.write_model_header(m)?;
            }
        }
        // Buffer info, or Syroot's default block when there is none.
        if has_buffer_info {
            self.write_offset(self.buffer_info_field);
            self.u32(0);
            self.ofs_total_buffer_size = self.pos;
            self.u32(0);
            self.reloc(self.pos, 1, 1, 0, 2);
            self.ofs_index_buffer = self.pos;
            self.u64(0);
            self.seek(16);
        } else {
            self.u32(0x22);
            self.seek(28);
        }
        // External file headers.
        if !file.external_files.is_empty() {
            self.write_offset(self.external_file_offset);
            for (index, external) in file.external_files.iter().enumerate() {
                self.reloc(self.pos, 1, 1, 0, 5);
                let field = self.pos;
                self.u64(0);
                self.u64(external.data.len() as u64);
                self.external_blocks.push((index, vec![field]));
            }
        }
        // Dictionaries of the root.
        if !self.p.models.is_empty() {
            self.write_offset(self.model_dict_offset);
            self.write_dict(&DictId::Model)?;
        }
        if !file.external_files.is_empty() {
            self.write_offset(self.external_file_dict_offset);
            self.write_dict(&DictId::ExternalFile)?;
        }

        for m in 0..self.p.models.len() {
            self.write_model(m)?;
        }
        for m in 0..self.p.models.len() {
            self.write_model_block(m)?;
        }

        self.write_strings();

        if has_buffer_info {
            self.write_index_buffer();
            self.write_vertex_buffer_data();
        }
        if has_memory_pool {
            self.write_memory_pool();
        }
        self.write_blocks();

        let sections = self.setup_relocation_table();
        self.write_relocation_table(&sections);

        // Header block fix-ups.
        let blocks = self.header_blocks.clone();
        for (i, position) in blocks.iter().enumerate() {
            if i == 0 {
                self.write_u16_at(*position, (blocks[1] - 4) as u16);
            } else if i == blocks.len() - 1 {
                self.write_u32_at(*position, 0);
                self.write_u64_at(*position + 4, (self.ofs_end_of_block - position) as u64);
            } else {
                let size = (blocks[i + 1] - position) as u32;
                self.write_u32_at(*position, size);
                self.write_u64_at(*position + 4, u64::from(size));
            }
        }
        self.write_u32_at(self.ofs_file_name, (self.ofs_file_name_string + 2) as u32);
        let length = self.length() as u32;
        self.write_u32_at(self.ofs_file_size, length);
        self.out.resize(self.length(), 0);
        Ok(())
    }

    // ---- Model ---------------------------------------------------------------

    fn write_model_header(&mut self, m: usize) -> Result<(), BfresError> {
        let pm = &self.p.models[m];
        let model = &pm.model;
        self.write(b"FMDL");
        self.u32(model.flags);
        self.reloc(self.pos, 11, 1, 0, 1);
        let name = model.name.clone();
        let path = model.path.clone();
        self.save_string(&name);
        self.save_string(&path);
        let mut positions = ModelPositions::default();
        positions.skeleton = self.save_offset();
        positions.vertex_buffers = self.save_offset();
        positions.shapes = self.save_offset();
        positions.shape_dict = self.save_offset();
        positions.materials = self.save_offset();
        positions.material_dict = self.save_offset();
        let assign_count = pm.shader_assigns.len();
        self.save_list(assign_count, |index| Obj::ShaderAssign(m, index));
        positions.user_data = self.save_offset();
        positions.user_data_dict = self.save_offset();
        self.u64(0);
        self.u16(model.vertex_buffers.len() as u16);
        self.u16(model.shapes.len() as u16);
        self.u16(model.materials.len() as u16);
        self.u16(assign_count as u16);
        self.u16(model.user_data.len() as u16);
        self.u16(0);
        self.u32(0);
        positions.vertex = vec![VertexPositions::default(); model.vertex_buffers.len()];
        positions.shape = model
            .shapes
            .iter()
            .map(|shape| ShapePositions {
                mesh: vec![(0, 0, 0); shape.meshes.len()],
                ..Default::default()
            })
            .collect();
        self.models[m] = positions;
        Ok(())
    }

    fn write_model(&mut self, m: usize) -> Result<(), BfresError> {
        let model = &self.p.models[m].model;
        if !model.vertex_buffers.is_empty() {
            self.write_offset(self.models[m].vertex_buffers);
            for v in 0..model.vertex_buffers.len() {
                self.current_index = v as i32;
                self.write_vertex_buffer_header(m, v)?;
            }
        }
        if !model.materials.is_empty() {
            self.write_offset(self.models[m].materials);
            for mat in 0..model.materials.len() {
                self.current_index = mat as i32;
                self.write_material_header(m, mat)?;
            }
        }
        if !model.shapes.is_empty() {
            self.write_offset(self.models[m].shapes);
            for s in 0..model.shapes.len() {
                self.current_index = s as i32;
                self.write_shape_header(m, s)?;
            }
        }
        if !model.user_data.is_empty() {
            let target = self.models[m].user_data;
            let records = self.save_user_data(&model.user_data, target);
            self.models[m].user_data_records = records;
        }
        self.write_offset(self.models[m].skeleton);
        self.write_skeleton_header(m)?;
        Ok(())
    }

    /// `SaveUserData`: the 32-byte records; returns the data pointer fields.
    fn save_user_data(&mut self, list: &[UserData], target: usize) -> Vec<usize> {
        self.align(8);
        self.reloc(self.pos, 2, list.len() as u32, 6, 1);
        self.write_offset(target);
        let mut fields = Vec::with_capacity(list.len());
        for entry in list {
            let name = entry.name.clone();
            self.save_string(&name);
            fields.push(self.save_offset());
            self.i32(entry.count() as i32);
            self.u8(entry.kind);
            self.seek(43);
        }
        fields
    }

    /// `SaveUserDataData`: the values, grouped by type.
    fn save_user_data_data(&mut self, list: &[UserData], fields: &[usize]) {
        let string_count: usize = list
            .iter()
            .filter(|u| u.kind == 2 || u.kind == 3)
            .map(|u| u.strings.len())
            .sum();
        if string_count != 0 {
            self.reloc(self.pos, string_count as u32, 1, 0, 1);
        }
        for kind in [2u8, 3, 0, 1, 4] {
            for (entry, field) in list.iter().zip(fields) {
                if entry.kind != kind || entry.count() == 0 {
                    continue;
                }
                self.write_offset(*field);
                match kind {
                    2 | 3 => {
                        for value in entry.strings.clone() {
                            self.save_string(&value);
                        }
                        self.align(8);
                    }
                    0 => {
                        for value in entry.ints.clone() {
                            self.i32(value);
                        }
                        self.align(8);
                    }
                    1 => {
                        for value in entry.floats.clone() {
                            self.f32(value);
                        }
                        self.align(8);
                    }
                    _ => {
                        let bytes = entry.bytes.clone();
                        self.write(&bytes);
                    }
                }
            }
        }
    }

    fn write_model_block(&mut self, m: usize) -> Result<(), BfresError> {
        let model = &self.p.models[m].model;
        if !model.skeleton.bones.is_empty() {
            self.reloc(self.pos, 3, model.skeleton.bones.len() as u32, 8, 1);
        }
        self.write_skeleton(m)?;
        if !model.shapes.is_empty() {
            self.write_offset(self.models[m].shape_dict);
            self.write_dict(&DictId::Shape(m))?;
        }
        if !model.materials.is_empty() {
            self.write_offset(self.models[m].material_dict);
            self.write_dict(&DictId::Material(m))?;
        }
        if !model.user_data.is_empty() {
            self.write_offset(self.models[m].user_data_dict);
            self.write_dict(&DictId::ModelUserData(m))?;
            let fields = self.models[m].user_data_records.clone();
            self.save_user_data_data(&model.user_data, &fields);
            self.align(8);
        }
        for s in 0..model.shapes.len() {
            self.write_shape(m, s)?;
        }
        for v in 0..model.vertex_buffers.len() {
            self.write_vertex_buffer(m, v)?;
        }
        for mat in 0..model.materials.len() {
            self.save_entries()?;
            let material = &self.p.models[m].model.materials[mat];
            if !material.user_data.is_empty() {
                let (target, dict_target) = self.models[m].material_user_data[mat];
                let fields = self.save_user_data(&material.user_data, target);
                self.write_offset(dict_target);
                self.write_dict(&DictId::MaterialUserData(m, mat))?;
                self.save_user_data_data(&material.user_data, &fields);
                self.align(8);
            }
        }
        Ok(())
    }

    // ---- Skeleton ------------------------------------------------------------

    fn write_skeleton_header(&mut self, m: usize) -> Result<(), BfresError> {
        let skeleton = &self.p.models[m].model.skeleton;
        self.write(b"FSKL");
        self.u32(skeleton.flags);
        self.reloc(self.pos, 4, 1, 0, 1);
        let mut skl = SkeletonPositions::default();
        skl.bone_dict = self.save_offset();
        skl.bones = self.save_offset();
        skl.matrix_to_bone = self.save_offset();
        skl.inverse = self.save_offset();
        self.seek(8);
        self.reloc(self.pos, 1, 1, 0, 1);
        skl.user_pointer = self.save_offset();
        self.u16(skeleton.bones.len() as u16);
        self.u16(skeleton.inverse_matrices.len() as u16);
        self.u16(
            (skeleton.matrix_to_bone.len() as i32 - skeleton.inverse_matrices.len() as i32) as u16,
        );
        self.seek(2);
        self.models[m].skl = skl;
        Ok(())
    }

    fn write_skeleton(&mut self, m: usize) -> Result<(), BfresError> {
        let skeleton = &self.p.models[m].model.skeleton;
        let skl = self.models[m].skl.clone();
        if !skeleton.bones.is_empty() {
            self.write_offset(skl.bones);
            for (index, bone) in skeleton.bones.iter().enumerate() {
                self.current_index = index as i32;
                let name = bone.name.clone();
                self.save_string(&name);
                let user_data = self.save_offset();
                let user_data_dict = self.save_offset();
                self.models[m]
                    .bone_user_data
                    .push((user_data, user_data_dict));
                self.seek(8);
                self.u16(index as u16);
                self.i16(bone.parent_index);
                self.i16(bone.smooth_matrix_index);
                self.i16(bone.rigid_matrix_index);
                self.i16(bone.billboard_index);
                self.u16(bone.user_data.len() as u16);
                self.u32(bone.flags);
                for v in bone.scale {
                    self.f32(v);
                }
                for v in bone.rotation {
                    self.f32(v);
                }
                for v in bone.position {
                    self.f32(v);
                }
            }
        }
        if !skeleton.matrix_to_bone.is_empty() {
            self.align(8);
            self.write_offset(skl.matrix_to_bone);
            for v in &skeleton.matrix_to_bone {
                self.u16(*v);
            }
        }
        if !skeleton.inverse_matrices.is_empty() {
            self.align(8);
            self.write_offset(skl.inverse);
            for matrix in &skeleton.inverse_matrices {
                for v in matrix {
                    self.f32(*v);
                }
            }
        }
        if !skeleton.mirrored_bones.is_empty() {
            self.align(8);
            self.write_offset(skl.user_pointer);
            for v in &skeleton.mirrored_bones {
                self.u16(*v);
            }
        }
        if !skeleton.bones.is_empty() {
            self.write_offset(skl.bone_dict);
            self.write_dict(&DictId::Bone(m))?;
        }
        for (b, bone) in skeleton.bones.iter().enumerate() {
            if bone.user_data.is_empty() {
                continue;
            }
            let (target, dict_target) = self.models[m].bone_user_data[b];
            let fields = self.save_user_data(&bone.user_data, target);
            self.write_offset(dict_target);
            self.write_dict(&DictId::BoneUserData(m, b))?;
            self.save_user_data_data(&bone.user_data, &fields);
            self.align(8);
        }
        Ok(())
    }

    // ---- Vertex buffers ------------------------------------------------------

    fn write_vertex_buffer_header(&mut self, m: usize, v: usize) -> Result<(), BfresError> {
        let vertex = &self.p.models[m].model.vertex_buffers[v];
        let mut vp = VertexPositions::default();
        vp.position = self.pos;
        self.write(b"FVTX");
        self.u32(vertex.flags);
        self.reloc(self.pos, 2, 1, 0, 1);
        vp.attributes = self.save_offset();
        vp.attribute_dict = self.save_offset();
        self.reloc(self.pos, 1, 1, 0, 4);
        self.save_memory_pool_pointer();
        self.reloc(self.pos, 4, 1, 0, 1);
        vp.unk = self.save_offset();
        vp.unk2 = self.save_offset();
        vp.sizes = self.save_offset();
        vp.strides = self.save_offset();
        self.u64(0);
        let array_offset = self.vertex_buffer_array_offset(m, v);
        self.u32(array_offset);
        self.u8(vertex.attributes.len() as u8);
        self.u8(vertex.buffers.len() as u8);
        self.u16(self.current_index as u16);
        self.u32(vertex.vertex_count);
        self.u16(vertex.vertex_skin_count);
        self.u16(vertex.gpu_alignment);
        self.models[m].vertex[v] = vp;
        Ok(())
    }

    /// `VertexBufferParser.SetVertexBufferArrayOffset`, relative to the
    /// GPU region the file was loaded from.
    fn vertex_buffer_array_offset(&self, m: usize, v: usize) -> u32 {
        let base = u64::from(self.p.file.source_file_size) + 288;
        let mut total = base;
        for pm in &self.p.models {
            for shape in &pm.model.shapes {
                for mesh in &shape.meshes {
                    if total % 8 != 0 {
                        total += 8 - total % 8;
                    }
                    total += mesh.data.len() as u64;
                }
            }
        }
        for (mi, pm) in self.p.models.iter().enumerate() {
            for (vi, vertex) in pm.model.vertex_buffers.iter().enumerate() {
                for buffer in &vertex.buffers {
                    if total % 8 != 0 {
                        total += 8 - total % 8;
                    }
                    if mi == m && vi == v {
                        return (total - base) as u32;
                    }
                    total += buffer.data.len() as u64;
                }
            }
        }
        (total - base) as u32
    }

    fn write_vertex_buffer(&mut self, m: usize, v: usize) -> Result<(), BfresError> {
        let vertex = &self.p.models[m].model.vertex_buffers[v];
        let vp = self.models[m].vertex[v].clone();
        if !vertex.attributes.is_empty() {
            self.reloc(self.pos, 1, vertex.attributes.len() as u32, 1, 1);
            self.write_offset(vp.attributes);
            for attribute in &vertex.attributes {
                let name = attribute.name.clone();
                self.save_string(&name);
                self.write(&attribute.format);
                self.i16(0);
                self.u16(attribute.offset);
                self.u16(attribute.buffer_index);
            }
        }
        if !vertex.buffers.is_empty() {
            self.write_offset(vp.unk);
            self.zeros(vertex.buffers.len() * 9 * 8);
            self.write_offset(vp.sizes);
            for buffer in &vertex.buffers {
                self.u32(buffer.data.len() as u32);
                self.u32(0);
                self.seek(8);
            }
            self.write_offset(vp.strides);
            for buffer in &vertex.buffers {
                self.u32(buffer.stride);
                self.seek(12);
            }
            self.write_offset(vp.unk2);
            self.zeros(vertex.buffers.len() * 8);
        }
        if !vertex.attributes.is_empty() {
            self.write_offset(vp.attribute_dict);
            self.write_dict(&DictId::Attribute(m, v))?;
        }
        Ok(())
    }

    // ---- Shapes --------------------------------------------------------------

    fn radius_list(shape: &Shape) -> Vec<[f32; 4]> {
        let bounding_count = if shape.skin_bone_indices.is_empty() {
            shape.meshes.len()
        } else {
            shape.skin_bone_indices.len()
        };
        if bounding_count != shape.radius_list.len() {
            let max = shape
                .radius_list
                .iter()
                .map(|r| r[3])
                .fold(f32::MIN, f32::max);
            let max = if shape.radius_list.is_empty() {
                0.0
            } else {
                max
            };
            return vec![[0.0, 0.0, 0.0, max]; bounding_count];
        }
        shape.radius_list.clone()
    }

    fn write_shape_header(&mut self, m: usize, s: usize) -> Result<(), BfresError> {
        let shape = &self.p.models[m].model.shapes[s];
        self.write(b"FSHP");
        self.u32(shape.flags);
        self.reloc(self.pos, 8, 1, 0, 1);
        let name = shape.name.clone();
        self.save_string(&name);
        let vertex_position =
            self.models[m].vertex[usize::from(shape.vertex_buffer_index)].position;
        self.u64(vertex_position as u64);
        let mut sp = ShapePositions {
            mesh: vec![(0, 0, 0); shape.meshes.len()],
            ..Default::default()
        };
        sp.meshes = self.save_offset();
        sp.skin = self.save_offset();
        self.save_offset();
        self.save_offset();
        sp.boundings = self.save_offset();
        sp.radius = self.save_offset();
        self.u64(0);
        self.u16(self.current_index as u16);
        self.u16(shape.material_index);
        self.u16(shape.bone_index);
        self.u16(shape.vertex_buffer_index);
        self.u16(shape.skin_bone_indices.len() as u16);
        self.u8(shape.vertex_skin_count);
        self.u8(shape.meshes.len() as u8);
        self.u8(0);
        self.u8(shape.target_attrib_count);
        self.seek(2);
        self.models[m].shape[s] = sp;
        Ok(())
    }

    fn face_buffer_offset(&self, m: usize, s: usize, mesh_index: usize) -> u32 {
        let mut total = 0u32;
        for (mi, pm) in self.p.models.iter().enumerate() {
            for (si, shape) in pm.model.shapes.iter().enumerate() {
                for (ki, mesh) in shape.meshes.iter().enumerate() {
                    if total % 8 != 0 {
                        total += 8 - total % 8;
                    }
                    if mi == m && si == s && ki == mesh_index {
                        return total;
                    }
                    total += mesh.data.len() as u32;
                }
            }
        }
        total
    }

    fn write_shape(&mut self, m: usize, s: usize) -> Result<(), BfresError> {
        let shape = &self.p.models[m].model.shapes[s];
        let mut sp = self.models[m].shape[s].clone();
        self.write_offset(sp.meshes);
        for (index, mesh) in shape.meshes.iter().enumerate() {
            self.reloc(self.pos, 1, 1, 0, 1);
            let submeshes = self.save_offset();
            self.reloc(self.pos, 1, 1, 0, 4);
            self.save_memory_pool_pointer();
            self.reloc(self.pos, 2, 1, 0, 1);
            let unk = self.save_offset();
            let size = self.save_offset();
            let face_offset = self.face_buffer_offset(m, s, index);
            self.u32(face_offset);
            self.u32(mesh.primitive_type);
            self.u32(mesh.index_format);
            self.u32(mesh.index_count);
            self.u32(mesh.first_vertex);
            self.u16(mesh.submeshes.len() as u16);
            self.seek(2);
            sp.mesh[index] = (submeshes, unk, size);
        }
        if !shape.skin_bone_indices.is_empty() {
            self.write_offset(sp.skin);
            for v in &shape.skin_bone_indices {
                self.u16(*v);
            }
        }
        if !shape.boundings.is_empty() {
            self.align(8);
            self.write_offset(sp.boundings);
            for bounding in &shape.boundings {
                for v in bounding {
                    self.f32(*v);
                }
            }
        }
        if !shape.radius_list.is_empty() && !shape.meshes.is_empty() {
            self.write_offset(sp.radius);
            for radius in Self::radius_list(shape) {
                for v in radius {
                    self.f32(v);
                }
            }
        }
        for (index, mesh) in shape.meshes.iter().enumerate() {
            let (submeshes, unk, size) = sp.mesh[index];
            self.align(8);
            self.write_offset(submeshes);
            for (offset, count) in &mesh.submeshes {
                self.u32(*offset);
                self.u32(*count);
            }
            self.write_offset(unk);
            self.zeros(9 * 8);
            self.write_offset(size);
            self.u32(mesh.data.len() as u32);
            self.u32(0);
            self.seek(8);
        }
        self.models[m].shape[s] = sp;
        Ok(())
    }

    // ---- Materials -----------------------------------------------------------

    fn write_material_header(&mut self, m: usize, mat: usize) -> Result<(), BfresError> {
        let material = &self.p.models[m].model.materials[mat];
        self.write(b"FMAT");
        self.u32(material.flags);
        self.reloc(self.pos, 12, 1, 0, 1);
        let name = material.name.clone();
        self.save_string(&name);
        self.save_ref(Obj::ShaderInfo(m, mat));
        self.save_custom(CustomId::TexArr(m, mat));
        self.save_custom(CustomId::TexNames(m, mat));
        self.save_custom(CustomId::SampArr(m, mat));
        let sampler_count = material.samplers.len();
        self.save_list(sampler_count, |index| Obj::Sampler(m, mat, index));
        self.save_dict(DictId::Sampler(m, mat), sampler_count);
        self.save_custom(CustomId::RiData(m, mat));
        self.save_custom(CustomId::RiCounts(m, mat));
        self.save_custom(CustomId::RiOffsets(m, mat));
        self.save_custom(CustomId::ParamData(m, mat));
        if material.param_indices.is_some() {
            self.save_custom(CustomId::ParamIdx(m, mat));
        } else {
            self.u64(0);
        }
        self.u64(0);
        self.reloc(self.pos, 3, 1, 0, 1);
        let user_data = self.save_offset();
        let user_data_dict = self.save_offset();
        self.models[m]
            .material_user_data
            .push((user_data, user_data_dict));
        self.save_custom(CustomId::Volatile(m, mat));
        self.u64(0);
        self.reloc(self.pos, 2, 1, 0, 1);
        if material.sampler_slots.is_some() {
            self.save_custom(CustomId::SampSlots(m, mat));
        } else {
            self.u64(0);
        }
        if material.texture_slots.is_some() {
            self.save_custom(CustomId::TexSlots(m, mat));
        } else {
            self.u64(0);
        }
        self.u16(self.current_index as u16);
        self.u8(material.texture_refs.len() as u8);
        self.u8(material.samplers.len() as u8);
        self.u16(0);
        self.u16(material.user_data.len() as u16);
        self.u16(material.render_info_size);
        self.u16(0);
        self.u16(0);
        self.u16(0);
        Ok(())
    }

    fn write_shader_info(&mut self, m: usize, mat: usize) -> Result<(), BfresError> {
        let info: &ShaderInfoV10 = &self.p.models[m].infos[mat];
        let rep = info.assign;
        self.reloc(self.pos, 8, 1, 0, 1);
        self.save_ref(Obj::ShaderAssign(m, rep));
        self.save_custom(CustomId::AttrStrs(m, mat));
        if info.attribute_indices.is_some() {
            self.save_custom(CustomId::AttrIdx(m, mat));
        } else {
            self.u64(0);
        }
        self.save_custom(CustomId::SampStrs(m, mat));
        if info.sampler_indices.is_some() {
            self.save_custom(CustomId::SampIdx(m, mat));
        } else {
            self.u64(0);
        }
        self.save_custom(CustomId::OptToggles(m, mat));
        self.save_custom(CustomId::OptStrs(m, mat));
        self.save_custom(CustomId::OptIdx(m, mat));
        self.u32(0);
        self.u8(info.attrib_assigns.len() as u8);
        self.u8(info.sampler_assigns.len() as u8);
        self.u16(info.option_toggles.len() as u16);
        self.u16((info.option_toggles.len() + info.option_values.len()) as u16);
        self.zeros(6);
        Ok(())
    }

    fn write_shader_assign(&mut self, m: usize, rep: usize) -> Result<(), BfresError> {
        let pm: &PreparedModel = &self.p.models[m];
        let mat = pm.shader_assigns[rep];
        let material = &pm.model.materials[mat];
        self.reloc(self.pos, 9, 1, 0, 1);
        let archive = material.shader_archive.clone();
        let model_name = material.shading_model.clone();
        self.save_string(&archive);
        self.save_string(&model_name);
        self.save_custom(CustomId::RiList(m, rep));
        self.save_dict(DictId::RenderInfo(m, mat), material.render_infos.len());
        self.save_custom(CustomId::ParamList(m, rep));
        self.save_dict(DictId::Param(m, mat), material.shader_params.len());
        self.save_dict(DictId::AttribAssign(m, mat), material.attrib_assign.len());
        self.save_dict(DictId::SamplerAssign(m, mat), material.sampler_assign.len());
        self.save_dict(DictId::Options(m, mat), material.options.len());
        self.u16(material.render_infos.len() as u16);
        self.u16(material.shader_params.len() as u16);
        self.u16(material.param_data.len() as u16);
        self.u16(0);
        self.u64(0);
        Ok(())
    }

    fn write_indices_i8(&mut self, indices: &[i8]) {
        for (i, value) in indices.iter().enumerate() {
            if *value != -1 {
                self.u8(i as u8);
            }
        }
        for value in indices {
            self.u8(*value as u8);
        }
        self.align(8);
    }

    fn write_indices_i16(&mut self, indices: &[i16]) {
        for (i, value) in indices.iter().enumerate() {
            if *value != -1 {
                self.i16(i as i16);
            }
        }
        for value in indices {
            self.i16(*value);
        }
        self.align(8);
    }

    fn write_custom(&mut self, id: &CustomId) -> Result<(), BfresError> {
        match id.clone() {
            CustomId::TexArr(m, mat) => {
                let count = self.p.models[m].model.materials[mat].texture_refs.len();
                self.zeros(count * 8);
            }
            CustomId::TexNames(m, mat) => {
                let names = self.p.models[m].model.materials[mat].texture_refs.clone();
                self.save_strings_relocated(&names);
            }
            CustomId::SampArr(m, mat) => {
                let count = self.p.models[m].model.materials[mat].samplers.len();
                self.zeros(count * 15 * 8);
            }
            CustomId::RiData(m, mat) => {
                let material = &self.p.models[m].model.materials[mat];
                let size = usize::from(material.render_info_size);
                let start = self.pos;
                self.zeros(size);
                self.pos = start;
                let string_count: usize = material
                    .render_infos
                    .iter()
                    .filter(|ri| ri.kind == 2)
                    .map(|ri| ri.strings.len())
                    .sum();
                let has_strings = material.render_infos.iter().any(|ri| ri.kind == 2);
                if has_strings {
                    self.reloc(self.pos, string_count as u32, 1, 0, 1);
                }
                let mut offsets = Vec::with_capacity(material.render_infos.len());
                let infos = material.render_infos.clone();
                for ri in &infos {
                    offsets.push((self.pos - start) as u16);
                    match ri.kind {
                        2 => {
                            for value in &ri.strings {
                                self.save_string(value);
                            }
                        }
                        1 => {
                            for value in &ri.floats {
                                self.f32(*value);
                            }
                        }
                        _ => {
                            for value in &ri.ints {
                                self.i32(*value);
                            }
                        }
                    }
                }
                self.render_info_offsets.insert((m, mat), offsets);
                self.pos = start + size;
            }
            CustomId::RiCounts(m, mat) => {
                let infos = &self.p.models[m].model.materials[mat].render_infos;
                for ri in infos {
                    self.u16(ri.count() as u16);
                }
            }
            CustomId::RiOffsets(m, mat) => {
                let offsets = self
                    .render_info_offsets
                    .get(&(m, mat))
                    .cloned()
                    .unwrap_or_default();
                for offset in offsets {
                    self.u16(offset);
                }
            }
            CustomId::ParamData(m, mat) => {
                let data = self.p.models[m].model.materials[mat].param_data.clone();
                self.write(&data);
                self.align(128);
            }
            CustomId::ParamIdx(m, mat) => {
                let indices = self.p.models[m].model.materials[mat]
                    .param_indices
                    .clone()
                    .unwrap_or_default();
                for value in indices {
                    self.i32(value);
                }
            }
            CustomId::Volatile(_, _) => self.zeros(32),
            CustomId::SampSlots(m, mat) => {
                let slots = self.p.models[m].model.materials[mat]
                    .sampler_slots
                    .clone()
                    .unwrap_or_default();
                for value in slots {
                    self.u64(value as u64);
                }
            }
            CustomId::TexSlots(m, mat) => {
                let slots = self.p.models[m].model.materials[mat]
                    .texture_slots
                    .clone()
                    .unwrap_or_default();
                for value in slots {
                    self.u64(value as u64);
                }
            }
            CustomId::AttrStrs(m, mat) => {
                let values = self.p.models[m].infos[mat].attrib_assigns.clone();
                self.save_strings_relocated(&values);
            }
            CustomId::AttrIdx(m, mat) => {
                let values = self.p.models[m].infos[mat]
                    .attribute_indices
                    .clone()
                    .unwrap_or_default();
                self.write_indices_i8(&values);
            }
            CustomId::SampStrs(m, mat) => {
                let values = self.p.models[m].infos[mat].sampler_assigns.clone();
                self.save_strings_relocated(&values);
            }
            CustomId::SampIdx(m, mat) => {
                let values = self.p.models[m].infos[mat]
                    .sampler_indices
                    .clone()
                    .unwrap_or_default();
                self.write_indices_i8(&values);
            }
            CustomId::OptToggles(m, mat) => {
                let toggles = &self.p.models[m].infos[mat].option_toggles;
                let count = 1 + toggles.len() / 64;
                let mut flags = vec![0u64; count];
                for (i, toggle) in toggles.iter().enumerate() {
                    if *toggle {
                        flags[i / 64] |= 1u64 << (i % 64);
                    }
                }
                for flag in flags {
                    self.u64(flag);
                }
            }
            CustomId::OptStrs(m, mat) => {
                let values = self.p.models[m].infos[mat].option_values.clone();
                self.save_strings_relocated(&values);
            }
            CustomId::OptIdx(m, mat) => {
                let values = self.p.models[m].infos[mat].option_indices.clone();
                self.write_indices_i16(&values);
            }
            CustomId::RiList(m, rep) => {
                let mat = self.p.models[m].shader_assigns[rep];
                let infos = self.p.models[m].model.materials[mat].render_infos.clone();
                self.reloc(self.pos, 1, infos.len() as u32, 1, 1);
                for ri in &infos {
                    self.save_string(&ri.name);
                    self.u8(ri.kind);
                    self.zeros(7);
                }
            }
            CustomId::ParamList(m, rep) => {
                let mat = self.p.models[m].shader_assigns[rep];
                let params = self.p.models[m].model.materials[mat].shader_params.clone();
                self.reloc(self.pos, 2, params.len() as u32, 1, 1);
                for param in &params {
                    self.zeros(8);
                    self.save_string(&param.name);
                    self.u16(param.data_offset);
                    self.u16(param.param_type);
                    self.zeros(4);
                }
            }
        }
        Ok(())
    }

    // ---- Strings, buffers, relocation ------------------------------------------

    /// `ResStringComparer.Compare`.
    fn compare_strings(a: &str, b: &str) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if a == b {
            return Ordering::Equal;
        }
        if a.is_empty() {
            return Ordering::Greater;
        }
        if b.is_empty() {
            return Ordering::Less;
        }
        let a16: Vec<u16> = a.encode_utf16().collect();
        let b16: Vec<u16> = b.encode_utf16().collect();
        a16.cmp(&b16)
    }

    fn write_strings(&mut self) {
        // Strings that appear in the loaded table keep their order, the rest
        // follow in ordinal order (the SortedDictionary enumeration).
        let mut sorted_new: Vec<usize> = (0..self.strings.len()).collect();
        sorted_new.sort_by(|a, b| {
            Self::compare_strings(&self.strings[*a].value, &self.strings[*b].value)
        });
        let mut order: Vec<usize> = Vec::with_capacity(self.strings.len());
        for original in &self.p.file.original_strings {
            if let Some(index) = self
                .strings
                .iter()
                .position(|entry| entry.value == *original)
            {
                if !order.contains(&index) {
                    order.push(index);
                }
            }
        }
        for index in sorted_new {
            if !order.contains(&index) {
                order.push(index);
            }
        }

        self.align(4);
        let pool_start = self.pos;
        self.write(b"_STR");
        self.save_header_block(false);
        self.u32(order.len() as u32);
        let pool_offset = self.pos;
        for index in order {
            let entry = self.strings[index].clone();
            if entry.value == self.file_name {
                self.ofs_file_name_string = self.pos;
            }
            let position = self.pos as u64;
            for offset in &entry.offsets {
                self.write_u64_at(*offset, position);
            }
            let chars: Vec<u16> = entry.value.encode_utf16().collect();
            self.i16(chars.len() as i16);
            let bytes: Vec<u8> = chars
                .iter()
                .map(|c| if *c < 0x80 { *c as u8 } else { b'?' })
                .collect();
            self.write(&bytes);
            self.u8(0);
            self.align(2);
        }
        self.align(2);
        self.ofs_end_of_string_table = self.pos;
        let pool_size = (self.ofs_end_of_string_table - pool_start) as u32;
        self.write_u32_at(self.ofs_string_pool, pool_offset as u32);
        self.write_u32_at(self.ofs_string_pool + 4, 0);
        self.write_u32_at(self.ofs_string_pool + 8, pool_size);
    }

    fn data_alignment(&self) -> usize {
        let file = &self.p.file;
        if file.reserve10 == 1 || file.external_flag != 0 {
            0x1000
        } else {
            1usize << file.alignment
        }
    }

    fn write_index_buffer(&mut self) {
        let alignment = self.data_alignment();
        self.align(alignment);
        self.buffer_info_offset = self.pos;
        let meshes: Vec<Vec<u8>> = self
            .p
            .models
            .iter()
            .flat_map(|pm| pm.model.shapes.iter())
            .flat_map(|shape| shape.meshes.iter().map(|mesh| mesh.data.clone()))
            .collect();
        for data in meshes {
            if self.pos % 8 != 0 {
                self.pos += 8 - self.pos % 8;
            }
            self.write(&data);
        }
        let offset = self.buffer_info_offset as u64;
        self.write_u64_at(self.ofs_index_buffer, offset);
    }

    fn write_vertex_buffer_data(&mut self) {
        self.ofs_vertex_buffer = self.pos;
        let buffers: Vec<(Vec<u8>, usize)> = self
            .p
            .models
            .iter()
            .flat_map(|pm| pm.model.vertex_buffers.iter())
            .flat_map(|vertex| {
                vertex
                    .buffers
                    .iter()
                    .map(move |buffer| (buffer.data.clone(), usize::from(vertex.gpu_alignment)))
            })
            .collect();
        for (data, alignment) in buffers {
            let alignment = alignment.max(1);
            if self.pos % alignment != 0 {
                self.pos += alignment - self.pos % alignment;
            }
            self.write(&data);
        }
    }

    fn write_memory_pool(&mut self) {
        let alignment = self.data_alignment();
        self.align(alignment);
        let total = (self.pos - self.buffer_info_offset) as u32;
        self.write_u32_at(self.ofs_total_buffer_size, total);
        self.memory_pool_offset = self.pos;
        self.zeros(288);
        let pointers = self.memory_pool_pointers.clone();
        let offset = self.memory_pool_offset as u64;
        for pointer in pointers {
            self.write_u64_at(pointer, offset);
        }
    }

    fn write_blocks(&mut self) {
        let blocks = self.external_blocks.clone();
        for (block_index, (external_index, offsets)) in blocks.iter().enumerate() {
            let data = self.p.file.external_files[*external_index].data.clone();
            let alignment = if data.len() <= 3 {
                512
            } else {
                1usize << self.p.file.alignment
            };
            self.align(alignment);
            if block_index == 0 {
                self.ofs_external_file_block = self.pos;
            }
            let position = self.pos as u64;
            for offset in offsets {
                self.write_u64_at(*offset, position);
            }
            self.write(&data);
        }
    }

    fn index_buffer_size(&self) -> u32 {
        let mut size = 0u32;
        for pm in &self.p.models {
            for shape in &pm.model.shapes {
                let align = pm
                    .model
                    .vertex_buffers
                    .get(usize::from(shape.vertex_buffer_index))
                    .map(|v| u32::from(v.gpu_alignment))
                    .unwrap_or(8)
                    .max(1);
                for mesh in &shape.meshes {
                    size += mesh.data.len() as u32;
                    if size % align != 0 {
                        size += align - size % align;
                    }
                }
            }
        }
        size
    }

    fn vertex_buffer_size(&self) -> u32 {
        let mut size = 0u32;
        for pm in &self.p.models {
            for vertex in &pm.model.vertex_buffers {
                let align = u32::from(vertex.gpu_alignment).max(1);
                for buffer in &vertex.buffers {
                    size += buffer.data.len() as u32;
                    if size % align != 0 {
                        size += align - size % align;
                    }
                }
            }
        }
        size
    }

    fn setup_relocation_table(&mut self) -> Vec<RelocSection> {
        let file = &self.p.file;
        if let Some(first) = file.external_files.first() {
            let alignment = if first.data.len() <= 3 {
                256
            } else {
                1usize << file.alignment
            };
            self.align(alignment);
        }
        let table_start = self.pos as u32;
        for entries in self.sections[1..].iter_mut() {
            entries.sort_by_key(|entry| entry.position);
        }
        let counts: Vec<u32> = self.sections.iter().map(|s| s.len() as u32).collect();
        let section_size = self.ofs_end_of_string_table as u32;
        let has_buffer_info = file.has_buffer_info;
        let has_memory_pool = !self.p.models.is_empty();

        let mut entry_index = 0u32;
        let mut entry_pos;
        let mut sections = Vec::with_capacity(SECTION_COUNT);
        sections.push(RelocSection {
            position: 0,
            size: section_size,
            entry_index,
            entries: self.sections[0].clone(),
        });
        entry_index += counts[0];
        if has_buffer_info {
            entry_pos = self.buffer_info_offset as u32;
            sections.push(RelocSection {
                position: entry_pos,
                size: self.index_buffer_size(),
                entry_index,
                entries: self.sections[1].clone(),
            });
            entry_index += counts[1];
            entry_pos = self.ofs_vertex_buffer as u32;
            sections.push(RelocSection {
                position: entry_pos,
                size: self.vertex_buffer_size(),
                entry_index,
                entries: self.sections[2].clone(),
            });
            entry_index += counts[2];
        } else {
            entry_pos = section_size;
            sections.push(RelocSection {
                position: entry_pos,
                size: 0,
                entry_index,
                entries: self.sections[1].clone(),
            });
            sections.push(RelocSection {
                position: entry_pos,
                size: 0,
                entry_index,
                entries: self.sections[2].clone(),
            });
        }
        if has_memory_pool {
            entry_pos = self.memory_pool_offset as u32;
            sections.push(RelocSection {
                position: entry_pos,
                size: 288,
                entry_index,
                entries: self.sections[3].clone(),
            });
            entry_index += counts[3];
        } else {
            entry_pos = section_size;
            sections.push(RelocSection {
                position: entry_pos,
                size: 0,
                entry_index,
                entries: self.sections[3].clone(),
            });
        }
        entry_index += counts[2];
        if !file.external_files.is_empty() {
            entry_pos = self.ofs_external_file_block as u32;
            sections.push(RelocSection {
                position: entry_pos,
                size: table_start - self.ofs_external_file_block as u32,
                entry_index,
                entries: self.sections[4].clone(),
            });
        } else {
            sections.push(RelocSection {
                position: entry_pos,
                size: 0,
                entry_index,
                entries: self.sections[4].clone(),
            });
        }
        sections
    }

    fn write_relocation_table(&mut self, sections: &[RelocSection]) {
        self.align(256);
        let offset = self.pos as u32;
        self.write(b"_RLT");
        self.ofs_end_of_block = self.pos;
        self.u32(offset);
        self.u32(sections.len() as u32);
        self.u32(0);
        for section in sections {
            self.u64(0);
            self.u32(section.position);
            self.u32(section.size);
            self.u32(section.entry_index);
            self.u32(section.entries.len() as u32);
        }
        for section in sections {
            for entry in &section.entries {
                self.u32(entry.position);
                self.u16(entry.struct_count as u16);
                self.u8(entry.offset_count as u8);
                self.u8(entry.padding_count as u8);
            }
        }
        self.write_u32_at(self.ofs_relocation_table, offset);
    }
}

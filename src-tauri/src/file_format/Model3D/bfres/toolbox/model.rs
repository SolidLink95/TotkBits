//! In-memory model of a BFRES v10 file shaped after Syroot's `NSW.Bfres`
//! object graph, which is what Switch Toolbox serializes. Keeping the same
//! shape makes the saver a line-by-line port of `ResFileSwitchSaver`.

#[derive(Clone, Debug, Default)]
pub struct ResFile {
    pub version: u32,
    pub alignment: u8,
    pub target_address_size: u8,
    pub flag: u16,
    pub name: String,
    pub models: Vec<Model>,
    /// Strings of the source `_STR` block in file order (count + 1 entries).
    pub original_strings: Vec<String>,
    /// Byte 0xEE of the source header.
    pub external_flag: u8,
    /// Byte 0xEF of the source header.
    pub reserve10: u8,
    /// Logical file size of the source header (0x1C).
    pub source_file_size: u32,
    /// Whether Syroot ends up with a `BufferInfo` object (a buffer info
    /// pointer or the external-GPU flag).
    pub has_buffer_info: bool,
    pub external_files: Vec<ExternalFile>,
}

#[derive(Clone, Debug, Default)]
pub struct ExternalFile {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub flags: u32,
    pub name: String,
    pub path: String,
    pub skeleton: Skeleton,
    pub vertex_buffers: Vec<VertexBuffer>,
    pub shapes: Vec<Shape>,
    pub materials: Vec<Material>,
    pub user_data: Vec<UserData>,
}

/// `UserData`: 0 = Int32, 1 = Single, 2 = String, 3 = WString, 4 = Byte.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserData {
    pub name: String,
    pub kind: u8,
    pub ints: Vec<i32>,
    pub floats: Vec<f32>,
    pub strings: Vec<String>,
    pub bytes: Vec<u8>,
}

impl UserData {
    pub fn count(&self) -> usize {
        match self.kind {
            0 => self.ints.len(),
            1 => self.floats.len(),
            2 | 3 => self.strings.len(),
            _ => self.bytes.len(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    pub flags: u32,
    pub bones: Vec<Bone>,
    pub matrix_to_bone: Vec<u16>,
    pub inverse_matrices: Vec<[f32; 12]>,
    pub mirrored_bones: Vec<u16>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent_index: i16,
    pub smooth_matrix_index: i16,
    pub rigid_matrix_index: i16,
    pub billboard_index: i16,
    pub flags: u32,
    pub scale: [f32; 3],
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    pub user_data: Vec<UserData>,
}

impl Bone {
    /// `BoneFlagsRotation.EulerXYZ` is bit 12 of the flags word.
    pub fn uses_euler(&self) -> bool {
        self.flags & (1 << 12) != 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct VertexBuffer {
    pub flags: u32,
    pub attributes: Vec<VertexAttrib>,
    pub buffers: Vec<VertexData>,
    pub vertex_count: u32,
    pub vertex_skin_count: u16,
    pub gpu_alignment: u16,
}

#[derive(Clone, Debug, Default)]
pub struct VertexAttrib {
    pub name: String,
    /// Raw 16-bit format written big endian in the attribute record.
    pub format: [u8; 2],
    pub offset: u16,
    pub buffer_index: u16,
}

#[derive(Clone, Debug, Default)]
pub struct VertexData {
    pub data: Vec<u8>,
    pub stride: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Shape {
    pub flags: u32,
    pub name: String,
    pub vertex_buffer_index: u16,
    pub meshes: Vec<Mesh>,
    pub skin_bone_indices: Vec<u16>,
    pub boundings: Vec<[f32; 6]>,
    pub radius_list: Vec<[f32; 4]>,
    pub material_index: u16,
    pub bone_index: u16,
    pub vertex_skin_count: u8,
    pub target_attrib_count: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub submeshes: Vec<(u32, u32)>,
    pub buffer_flag: u32,
    pub data: Vec<u8>,
    pub primitive_type: u32,
    pub index_format: u32,
    pub index_count: u32,
    pub first_vertex: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub flags: u32,
    pub name: String,
    pub shader_archive: String,
    pub shading_model: String,
    pub render_infos: Vec<RenderInfo>,
    pub shader_params: Vec<ShaderParam>,
    /// Source shader parameter data block.
    pub param_data: Vec<u8>,
    pub param_indices: Option<Vec<i32>>,
    /// (key, value) pairs; a value of `<Default Value>` means index -1.
    pub attrib_assign: Vec<(String, String)>,
    pub sampler_assign: Vec<(String, String)>,
    pub options: Vec<(String, String)>,
    pub texture_refs: Vec<String>,
    pub samplers: Vec<Sampler>,
    /// `None` when the source pointer was zero (Syroot keeps a null array).
    pub sampler_slots: Option<Vec<i64>>,
    pub texture_slots: Option<Vec<i64>>,
    pub render_info_size: u16,
    pub user_data: Vec<UserData>,
}

#[derive(Clone, Debug, Default)]
pub struct Sampler {
    pub name: String,
    pub raw: [u8; 32],
}

#[derive(Clone, Debug, Default)]
pub struct RenderInfo {
    pub name: String,
    /// 0 = Int32, 1 = Single, 2 = String.
    pub kind: u8,
    pub ints: Vec<i32>,
    pub floats: Vec<f32>,
    pub strings: Vec<String>,
}

impl RenderInfo {
    pub fn count(&self) -> usize {
        match self.kind {
            0 => self.ints.len(),
            1 => self.floats.len(),
            _ => self.strings.len(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ShaderParam {
    pub name: String,
    pub param_type: u16,
    pub data_offset: u16,
}

impl ShaderParam {
    /// `ShaderParam.DataSize` from Syroot.
    pub fn data_size(&self) -> Option<u32> {
        let t = u32::from(self.param_type);
        if t <= 15 {
            return Some(4 * ((t & 3) + 1));
        }
        if t <= 27 {
            let cols = (t & 3) + 1;
            let rows = ((t - 16) >> 2) + 2;
            return Some(4 * cols * rows);
        }
        match t {
            28 => Some(20),
            29 => Some(36),
            30 => Some(24),
            31 => Some(28),
            _ => None,
        }
    }
}

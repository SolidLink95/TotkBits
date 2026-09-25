//! In-memory model of a Phive mesh shape (`.bphsh`): the `hknpMeshShape`
//! object graph TOTK stores in the TAG0 DATA section, plus the two Phive
//! material tables that follow the tagfile.
//!
//! Every field mirrors the binary one to one so a parsed shape can be
//! written back byte for byte; the builder fills the same structures from a
//! triangle soup.

/// A `hknpMeshShape::ShapeTagTableEntry`: the first mesh primitive key that
/// carries `shape_tag`, until the next entry's key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapeTagEntry {
    pub mesh_primitive_key: u32,
    pub shape_tag: u16,
}

/// A `hkcdSimdTree::Node`: four child AABBs stored transposed
/// (`lx[4], hx[4], ly[4], hy[4], lz[4], hz[4]`) plus per-lane data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimdTreeNode {
    pub lx: [f32; 4],
    pub hx: [f32; 4],
    pub ly: [f32; 4],
    pub hy: [f32; 4],
    pub lz: [f32; 4],
    pub hz: [f32; 4],
    pub data: [u32; 4],
    pub is_leaf: bool,
    pub is_active: bool,
}

/// Bit pattern Havok stores in unused lanes of a SIMD tree node.
pub const SIMD_EMPTY_MIN_BITS: u32 = 0x7F7F_FFEE;
pub const SIMD_EMPTY_MAX_BITS: u32 = 0xFF7F_FFEE;

impl SimdTreeNode {
    pub fn empty_min() -> f32 {
        f32::from_bits(SIMD_EMPTY_MIN_BITS)
    }
    pub fn empty_max() -> f32 {
        f32::from_bits(SIMD_EMPTY_MAX_BITS)
    }
    /// A node with every lane cleared (`clear()` in Havok).
    pub fn cleared() -> Self {
        Self {
            lx: [Self::empty_min(); 4],
            hx: [Self::empty_max(); 4],
            ly: [Self::empty_min(); 4],
            hy: [Self::empty_max(); 4],
            lz: [Self::empty_min(); 4],
            hz: [Self::empty_max(); 4],
            data: [0; 4],
            is_leaf: false,
            is_active: false,
        }
    }
    pub fn lane_valid(&self, lane: usize) -> bool {
        if self.is_leaf {
            self.data[lane] != 0xFFFF_FFFF
        } else {
            self.data[lane] != 0
        }
    }
    pub fn lane_aabb(&self, lane: usize) -> Aabb {
        Aabb {
            min: [self.lx[lane], self.ly[lane], self.lz[lane]],
            max: [self.hx[lane], self.hy[lane], self.hz[lane]],
        }
    }
    pub fn set_lane_aabb(&mut self, lane: usize, aabb: &Aabb) {
        self.lx[lane] = aabb.min[0];
        self.hx[lane] = aabb.max[0];
        self.ly[lane] = aabb.min[1];
        self.hy[lane] = aabb.max[1];
        self.lz[lane] = aabb.min[2];
        self.hz[lane] = aabb.max[2];
    }
    /// Union of every valid lane.
    pub fn compound_aabb(&self) -> Aabb {
        let mut out = Aabb::empty();
        for lane in 0..4 {
            if self.lane_valid(lane) {
                out.include_aabb(&self.lane_aabb(lane));
            }
        }
        out
    }
}

/// A `hknpAabb8TreeNode`: four child AABBs quantized to 8 bits per bound,
/// stored transposed as `u32` lanes (`lx`, `hx`, `ly`, `hy`, `lz`, `hz`),
/// plus four data bytes (child node index for internal nodes, primitive
/// index for leaves; a node is a leaf when `data[2] > data[3]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aabb8TreeNode {
    pub lx: [u8; 4],
    pub hx: [u8; 4],
    pub ly: [u8; 4],
    pub hy: [u8; 4],
    pub lz: [u8; 4],
    pub hz: [u8; 4],
    pub data: [u8; 4],
}

impl Aabb8TreeNode {
    pub fn is_leaf(&self) -> bool {
        self.data[2] > self.data[3]
    }
    pub fn lane_valid(&self, lane: usize) -> bool {
        self.lx[lane] <= self.hx[lane]
    }
    pub fn set_lane(&mut self, lane: usize, b: [u8; 6]) {
        self.lx[lane] = b[0];
        self.hx[lane] = b[1];
        self.ly[lane] = b[2];
        self.hy[lane] = b[3];
        self.lz[lane] = b[4];
        self.hz[lane] = b[5];
    }
    pub fn lane(&self, lane: usize) -> [u8; 6] {
        [
            self.lx[lane],
            self.hx[lane],
            self.ly[lane],
            self.hy[lane],
            self.lz[lane],
            self.hz[lane],
        ]
    }
    pub fn clear_lane(&mut self, lane: usize) {
        self.set_lane(lane, [0xFF, 0, 0xFF, 0, 0xFF, 0]);
    }
    /// Union of the valid lanes, as quantized bounds.
    pub fn compound(&self) -> [u8; 6] {
        let mut out = [255u8, 0, 255, 0, 255, 0];
        for lane in 0..4 {
            if !self.lane_valid(lane) {
                continue;
            }
            let l = self.lane(lane);
            out[0] = out[0].min(l[0]);
            out[1] = out[1].max(l[1]);
            out[2] = out[2].min(l[2]);
            out[3] = out[3].max(l[3]);
            out[4] = out[4].min(l[4]);
            out[5] = out[5].max(l[5]);
        }
        out
    }
    pub fn swap_lanes(&mut self, a: usize, b: usize) {
        let la = self.lane(a);
        let lb = self.lane(b);
        self.set_lane(a, lb);
        self.set_lane(b, la);
        self.data.swap(a, b);
    }
}

/// A `GeometrySection::Primitive`: a quad (or a triangle when `c == d`) of
/// section-local vertex indices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Primitive {
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
}

impl Primitive {
    pub fn is_triangle(&self) -> bool {
        self.c == self.d
    }
    pub fn ids(&self) -> [u8; 4] {
        [self.a, self.b, self.c, self.d]
    }
}

/// A `hknpMeshShape::GeometrySection`: up to 256 quantized vertices, the
/// primitives over them, their 8-bit BVH, and the interior-primitive bits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometrySection {
    pub bvh: Vec<Aabb8TreeNode>,
    pub primitives: Vec<Primitive>,
    /// Quantized vertices (`Vertex16_3`), relative to `section_offset`.
    pub vertices: Vec<[u16; 3]>,
    pub interior_primitive_bits: Vec<u8>,
    /// Byte size TOTK records for the vertex buffer (`vertices * 6` plus
    /// padding, see `reader`).
    pub vertex_buffer_size: usize,
    pub section_offset: [u32; 3],
    pub bit_scale8_inv: [f32; 3],
    pub bit_offset: [i16; 3],
}

/// Section offset marking a section whose vertex buffer holds raw `f32`
/// positions (12 bytes each) instead of 16-bit quantized ones. TOTK's
/// large dungeon shapes carry a few such sections; the buffer still reads
/// and writes as 16-bit words, so files round-trip unchanged.
pub const FLOAT_VERTEX_SECTION_OFFSET: [u32; 3] = [0x7FFF_FFFF; 3];

impl GeometrySection {
    pub const MAX_VERTICES: usize = 256;

    /// Whether the vertex buffer holds raw `f32` positions.
    pub fn has_float_vertices(&self) -> bool {
        self.section_offset == FLOAT_VERTEX_SECTION_OFFSET
    }

    /// Vertices the primitives can index: two 16-bit triples make one
    /// float vertex in a float section.
    pub fn vertex_count(&self) -> usize {
        if self.has_float_vertices() {
            self.vertices.len() / 2
        } else {
            self.vertices.len()
        }
    }

    /// Position of float vertex `index` (little-endian `f32` x, y, z spread
    /// over the words of two consecutive 16-bit triples).
    pub fn float_vertex(&self, index: usize) -> [f32; 3] {
        let a = self.vertices.get(index * 2).copied().unwrap_or_default();
        let b = self
            .vertices
            .get(index * 2 + 1)
            .copied()
            .unwrap_or_default();
        let word = |lo: u16, hi: u16| f32::from_bits((hi as u32) << 16 | lo as u32);
        [word(a[0], a[1]), word(a[2], b[0]), word(b[1], b[2])]
    }
}

/// `hknpMeshShapePrimitiveMapping`, present on some shapes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrimitiveMapping {
    pub section_start: Vec<u32>,
    pub bit_string: Vec<u32>,
    pub bits_per_entry: u32,
    pub triangle_index_bit_mask: u32,
}

/// The `hknpMeshShape` root object.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshShape {
    pub shape_type: u8,
    pub dispatch_type: u8,
    pub flags: u16,
    pub num_shape_key_bits: u8,
    pub convex_radius: f32,
    pub user_data: u64,
    pub shape_tag_codec_info: u32,
    pub bit_scale16: [f32; 4],
    pub bit_scale16_inv: [f32; 4],
    pub shape_tag_table: Vec<ShapeTagEntry>,
    pub top_level_tree: Vec<SimdTreeNode>,
    pub top_level_tree_is_compact: bool,
    pub sections: Vec<GeometrySection>,
    pub primitive_mapping: Option<PrimitiveMapping>,
}

impl MeshShape {
    pub const TYPE_MESH: u8 = 8;
    pub const DISPATCH_COMPOSITE: u8 = 3;
    pub const FLAGS_DEFAULT: u16 = 4;

    /// Section index shift inside a mesh primitive key
    /// (`numShapeKeyBits - bits(numSections - 1)`).
    pub fn section_shift(&self) -> u32 {
        let section_bits = bit_width(self.sections.len() as i64 - 1);
        (self.num_shape_key_bits as u32).saturating_sub(section_bits)
    }

    /// World position of vertex `index` of `section`, whichever encoding
    /// the section uses.
    pub fn vertex_position(&self, section: &GeometrySection, index: usize) -> [f32; 3] {
        if section.has_float_vertices() {
            section.float_vertex(index)
        } else {
            self.unpack_vertex(
                section,
                section.vertices.get(index).copied().unwrap_or_default(),
            )
        }
    }

    /// Unpacks a quantized vertex of `section` to world units.
    pub fn unpack_vertex(&self, section: &GeometrySection, packed: [u16; 3]) -> [f32; 3] {
        let mut out = [0f32; 3];
        for i in 0..3 {
            let value = (packed[i] as i32).wrapping_add(section.section_offset[i] as i32);
            out[i] = value as f32 * self.bit_scale16_inv[i];
        }
        out
    }

    /// Shape tag of the primitive `primitive` of section `section`.
    pub fn shape_tag_of(&self, section: usize, primitive: usize) -> u16 {
        let key =
            pack_mesh_primitive_key(section as u32, primitive as u32, 0, self.section_shift());
        let table = &self.shape_tag_table;
        if table.is_empty() {
            return 0;
        }
        let (mut lo, mut hi) = (0usize, table.len() - 1);
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            if table[mid].mesh_primitive_key <= key {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        table[lo].shape_tag
    }
}

/// One Phive material table row: `{ material id, padding, user shape tag mask }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialEntry {
    pub material_id: i32,
    pub reserved: i32,
    pub flags: u64,
}

/// A complete `.bphsh`: the mesh shape and its material tables.
#[derive(Clone, Debug, PartialEq)]
pub struct BphshShape {
    pub shape: MeshShape,
    /// Table 0, one row per material.
    pub materials: Vec<MaterialEntry>,
    /// Table 1, one collision mask per material (`u64`).
    pub collision_masks: Vec<u64>,
}

/// Axis-aligned box in world units (`hkAabb` without the `w` lane).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn empty() -> Self {
        Self {
            min: [f32::MAX; 3],
            max: [-f32::MAX; 3],
        }
    }
    pub fn include_point(&mut self, p: [f32; 3]) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }
    pub fn include_aabb(&mut self, o: &Aabb) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(o.min[i]);
            self.max[i] = self.max[i].max(o.max[i]);
        }
    }
    pub fn expand_by(&mut self, e: f32) {
        for i in 0..3 {
            self.min[i] -= e;
            self.max[i] += e;
        }
    }
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
    pub fn extents(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
    /// Havok's `surfaceArea`, in its exact operation order.
    pub fn surface_area(&self) -> f32 {
        let ex = self.max[0] - self.min[0];
        let ey = self.max[1] - self.min[1];
        let ez = self.max[2] - self.min[2];
        let p0 = (2.0 * ex) * ey;
        let p1 = (2.0 * ey) * ez;
        let p2 = (2.0 * ez) * ex;
        p2 + (p1 + p0)
    }
}

/// Number of bits needed to hold `v` (`0` for `v <= 0`).
pub fn bit_width(v: i64) -> u32 {
    if v <= 0 {
        return 0;
    }
    let mut bits = 0;
    while (1i64 << bits) <= v {
        bits += 1;
    }
    bits
}

pub fn pack_mesh_primitive_key(section: u32, primitive: u32, sub_triangle: u32, shift: u32) -> u32 {
    (section << shift) | (primitive << 1) | sub_triangle
}

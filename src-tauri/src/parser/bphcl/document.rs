use super::{
    AampSection, Bone, BphclHeader, Cloth, ClothSkeletonPair, Collidable, CollidableShape, Item,
    NamedVariant, Particle, Patch, Section, SimCloth, Skeleton, TypeTable, Vector4,
};
use crate::parser::binary::BinaryReader;
use std::{
    collections::HashSet,
    io::{self, ErrorKind},
};

#[derive(Clone, Debug)]
pub struct BphclDocument {
    pub raw: Vec<u8>,
    pub header: BphclHeader,
    pub tag: Section,
    pub items: Vec<Item>,
    pub patches: Vec<Patch>,
    pub type_names: Vec<String>,
    pub type_table: TypeTable,
    pub variants: Vec<NamedVariant>,
    pub cloth: Vec<Cloth>,
    pub skeletons: Vec<Skeleton>,
    pub cloth_skeleton_pairs: Vec<ClothSkeletonPair>,
    pub collidables: Vec<Collidable>,
    pub aamp: Option<AampSection>,
}
impl BphclDocument {
    pub fn neutral_physics_graph(&self) -> crate::parser::physics_graph::FormatNeutralPhysicsGraph {
        self.into()
    }

    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let header = BphclHeader::read(data)?;
        let tag = Section::read(data, header.tag_offset as usize, data.len(), Some("TAG0"))?;
        let items = Self::items(data, &tag)?;
        let patches = Self::patches(data, &tag)?;
        let type_names = Self::type_names(data, &tag)?;
        let type_table = TypeTable::parse(&tag, data)?;
        let aamp = AampSection::read(data, header.parameter_offset, header.parameter_size)?;
        let mut d = Self {
            raw: data.to_vec(),
            header,
            tag,
            items,
            patches,
            type_names,
            type_table,
            variants: vec![],
            cloth: vec![],
            skeletons: vec![],
            cloth_skeleton_pairs: vec![],
            collidables: vec![],
            aamp,
        };
        d.variants = d.read_variants()?;
        d.read_domain()?;
        Ok(d)
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        self.raw.clone()
    }
    fn items(data: &[u8], tag: &Section) -> io::Result<Vec<Item>> {
        let Some(s) = tag.find("ITEM") else {
            return Ok(vec![]);
        };
        if (s.size - 8) % 12 != 0 {
            return Err(invalid("ITEM payload is not aligned"));
        }
        let mut r = BinaryReader::new(data);
        r.seek(s.payload_offset)?;
        let mut out = vec![];
        while r.position() < s.payload_end() {
            let flags = r.read_u32()?;
            out.push(Item {
                flags,
                type_index: flags & 0xffffff,
                data_offset: r.read_u32()?,
                count: r.read_u32()?,
            })
        }
        Ok(out)
    }
    fn patches(data: &[u8], tag: &Section) -> io::Result<Vec<Patch>> {
        let Some(s) = tag.find("PTCH") else {
            return Ok(vec![]);
        };
        let mut r = BinaryReader::new(data);
        r.seek(s.payload_offset)?;
        let mut out = vec![];
        while r.position() + 4 <= s.payload_end() {
            let t = r.read_u32()?;
            if t == 0 {
                break;
            }
            let n = r.read_u32()? as usize;
            if n > (s.payload_end() - r.position()) / 4 {
                return Err(invalid("PTCH exceeds section"));
            }
            let mut offsets = Vec::with_capacity(n);
            for _ in 0..n {
                offsets.push(r.read_u32()?)
            }
            out.push(Patch {
                type_index: t,
                offsets,
            })
        }
        Ok(out)
    }
    fn type_names(data: &[u8], tag: &Section) -> io::Result<Vec<String>> {
        let Some(ss) = tag.find("TST1").or_else(|| tag.find("TSTR")) else {
            return Ok(vec![]);
        };
        let Some(ns) = tag.find("TNA1").or_else(|| tag.find("TNAM")) else {
            return Ok(vec![]);
        };
        let strings = data[ss.payload_offset..ss.payload_end()]
            .split(|b| *b == 0)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect::<Vec<_>>();
        let mut p = ns.payload_offset;
        let count = varuint(data, &mut p, ns.payload_end())? as usize;
        let mut out = vec![String::new()];
        for _ in 1..count {
            let i = varuint(data, &mut p, ns.payload_end())? as usize;
            let templates = varuint(data, &mut p, ns.payload_end())?;
            out.push(
                strings
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("type-string-{i}")),
            );
            for _ in 0..templates {
                varuint(data, &mut p, ns.payload_end())?;
                varuint(data, &mut p, ns.payload_end())?;
            }
        }
        Ok(out)
    }
    fn referenced(&self, offset: u32) -> Option<usize> {
        if !self.patches.iter().any(|p| p.offsets.contains(&offset)) {
            return None;
        }
        let data = self.tag.find("DATA")?;
        let o = data.payload_offset.checked_add(offset as usize)?;
        let i = u32::from_le_bytes(self.raw.get(o..o + 4)?.try_into().ok()?) as usize;
        (i < self.items.len()).then_some(i)
    }
    fn string_ptr(&self, offset: u32) -> Option<String> {
        let i = self.referenced(offset)?;
        let item = self.items.get(i)?;
        let data = self.tag.find("DATA")?;
        let o = data.payload_offset + item.data_offset as usize;
        let end = self.raw[o..].iter().position(|b| *b == 0)?;
        String::from_utf8(self.raw[o..o + end].to_vec()).ok()
    }
    fn reference_array(&self, field: u32) -> Vec<usize> {
        let Some(storage) = self.referenced(field) else {
            return vec![];
        };
        let Some(item) = self.items.get(storage) else {
            return vec![];
        };
        let mut out = vec![];
        for n in 0..item.count {
            if let Some(i) = self.referenced(item.data_offset + n * 8) {
                out.push(i)
            }
        }
        out
    }
    pub fn reference_item_indices(&self, field: u32) -> io::Result<Vec<usize>> {
        let storage = self
            .referenced(field)
            .ok_or_else(|| invalid(&format!("array at DATA+{field:#x} has no ITEM reference")))?;
        let item = self
            .items
            .get(storage)
            .ok_or_else(|| invalid("array storage ITEM is missing"))?;
        (0..item.count)
            .map(|index| {
                let offset = item.data_offset + index * 8;
                self.referenced(offset).ok_or_else(|| {
                    invalid(&format!("unresolved array pointer at DATA+{offset:#x}"))
                })
            })
            .collect()
    }

    pub fn reference_array_metadata(&self, field: u32) -> io::Result<super::ReferenceArray> {
        let storage_item_index = self
            .referenced(field)
            .ok_or_else(|| invalid(&format!("array at DATA+{field:#x} has no ITEM reference")))?;
        let storage_item = self
            .items
            .get(storage_item_index)
            .cloned()
            .ok_or_else(|| invalid("array storage ITEM is missing"))?;
        let entry_patch_type_index = self
            .patches
            .iter()
            .find(|patch| patch.offsets.contains(&storage_item.data_offset))
            .map(|patch| patch.type_index)
            .ok_or_else(|| invalid("array storage has no entry relocation patch"))?;
        Ok(super::ReferenceArray {
            field_offset: field,
            storage_item_index,
            storage_item,
            entry_patch_type_index,
        })
    }
    fn array_item(&self, field: u32) -> Option<&Item> {
        self.items.get(self.referenced(field)?)
    }
    fn data_bytes(&self, offset: u32, count: usize) -> Option<&[u8]> {
        let data = self.tag.find("DATA")?;
        let o = data.payload_offset.checked_add(offset as usize)?;
        self.raw.get(o..o.checked_add(count)?)
    }
    fn u16(&self, o: u32) -> Option<u16> {
        Some(u16::from_le_bytes(self.data_bytes(o, 2)?.try_into().ok()?))
    }
    fn u32(&self, o: u32) -> Option<u32> {
        Some(u32::from_le_bytes(self.data_bytes(o, 4)?.try_into().ok()?))
    }
    fn f32(&self, o: u32) -> Option<f32> {
        Some(f32::from_bits(self.u32(o)?))
    }
    fn vec4(&self, o: u32) -> Vector4 {
        Vector4 {
            x: self.f32(o).unwrap_or_default(),
            y: self.f32(o + 4).unwrap_or_default(),
            z: self.f32(o + 8).unwrap_or_default(),
            w: self.f32(o + 12).unwrap_or_default(),
        }
    }
    fn read_sim(&self, index: usize, item_index: usize) -> SimCloth {
        let item = &self.items[item_index];
        let fixed: Vec<u16> = self
            .array_item(item.data_offset + 80)
            .map(|a| {
                (0..a.count)
                    .filter_map(|i| self.u16(a.data_offset + i * 2))
                    .collect()
            })
            .unwrap_or_default();
        let positions = self
            .reference_array(item.data_offset + 120)
            .first()
            .and_then(|pose| self.items.get(*pose))
            .and_then(|pose| self.array_item(pose.data_offset + 32))
            .map(|a| {
                (0..a.count)
                    .map(|i| self.vec4(a.data_offset + i * 16))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let particles = self
            .array_item(item.data_offset + 64)
            .map(|a| {
                (0..a.count as usize)
                    .map(|i| {
                        let o = a.data_offset + i as u32 * 16;
                        Particle {
                            index: i,
                            position: positions.get(i).copied().unwrap_or_default(),
                            fixed: fixed.contains(&(i as u16)),
                            mass: self.f32(o).unwrap_or_default(),
                            inverse_mass: self.f32(o + 4).unwrap_or_default(),
                            radius: self.f32(o + 8).unwrap_or_default(),
                            friction: self.f32(o + 12).unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut constraints = self.reference_array(item.data_offset + 136);
        constraints.extend(self.reference_array(item.data_offset + 152));
        SimCloth {
            index,
            name: self
                .string_ptr(item.data_offset + 24)
                .unwrap_or_else(|| format!("Simulation {index}")),
            item_index,
            particles,
            fixed_particle_indices: fixed,
            constraint_item_indices: constraints,
            collidable_item_indices: self.reference_array(item.data_offset + 208),
        }
    }
    /// The class name and `type` word of a collidable's shape object.
    fn shape_identity(&self, collider: &Item) -> (String, u32) {
        let Some(si) = self.referenced(collider.data_offset + 136) else {
            return (String::new(), 0);
        };
        let shape = &self.items[si];
        let class = self
            .type_names
            .get(shape.type_index as usize)
            .cloned()
            .unwrap_or_default();
        (class, self.u32(shape.data_offset + 24).unwrap_or_default())
    }
    fn read_shape(&self, collider: &Item) -> CollidableShape {
        let Some(si) = self.referenced(collider.data_offset + 136) else {
            return CollidableShape::Unknown {
                class_name: String::new(),
                kind: 0,
            };
        };
        let shape = &self.items[si];
        let class = self
            .type_names
            .get(shape.type_index as usize)
            .cloned()
            .unwrap_or_default();
        let kind = self.u32(shape.data_offset + 24).unwrap_or_default();
        if class.contains("TaperedCapsule") {
            CollidableShape::TaperedCapsule {
                start: self.vec4(shape.data_offset + 32),
                end: self.vec4(shape.data_offset + 48),
                start_radius: self.f32(shape.data_offset + 144).unwrap_or_default(),
                end_radius: self.f32(shape.data_offset + 148).unwrap_or_default(),
            }
        } else if class.contains("Capsule") {
            CollidableShape::Capsule {
                start: self.vec4(shape.data_offset + 32),
                end: self.vec4(shape.data_offset + 48),
                radius: self.f32(shape.data_offset + 80).unwrap_or_default(),
            }
        } else if class.contains("Sphere") {
            let center = self.vec4(shape.data_offset + 32);
            CollidableShape::Sphere {
                center,
                radius: center.w,
            }
        } else if class.contains("Plane") {
            CollidableShape::Plane {
                equation: self.vec4(shape.data_offset + 32),
            }
        } else {
            CollidableShape::Unknown {
                class_name: class,
                kind,
            }
        }
    }
    fn read_skeleton_bones(&self, skeleton_offset: u32) -> Vec<Bone> {
        let parents = self
            .array_item(skeleton_offset + 32)
            .map(|item| {
                (0..item.count)
                    .filter_map(|index| self.u16(item.data_offset + index * 2))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let Some(bone_entries) = self.array_item(skeleton_offset + 48) else {
            return vec![];
        };
        let poses = self.array_item(skeleton_offset + 64);
        (0..bone_entries.count as usize)
            .map(|index| {
                let bone_offset = bone_entries.data_offset + index as u32 * 16;
                let pose_offset = poses
                    .filter(|item| index < item.count as usize)
                    .map(|item| item.data_offset + index as u32 * 48);
                Bone {
                    index,
                    name: self
                        .string_ptr(bone_offset)
                        .unwrap_or_else(|| format!("Bone {index}")),
                    parent_index: parents
                        .get(index)
                        .copied()
                        .filter(|parent| *parent != u16::MAX)
                        .map(usize::from),
                    lock_translation: self
                        .data_bytes(bone_offset + 8, 1)
                        .is_some_and(|data| data[0] != 0),
                    translation: pose_offset
                        .map(|offset| self.vec4(offset))
                        .unwrap_or_default(),
                    rotation: pose_offset
                        .map(|offset| self.vec4(offset + 16))
                        .unwrap_or_default(),
                }
            })
            .collect()
    }
    fn read_variants(&self) -> io::Result<Vec<NamedVariant>> {
        let Some(vi) = self.referenced(0) else {
            return Ok(vec![]);
        };
        let item = &self.items[vi];
        let mut out = vec![];
        for i in 0..item.count as usize {
            let o = item.data_offset + i as u32 * 24;
            let oi = self
                .referenced(o + 16)
                .ok_or_else(|| invalid("unresolved root variant"))?;
            let object = &self.items[oi];
            out.push(NamedVariant {
                index: i,
                name: self.string_ptr(o).unwrap_or_default(),
                class_name: self.string_ptr(o + 8).unwrap_or_default(),
                item_index: oi,
                object_type: self
                    .type_names
                    .get(object.type_index as usize)
                    .cloned()
                    .unwrap_or_default(),
            })
        }
        Ok(out)
    }
    fn read_domain(&mut self) -> io::Result<()> {
        if let Some(root) = self
            .variants
            .iter()
            .find(|v| v.class_name == "hclClothContainer")
        {
            let base = self.items[root.item_index].data_offset;
            let coll = self.reference_array(base + 24);
            self.collidables = coll
                .into_iter()
                .enumerate()
                .map(|(index, item_index)| {
                    let item = &self.items[item_index];
                    let (shape_class_name, shape_kind) = self.shape_identity(item);
                    Collidable {
                        index,
                        name: self
                            .string_ptr(item.data_offset + 144)
                            .unwrap_or_else(|| format!("Collidable {index}")),
                        item_index,
                        class_name: self
                            .type_names
                            .get(item.type_index as usize)
                            .cloned()
                            .unwrap_or_default(),
                        translation: self.vec4(item.data_offset + 80),
                        axis_x: self.vec4(item.data_offset + 32),
                        axis_y: self.vec4(item.data_offset + 48),
                        axis_z: self.vec4(item.data_offset + 64),
                        enabled: self
                            .data_bytes(item.data_offset + 159, 1)
                            .is_some_and(|b| b[0] != 0),
                        shape: self.read_shape(item),
                        shape_class_name,
                        shape_kind,
                        // hclCollidable keeps its pinch settings right before
                        // the enabled flag: radius, priority, then two flags.
                        pinch_detection_radius: self
                            .f32(item.data_offset + 152)
                            .unwrap_or_default(),
                        pinch_detection_priority: self
                            .data_bytes(item.data_offset + 156, 1)
                            .map_or(0, |b| b[0] as i8),
                        pinch_detection_enabled: self
                            .data_bytes(item.data_offset + 157, 1)
                            .is_some_and(|b| b[0] != 0),
                        virtual_collision_point_collision_enabled: self
                            .data_bytes(item.data_offset + 158, 1)
                            .is_some_and(|b| b[0] != 0),
                    }
                })
                .collect();
            let cloth = self.reference_array(base + 40);
            self.cloth = cloth
                .into_iter()
                .enumerate()
                .map(|(index, item_index)| {
                    let item = &self.items[item_index];
                    Cloth {
                        index,
                        name: self
                            .string_ptr(item.data_offset + 24)
                            .unwrap_or_else(|| format!("Cloth {index}")),
                        item_index,
                        simulations: self
                            .reference_array(item.data_offset + 32)
                            .into_iter()
                            .enumerate()
                            .map(|(i, si)| self.read_sim(i, si))
                            .collect(),
                    }
                })
                .collect();
        }

        if let Some(root) = self
            .variants
            .iter()
            .find(|variant| variant.class_name == "hkaAnimationContainer")
        {
            let base = self.items[root.item_index].data_offset;
            self.skeletons = self
                .reference_array(base + 24)
                .into_iter()
                .enumerate()
                .map(|(index, item_index)| {
                    let item = &self.items[item_index];
                    Skeleton {
                        index,
                        name: self
                            .string_ptr(item.data_offset + 24)
                            .unwrap_or_else(|| format!("Skeleton {index}")),
                        item_index,
                        bones: self.read_skeleton_bones(item.data_offset),
                    }
                })
                .collect();
        }
        self.cloth_skeleton_pairs = pair_cloths_and_skeletons(&self.cloth, &self.skeletons);
        Ok(())
    }

    pub fn paired_skeleton(&self, cloth_index: usize) -> Option<&Skeleton> {
        let pair = self
            .cloth_skeleton_pairs
            .iter()
            .find(|pair| pair.index == cloth_index)?;
        self.skeletons
            .iter()
            .find(|skeleton| skeleton.item_index == pair.skeleton_item_index)
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.to_bytes() != self.raw {
            return Err(invalid("raw roundtrip changed bytes"));
        }
        let mut seen = HashSet::new();
        for p in &self.patches {
            for o in &p.offsets {
                if !seen.insert(*o) {
                    return Err(invalid("duplicate PTCH offset"));
                }
            }
        }
        self.validate_item_graph()?;
        for cloth in &self.cloth {
            let closure = self.collect_item_closure([cloth.item_index])?;
            let patches = self.patches_for_items(closure.iter().copied())?;
            let copied = self.copy_item_closure(closure, Vec::new())?;
            let item_map: std::collections::HashMap<usize, usize> = copied
                .ranges_by_old_start
                .keys()
                .flat_map(|start| {
                    self.items
                        .iter()
                        .enumerate()
                        .filter(move |(_, item)| item.data_offset == *start)
                        .map(|(index, _)| (index, index))
                })
                .collect();
            let mut relocated = copied.data;
            self.relocate_copied_pointers(
                &mut relocated,
                &patches,
                &copied.ranges_by_old_start,
                &item_map,
                &std::collections::HashMap::new(),
            )?;
        }
        Ok(())
    }
}
fn pair_cloths_and_skeletons(cloths: &[Cloth], skeletons: &[Skeleton]) -> Vec<ClothSkeletonPair> {
    cloths
        .iter()
        .zip(skeletons)
        .enumerate()
        .map(|(index, (cloth, skeleton))| ClothSkeletonPair {
            index,
            cloth_item_index: cloth.item_index,
            skeleton_item_index: skeleton.item_index,
        })
        .collect()
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, s)
}
fn varuint(data: &[u8], p: &mut usize, end: usize) -> io::Result<u64> {
    let first = *data.get(*p).ok_or_else(|| invalid("truncated VarUInt"))?;
    *p += 1;
    if first & 0x80 == 0 {
        return Ok(first as u64);
    }
    let marker = first >> 3;
    let n = match marker {
        0x10..=0x17 => 1,
        0x18..=0x1b => 2,
        0x1c => 3,
        0x1d => 4,
        0x1e => 7,
        _ => return Err(invalid("unsupported VarUInt")),
    };
    let mask = match marker {
        0x10..=0x17 => 0x3f,
        0x18..=0x1b => 0x1f,
        _ => 7,
    };
    let mut v = (first & mask) as u64;
    for _ in 0..n {
        if *p >= end {
            return Err(invalid("truncated VarUInt"));
        }
        v = (v << 8) | data[*p] as u64;
        *p += 1
    }
    Ok(v)
}

#[cfg(test)]
mod pairing_tests {
    use super::*;

    #[test]
    fn cloth_and_skeleton_arrays_pair_by_index_and_preserve_extras() {
        let cloths = vec![
            Cloth {
                index: 0,
                name: "A".into(),
                item_index: 10,
                simulations: vec![],
            },
            Cloth {
                index: 1,
                name: "B".into(),
                item_index: 11,
                simulations: vec![],
            },
        ];
        let skeletons = vec![Skeleton {
            index: 0,
            name: "A skeleton".into(),
            item_index: 20,
            bones: vec![],
        }];
        assert_eq!(
            pair_cloths_and_skeletons(&cloths, &skeletons),
            vec![ClothSkeletonPair {
                index: 0,
                cloth_item_index: 10,
                skeleton_item_index: 20
            }]
        );
        assert_eq!(cloths.len(), 2);
        assert_eq!(skeletons.len(), 1);
    }
}

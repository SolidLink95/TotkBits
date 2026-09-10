use super::{f32_at, i16_at, read_string, u16_at, u32_at, u64_at, BfresBone, BfresError, Endian};

pub fn parse_skeleton(
    data: &[u8],
    offset: usize,
    endian: Endian,
    version_major: u8,
) -> Result<(Vec<BfresBone>, Vec<u16>), BfresError> {
    let is_v8 = version_major <= 8;
    // Version 0.9 (Nintendo Switch Sports) uses the version 10 FSKL header
    // but still stores the older 96-byte bone records, whose scalar and
    // transform fields sit eight bytes later than in version 10.
    let legacy_bones = version_major <= 9;
    let pointer_shift = usize::from(is_v8) * 8;
    let scalar_shift = usize::from(is_v8) * 20;
    let bones_offset = u64_at(data, offset + 16 + pointer_shift, endian)? as usize;
    let palette_offset = u64_at(data, offset + 24 + pointer_shift, endian)? as usize;
    let count = u16_at(data, offset + 56 + scalar_shift, endian)? as usize;
    let palette_count = u16_at(data, offset + 58 + scalar_shift, endian)? as usize
        + u16_at(data, offset + 60 + scalar_shift, endian)? as usize;
    let quaternion_rotation = if version_major == 9 {
        // Version 0.9 keeps the flags right after the signature; rotation mode
        // lives in bits 12-14 (0 = quaternion, 1 = Euler XYZ).
        u32_at(data, offset + 4, endian)? & 0x7000 == 0
    } else {
        u32_at(data, offset + 48 + scalar_shift, endian)? & 1 != 0
    };
    let entry_size = if legacy_bones { 96 } else { 88 };
    let bone_scalar_shift = if legacy_bones { 8 } else { 0 };
    let bone_transform_shift = if legacy_bones { 8 } else { 0 };
    let mut bones = Vec::with_capacity(count);
    for index in 0..count {
        let entry = bones_offset + index * entry_size;
        bones.push(BfresBone {
            name: read_string(data, u64_at(data, entry, endian)?)
                .unwrap_or_else(|| format!("Bone_{index}")),
            parent_index: i16_at(data, entry + 34 + bone_scalar_shift, endian)?,
            smooth_matrix_index: i16_at(data, entry + 36 + bone_scalar_shift, endian)?,
            rigid_matrix_index: i16_at(data, entry + 38 + bone_scalar_shift, endian)?,
            // Rotation mode belongs to FSKL, not each Bone. BfresLibrary's
            // SkeletonFlagsRotation uses zero for EulerXYZ and one for Quaternion.
            rotation_mode: if quaternion_rotation {
                "quaternion".into()
            } else {
                "euler_xyz".into()
            },
            scale: [
                f32_at(data, entry + 48 + bone_transform_shift, endian)?,
                f32_at(data, entry + 52 + bone_transform_shift, endian)?,
                f32_at(data, entry + 56 + bone_transform_shift, endian)?,
            ],
            rotation: [
                f32_at(data, entry + 60 + bone_transform_shift, endian)?,
                f32_at(data, entry + 64 + bone_transform_shift, endian)?,
                f32_at(data, entry + 68 + bone_transform_shift, endian)?,
                f32_at(data, entry + 72 + bone_transform_shift, endian)?,
            ],
            translation: [
                f32_at(data, entry + 76 + bone_transform_shift, endian)?,
                f32_at(data, entry + 80 + bone_transform_shift, endian)?,
                f32_at(data, entry + 84 + bone_transform_shift, endian)?,
            ],
        });
    }
    let palette = (0..palette_count)
        .map(|index| u16_at(data, palette_offset + index * 2, endian))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((bones, palette))
}

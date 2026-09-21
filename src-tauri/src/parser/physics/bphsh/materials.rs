//! TOTK Phive material and shape-tag names, from `Phive/Config/PhiveConfig.byml`
//! (Bootup pack, TOTK 1.2.1): `MaterialCollection` order gives the material id
//! stored in a `.bphsh` material table, `UserShapeTagMaskCollection` gives the
//! bit names of its 64-bit flags word, and `LayerEntityCollection` the bit
//! order of the collision mask table.

/// `MaterialCollection` component names; the index is the material id.
pub const MATERIAL_NAMES: &[&str] = &[
    "Undefined",
    "Soil",
    "Grass",
    "Sand",
    "HeavySand",
    "Snow",
    "HeavySnow",
    "Stone",
    "StoneSlip",
    "StoneNoSlip",
    "SlipBoard",
    "Cart",
    "Metal",
    "MetalSlip",
    "MetalNoSlip",
    "WireNet",
    "Wood",
    "Ice",
    "Cloth",
    "Glass",
    "Bone",
    "Rope",
    "Character",
    "Ragdoll",
    "Surfing",
    "GuardianFoot",
    "LaunchPad",
    "Conveyer",
    "Rail",
    "Grudge",
    "Meat",
    "Vegetable",
    "Bomb",
    "MagicBall",
    "Barrier",
    "AirWall",
    "GrudgeSlow",
    "Tar",
    "Water",
    "HotWater",
    "IceWater",
    "Lava",
    "Bog",
    "ContaminatedWater",
    "DungeonCeil",
    "Gas",
    "InvalidateRestartPos",
    "HorseSpeedLimit",
    "ForbidDynamicCuttingAreaForHugeCharacter",
    "ForbidHorseReturnToSafePosByClimate",
    "Dragon",
    "ReferenceSurfing",
    "WaterSlip",
    "HorseDeleteImmediatelyOnDeath",
];

/// `UserShapeTagMaskCollection`: (name, mask). Single-bit masks name the
/// bits of a material flags word; compound masks are presets.
pub const USER_SHAPE_TAGS: &[(&str, u64)] = &[
    ("DisableSurfaceVelocity", 0x2000000),
    ("DisableRespawn", 0x1000000),
    ("NoStick", 0x8000),
    ("NoClimb", 0x1),
    ("Ladder", 0x4),
    ("LadderUp", 0x2),
    ("LadderSide", 0x10),
    ("Slip", 0x8),
    ("NoSlipRainOnClimb", 0x20),
    ("NoDashUpAndNoClimb", 0x41),
    ("Abyss", 0x80),
    ("TopBroadLeafTree", 0x200),
    ("TopConiferousTree", 0x400),
    ("Attach", 0x0),
    ("NarrowPlace", 0x100),
    ("StopUntilStickNeutral", 0x8000000),
    ("EdgeOfTheWorld", 0x800000),
    ("Wet", 0x400000),
    ("EnableWallJump", 0x200000),
    ("RoofWall_InCamera", 0x70000),
    ("RoofWall_OutCamera_NotStat", 0xE0000),
    ("HyurleCastle", 0x100000),
    ("RoofWall_OutCamera", 0x60000),
    ("Area_Roof_OutCamera", 0x40000),
    ("Remains", 0x0),
    ("Roof_InCamera", 0x50000),
    ("FlowStraight", 0x0),
    ("FlowLeft", 0x0),
    ("FlowRight", 0x0),
    ("NoImpulseUpperMove", 0x0),
    ("NoPreventFall", 0x4000),
    ("DontIgnoreInertiaForce", 0x10000000),
    ("HeadShot", 0x20000000),
    ("DontGenerateNavMesh", 0x40000000),
    ("Unridable", 0x80000000),
    ("SightDown", 0x100000000),
    ("DisableImpulse", 0x4000000),
    ("Miasma", 0x200000000),
    ("MiasmaLv1", 0x800000000),
    ("MiasmaLv3", 0x800000000000000),
    ("IgnoredByNavMesh", 0x80000000000),
    ("CeilingClipperForceAccept", 0x100000000000),
    ("StopCeilingClipper", 0x200000000000),
    ("Spike_Y_Minus", 0x400000000000),
    ("ForbidAutoPlacementAndEnemyEntry", 0x107F000000000),
    ("ForbidAutoPlacementAll", 0x103F000000000),
    ("Dragon", 0x800004000000),
    ("BakeInstanceID", 0x2000000000000),
    ("NoEntrySage", 0x4000000000000),
    ("MiasmaCancel", 0x8000000000000),
    ("Sunlight", 0x10000000000000),
    ("Confusion", 0x20000000000000),
    ("SkyLomeiWall", 0x40000000000041),
    ("Fruit", 0x80000000000000),
    ("ShrineEntrance", 0x1100000000000000),
    ("NavMeshNPCOnly", 0x200000000000000),
    ("NavMeshGiantNoEntry", 0x400000000000000),
    ("CameraAtBlock", 0x1000000000000000),
    ("MiasmaEntity", 0x2000000000000000),
    ("IgnoreAttach", 0x4000000000000000),
    ("IgnoreBondConnection", 0x8000000000000000),
    ("SwitchHit", 0x40000000000041),
    ("TrolleyRotationFloor", 0x1000),
    ("InvalidateRestartPosForHorse", 0x2000),
];

/// `LayerEntityCollection` names; the index is the bit of a collision mask.
pub const LAYER_ENTITY_NAMES: &[&str] = &[
    "Object",
    "SmallObject",
    "GroundObject",
    "Player",
    "Friendly",
    "Character",
    "HugeCharacter",
    "Water",
    "GroundHighRes",
    "GroundHighResSmooth",
    "GroundHighResRough",
    "GroundStaticTree",
    "HitOnlyGround",
    "CharacterSight",
    "CameraSight",
    "CameraBody",
    "NoHit",
    "CustomReceiver",
    "Fluid",
    "UltraHandBlocker",
    "Smoke",
    "Gas",
    "Tar",
    "LowTree",
    "IK",
    "RiddenRaumiGolem",
];

/// Name of a material id, when known.
pub fn material_name(id: i32) -> Option<&'static str> {
    usize::try_from(id)
        .ok()
        .and_then(|i| MATERIAL_NAMES.get(i).copied())
}

/// Material id of a name (case sensitive), when known.
pub fn material_id(name: &str) -> Option<i32> {
    MATERIAL_NAMES
        .iter()
        .position(|n| *n == name)
        .map(|i| i as i32)
}

/// Names of the single-bit user shape tags set in `flags`, plus a hex
/// literal for any bit no name covers.
pub fn flag_names(flags: u64) -> Vec<String> {
    let mut out = Vec::new();
    let mut covered = 0u64;
    for (name, mask) in USER_SHAPE_TAGS {
        if mask.count_ones() == 1 && flags & mask != 0 {
            out.push((*name).to_string());
            covered |= mask;
        }
    }
    let rest = flags & !covered;
    if rest != 0 {
        out.push(format!("{rest:#x}"));
    }
    out
}

/// Flags word for a list of tag names (single-bit or preset) and hex literals.
pub fn flags_from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<u64, String> {
    let mut flags = 0u64;
    for name in names {
        let name = name.trim();
        if let Some(hex) = name.strip_prefix("0x").or_else(|| name.strip_prefix("0X")) {
            flags |= u64::from_str_radix(hex, 16).map_err(|e| format!("{name}: {e}"))?;
            continue;
        }
        match USER_SHAPE_TAGS.iter().find(|(n, _)| *n == name) {
            Some((_, mask)) => flags |= mask,
            None => return Err(format!("unknown user shape tag {name}")),
        }
    }
    Ok(flags)
}

/// Layer names whose bit is *clear* in `mask` (the layers a material does
/// not collide with), plus a hex literal for unnamed clear bits.
pub fn disabled_layer_names(mask: u64) -> Vec<String> {
    let mut out = Vec::new();
    let mut unnamed = 0u64;
    for bit in 0..64 {
        if mask & (1u64 << bit) != 0 {
            continue;
        }
        match LAYER_ENTITY_NAMES.get(bit) {
            Some(name) => out.push((*name).to_string()),
            None => unnamed |= 1u64 << bit,
        }
    }
    if unnamed != 0 {
        out.push(format!("{unnamed:#x}"));
    }
    out
}

/// Collision mask from the names of the layers to disable (all other bits set).
pub fn mask_from_disabled_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Result<u64, String> {
    let mut mask = u64::MAX;
    for name in names {
        let name = name.trim();
        if let Some(hex) = name.strip_prefix("0x").or_else(|| name.strip_prefix("0X")) {
            mask &= !u64::from_str_radix(hex, 16).map_err(|e| format!("{name}: {e}"))?;
            continue;
        }
        match LAYER_ENTITY_NAMES.iter().position(|n| *n == name) {
            Some(bit) => mask &= !(1u64 << bit),
            None => return Err(format!("unknown layer {name}")),
        }
    }
    Ok(mask)
}

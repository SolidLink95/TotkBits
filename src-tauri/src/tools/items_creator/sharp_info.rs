//! SharpInfo generation for weapon modifier initialization.

use crate::{file_format::BinTextFile::BymlFile, Zstd::TotkZstd};
use roead::byml::Byml;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const SHARP_INFO_PATH: &str =
    "GameParameter/SharpInfo/Default.game__weapon__SharpInfoTable.bgyml";

pub fn generate_weapon_sharp_info(
    clean_romfs: &Path,
    output_romfs: &Path,
    actor_name: &str,
    template_actor: &str,
    zstd: Arc<TotkZstd<'_>>,
) -> io::Result<PathBuf> {
    super::assets::ensure_output_outside_romfs(clean_romfs, output_romfs)?;
    let output = output_romfs.join(SHARP_INFO_PATH);
    let clean_source = clean_romfs.join(SHARP_INFO_PATH);
    let source = if output.is_file() {
        &output
    } else {
        &clean_source
    };
    let mut file = BymlFile::new(&source, zstd.clone())
        .ok_or_else(|| invalid_data("invalid clean SharpInfo table"))?;
    // The table is split per weapon family (SharpInfoList for melee weapons,
    // SharpInfoBowList, SharpInfoShieldList); the custom row joins the list
    // that holds its template, or failing that any weapon of the same kind.
    let kind_prefix = template_actor
        .rfind('_')
        .map(|index| &template_actor[..=index])
        .unwrap_or(template_actor);
    let list_key = sharp_info_list_key(&file.pio, template_actor, kind_prefix)?;
    for key in sharp_info_list_keys(&file.pio)? {
        if find_row(list_rows(&file.pio, &key)?, actor_name).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("SharpInfo already contains {actor_name}"),
            ));
        }
    }
    let rows = list_rows_mut(&mut file.pio, &list_key)?;
    let mut row = find_row(rows, template_actor)
        .or_else(|| {
            rows.iter()
                .find(|row| row_actor(row).is_some_and(|name| name.starts_with(kind_prefix)))
        })
        .cloned()
        .ok_or_else(|| {
            invalid_data(format!(
                "SharpInfo has no row for {template_actor} or any {kind_prefix}* weapon"
            ))
        })?;
    let map = row
        .as_mut_map()
        .map_err(|_| invalid_data("SharpInfo template row is not a map"))?;
    require_key(map, "ActorFilePath")?;
    require_key(map, "ActorName")?;
    require_key(map, "ActorNameHash")?;
    map.insert(
        "ActorFilePath".into(),
        Byml::String(format!("Work/Actor/{actor_name}.engine__actor__ActorParam.gyml").into()),
    );
    map.insert("ActorName".into(), Byml::String(actor_name.into()));
    map.insert(
        "ActorNameHash".into(),
        Byml::U32(super::gamedata::murmur3_hash(actor_name)),
    );
    rows.push(row);
    rows.sort_by(|left, right| row_actor(left).cmp(&row_actor(right)));

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    file.save(output.to_string_lossy().into_owned())?;
    let saved = BymlFile::new(&output, zstd)
        .ok_or_else(|| invalid_data("generated SharpInfo cannot be reopened"))?;
    let saved_rows = list_rows(&saved.pio, &list_key)?;
    let saved_row = find_row(saved_rows, actor_name)
        .ok_or_else(|| invalid_data("generated SharpInfo row is missing"))?;
    let saved_map = saved_row
        .as_map()
        .map_err(|_| invalid_data("generated SharpInfo row is not a map"))?;
    if saved_map.get("ActorNameHash") != Some(&Byml::U32(super::gamedata::murmur3_hash(actor_name)))
    {
        return Err(invalid_data("generated SharpInfo hash is incorrect"));
    }
    Ok(output)
}

fn find_row<'a>(rows: &'a [Byml], actor_name: &str) -> Option<&'a Byml> {
    rows.iter()
        .find(|row| row_actor(row).is_some_and(|name| name == actor_name))
}

/// Every top-level array of the table, in file order.
fn sharp_info_list_keys(root: &Byml) -> io::Result<Vec<String>> {
    Ok(root
        .as_map()
        .map_err(|_| invalid_data("SharpInfo root is not a map"))?
        .iter()
        .filter(|(_, value)| value.as_array().is_ok())
        .map(|(key, _)| key.to_string())
        .collect())
}

/// The list holding `template_actor`, else the first list with a `kind_prefix` row.
fn sharp_info_list_key(root: &Byml, template_actor: &str, kind_prefix: &str) -> io::Result<String> {
    let keys = sharp_info_list_keys(root)?;
    for key in &keys {
        if find_row(list_rows(root, key)?, template_actor).is_some() {
            return Ok(key.clone());
        }
    }
    for key in &keys {
        if list_rows(root, key)?
            .iter()
            .any(|row| row_actor(row).is_some_and(|name| name.starts_with(kind_prefix)))
        {
            return Ok(key.clone());
        }
    }
    Err(invalid_data(format!(
        "SharpInfo has no list containing {template_actor} or any {kind_prefix}* weapon"
    )))
}

fn list_rows<'a>(root: &'a Byml, key: &str) -> io::Result<&'a [Byml]> {
    root.as_map()
        .map_err(|_| invalid_data("SharpInfo root is not a map"))?
        .get(key)
        .ok_or_else(|| invalid_data(format!("SharpInfo root has no {key}")))?
        .as_array()
        .map_err(|_| invalid_data(format!("{key} is not an array")))
}

fn list_rows_mut<'a>(root: &'a mut Byml, key: &str) -> io::Result<&'a mut Vec<Byml>> {
    root.as_mut_map()
        .map_err(|_| invalid_data("SharpInfo root is not a map"))?
        .get_mut(key)
        .ok_or_else(|| invalid_data(format!("SharpInfo root has no {key}")))?
        .as_mut_array()
        .map_err(|_| invalid_data(format!("{key} is not an array")))
}

fn row_actor(row: &Byml) -> Option<&str> {
    row.as_map()
        .ok()?
        .get("ActorName")?
        .as_string()
        .ok()
        .map(|value| value.as_str())
}

fn require_key(map: &roead::byml::Map, key: &str) -> io::Result<()> {
    map.contains_key(key)
        .then_some(())
        .ok_or_else(|| invalid_data(format!("SharpInfo template has no {key}")))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharp_info_hashes_match_working_rows() {
        assert_eq!(
            super::super::gamedata::murmur3_hash("Weapon_Lsword_005"),
            0x2412_FFDC
        );
        assert_eq!(
            super::super::gamedata::murmur3_hash("Weapon_Lsword_108"),
            0x17BA_2BC2
        );
    }
}

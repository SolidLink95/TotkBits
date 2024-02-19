use anyhow::Context;
use byteordered::Endianness;
use clap::ArgMatches;
use msbt::Encoding;
use rayon::prelude::*;

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use crate::{model::Msyt, subcommand::find_files, Result};

pub fn create(matches: &ArgMatches) -> Result<()> {
    let input_paths: Vec<&str> = matches
        .values_of("paths")
        .expect("required clap arg")
        .collect();
    let paths: Vec<PathBuf> = if matches.is_present("dir_mode") {
        find_files(input_paths.iter().cloned(), "msyt")?
    } else {
        input_paths.iter().map(PathBuf::from).collect()
    };

    let endianness = match matches.value_of("platform").expect("required clap arg") {
        "switch" => Endianness::Little,
        "wiiu" => Endianness::Big,
        _ => unreachable!("clap arg with possible values"),
    };
    let encoding = match matches.value_of("encoding").expect("clap arg with default") {
        "utf16" => Encoding::Utf16,
        "utf8" => Encoding::Utf8,
        _ => unreachable!("clap arg with possible values"),
    };
    let extension = matches
        .value_of("extension")
        .expect("clap arg with default");
    let backup = !matches.is_present("no-backup");
    let output = Path::new(matches.value_of("output").expect("required clap arg"));
    if !output.exists() {
        std::fs::create_dir_all(output)
            .with_context(|| format!("could not create dir {}", output.to_string_lossy()))?;
    } else if !output.is_dir() {
        anyhow::bail!("output directory is not a directory");
    }

    paths
        .into_par_iter()
        .map(|path| {
            let msyt_file = File::open(&path)
                .with_context(|| format!("could not open file {}", path.to_string_lossy()))?;
            let msyt: Msyt =
                serde_yaml::from_reader(BufReader::new(msyt_file)).with_context(|| {
                    format!("could not read valid yaml from {}", path.to_string_lossy())
                })?;
            let stripped_path = match input_paths
                .iter()
                .flat_map(|input| path.strip_prefix(input))
                .next()
            {
                Some(s) => s,
                None => {
                    return Err(anyhow::anyhow!(
                        "no input path works as a prefix on {}",
                        path.to_string_lossy()
                    ))
                }
            };
            let dest_path = output.join(stripped_path).with_extension(extension);
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("could not create directory {}", parent.to_string_lossy())
                })?;
            }

            if backup && dest_path.exists() {
                let backup_path = dest_path.with_extension(format!("{}.bak", extension));
                std::fs::rename(&dest_path, &backup_path).with_context(|| {
                    format!(
                        "could not backup {} to {}",
                        dest_path.to_string_lossy(),
                        backup_path.to_string_lossy()
                    )
                })?;
            }

            let new_msbt = File::create(&dest_path).with_context(|| {
                format!("could not create file {}", dest_path.to_string_lossy())
            })?;
            msyt.write_as_msbt_with_encoding(&mut BufWriter::new(new_msbt), encoding, endianness)
                .with_context(|| {
                    format!("could not write msbt to {}", dest_path.to_string_lossy())
                })?;

            Ok(())
        })
        .collect::<Result<_>>()
}

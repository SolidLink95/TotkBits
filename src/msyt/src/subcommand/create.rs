use byteordered::Endianness;
use clap::ArgMatches;
use failure::ResultExt;
use msbt::{
  Encoding,
  builder::MsbtBuilder,
};
use rayon::prelude::*;

use std::{
  fs::File,
  io::{BufReader, BufWriter},
  path::{Path, PathBuf},
};

use crate::{
  Result,
  model::{Msyt, Content},
  subcommand::find_files,
};

pub fn create(matches: &ArgMatches) -> Result<()> {
  let input_paths: Vec<&str> = matches.values_of("paths").expect("required clap arg").collect();
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
  let extension = matches.value_of("extension").expect("clap arg with default");
  let backup = !matches.is_present("no-backup");
  let output = Path::new(matches.value_of("output").expect("required clap arg"));
  if !output.exists() {
    std::fs::create_dir_all(&output)
      .with_context(|_| format!("could not create dir {}", output.to_string_lossy()))?;
  } else if !output.is_dir() {
    failure::bail!("output directory is not a directory");
  }

  paths
    .into_par_iter()
    .map(|path| {
      let msyt_file = File::open(&path).with_context(|_| format!("could not open file {}", path.to_string_lossy()))?;
      let msyt: Msyt = serde_yaml::from_reader(BufReader::new(msyt_file))
        .with_context(|_| format!("could not read valid yaml from {}", path.to_string_lossy()))?;

      let mut builder = MsbtBuilder::new(endianness, encoding, Some(msyt.msbt.group_count));
      if let Some(unknown_bytes) = msyt.msbt.ato1 {
        builder = builder.ato1(msbt::section::Ato1::new_unlinked(unknown_bytes));
      }
      if let Some(unknown_1) = msyt.msbt.atr1_unknown {
        // ATR1 should have exactly the same amount of entries as TXT2. In the BotW files, sometimes
        // an ATR1 section is specified to have that amount but the section is actually empty. For
        // msyt's purposes, if the msyt does not contain the same amount of attributes as it does
        // text entries (i.e. not every label has an `attributes` node), it will be assumed that the
        // ATR1 section should specify that it has the correct amount of entries but actually be
        // empty.
        let strings: Option<Vec<String>> = msyt.entries
          .iter()
          .map(|(_, e)| e.attributes.clone())
          .map(|s| s.map(crate::util::append_nul))
          .collect();
        let atr_len = match strings {
          Some(ref s) => s.len(),
          None => msyt.entries.len(),
        };
        let strings = strings.unwrap_or_default();
        builder = builder.atr1(msbt::section::Atr1::new_unlinked(atr_len as u32, unknown_1, strings));
      }
      if let Some(unknown_bytes) = msyt.msbt.tsy1 {
        builder = builder.tsy1(msbt::section::Tsy1::new_unlinked(unknown_bytes));
      }
      if let Some(nli1) = msyt.msbt.nli1 {
        builder = builder.nli1(msbt::section::Nli1::new_unlinked(nli1.id_count, nli1.global_ids));
      }
      for (label, entry) in msyt.entries.into_iter() {
        let new_val = Content::write_all(builder.header(), &entry.contents)?;
        builder = builder.add_label(label, new_val);
      }
      let msbt = builder.build();

      let stripped_path = match input_paths.iter().flat_map(|input| path.strip_prefix(input)).next() {
        Some(s) => s,
        None => failure::bail!("no input path works as a prefix on {}", path.to_string_lossy()),
      };
      let dest_path = output.join(stripped_path).with_extension(extension);
      if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)
          .with_context(|_| format!("could not create directory {}", parent.to_string_lossy()))?;
      }

      if backup && dest_path.exists() {
        let backup_path = dest_path.with_extension(format!("{}.bak", extension));
        std::fs::rename(&dest_path, &backup_path)
          .with_context(|_| format!("could not backup {} to {}", dest_path.to_string_lossy(), backup_path.to_string_lossy()))?;
      }

      let new_msbt = File::create(&dest_path)
        .with_context(|_| format!("could not create file {}", dest_path.to_string_lossy()))?;
      msbt.write_to(BufWriter::new(new_msbt))
        .with_context(|_| format!("could not write msbt to {}", dest_path.to_string_lossy()))?;

      Ok(())
    })
    .collect::<Result<_>>()
}

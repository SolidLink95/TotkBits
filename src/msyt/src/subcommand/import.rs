use clap::ArgMatches;
use msbt::Msbt;
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

pub fn import(matches: &ArgMatches) -> Result<()> {
  let input_paths: Vec<&str> = matches.values_of("paths").expect("required clap arg").collect();
  let paths: Vec<PathBuf> = if matches.is_present("dir_mode") {
    find_files(input_paths.iter().map(Clone::clone), "msyt")?
  } else {
    input_paths.iter().map(PathBuf::from).collect()
  };
  let output_path = matches.value_of("output").map(Path::new);

  let extension = matches.value_of("extension").expect("clap arg with default");
  let backup = !matches.is_present("no-backup");

  paths
    .into_par_iter()
    .map(|path| {
      let msyt_file = File::open(&path)?;
      let msyt: Msyt = serde_yaml::from_reader(BufReader::new(msyt_file))?;

      let msbt_path = path.with_extension("msbt");
      let msbt_file = File::open(&msbt_path)?;

      let mut msbt = Msbt::from_reader(BufReader::new(msbt_file))?;

      for (key, entry) in msyt.entries {
        let new_val = Content::write_all(msbt.header(), &entry.contents)?;
        if let Some(ref mut lbl1) = msbt.lbl1_mut() {
          if let Some(label) = lbl1.labels_mut().iter_mut().find(|x| x.name() == key) {
            if let Err(()) = label.set_value_raw(new_val) {
              failure::bail!("could not set raw string at index {}", label.index());
            }
          }
        }
      }

      let dest_path = match output_path {
        Some(output) => {
          let stripped_path = match input_paths.iter().flat_map(|input| path.strip_prefix(input)).next() {
            Some(s) => s,
            None => failure::bail!("no input path works as a prefix on {}", path.to_string_lossy()),
          };
          output.join(stripped_path).with_extension(extension)
        },
        None => path.with_extension(extension),
      };
      if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
      }

      if backup && dest_path.exists() {
        let backup_path = dest_path.with_extension(format!("{}.bak", extension));
        std::fs::rename(&dest_path, backup_path)?;
      }

      let new_msbt = File::create(&dest_path)?;
      msbt.write_to(BufWriter::new(new_msbt))?;

      Ok(())
    })
    .collect::<Result<_>>()
}

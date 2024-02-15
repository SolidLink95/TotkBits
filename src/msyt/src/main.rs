#![warn(bare_trait_objects)]

mod botw;
mod cli;
mod model;
mod subcommand;
mod util;

pub type Result<T> = std::result::Result<T, failure::Error>;

fn main() {
  std::process::exit(match inner() {
    Ok(()) => 0,
    Err(e) => {
      eprintln!("an error occurred - see below for details");
      eprintln!();
      eprintln!("{}", e);
      for (indent, err) in e.iter_causes().enumerate() {
        let indent_str: String = std::iter::repeat("  ").take(indent + 1).collect();
        eprintln!("{}{}", indent_str, err);
      }
      1
    },
  });
}

fn inner() -> Result<()> {
  let matches = self::cli::app().get_matches();

  match matches.subcommand() {
    ("export", Some(sub_matches)) => self::subcommand::export(sub_matches),
    ("import", Some(sub_matches)) => self::subcommand::import(sub_matches),
    ("create", Some(sub_matches)) => self::subcommand::create(sub_matches),
    _ => unreachable!("clap allowed an unspecified subcommand"),
  }
}

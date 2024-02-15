use clap::{App, AppSettings, Arg, SubCommand};

pub fn app<'a, 'b: 'a>() -> App<'a, 'b> {
  App::new(clap::crate_name!())
    .version(clap::crate_version!())
    .author(clap::crate_authors!())
    .about(clap::crate_description!())

    .settings(&[
      AppSettings::SubcommandRequiredElseHelp,
      AppSettings::DeriveDisplayOrder,
      AppSettings::VersionlessSubcommands,
    ])

    .subcommand(SubCommand::with_name("import")
      .about("Import from MSYT files to MSBT files")

      .arg(Arg::with_name("dir_mode")
        .help("Allow specifying directories. msyt will search for all files with the correct extension in the provided directories.")
        .short("d")
        .long("directories")
        .alias("directory"))

      .arg(Arg::with_name("no-backup")
        .help("Do not create a backup of any existing output files")
        .short("B")
        .long("no-backup")
        .conflicts_with("backup"))

      .arg(Arg::with_name("extension")
        .help("The extension to use when exporting")
        .short("e")
        .long("extension")
        .alias("ext")
        .takes_value(true)
        .default_value("msbt"))

      .arg(Arg::with_name("output")
        .help("The directory to place output files in. If not specified, output files will be placed next to input files.")
        .short("o")
        .long("output")
        .takes_value(true))

      .arg(Arg::with_name("paths")
        .help("MSYT paths to import (MSBT files should be adjacent)")
        .required(true)
        .multiple(true)))
    .subcommand(SubCommand::with_name("create")
      .about("Create a MSBT file from a MSYT file")

      .arg(Arg::with_name("dir_mode")
        .help("Allow specifying directories. msyt will search for all files with the correct extension in the provided directories.")
        .short("d")
        .long("directories")
        .alias("directory"))

      .arg(Arg::with_name("no-backup")
        .help("Do not create a backup of any existing output files")
        .short("B")
        .long("no-backup")
        .conflicts_with("backup"))

      .arg(Arg::with_name("extension")
        .help("The extension to use for output files")
        .short("e")
        .long("extension")
        .alias("ext")
        .takes_value(true)
        .default_value("msbt"))

      .arg(Arg::with_name("platform")
        .help("The platform to create the MSBT for")
        .short("p")
        .long("platform")
        .takes_value(true)
        .required(true)
        .possible_values(&["switch", "wiiu"]))

      .arg(Arg::with_name("encoding")
        .help("The encoding to create the MSBT with")
        .short("E")
        .long("encoding")
        .takes_value(true)
        .required(true)
        .possible_values(&["utf16", "utf8"])
        .default_value("utf16"))

      .arg(Arg::with_name("output")
        .help("The directory to place output files in")
        .short("o")
        .long("output")
        .takes_value(true)
        .required(true))

      .arg(Arg::with_name("paths")
        .help("MSYT paths to create MSBT files from")
        .required(true)
        .multiple(true)))
    .subcommand(SubCommand::with_name("export")
      .about("Export from MSBT files to MSYT files")

      .arg(Arg::with_name("dir_mode")
        .help("Allow specifying directories. msyt will search for all files with the correct extension in the provided directories.")
        .short("d")
        .long("directories")
        .alias("directory"))

      .arg(Arg::with_name("output")
        .help("The directory to place output files in. If not specified, output files will be placed next to input files.")
        .short("o")
        .long("output")
        .takes_value(true))

      .arg(Arg::with_name("paths")
        .help("MSBT paths to export")
        .required(true)
        .multiple(true)))
}

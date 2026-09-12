use crate::{
    file_format::{
        Ainb::AinbFile,
        Archive::{ArchiveCodec, Rar::RarFile, SevenZipCmd::SevenZipCmd, Zip::ZipFile},
        BinTextFile::OpenedFile,
    },
    Open_and_Save::{get_binary_by_filetype, get_string_from_data},
    TotkConfig::TotkConfig,
    Zstd::{TotkFileType, ZstdDictionary},
};
use roead::{
    sarc::{Sarc, SarcWriter},
    Endian,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
pub struct CliCommand {
    operation: String,
    file_type: String,
    input: PathBuf,
    output: PathBuf,
    replacement_folder: Option<PathBuf>,
    /// Free-form arguments for operations with their own option syntax.
    extra: Vec<String>,
}

impl CliCommand {
    pub fn from_env() -> Option<Self> {
        let arguments: Vec<_> = env::args_os().collect();
        if !matches!(
            arguments.get(1).and_then(|v| v.to_str()),
            Some("-c" | "--cli")
        ) {
            return None;
        }
        let operation = arguments
            .get(2)
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let is_public_operation = matches!(
            operation.as_str(),
            "bin_to_text"
                | "text_to_bin"
                | "extract_archive"
                | "dir_to_archive"
                | "decompress"
                | "decompress_dir"
                | "compress"
                | "replace_bars_from_folder"
                | "batch_g1m_worker"
                | "replace_g1m"
                | "replace_bfres"
                | "g1m_to_fbx"
                | "lm3_render"
                | "lm3_render_all"
                | "lm3_slot_sizes"
                | "bfres_render"
                | "bfres_edit"
                | "bntx_edit"
                | "create_weapon"
        );
        let expected_arguments = if operation == "decompress" { 5 } else { 6 };
        let valid_arguments = if operation == "decompress_dir" {
            arguments.len() == 7
        } else if matches!(
            operation.as_str(),
            "bfres_edit" | "bntx_edit" | "create_weapon"
        ) {
            arguments.len() >= 3
        } else {
            arguments.len() == expected_arguments
        };
        if !is_public_operation || !valid_arguments {
            eprintln!("Usage:\n  Totkbits.exe --cli <bin_to_text|text_to_bin|extract_archive|dir_to_archive> <type> <input> <output>\n  Totkbits.exe --cli decompress <input> <output>\n  Totkbits.exe --cli decompress_dir -i <input_dir> -o <output_dir>\n  Totkbits.exe --cli compress <zs|pack|empty|bcett|yaz0> <input> <output>\n  Totkbits.exe --cli replace_bars_from_folder <input.bars> <audio-folder> <output.bars>\n  Totkbits.exe --cli replace_g1m <input.g1m> <input.fbx> <output.g1m>\n  Totkbits.exe --cli replace_bfres <input.bfres> <input.fbx> <output.bfres>\n  Totkbits.exe --cli g1m_to_fbx <none|png|dds> <input.g1m> <output.fbx>\n  Totkbits.exe --cli lm3_render <archive>_<slot> <lm3_romfs> <output.png>\n  Totkbits.exe --cli lm3_render_all <skip|overwrite> <lm3_romfs> <output_dir>\n  Totkbits.exe --cli lm3_slot_sizes all <lm3_romfs> <output.json>\n  Totkbits.exe --cli bfres_render <default|none|skin,hair,outfit> <input.bfres[.zs]> <output.png>\n  Totkbits.exe --cli bfres_edit -i <in.bfres[.mc]> -o <out.bfres.mc> [--swap_int_name N] [--swap_model_name N] [--fbx m.fbx [--import_skeleton]] [--rename_tex FROM TO]... [--totk_path <romfs>]\n  Totkbits.exe --cli bntx_edit -i <in.bntx[.zs]> -o <out.bntx[.zs]> [--swap_int_name N] [--rename_tex FROM TO]... [--replace_tex <image.png|.dds> [TEXTURE]]... [--export_tex DIR] [--astcenc <astcenc.exe>]\n  Totkbits.exe --cli create_weapon -i <spec.json|spec.toml> -o <output_romfs> [--totk_path <romfs>] [--zstd_level N] [--plan]\n  Totkbits.exe --cli create_weapon --rstb_only -o <output_romfs> [--totk_path <romfs>] [--zstd_level N]\n");
            return Some(Self {
                operation: String::new(),
                file_type: String::new(),
                input: PathBuf::new(),
                output: PathBuf::new(),
                replacement_folder: None,
                extra: Vec::new(),
            });
        }
        let cwd = env::current_dir().ok()?;
        let absolute = |value: &std::ffi::OsStr| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        };
        if matches!(
            operation.as_str(),
            "bfres_edit" | "bntx_edit" | "create_weapon"
        ) {
            return Some(Self {
                operation,
                file_type: String::new(),
                input: PathBuf::new(),
                output: PathBuf::new(),
                replacement_folder: None,
                extra: arguments[3..]
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect(),
            });
        }
        if operation == "decompress_dir" {
            let mut input = None;
            let mut output = None;
            for pair in arguments[3..].chunks_exact(2) {
                match pair[0].to_str() {
                    Some("-i") => input = Some(absolute(&pair[1])),
                    Some("-o") => output = Some(absolute(&pair[1])),
                    _ => {}
                }
            }
            Some(Self {
                operation,
                file_type: String::new(),
                input: input.unwrap_or_default(),
                output: output.unwrap_or_default(),
                replacement_folder: None,
                extra: Vec::new(),
            })
        } else if operation == "decompress" {
            Some(Self {
                operation,
                file_type: String::new(),
                input: absolute(&arguments[3]),
                output: absolute(&arguments[4]),
                replacement_folder: None,
                extra: Vec::new(),
            })
        } else if operation == "replace_bars_from_folder" {
            Some(Self {
                operation,
                file_type: "bars".into(),
                input: absolute(&arguments[3]),
                replacement_folder: Some(absolute(&arguments[4])),
                output: absolute(&arguments[5]),
                extra: Vec::new(),
            })
        } else {
            Some(Self {
                operation,
                file_type: arguments[3].to_string_lossy().to_ascii_lowercase(),
                input: absolute(&arguments[4]),
                output: absolute(&arguments[5]),
                replacement_folder: None,
                extra: Vec::new(),
            })
        }
    }

    pub fn execute(&self) -> Result<(), String> {
        if self.operation.is_empty() {
            return Err("missing CLI arguments".into());
        }
        match self.operation.as_str() {
            "bin_to_text" => self.bin_to_text(),
            "text_to_bin" => self.text_to_bin(),
            "extract_archive" => self.extract_archive(),
            "dir_to_archive" => self.dir_to_archive(),
            "decompress" => self.decompress(),
            "decompress_dir" => self.decompress_dir(),
            "compress" => self.compress(),
            "replace_bars_from_folder" => self.replace_bars_from_folder(),
            "batch_g1m_worker" => self.batch_g1m_worker(),
            "replace_g1m" => self.replace_g1m(),
            "replace_bfres" => self.replace_bfres(),
            "g1m_to_fbx" => self.g1m_to_fbx(),
            "lm3_render" => self.lm3_render(),
            "lm3_render_all" => self.lm3_render_all(),
            "lm3_slot_sizes" => self.lm3_slot_sizes(),
            "bfres_render" => self.bfres_render(),
            "bfres_edit" => self.bfres_edit(),
            "bntx_edit" => self.bntx_edit(),
            "create_weapon" => self.create_weapon(),
            value => Err(format!("unknown CLI operation: {value}")),
        }
    }

    fn zstd(&self) -> Result<Arc<crate::Zstd::TotkZstd<'static>>, String> {
        let mut config = TotkConfig::safe_new(false).map_err(|e| e.to_string())?;
        if let Ok(format) = env::var("TOTKBITS_XLINK_FORMAT") {
            let format = format.to_ascii_lowercase();
            if !matches!(format.as_str(), "legacy" | "modern") {
                return Err("TOTKBITS_XLINK_FORMAT must be legacy or modern".into());
            }
            config.xlink_format = format;
        }
        let config = Arc::new(config);
        let codec =
            crate::Zstd::TotkZstd::new(config.clone(), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .unwrap_or_else(|_| {
                    crate::Zstd::TotkZstd::dictionaryless(
                        config,
                        crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
                    )
                });
        Ok(Arc::new(codec))
    }

    fn bin_to_text(&self) -> Result<(), String> {
        let expected = parse_file_type(&self.file_type)?;
        let bytes = fs::read(&self.input).map_err(|e| format!("failed to read input: {e}"))?;
        if expected == TotkFileType::AINB {
            let text = AinbFile::binary_to_text(&bytes)
                .map_err(|e| format!("input could not be converted to AINB text: {e}"))?;
            return write_output(&self.output, text.as_bytes());
        }
        let is_zstandard = crate::Settings::Magic::is_zstd(&bytes);
        let (parsed, text) = get_string_from_data(&self.input, bytes, self.zstd()?).ok_or_else(|| {
            if is_zstandard {
                "input could not be converted to text; it may require a game Zstandard dictionary unavailable in lightweight CLI mode".to_string()
            } else {
                "input could not be converted to text".to_string()
            }
        })?;
        if parsed.file_type != expected
            && !(expected == TotkFileType::Byml && parsed.file_type == TotkFileType::Bcett)
        {
            return Err(format!(
                "input parsed as {:?}, not {:?}",
                parsed.file_type, expected
            ));
        }
        write_output(&self.output, text.as_bytes())
    }

    fn text_to_bin(&self) -> Result<(), String> {
        let file_type = parse_file_type(&self.file_type)?;
        let text = fs::read_to_string(&self.input)
            .map_err(|e| format!("failed to read input text: {e}"))?;
        let mut opened = OpenedFile::default();
        let output_name = self.output.to_string_lossy();
        let bytes = get_binary_by_filetype(
            file_type,
            &text,
            Endian::Little,
            self.zstd()?,
            &output_name,
            &mut opened,
            None,
            0,
            None,
            None,
            None,
            None,
            false,
        )
        .filter(|bytes| !bytes.is_empty())
        .ok_or_else(|| format!("conversion to {} produced no data", self.file_type))?;
        write_output(&self.output, &bytes)
    }

    fn extract_archive(&self) -> Result<(), String> {
        let bytes = fs::read(&self.input).map_err(|e| format!("failed to read archive: {e}"))?;
        let entries = archive_entries(&self.file_type, &bytes)?;
        fs::create_dir_all(&self.output).map_err(|e| e.to_string())?;
        for (name, data) in entries {
            let destination = safe_destination(&self.output, &name)?;
            write_output(&destination, &data)?;
        }
        Ok(())
    }

    fn dir_to_archive(&self) -> Result<(), String> {
        if !self.input.is_dir() {
            return Err("input must be a directory".into());
        }
        let mut entries = Vec::new();
        collect_directory(&self.input, &self.input, &mut entries)?;
        let bytes = build_archive(&self.file_type, entries)?;
        write_output(&self.output, &bytes)
    }

    fn decompress(&self) -> Result<(), String> {
        let bytes = fs::read(&self.input).map_err(|e| format!("failed to read input: {e}"))?;
        let decompressed = self
            .zstd()?
            .try_decompress_for_path(&self.input, &bytes)
            .map(|(data, _)| data)
            .or_else(|_| crate::compression::meshcodec::MeshCodec::decompress(&bytes))
            .map_err(|e| format!("failed to decompress input: {e}"))?;
        write_output(&self.output, &decompressed)
    }

    fn decompress_dir(&self) -> Result<(), String> {
        if self.input.as_os_str().is_empty() || self.output.as_os_str().is_empty() {
            return Err("decompress_dir requires -i <input_dir> and -o <output_dir>".into());
        }
        if !self.input.is_dir() {
            return Err(format!(
                "input must be a directory: {}",
                self.input.display()
            ));
        }
        if paths_are_equal(&self.input, &self.output)? {
            return Err("input and output directories cannot be the same".into());
        }

        let mut files = Vec::new();
        collect_file_paths(&self.input, &self.input, &mut files)?;
        let zstd = self.zstd()?;
        let mut decompressed_count = 0usize;
        let mut failed_count = 0usize;

        for (relative, source) in files {
            let bytes = match fs::read(&source) {
                Ok(bytes) => bytes,
                Err(error) => {
                    println!("Could not decompress {}: {error}", source.display());
                    failed_count += 1;
                    continue;
                }
            };
            let result = if crate::Settings::Magic::is_mcpk(&bytes) {
                crate::compression::meshcodec::MeshCodec::decompress(&bytes)
            } else {
                zstd.try_decompress_for_path(&source, &bytes)
                    .map(|(data, _)| data)
            };
            match result {
                Ok(data) => {
                    let destination = self.output.join(strip_zs_suffix(&relative));
                    write_output(&destination, &data)?;
                    decompressed_count += 1;
                }
                Err(error) => {
                    println!("Could not decompress {}: {error}", source.display());
                    failed_count += 1;
                }
            }
        }
        println!(
            "Decompressed {decompressed_count} file(s); could not decompress {failed_count} file(s)"
        );
        Ok(())
    }

    fn compress(&self) -> Result<(), String> {
        let dictionary = parse_dictionary(&self.file_type)?;
        let bytes = fs::read(&self.input).map_err(|e| format!("failed to read input: {e}"))?;
        let compressed = if dictionary == ZstdDictionary::Yaz0 {
            crate::Zstd::TotkZstd::compress_yaz0(&bytes)
        } else {
            self.zstd()?.compress_with_dictionary(&bytes, dictionary)
        }
        .map_err(|e| format!("failed to compress input: {e}"))?;
        write_output(&self.output, &compressed)
    }

    fn replace_bars_from_folder(&self) -> Result<(), String> {
        use std::collections::HashMap;

        let folder = self
            .replacement_folder
            .as_deref()
            .ok_or("missing audio replacement folder")?;
        let zstd = self.zstd()?;
        let mut archive =
            crate::file_format::Archive::ArchiveDocument::open_with_zstd(&self.input, &zstd)?
                .ok_or_else(|| format!("input is not a BARS archive: {}", self.input.display()))?;
        if !matches!(
            archive.archive,
            crate::file_format::Archive::RootArchive::Bars(_)
        ) {
            return Err(format!(
                "input is not a BARS archive: {}",
                self.input.display()
            ));
        }

        if !folder.is_dir() {
            return Err(format!(
                "audio replacement folder does not exist: {}",
                folder.display()
            ));
        }
        let mut sources = HashMap::new();
        for item in fs::read_dir(folder).map_err(|error| error.to_string())? {
            let path = item.map_err(|error| error.to_string())?.path();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !extension.eq_ignore_ascii_case("wav") && !extension.eq_ignore_ascii_case("mp3") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                sources.entry(stem.to_ascii_lowercase()).or_insert(path);
            }
        }
        let targets: Vec<_> = archive
            .archive
            .entries()
            .keys()
            .filter(|path| {
                path.starts_with("Audio/") && (path.ends_with(".bfwav") || path.ends_with(".bwav"))
            })
            .cloned()
            .collect();
        let mut replaced = 0usize;
        let mut skipped = 0usize;
        let mut failures = Vec::new();
        let mut oversized = 0usize;
        for target in targets {
            let stem = Path::new(&target)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let Some(source_path) = sources.get(&stem) else {
                skipped += 1;
                continue;
            };
            let original = archive
                .get(&target)
                .ok_or_else(|| format!("archive entry disappeared: {target}"))?
                .to_vec();
            let replacement = crate::file_format::Audio::Bfwav::decode_source(source_path)
                .and_then(|source| {
                    crate::file_format::Audio::encode_replacement(&original, &source)
                });
            match replacement {
                Ok(bytes) => {
                    if bytes.len() > original.len() {
                        oversized += 1;
                    }
                    archive.set(&target, bytes)?;
                    replaced += 1;
                }
                Err(error) => failures.push(format!("{target}: {error}")),
            }
        }
        archive.save_atomic_with_zstd(&self.output, &zstd)?;
        println!(
            "Replaced {} audio file(s); skipped {}; failed {}; oversized {}",
            replaced,
            skipped,
            failures.len(),
            oversized
        );
        for failure in failures {
            eprintln!("{failure}");
        }
        Ok(())
    }

    fn batch_g1m_worker(&self) -> Result<(), String> {
        let source = fs::read(&self.input).map_err(|error| error.to_string())?;
        let zstd = self.zstd()?;
        let data = zstd.try_decompress_safe(&source);
        if !crate::Settings::Magic::is_g1m(&data) {
            return Err("input is not a G1M model after decompression".into());
        }
        let model = crate::parser::AOC::g1m::G1mFile::parse(
            &data,
            self.input
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("G1M"),
        )
        .map_err(|error| error.to_string())?;
        let resolution = model.resolve_textures(&self.input, Path::new(&zstd.totk_config.aoc_path));
        let mut value = serde_json::to_value(model).map_err(|error| error.to_string())?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "resolvedTextures".into(),
                serde_json::to_value(resolution.textures).map_err(|error| error.to_string())?,
            );
            object.insert(
                "textureStats".into(),
                serde_json::json!({ "total": resolution.total, "skipped": resolution.skipped }),
            );
        }
        write_output(
            &self.output,
            &serde_json::to_vec(&value).map_err(|error| error.to_string())?,
        )
    }

    fn replace_g1m(&self) -> Result<(), String> {
        let source_path = Path::new(&self.file_type);
        let source = fs::read(source_path)
            .map_err(|error| format!("failed to read {}: {error}", source_path.display()))?;
        let fbx = fs::read(&self.input)
            .map_err(|error| format!("failed to read {}: {error}", self.input.display()))?;
        let name = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("G1M");
        let rebuilt = crate::parser::AOC::g1m_replace::replace_meshes_from_fbx(&source, &fbx, name)
            .map_err(|error| error.to_string())?;
        write_output(&self.output, &rebuilt)
    }

    fn replace_bfres(&self) -> Result<(), String> {
        let source_path = Path::new(&self.file_type);
        let source = fs::read(source_path)
            .map_err(|error| format!("failed to read {}: {error}", source_path.display()))?;
        let fbx = fs::read(&self.input)
            .map_err(|error| format!("failed to read {}: {error}", self.input.display()))?;
        let rebuilt =
            crate::file_format::Model3D::bfres::BfresFile::replace_geometry_from_fbx(&source, &fbx)
                .map_err(|error| error.to_string())?;
        write_output(&self.output, &rebuilt)
    }

    /// Switch Toolbox compatible BFRES editing. Mirrors the Toolbox CLI:
    /// exit code 1 for bad arguments or a non-BFRES input, 2 when
    /// `--swap_model_name` meets a file with two or more models (nothing is
    /// written), 3 for any other failure. The output is always pseudo-MCPK.
    fn bfres_edit(&self) -> Result<(), String> {
        use crate::file_format::Model3D::bfres::toolbox::{ExternalStrings, ResFile};

        fn fail(code: i32, message: String) -> ! {
            eprintln!("error: {message}");
            std::process::exit(code);
        }

        let mut input = None;
        let mut output = None;
        let mut internal_name = None;
        let mut model_name = None;
        let mut fbx = None;
        let mut import_skeleton = false;
        let mut totk_path = None;
        let mut renames: Vec<(String, String)> = Vec::new();
        let args = &self.extra;
        let mut i = 0;
        while i < args.len() {
            let take = |i: &mut usize| -> String {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .unwrap_or_else(|| fail(1, format!("{} requires a value", args[*i - 1])))
            };
            match args[i].as_str() {
                "-i" => input = Some(take(&mut i)),
                "-o" => output = Some(take(&mut i)),
                "--swap_int_name" => internal_name = Some(take(&mut i)),
                "--swap_model_name" => model_name = Some(take(&mut i)),
                "--fbx" => fbx = Some(take(&mut i)),
                "--import_skeleton" => import_skeleton = true,
                "--totk_path" => totk_path = Some(take(&mut i)),
                "--rename_tex" => {
                    let from = take(&mut i);
                    let to = take(&mut i);
                    if from.is_empty() {
                        fail(1, "--rename_tex: the search string cannot be empty".into());
                    }
                    renames.push((from, to));
                }
                other => fail(1, format!("unknown argument {other}")),
            }
            i += 1;
        }
        let Some(input) = input else {
            fail(1, "-i <input bfres> is required".into())
        };
        let Some(output) = output else {
            fail(1, "-o <output bfres> is required".into())
        };
        if import_skeleton && fbx.is_none() {
            fail(1, "--import_skeleton requires --fbx".into());
        }
        let input = Path::new(&input);
        let output = Path::new(&output);
        if !input.is_file() {
            fail(1, format!("input file not found: {}", input.display()));
        }

        let mut raw = fs::read(input).unwrap_or_else(|e| fail(1, e.to_string()));
        if crate::Settings::Magic::is_mcpk(&raw) {
            raw = crate::compression::meshcodec::MeshCodec::decompress(&raw)
                .unwrap_or_else(|e| fail(1, format!("{}: {e}", input.display())));
        }
        if !crate::Settings::Magic::is_bfres(&raw) {
            fail(1, format!("{} is not a bfres file", input.display()));
        }

        let romfs = totk_path.map(PathBuf::from).or_else(|| {
            crate::TotkConfig::TotkConfig::safe_new(false)
                .ok()
                .map(|config| PathBuf::from(config.romfs))
                .filter(|path| !path.as_os_str().is_empty())
        });
        let needs_external = raw.get(0xee).is_some_and(|flags| flags & 0x02 != 0);
        let external = match romfs {
            Some(romfs) => match ExternalStrings::from_romfs(&romfs) {
                Ok(table) => table,
                Err(error) if needs_external => fail(3, error.to_string()),
                Err(_) => ExternalStrings::empty(),
            },
            None if needs_external => fail(
                3,
                "this file uses TOTK external strings; pass --totk_path <romfs> or configure the RomFS".into(),
            ),
            None => ExternalStrings::empty(),
        };

        let mut file = ResFile::load(&raw, &external).unwrap_or_else(|e| fail(3, e.to_string()));
        println!(
            "opened {} ({} model(s))",
            input
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default(),
            file.model_count()
        );
        if model_name.is_some() && file.model_count() >= 2 {
            fail(
                2,
                format!(
                    "--swap_model_name needs a single model but the file has {}; nothing written",
                    file.model_count()
                ),
            );
        }
        if (model_name.is_some() || fbx.is_some()) && file.model_count() == 0 {
            fail(3, "the file has no models to edit; nothing written".into());
        }

        if let Some(fbx) = fbx {
            let fbx_bytes = fs::read(&fbx)
                .unwrap_or_else(|e| fail(1, format!("fbx file not found: {fbx} ({e})")));
            let replaced =
                crate::file_format::Model3D::bfres::BfresFile::replace_geometry_from_fbx(
                    &raw, &fbx_bytes,
                )
                .unwrap_or_else(|e| fail(3, e.to_string()));
            file = ResFile::load(&replaced, &external).unwrap_or_else(|e| fail(3, e.to_string()));
            println!(
                "replaced model {} from {fbx} (native importer, not byte-identical to Toolbox)",
                file.first_model_name().unwrap_or_default()
            );
            if !import_skeleton {
                // Toolbox regenerates the skinning palette on every import.
                let report = file
                    .regenerate_skinning_like_toolbox(&fbx_bytes)
                    .unwrap_or_else(|e| fail(3, e.to_string()));
                println!(
                    "regenerated skinning palette from {fbx}: {} smooth + {} rigid",
                    report.smooth_count, report.rigid_count
                );
            }
            if import_skeleton {
                let report = file
                    .import_skeleton_like_toolbox(&fbx_bytes)
                    .unwrap_or_else(|e| fail(3, e.to_string()));
                println!(
                    "imported skeleton from {fbx}: bones {} -> {} (added {}, removed {}, transforms updated {}; palette {} smooth + {} rigid)",
                    report.bones_before,
                    report.bones_after,
                    report.added.len(),
                    report.removed.len(),
                    report.transforms_updated.len(),
                    report.smooth_count,
                    report.rigid_count
                );
                for (label, names) in [
                    ("added", &report.added),
                    ("removed", &report.removed),
                    ("updated", &report.transforms_updated),
                ] {
                    if !names.is_empty() {
                        println!("  {label}: {}", names.join(", "));
                    }
                }
            }
        }
        if let Some(name) = internal_name {
            println!("internal name: {} -> {name}", file.name);
            file.set_internal_name(&name);
        }
        if let Some(name) = model_name {
            println!(
                "model name: {} -> {name}",
                file.first_model_name().unwrap_or_default()
            );
            file.rename_first_model(&name)
                .unwrap_or_else(|e| fail(3, e.to_string()));
        }
        if !renames.is_empty() {
            let mut changed = 0;
            for (from, to) in &renames {
                changed += file.rename_texture_slots(from, to);
            }
            println!("renamed {changed} texture slot(s)");
        }

        let saved = file
            .save_like_toolbox()
            .unwrap_or_else(|e| fail(3, e.to_string()));
        let compressed = crate::compression::meshcodec::MeshCodec::compress(&saved)
            .unwrap_or_else(|e| fail(3, e.to_string()));
        write_output(output, &compressed).unwrap_or_else(|e| fail(3, e));
        println!("saved {} ({} bytes)", output.display(), compressed.len());
        Ok(())
    }

    /// Switch Toolbox compatible BNTX editing (`Toolbox.exe -i x.bntx …`):
    /// internal name, texture renames, image replacement and PNG export.
    /// The output is zstd compressed only when `-o` ends with `.zs`, and
    /// matches Toolbox's bytes in both cases.
    fn bntx_edit(&self) -> Result<(), String> {
        use crate::file_format::Image::bntx_toolbox::{find_astc_encoder, format_name, BntxFile};

        fn fail(code: i32, message: String) -> ! {
            eprintln!("error: {message}");
            std::process::exit(code);
        }

        let mut input = None;
        let mut output = None;
        let mut internal_name = None;
        let mut export_dir = None;
        let mut astcenc = None;
        let mut renames: Vec<(String, String)> = Vec::new();
        let mut replacements: Vec<(String, Option<String>)> = Vec::new();
        let args = &self.extra;
        let mut i = 0;
        while i < args.len() {
            let take = |i: &mut usize| -> String {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .unwrap_or_else(|| fail(1, format!("{} requires a value", args[*i - 1])))
            };
            match args[i].as_str() {
                "-i" => input = Some(take(&mut i)),
                "-o" => output = Some(take(&mut i)),
                "--swap_int_name" => internal_name = Some(take(&mut i)),
                "--export_tex" => export_dir = Some(take(&mut i)),
                "--astcenc" => astcenc = Some(take(&mut i)),
                "--rename_tex" => {
                    let from = take(&mut i);
                    let to = take(&mut i);
                    if from.is_empty() {
                        fail(1, "--rename_tex: the search string cannot be empty".into());
                    }
                    renames.push((from, to));
                }
                "--replace_tex" => {
                    let image = take(&mut i);
                    // The texture name is optional: the next token unless it is a switch.
                    let texture = match args.get(i + 1) {
                        Some(next) if !next.starts_with('-') => {
                            i += 1;
                            Some(next.clone())
                        }
                        _ => None,
                    };
                    replacements.push((image, texture));
                }
                other => fail(1, format!("unknown argument {other}")),
            }
            i += 1;
        }
        let Some(input) = input else {
            fail(1, "-i <input bntx> is required".into())
        };
        let Some(output) = output else {
            fail(1, "-o <output bntx> is required".into())
        };
        let input = Path::new(&input);
        let output = Path::new(&output);
        if !input.is_file() {
            fail(1, format!("input file not found: {}", input.display()));
        }
        for (image, _) in &replacements {
            if !Path::new(image).is_file() {
                fail(1, format!("image file not found: {image}"));
            }
        }
        let astcenc = astcenc.map(PathBuf::from);
        if let Some(path) = &astcenc {
            if !path.is_file() {
                fail(1, format!("astcenc not found: {}", path.display()));
            }
        }

        let mut raw = fs::read(input).unwrap_or_else(|e| fail(1, e.to_string()));
        if crate::Settings::Magic::is_zstd(&raw) {
            raw = match zstd::decode_all(raw.as_slice()) {
                Ok(data) => data,
                Err(_) => {
                    let zstd = self.zstd()?;
                    zstd.try_decompress(&raw)
                        .unwrap_or_else(|e| fail(1, format!("{}: {e}", input.display())))
                }
            };
        }
        if !crate::Settings::Magic::is_bntx(&raw) {
            fail(1, format!("{} is not a bntx file", input.display()));
        }
        let mut file = BntxFile::load(&raw).unwrap_or_else(|e| fail(3, e.to_string()));
        let file_name = input
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        println!(
            "opened {file_name} (bntx, {} texture(s))",
            file.textures.len()
        );
        for texture in &file.textures {
            println!("  {}", texture.describe());
        }

        if let Some(name) = internal_name {
            println!("internal name: {} -> {name}", file.name);
            file.set_internal_name(&name);
        }
        if !renames.is_empty() {
            let mut changed = 0;
            for index in 0..file.textures.len() {
                let old = file.textures[index].name.clone();
                let mut name = old.clone();
                for (from, to) in &renames {
                    name = name.replace(from.as_str(), to);
                }
                if name != old {
                    file.rename_texture(index, &name)
                        .unwrap_or_else(|e| fail(3, e.to_string()));
                    changed += 1;
                    println!("texture: {old} -> {name}");
                }
            }
            println!("renamed {changed} texture(s)");
        }
        for (image, texture) in &replacements {
            let index = match texture {
                None => {
                    if file.textures.is_empty() {
                        fail(3, "the file has no textures to replace; nothing written".into());
                    }
                    0
                }
                Some(name) => file
                    .textures
                    .iter()
                    .position(|t| t.name.eq_ignore_ascii_case(name))
                    .unwrap_or_else(|| {
                        let have: Vec<_> = file.textures.iter().map(|t| t.name.as_str()).collect();
                        fail(
                            3,
                            format!(
                                "texture {name} not found in {file_name} (have: {}); nothing written",
                                have.join(", ")
                            ),
                        )
                    }),
            };
            let before = file.textures[index].describe();
            let encoder = find_astc_encoder(astcenc.as_deref());
            if file.textures[index].is_astc() && encoder.is_none() {
                fail(
                    3,
                    format!(
                        "texture {} is {} and no astcenc executable was found; pass --astcenc <astcenc-avx2.exe>, set ASTCENC, or put it next to the executable",
                        file.textures[index].name,
                        format_name(file.textures[index].format)
                    ),
                );
            }
            let warning = file
                .replace_texture_from_file(index, Path::new(image), encoder.as_deref())
                .unwrap_or_else(|e| fail(3, e.to_string()));
            if let Some(warning) = warning {
                println!("warning: {warning}");
            }
            println!(
                "replaced {before} from {} -> {}",
                Path::new(image)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default(),
                file.textures[index].describe()
            );
        }

        let compress = output
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".zs");
        let saved = if compress {
            file.save_like_toolbox_zs()
        } else {
            file.save_like_toolbox()
        }
        .unwrap_or_else(|e| fail(3, e.to_string()));
        write_output(output, &saved).unwrap_or_else(|e| fail(3, e));
        println!(
            "saved {} ({} bytes{})",
            output.display(),
            saved.len(),
            if compress { ", zstd" } else { "" }
        );

        if let Some(dir) = export_dir {
            let dir = Path::new(&dir);
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            for (index, texture) in file.textures.iter().enumerate() {
                let png = dir.join(format!("{}.png", texture.name));
                let image = file
                    .decode_texture(index)
                    .unwrap_or_else(|e| fail(3, format!("{}: {e}", texture.name)));
                image
                    .save_with_format(&png, image::ImageFormat::Png)
                    .unwrap_or_else(|e| fail(3, format!("{}: {e}", png.display())));
                println!("exported {}", png.display());
            }
        }
        Ok(())
    }

    /// Builds a complete custom weapon mod from a specification file.
    ///
    /// `-i` is a JSON (object or array) or TOML weapon specification; relative
    /// asset paths inside it resolve against the specification's directory.
    /// `-o` is the mod `romfs` directory to create. `--plan` prints the file
    /// plan and exits without writing anything.
    /// `create_weapon --rstb_only`: re-estimates every file already present in
    /// the generated mod ROMFS and rewrites only its ResourceSizeTable and
    /// `rstb.yaml`, leaving the other outputs untouched.
    fn recalculate_mod_rstb(
        &self,
        output: &str,
        totk_path: Option<&str>,
        zstd_level: Option<i32>,
    ) -> Result<(), String> {
        use crate::tools::items_creator::rstb::ModRstbProcessor;

        let cwd = env::current_dir().map_err(|e| e.to_string())?;
        let absolute = |value: &str| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        };
        let output = absolute(output);
        if !output.is_dir() {
            return Err(format!("mod ROMFS not found: {}", output.display()));
        }
        let mut config = TotkConfig::safe_new(false).map_err(|e| e.to_string())?;
        if let Some(romfs) = totk_path {
            config.romfs = absolute(romfs).to_string_lossy().into_owned();
        }
        let clean_romfs = PathBuf::from(&config.romfs);
        if config.romfs.is_empty() || !clean_romfs.is_dir() {
            return Err(
                "a TOTK RomFS is required; pass --totk_path <romfs> or configure it in the app"
                    .into(),
            );
        }
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .map_err(|e| format!("failed to load RomFS zstd dictionaries: {e}"))?,
        );
        let started = std::time::Instant::now();
        let report = ModRstbProcessor::new(&clean_romfs, &output, zstd)
            .with_compression_level(zstd_level)
            .generate()
            .map_err(|e| e.to_string())?;
        println!(
            "rstb          {} ({} entries, product {})",
            report.output.display(),
            report.entries.len(),
            report.product_version
        );
        println!("rstb yaml     {}", report.yaml.display());
        println!("done in {:.1}s", started.elapsed().as_secs_f64());
        Ok(())
    }

    fn create_weapon(&self) -> Result<(), String> {
        use crate::tools::items_creator::{
            generate_item_mod, load_item_specs, GenerationPlan, ItemSpec,
        };

        let mut input = None;
        let mut output = None;
        let mut totk_path = None;
        let mut plan_only = false;
        let mut rstb_only = false;
        let mut zstd_level = None;
        let args = &self.extra;
        let mut i = 0;
        while i < args.len() {
            let take = |i: &mut usize| -> Result<String, String> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| format!("{} requires a value", args[*i - 1]))
            };
            match args[i].as_str() {
                "-i" => input = Some(take(&mut i)?),
                "-o" => output = Some(take(&mut i)?),
                "--totk_path" => totk_path = Some(take(&mut i)?),
                "--plan" => plan_only = true,
                "--rstb_only" => rstb_only = true,
                "--zstd_level" => {
                    let value = take(&mut i)?;
                    zstd_level =
                        Some(value.parse::<i32>().map_err(|_| {
                            format!("--zstd_level expects an integer, got {value}")
                        })?);
                }
                other => return Err(format!("unknown argument {other}")),
            }
            i += 1;
        }
        if plan_only && rstb_only {
            return Err("--plan and --rstb_only are mutually exclusive".into());
        }
        let output = output.ok_or("-o <output_romfs> is required")?;
        if rstb_only {
            return self.recalculate_mod_rstb(&output, totk_path.as_deref(), zstd_level);
        }
        let input = input.ok_or("-i <spec.json|spec.toml> is required")?;
        let cwd = env::current_dir().map_err(|e| e.to_string())?;
        let absolute = |value: &str| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        };
        let input = absolute(&input);
        let output = absolute(&output);
        if !input.is_file() {
            return Err(format!("specification not found: {}", input.display()));
        }
        let asset_root = input
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cwd.clone());

        let mut config = TotkConfig::safe_new(false).map_err(|e| e.to_string())?;
        if let Some(romfs) = totk_path {
            config.romfs = absolute(&romfs).to_string_lossy().into_owned();
        }
        let clean_romfs = PathBuf::from(&config.romfs);
        if config.romfs.is_empty() || !clean_romfs.is_dir() {
            return Err(
                "a TOTK RomFS is required; pass --totk_path <romfs> or configure it in the app"
                    .into(),
            );
        }
        let specs = load_item_specs(&input).map_err(|e| format!("{}: {e}", input.display()))?;
        for spec in &specs {
            spec.validate(&asset_root)
                .map_err(|e| format!("{}: {e}", spec.actor_name()))?;
            match spec {
                ItemSpec::Weapon(spec) => println!(
                    "{}: {:?} from {} ({} vendor(s))",
                    spec.actor_name,
                    spec.kind,
                    spec.template_actor,
                    spec.vendors.len()
                ),
                ItemSpec::Armor(spec) => println!(
                    "{}: armor {:?} from {} ({} vendor(s){})",
                    spec.actor_name,
                    spec.slot().map_err(|e| e.to_string())?,
                    spec.template_actor,
                    spec.vendors.len(),
                    if spec.model.is_some() {
                        ", skinned cube"
                    } else {
                        ""
                    }
                ),
            }
        }
        if plan_only {
            for spec in &specs {
                let ItemSpec::Weapon(spec) = spec else {
                    println!("  (no plan preview for armor {})", spec.actor_name());
                    continue;
                };
                let plan =
                    GenerationPlan::for_weapon(spec, &clean_romfs).map_err(|e| e.to_string())?;
                for file in plan.files {
                    println!(
                        "  {:<18} {}  ({})",
                        format!("{:?}", file.action),
                        file.relative_path.display(),
                        file.reason
                    );
                }
            }
            return Ok(());
        }

        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .map_err(|e| format!("failed to load RomFS zstd dictionaries: {e}"))?,
        );
        let started = std::time::Instant::now();
        let report =
            generate_item_mod(&specs, &clean_romfs, &output, &asset_root, zstd, zstd_level)
                .map_err(|e| e.to_string())?;
        for weapon in &report.weapons {
            println!("{}:", weapon.actor_name);
            println!("  actor pack  {}", weapon.actor_pack.display());
            println!(
                "  model       {} (textures: {})",
                weapon.model.display(),
                weapon.texture_names.join(", ")
            );
            for texture in &weapon.ui_textures {
                println!(
                    "  ui texture  {} {} {}x{}{}",
                    texture.destination.display(),
                    texture.format,
                    texture.width,
                    texture.height,
                    if texture.png_applied {
                        " (png applied)"
                    } else {
                        ""
                    }
                );
                if let Some(warning) = &texture.warning {
                    println!("  warning     {warning}");
                }
            }
            println!("  messages    {}", weapon.messages.display());
            for path in &weapon.rsdb {
                println!("  rsdb        {}", path.display());
            }
            println!("  sharp info  {}", weapon.sharp_info.display());
            println!("  game data   {}", weapon.game_data.output.display());
            for vendor in &weapon.vendor_packs {
                println!(
                    "  vendor      {} x{} in {}",
                    vendor.vendor_actor,
                    vendor.quantity,
                    vendor.output.display()
                );
            }
        }
        for armor in &report.armors {
            println!("{}:", armor.actor_name);
            println!("  actor pack  {}", armor.actor_pack.display());
            println!(
                "  model       {} (textures: {})",
                armor.model.display(),
                armor.texture_names.join(", ")
            );
            if let Some(anim) = &armor.model_anim {
                println!("  model anim  {}", anim.display());
            }
            if let Some(cube) = &armor.cube {
                println!(
                    "  cube        {} at ({:.3}, {:.3}, {:.3}) size ({:.2}, {:.2}, {:.2}); {}{}",
                    cube.shape,
                    cube.center[0],
                    cube.center[1],
                    cube.center[2],
                    cube.size[0],
                    cube.size[1],
                    cube.size[2],
                    cube.influences
                        .iter()
                        .map(|(bone, matrix, weight)| format!("{bone}={weight:.3} (m{matrix})"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if cube.palette_added.is_empty() {
                        String::new()
                    } else {
                        format!("; palette += {}", cube.palette_added.join(", "))
                    }
                );
                if !cube.kept_shapes.is_empty() {
                    println!("  kept shapes {}", cube.kept_shapes.join(", "));
                }
            }
            println!("  ui textures {} icon BNTX", armor.ui_textures.len());
            for texture in &armor.ui_textures {
                if let Some(warning) = &texture.warning {
                    println!("  warning     {warning}");
                }
            }
            println!("  messages    {}", armor.messages.display());
            for path in &armor.rsdb {
                println!("  rsdb        {}", path.display());
            }
            println!("  game data   {}", armor.game_data.output.display());
            for vendor in &armor.vendor_packs {
                println!(
                    "  vendor      {} x{} in {}",
                    vendor.vendor_actor,
                    vendor.quantity,
                    vendor.output.display()
                );
            }
        }
        println!(
            "rstb          {} ({} entries)",
            report.rstb.output.display(),
            report.rstb.entries.len()
        );
        if let Some(parent) = output.parent() {
            let report_path = parent.join("items_creator_report.json");
            let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
            write_output(&report_path, json.as_bytes())?;
            println!("report        {}", report_path.display());
        }
        println!("done in {:.1}s", started.elapsed().as_secs_f64());
        Ok(())
    }

    fn g1m_to_fbx(&self) -> Result<(), String> {
        let texture_format = crate::parser::fbx::TextureExportFormat::parse(&self.file_type)
            .map_err(|error| error.to_string())?;
        let source = fs::read(&self.input)
            .map_err(|error| format!("failed to read {}: {error}", self.input.display()))?;
        let name = self
            .input
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("G1M");
        let model = crate::parser::AOC::g1m::G1mFile::parse_for_export(&source, name)
            .map_err(|error| error.to_string())?;
        let textures = model
            .resolve_textures_for_export(
                &self.input,
                Path::new(
                    &crate::TotkConfig::TotkConfig::safe_new(false)
                        .map_err(|error| error.to_string())?
                        .aoc_path,
                ),
            )
            .textures;
        crate::parser::fbx::export_g1m(
            &[(&model, textures.as_slice(), String::new())],
            &self.output,
            texture_format,
            name,
        )
        .map_err(|error| error.to_string())
    }

    /// Headless BFRES preview: resolves the embedded textures, bakes rigid
    /// bone binds, applies the Switch Sports skin/hair/outfit tint through the
    /// material's `*_Tcl` mask and rasterizes the model to a PNG.
    fn bfres_render(&self) -> Result<(), String> {
        use crate::file_format::Model3D::{SoftRender, SportsTint};
        use crate::parser::AOC::g1m::{G1mMaterial, G1mTextureSlot, ResolvedG1tTexture};
        use std::collections::HashMap;

        let tint = SportsTint::TintColors::parse(&self.file_type)?;
        let zstd = self.zstd().unwrap_or_else(|_| {
            Arc::new(crate::Zstd::TotkZstd::dictionaryless(
                Arc::new(TotkConfig::default()),
                crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL,
            ))
        });
        let (opened, _) =
            crate::file_format::Model3D::bfres::BfresFile::open(&self.input, zstd.clone())
                .ok_or_else(|| format!("failed to open BFRES {}", self.input.display()))?;
        let bfres = opened.bfres.ok_or("BFRES did not parse")?;
        let romfs = zstd.totk_config.romfs.clone();
        let tomodachi = zstd.totk_config.tomodachi_path.clone();
        let resolved = crate::TauriCommands::visuals::resolve_bfres_textures(
            &bfres,
            &self.input,
            opened.bfres_data.as_deref(),
            Path::new(&romfs),
            Path::new(&tomodachi),
            Some(&zstd),
        );
        let mut textures: Vec<ResolvedG1tTexture> = resolved
            .iter()
            .map(|texture| ResolvedG1tTexture {
                name: texture.name.clone(),
                aliases: texture.aliases.clone(),
                path: texture.path.clone(),
                source: texture.source.clone(),
                data_url: texture.data_url.clone(),
                width: texture.width,
                height: texture.height,
                array_count: 1,
                renderable: true,
                data_urls: vec![texture.data_url.clone()],
            })
            .collect();
        let png_bytes = |data_url: &str| -> Option<Vec<u8>> {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(data_url.split_once("base64,")?.1)
                .ok()
        };
        let mut tinted_names: HashMap<(String, String), String> = HashMap::new();
        let mut materials = Vec::with_capacity(bfres.materials.len());
        for material in &bfres.materials {
            let mut slots: Vec<G1mTextureSlot> = material
                .texture_slots
                .iter()
                .map(|slot| G1mTextureSlot {
                    index: slot.index,
                    name: slot.name.clone(),
                    uv_layer: 0,
                    sampler: slot.sampler.clone(),
                    texture_type: slot.texture_type.clone(),
                })
                .collect();
            let base_index = slots.iter().position(|slot| {
                slot.sampler.eq_ignore_ascii_case("_a0") || slot.texture_type == "Base color"
            });
            let mask_name = slots
                .iter()
                .find(|slot| slot.texture_type == "Tint mask")
                .map(|slot| slot.name.clone());
            if let (Some(base_index), Some(mask_name), false) =
                (base_index, mask_name, tint.is_empty())
            {
                let base_name = slots[base_index].name.clone();
                let key = (base_name.clone(), mask_name.clone());
                if !tinted_names.contains_key(&key) {
                    let find = |name: &str| {
                        textures
                            .iter()
                            .find(|texture| {
                                texture.name == name
                                    || texture.aliases.iter().any(|alias| alias == name)
                            })
                            .cloned()
                    };
                    if let (Some(base), Some(mask)) = (find(&base_name), find(&mask_name)) {
                        if let (Some(base_png), Some(mask_png)) =
                            (png_bytes(&base.data_url), png_bytes(&mask.data_url))
                        {
                            use base64::Engine;
                            let tinted = SportsTint::tint_albedo_png(&base_png, &mask_png, &tint)?;
                            let data_url = format!(
                                "data:image/png;base64,{}",
                                base64::engine::general_purpose::STANDARD.encode(tinted)
                            );
                            let name = format!("{base_name}#tint");
                            textures.push(ResolvedG1tTexture {
                                name: name.clone(),
                                aliases: Vec::new(),
                                path: base.path.clone(),
                                source: "tinted".into(),
                                data_url: data_url.clone(),
                                width: base.width,
                                height: base.height,
                                array_count: 1,
                                renderable: true,
                                data_urls: vec![data_url],
                            });
                            tinted_names.insert(key.clone(), name);
                        }
                    }
                }
                if let Some(name) = tinted_names.get(&key) {
                    slots[base_index].name = name.clone();
                }
            }
            materials.push(G1mMaterial {
                name: material.name.clone(),
                offset: material.offset,
                texture_slots: slots,
            });
        }
        let render = SoftRender::bake_rigid_bind(&bfres.render);
        let png = SoftRender::render_to_png(
            &render,
            &materials,
            &textures,
            SoftRender::DEFAULT_SIZE,
            SoftRender::DEFAULT_SIZE,
        )?;
        write_output(&self.output, &png)
    }

    fn lm3_render(&self) -> Result<(), String> {
        let (archive_name, slot) = parse_lm3_slot_id(&self.file_type)?;
        let dict_path = match crate::parser::lm3::archive_spec(&archive_name) {
            Some(spec) => self.input.join(&spec.dict),
            None => crate::parser::lm3::find_archive_dict(&self.input, &archive_name)
                .ok_or_else(|| format!("unknown LM3 archive: {archive_name}"))?,
        };
        let (model, textures) =
            crate::parser::lm3_parallel::parse_slot_parallel(&dict_path, &archive_name, slot)
                .map_err(|error| {
                    format!("failed to read LM3 slot {archive_name}_{slot}: {error}")
                })?;
        let png = crate::file_format::Model3D::SoftRender::render_to_png(
            &model.render,
            &model.materials,
            &textures,
            crate::file_format::Model3D::SoftRender::DEFAULT_SIZE,
            crate::file_format::Model3D::SoftRender::DEFAULT_SIZE,
        )?;
        write_output(&self.output, &png)
    }

    fn lm3_render_all(&self) -> Result<(), String> {
        use crate::parser::lm3;
        let skip_existing = match self.file_type.as_str() {
            "skip" => true,
            "overwrite" => false,
            other => {
                return Err(format!(
                    "unsupported existing-file mode: {other}; expected skip or overwrite"
                ))
            }
        };
        if !self.input.is_dir() {
            return Err(format!(
                "LM3 romfs is not a directory: {}",
                self.input.display()
            ));
        }
        fs::create_dir_all(&self.output).map_err(|error| error.to_string())?;

        // Every *.dict under the romfs is an archive; the shared discovery
        // names them the same way the model browser does.
        let archives = lm3::discover_archives(&self.input);

        // Non-global archives all borrow textures from `global`; inflating its
        // texture entries once here avoids re-reading them per archive.
        let global_source = self
            .input
            .join("global.dict")
            .is_file()
            .then(|| lm3::Lm3Archive::open(&self.input.join("global.dict")).ok())
            .flatten()
            .and_then(|archive| lm3::Lm3TextureSource::from_archive(&archive));

        let mut rendered = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        for (name, dict_path) in &archives {
            // The heavy zlib entries are archive-wide, not per slot: inflate
            // them once and reuse the buffers for every slot of the archive.
            let archive = match lm3::Lm3Archive::open(dict_path) {
                Ok(archive) => archive,
                Err(error) => {
                    failed += 1;
                    eprintln!("{name}: failed to open: {error}");
                    continue;
                }
            };
            let files = match lm3::Lm3SlotFiles::read(&archive) {
                Ok(files) => files,
                Err(error) => {
                    failed += 1;
                    eprintln!("{name}: failed to read entries: {error}");
                    continue;
                }
            };
            let shared = if name == "global" {
                None
            } else {
                global_source.as_ref()
            };
            // Cataloged archives keep their researched slot counts; the rest
            // hold exactly the model slots their chunk table declares.
            let slots = lm3::archive_spec(name)
                .map(|spec| spec.slots)
                .unwrap_or_else(|| lm3::model_slot_count(&files.table));
            if slots == 0 {
                println!("{name}: no model slots");
                continue;
            }
            println!("{name}: {slots} slot(s)");
            for slot in 0..slots {
                let output = self.output.join(format!("{name}_{slot}.png"));
                if skip_existing && output.is_file() {
                    skipped += 1;
                    continue;
                }
                // Texture decoding can panic inside third-party decoders on
                // malformed slot data; one bad slot must not end the batch.
                let result = crate::Settings::catch_panic(|| {
                    lm3::parse_slot_from_files(&files, name, slot, shared)
                        .map_err(|error| error.to_string())
                        .and_then(|(model, textures)| {
                            crate::file_format::Model3D::SoftRender::render_to_png(
                                &model.render,
                                &model.materials,
                                &textures,
                                crate::file_format::Model3D::SoftRender::DEFAULT_SIZE,
                                crate::file_format::Model3D::SoftRender::DEFAULT_SIZE,
                            )
                        })
                })
                .and_then(|png| write_output(&output, &png));
                match result {
                    Ok(()) => {
                        rendered += 1;
                        println!("{name}_{slot}: rendered");
                    }
                    Err(error) => {
                        failed += 1;
                        eprintln!("{name}_{slot}: {error}");
                    }
                }
            }
        }
        println!("Rendered {rendered} slot(s); skipped {skipped} existing; failed {failed}");
        Ok(())
    }

    /// Emits `{archive: {slot: bytes}}` for every archive in the romfs, ready
    /// to sit under the "sizes" key of `misc/lm3_slot_names.json`.
    fn lm3_slot_sizes(&self) -> Result<(), String> {
        use crate::parser::lm3;
        use std::collections::BTreeMap;
        if !self.input.is_dir() {
            return Err(format!(
                "LM3 romfs is not a directory: {}",
                self.input.display()
            ));
        }
        let mut sizes: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        for (name, dict_path) in lm3::discover_archives(&self.input) {
            let table = match lm3::read_entry_from_disk(&dict_path, lm3::TABLE_ENTRY) {
                Ok(table) => table,
                Err(error) => {
                    eprintln!("{name}: {error}");
                    continue;
                }
            };
            let per_slot = lm3::slot_sizes_from_table(&table);
            if per_slot.is_empty() {
                continue;
            }
            println!("{name}: {} slot(s)", per_slot.len());
            sizes.insert(
                name,
                per_slot
                    .iter()
                    .enumerate()
                    .map(|(slot, size)| (slot.to_string(), *size))
                    .collect(),
            );
        }
        let json = serde_json::to_string_pretty(&sizes).map_err(|error| error.to_string())?;
        write_output(&self.output, json.as_bytes())
    }
}

fn parse_lm3_slot_id(value: &str) -> Result<(String, usize), String> {
    let (archive, slot) = value.rsplit_once('_').ok_or_else(|| {
        format!("invalid LM3 slot id: {value}; expected <archive>_<slot> like global_27")
    })?;
    let slot = slot
        .parse()
        .map_err(|_| format!("invalid LM3 slot number in {value}"))?;
    Ok((archive.to_string(), slot))
}

fn parse_dictionary(value: &str) -> Result<ZstdDictionary, String> {
    match value {
        "zs" => Ok(ZstdDictionary::Zs),
        "pack" => Ok(ZstdDictionary::Pack),
        "empty" => Ok(ZstdDictionary::Empty),
        "bcett" => Ok(ZstdDictionary::Bcett),
        "yaz0" => Ok(ZstdDictionary::Yaz0),
        _ => Err(format!(
            "unsupported compression: {value}; expected zs, pack, empty, bcett, or yaz0"
        )),
    }
}

fn parse_file_type(value: &str) -> Result<TotkFileType, String> {
    match value {
        "ainb" => Ok(TotkFileType::AINB),
        "asb" => Ok(TotkFileType::ASB),
        "byml" => Ok(TotkFileType::Byml),
        "bcett" => Ok(TotkFileType::Bcett),
        "tagproduct" | "tag_product" => Ok(TotkFileType::TagProduct),
        "aamp" => Ok(TotkFileType::Aamp),
        "msbt" | "msyt" => Ok(TotkFileType::Msbt),
        "evfl" | "bfevfl" => Ok(TotkFileType::Evfl),
        "xlink" | "belnk" => Ok(TotkFileType::Xlink),
        "text" => Ok(TotkFileType::Text),
        "smo" => Ok(TotkFileType::SmoSaveFile),
        _ => Err(format!("unsupported file type: {value}")),
    }
}

fn archive_entries(kind: &str, bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    match kind {
        "zip" => Ok(ZipFile::from_bytes(bytes)?
            .entries()
            .iter()
            .map(|(n, d)| (n.clone(), d.clone()))
            .collect()),
        "7z" => Ok(SevenZipCmd::from_bytes(bytes)?
            .entries()
            .iter()
            .map(|(n, d)| (n.clone(), d.clone()))
            .collect()),
        "rar" => Ok(RarFile::from_bytes(bytes)?
            .entries()
            .iter()
            .map(|(n, d)| (n.clone(), d.clone()))
            .collect()),
        "sarc" => {
            let sarc = Sarc::new(bytes).map_err(|e| format!("invalid SARC: {e}"))?;
            sarc.files()
                .map(|file| {
                    file.name()
                        .map(|n| (n.to_string(), file.data.to_vec()))
                        .ok_or_else(|| "unnamed SARC entries are unsupported".to_string())
                })
                .collect()
        }
        _ => Err(format!("unsupported archive type: {kind}")),
    }
}

fn build_archive(kind: &str, entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    macro_rules! codec {
        ($ty:ty) => {{
            let mut archive = <$ty>::default();
            for (name, data) in entries {
                archive.entries_mut().insert(name, data);
            }
            archive.to_bytes()
        }};
    }
    match kind {
        "zip" => codec!(ZipFile),
        "7z" => codec!(SevenZipCmd),
        "rar" => codec!(RarFile),
        "sarc" => {
            let mut writer = SarcWriter::new(Endian::Little);
            for (name, data) in entries {
                writer.add_file(&name, data);
            }
            Ok(writer.to_binary())
        }
        _ => Err(format!("unsupported archive type: {kind}")),
    }
}

fn collect_directory(
    root: &Path,
    current: &Path,
    result: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_directory(root, &path, result)?;
        } else {
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            crate::file_format::Archive::validate_entry_path(&name)?;
            result.push((name, fs::read(path).map_err(|e| e.to_string())?));
        }
    }
    Ok(())
}

fn collect_file_paths(
    root: &Path,
    current: &Path,
    result: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_file_paths(root, &path, result)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            result.push((relative, path));
        }
    }
    Ok(())
}

fn strip_zs_suffix(path: &Path) -> PathBuf {
    let mut result = path.to_path_buf();
    let Some(name) = result
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
    else {
        return result;
    };
    if name.to_ascii_lowercase().ends_with(".zs") {
        result.set_file_name(&name[..name.len() - 3]);
    }
    result
}

fn paths_are_equal(left: &Path, right: &Path) -> Result<bool, String> {
    let left =
        fs::canonicalize(left).map_err(|e| format!("failed to resolve input directory: {e}"))?;
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    Ok(left
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(right.to_string_lossy().trim_end_matches(['\\', '/'])))
}

fn safe_destination(root: &Path, name: &str) -> Result<PathBuf, String> {
    crate::file_format::Archive::validate_entry_path(name)?;
    Ok(root.join(name))
}

fn write_output(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, data).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

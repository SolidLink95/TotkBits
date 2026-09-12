# Changes since TotkBits 0.0.9

User-visible additions and improvements in the current 1.0.1 codebase:

## Format inventory

| Category | Formats | Access |
| --- | --- | --- |
| New/upgraded text, YAML and JSON | **AINB**, **ASB/BAEV**, **XLink**, **ESETB/PTCL**, **BPHCL AAMP**, **BFEVFL**, **MSBT**, **Tag.Product** | Editable round trips; BFEVFL and Tag.Product use JSON, MSBT/MSYT use structured text, and the others use YAML. |
| New archive types | **ZIP**, **7z**, **BARS**, folder projects; nested **SARC/ZIP/7z/BARS** | Editable and saveable; RAR remains unsupported. |
| Native/in-process parsers | **AINB**, **ASB/BAEV**, **BFEVFL**, **MSBT**, **RSTB**, **ESETB/PTCL**, **BPHCL**, **HKCL**, **BPHHB**, **BFRES**, binary **FBX**, **BNTX**, **TexToGo**, **BWAV**, **AMTA**, **ZIP**, **7z**, **BARS**; XLink uses a native binding | No external Python/.NET conversion process. |
| New read-only/viewer formats | **HKCL**, **BPHHB**, BPHCL structure nodes, **BFRES**, binary **FBX**, **AMTA**, **TexToGo**, common raster images | BNTX/DDS are viewer-based but also provide targeted surface replacement; BNTX names are editable. |

## BPHCL physics merger

The Physics Merge workspace can copy selected cloth and collidable nodes between open BPHCL documents, validate compatibility, rebuild the target, and report added or skipped nodes. **HKCL → BPHCL** cloth import now materializes shared HKCL particle, constraint, skeleton and collider values through a compatible BPHCL template, rebuilds native ITEM/PTCH/AAMP data, and produces a saveable BPHCL; BPHCL → HKCL remains a validated graph preview.

## File formats and content tools

- Replaced the Python/.NET bridges with native Rust readers and writers for **AINB**, **ASB/BAEV**, **BFEVFL/EVFL**, and **MSBT/MSYT**. ASB retains/imports companion BAEV data; MSBT editing preserves labels, attributes, styles, encodings, endianness, and TOTK text tags.
- Added editable **XLink** (`.belnk[.zs]`) support with a dedicated Monaco language definition and syntax highlighting.
- Improved **ESETB** by exposing its embedded PTCL particle data as editable YAML and rebuilding it on save. **Tag.Product** parsing/writing now validates fields and tag references, rebuilds the bit matrix correctly, and preserves the binary rank table.
- Added native **RSTB** parsing/writing with search, add/edit/remove operations and original-compression preservation. BYML/BCETT, GameDataList, AAMP, SARC and other editable formats now restore the detected Zstandard dictionary or Yaz0 alignment when saved.
- Added physics tooling: hierarchical **BPHCL** browsing, editable AAMP leaves, node removal and BPHCL-to-BPHCL cloth/collidable merging. **HKCL** and **BPHHB** remain read-only source formats, while compatible HKCL cloths can now be imported and saved into BPHCL; BPHCL-to-HKCL uses validated graph previews with optional BPHHB-assisted bone mapping.
- Added read-only 3D workspaces for **BFRES**, MeshCodec **BFRES.MC**, and binary **FBX**. The React/Three.js viewer displays meshes, materials, resolved textures, bones, normals and UVs, with visibility, shading, brightness, section metadata and YAML controls.
- **Nintendo Switch Sports** BFRES 0.9 support: the viewer now reads the version 0.9 skeleton and material layouts (embedded textures resolve, one-bone shapes render), decodes shader options, render info and shader parameters into the material inspector, and reproduces the game's "insert color" skin/hair/outfit tint through the `*_Tcl` mask using the game's own colour tables. `Totkbits.exe --cli bfres_render <default|none|skin,hair,outfit> <input.bfres[.zs]> <output.png>` renders a BFRES headlessly with the same tint.
- Added Switch Toolbox compatible command-line editing for **BFRES** and **BNTX**: `Totkbits.exe --cli bfres_edit -i <in.bfres[.mc]> -o <out.bfres.mc> [--swap_int_name N] [--swap_model_name N] [--fbx m.fbx] [--rename_tex FROM TO]... [--totk_path <romfs>]` and `Totkbits.exe --cli bntx_edit -i <in.bntx[.zs]> -o <out.bntx[.zs]> [--swap_int_name N] [--rename_tex FROM TO]... [--replace_tex <image.png|.dds> [TEXTURE]]... [--export_tex DIR] [--astcenc <astcenc.exe>]`. Both use ports of the Toolbox loaders and savers (the BFRES v10 `ResFileSwitchSaver` with its implicit shader-parameter, bind-matrix and render-info rewrites, and the BNTX `BntxFileSaver`), plus Toolbox's own zstd/MeshCodec compression settings, so an open-and-save, rename or texture replacement produces the same bytes Toolbox writes; ASTC textures are encoded through an external `astcenc` when one is available. Only `--fbx` geometry import differs from Toolbox, since it uses the native importer.
- Added a command-line **custom weapon creator**: `Totkbits.exe --cli create_weapon -i <spec.json|spec.toml> -o <output_romfs> [--totk_path <romfs>] [--plan]` builds a complete mod ROMFS for one or more small swords, large swords, spears, bows or shields from a JSON/TOML specification. It clones and specializes the template actor pack (component files are located through the template's ActorParam references, inherited components stay inherited), rebuilds the BFRES through the Switch Toolbox compatible writer (model, shape and texture names renamed, optional FBX geometry, MeshCodec output), copies or placeholder-fills TexToGo textures, clones the inventory and compendium BNTX images under the new names (optional PNG replacement), and registers the weapon in RSDB, Tag.Product, SharpInfo, GameDataList, the US English messages, optional travelling-merchant shops and the ResourceSizeTable. A JSON report and an `rstb.yaml` review file are written next to the output ROMFS. `--rstb_only` re-estimates an already generated mod and rewrites only its ResourceSizeTable; `--zstd_level N` picks the Zstandard level of that table. The generated table matches what TKMM builds for the same mod byte for byte when its compression level is passed: every archive member is sized with TKMM's rules (MeshCodec models from the container header), the hash table is written sorted so the game's binary search finds new entries, the name block is sized to its longest key, and every Zstandard frame declares its content size. GameDataList output stores enum choices as 64-bit values, appends new flags without reordering the vanilla tables, and recomputes the save-data layout (`AllDataSaveOffset`, `AllDataSaveSize`, `SaveDataOffsetPos`, `SaveDataSize`) for the added flags.
- Added an image workspace for **BNTX**, **TexToGo**, **DDS**, and common raster images. It supports zoom, texture/array/mip selection, PNG export, DDS/BNTX surface replacement from PNG with selectable output formats, and BNTX texture renaming.
- Added editable **BARS** archives plus **BFWAV/BWAV** playback and metadata. Audio can be exported as WAV/MP3 or replaced from WAV/MP3, including bulk filename-matched replacement from a folder; AMTA audio metadata can also be inspected.
- Added editable **ZIP** and **7z** archives and folder-as-archive projects. Nested SARC, ZIP, 7z and BARS files can be expanded recursively and edited in place, including add, replace, rename, delete, extract and compare operations.



## React interface and workflow

- Added independent document tabs that retain editor/view state, archive selection, filters, searches and comparisons. Tabs support close buttons, middle-click, `Ctrl+W`, parent navigation and simultaneous parent/child archive documents.
- Added recent files, Open Folder, multi-file drag-and-drop, archive filtering, All/Added/Modded views, persistent expansion, bounded context menus and read-only banners.
- Replaced Notepad-based configuration with an in-app settings editor for RomFS paths, themes, fonts, minimap, BYML layout/precision, rotations, compression prompts and close behavior; visual editor settings apply immediately.
- Added parsing, model-loading, audio-processing, comparing and saving overlays. Comparison now skips identical files and provides reliable previous/next-difference navigation; RSTB search accepts Enter, and editor undo/redo and standard open/save/search/close shortcuts are exposed in the toolbar.

## Runtime and automation

- Improved opening speed and routing through native parsers, safer per-document backend state, compression-aware/empty-output save guards, and clearer errors. TotkBits can run without a configured TOTK RomFS; dictionary-dependent Zstandard features are disabled with a warning instead of blocking startup.
- Added a CLI for binary/text conversion, ZIP/7z/SARC extraction and creation, Zstandard/Yaz0 compression and decompression, MeshCodec decompression, recursive folders, and batch BARS audio replacement, with an in-app command reference.

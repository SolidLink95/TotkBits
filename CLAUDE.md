# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TotkBits is a Windows-only Tauri 2 desktop app for inspecting and editing Nintendo Switch game files (primarily Zelda: TOTK, plus BOTW, Super Mario Odyssey, Tomodachi Life, Luigi's Mansion 3). React/Vite frontend, Rust backend.

## Repository boundaries

Never read, write, or execute anything outside `%USERPROFILE%\Desktop\coding\TotkBits` (or its mirror `W:\coding\TotkBits`). Put temporary and test files in `./tmp/` (gitignored). Never read `*.rs` files under `src-tauri/misc/` — those are backups, not build inputs.

Exceptions:

- `E:\TOTK_modding\0100F2C0115B6000\romfs` — **read-only** access is granted (a TOTK RomFS dump, useful as test input). Never write, delete, or move anything there.
- All writes outside the repo working set — test fixtures, scratch output, copies of RomFS files — are permitted **only inside `./tmp/`**.

### In-scope paths

Only these are part of the working set — they are the same paths `.vscode/settings.json` leaves visible to VS Code's explorer, search, file watcher and rust-analyzer:

- `src/` — the whole React frontend.
- `src-tauri/` — the Rust backend, **except** `target/`, `bin/`, `external/`, `gen/`, `Cargo.lock`, and `*.rs` under `misc/`.
- The text files sitting directly in the repo root (`*.md`, `package.json`, `vite.config.js`, `index.html`, `repo_init.py`, `tauri_build.py`, `*.bat`, `.gitignore`, `.gitmodules`).
- `tmp/` — your own scratch area: read, write and list it freely (it is gitignored and hidden from VS Code, but not from you).

Everything else in the repo is out of scope: `ext_projects/`, `preview/`, `public/`, `target/`, `res/`, `bin/`, `dist/`, `node_modules/`, `.cargo/`, `.codex/`, and any `__pycache__`. Do not grep, read, or edit inside them, and do not cite them as evidence — they are build output, vendored sources, or backups. If a task genuinely needs one of them (e.g. editing `repo_init.py`'s payload in `src-tauri/misc/`), say so and ask first.

Searches should be scoped accordingly — e.g. `Grep(pattern, path="src")` / `path="src-tauri/src"` rather than a bare repo-wide sweep.

## Commands

```powershell
python repo_init.py           # REQUIRED first-time setup: npm install, populate src-tauri/bin/
npm run tauri dev             # run the full desktop app
npm run dev                   # Vite only (UI shell; all backend invokes fail)
npm run build                 # frontend production bundle into dist/
cargo check   --manifest-path src-tauri/Cargo.toml
cargo test    --manifest-path src-tauri/Cargo.toml
cargo fmt     --manifest-path src-tauri/Cargo.toml -- --check
python tauri_build.py         # full Windows release: clean, NSIS bundle, silent install, 7z/zip package
.\update_git_libs.bat         # cargo update the git deps (roead, meshcodec_bindings, xlink2_bindings)
```

`repo_init.py` is not optional. It copies `src-tauri/misc/*.{bin,json,txt}` into `src-tauri/bin/` and downloads native sidecars (`bin/dlls/xlink_tool.dll`, `bin/dlls/meshcodec.dll`, `bin/cpp/oead_byml_pipe.exe`). Those paths are resolved relative to the executable directory at runtime and are declared as bundle `resources` in `src-tauri/tauri.conf.json`; without them XLink, MeshCodec, and GameDataList silently degrade to errors.

Build prerequisites beyond the Rust/Node toolchain: LLVM (for the C++ bindings crates) and CMake.

There is no JS test runner and the Rust tree has essentially no test modules — verify changes by running the app and exercising open → edit → save for the affected format.

## Architecture

### Per-document backend state

The core structural idea: **the frontend owns document identity, the backend owns document state, and they are joined by a UUID string.**

- `src/DocumentState.js` is a hand-rolled external store (`useSyncExternalStore`) holding the tab list. It exports an `invoke` wrapper that **must** be used instead of `@tauri-apps/api`'s `invoke` for any command touching an open file.
- That wrapper checks the command name against the `documentCommands` set, allocates/reuses a document tab, and injects `documentId` into the args automatically. It also handles open/child-open/comparison tab lifecycles, failure rollback (closes the tab on `tab === 'ERROR'`), and the loading overlays via `window` CustomEvents (`totkbits:model-loading`, `totkbits:comparing`, `totkbits:audio-processing`, `totkbits:recent-files-changed`, `totkbits:model-cache-purge`).
- `src-tauri/src/DocumentState.rs` holds `Mutex<HashMap<String, TotkBitsApp<'static>>>` as Tauri managed state. Commands reach into it through the `with_document!` / `with_document_mut!` macros defined in `TauriCommands.rs`.

**Adding a command that operates on an open document requires three edits:** implement it in the right `src-tauri/src/commands/*.rs`, register it in the `tauri::generate_handler!` list in `main.rs`, and add its name to `documentCommands` in `src/DocumentState.js`. Miss the last one and the command runs without a `documentId` and panics/errors on lookup.

### The SendData contract

Nearly every backend command returns `Option<SendData>` (`src-tauri/src/Open_and_Save.rs`). It is the single wire format: `text`, `path` (`Pathlib`), `file_label`, `file_metadata`, `file_type` (`TotkFileType`), `status_text`, `sarc_paths`, `rstb_paths`, `lang`, `compare_data`, `read_only`, and crucially **`tab`** — a string the frontend switches on to decide which view to show (`SARC`, `YAML`, `RSTB`, `3D`, `IMAGE`, `AUDIO`, `AMTA`, `COMPARER`, `ERROR`, …). `tab: "ERROR"` is the universal failure signal; `status_text` starting with `error`/`warning` colors the status bar.

The tab-dispatch chains live in `src/ButtonClicks.jsx` (repeated per entry point: open from dialog, open from path, edit internal SARC file, edit nested SARC file). A new view type means extending each of those chains plus `App.jsx`'s conditional render list.

### Rust module layout

- `main.rs` — CLI short-circuit (`Totkbits.exe --cli …`, see `tools/Cli.rs`), plugin registration, the full command handler list.
- `TotkApp.rs` — `TotkBitsApp`, the per-document state machine: opened file, SARC pack comparer, nested archives, internal file, and all open/save/add/rename/extract/search operations.
- `TauriCommands.rs` — thin module that re-exports `commands/*.rs` (split by domain: archives, audio, files, general, physics, rstb, settings, visuals) and defines the document-access macros plus native error dialogs.
- `Zstd.rs` — `TotkZstd`, `ZsDic`, `TotkFileType`, `ZstdDictionary`. TOTK's zstd dictionaries come from the configured RomFS. **The app must start without a RomFS**: `TotkZstd::dictionaryless()` is the degraded path that keeps plain/empty-dictionary files working while dictionary-dependent operations return `NotFound`. Don't reintroduce hard failures here.
- `TotkConfig.rs` — TOML config at `%LOCALAPPDATA%\Totkbits\config.toml` (falls back to `%APPDATA%`, then exe dir); holds RomFS paths for TOTK/BOTW/AOC/Tomodachi plus editor preferences. Also imports NX Editor's `%LOCALAPPDATA%\Totk\config.json` when present.
- `file_format/` — one module per container/format (SARC/Pack, RSTB, BYML, AAMP, MSBT, AINB, ASB, Xlink, Esetb, BfevFile, TagProduct, GameDataList, bphcl/bphhb/hkcl, plus `Archive/`, `Audio/`, `Image/`, `Model3D/`, `Mii/`, `SMO/`, `Animation/`).
- `parser/` — the low-level binary readers/writers those format modules sit on top of. Format detection and dispatch: `Zstd.rs`, `utils/magic.rs`, `Open_and_Save.rs` (`open_file_from_disk_name_guess`, `file_from_bytes_to_senddata`, `get_binary_by_filetype`).
- `Comparer.rs` / `NestedSarc.rs` — vanilla-vs-mod diffing and recursive archive-in-archive editing.
- `utils/` — `AppPaths.rs` (`Pathlib`, exe-relative path resolution), `LookupData.rs` (the `misc/*.bin` name tables), `Startup.rs`, `magic.rs`.

Everything is Zstd/Yaz0 aware end to end: the dictionary a file was compressed with is detected on open, recorded in `file_metadata`, and restored on save. Saving with a plaintext extension (`.json`/`.yaml`/`.yml`/`.txt`) or without `.zs` is how the user opts out of compression.

### Frontend layout

`src/` is flat. `StateManager.jsx` is a single big React context (`useEditorContext`) carrying `activeTab`, the Monaco refs, `paths`, `settings`, `compareData`, overlays, and the per-document Monaco maps (`documentModels`, `documentViewStates`, `documentSnapshots`). `DocumentTabs.jsx` owns snapshot save/restore when the active document changes — a single Monaco editor instance is reused across tabs, with models and view states swapped in.

`App.jsx` renders every view simultaneously and hides them with `style.display` / early returns keyed on `activeTab`; it does not unmount them. Views: `DirectoryTree` (SARC), Monaco (`YAML`), `RstbTree`, `Comparer`, `PhysicsMerge`, `Bfres3DView` (three.js), `ImageView`, `AudioView`, `AmtaView`, `AocModelView`, `Mii`, `Tomodachi`.

`vite.config.js` carries a required `fixMiiJsLodashInterop` plugin — MiiJS's prebuilt ESM chunk has broken CommonJS lodash interop and its format tables hold functions that a structural clone would drop. `miijs` is excluded from `optimizeDeps` so the transform still runs in dev. Don't remove either half.

## Conventions

- Rust module files are PascalCase (`TotkApp.rs`, `Open_and_Save.rs`) even though items inside are `snake_case`; `main.rs` sets `#![allow(non_snake_case, non_camel_case_types)]`. Consequently **Tauri command parameters are declared in camelCase in Rust** (`documentId`, `internalPath`) and match the JS arg keys verbatim — there is no snake_case↔camelCase bridging.
- React components and files are PascalCase; four-space indent in JSX; semicolons.
- New format parsing belongs in `src-tauri/src/file_format/` (dispatch) and `src-tauri/src/parser/` (binary I/O), never inline in commands.
- `rustfmt` defines Rust layout. `.cargo/config.toml` sets `warnings = "allow"`, so `cargo check` being quiet does not mean warning-free.
- Commit subjects are short and imperative (`fixed mii rfl_db replacing`, `Xlink editing support`). Work happens on `dev-*` branches; `main` is the release branch.
- `README.md` is stale on one point: Python/.NET bridges were removed in 1.0.x — AINB, ASB, EVFL, and MSBT are all native Rust now. `CHANGELOG.md` is the accurate feature inventory.

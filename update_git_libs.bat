@echo off
cd src-tauri
cargo update -p msyt --verbose
cargo update -p msbt --verbose
cargo update -p msbt_bindings_rs --verbose
@REM cargo update -p roead
@REM cargo update -p zstud-sys
cd ..
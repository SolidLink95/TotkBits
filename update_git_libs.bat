@echo off
cd src-tauri
cargo update -p msyt
cargo update -p msbt
cargo update -p msbt_bindings_rs
cd ..
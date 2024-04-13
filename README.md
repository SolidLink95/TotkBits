# TotkBits
An attempt to write custom TOTK files editor, similar to [WildBits](https://github.com/NiceneNerd/Wild-Bits). Written in rust, the gui is made in react. For now some files' formats are parsed through Python. It is a temporary solution until I rewrite it completely to rust.

## Features
Mostly those already available in NX Editor, but also:
- filtering SARC contents,
- highlighting  <span style="color:#205F63;">added</span> and <span style="color:#826C00;">modded</span> SARC entries,
- searching for specific text query in entire SARC archive,
- more supported formats: ASB, AINB, Tag.Product, etc.
- drag-and-drop: simply drag the file to the window area in order to open it (dragging multiple files will open only the first one)

## Zstd

All `*.zs` files all automatically compressed/decompressed. If you wish to save the file without compression click `Save as` then simply remove `.zs` extension from file path.

# Supported formats

- ASB
- AINB
- Tag.Product
- BYML
- SARC
- AAMP
- MSBT
- RESTBL (RSTB)
- esetb.byml (editing everything except PTCL file section is supported)
- JSON/YAML and other plaintext formats

In order to save the file as plaintext, click `Save as` and choose one of the extensions: json, yaml, yml, txt.

# Contributors

- [NiceneNerd](https://github.com/NiceneNerd): [roead](https://github.com/NiceneNerd/roead), part of [msbt](https://github.com/NiceneNerd/msyt) library (used only for Big Endian msbt) and [RESTBL library](https://github.com/NiceneNerd/restbl)
- [jordanbtucker](https://github.com/jordanbtucker): [msbt c++ library](https://github.com/EPD-Libraries/msbt) (used for Little Endian msbt)
- [dt-12345](https://github.com/dt-12345): [AINB](https://github.com/dt-12345/ainb.git) and [ASB](https://github.com/dt-12345/asb.git) parsers

# Known Issues
- .szs files can be edited and saved but for some reason the games like Super Mario Oddyssey won't boot with those (its the fault of roead::Yaz0::compress). Opening file again in Switch Toolbox and saving fixes this issue.
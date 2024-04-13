# TotkBits
TotkBits is a custom TOTK file editor project, similar to [WildBits](https://github.com/NiceneNerd/Wild-Bits). It's developed in Rust, and the GUI is implemented in React. Currently, some file formats are parsed using Python, but this is a temporary solution until the functionality is fully rewritten in Rust.

## Features
This tool includes most functionalities found in the NX Editor and introduces additional features:
- Filtering SARC contents,
- Highlighting <span style="color:#205F63;">added</span> and <span style="color:#826C00;">modded</span> SARC entries, ![Alt text](preview/p1.png "")
- Searching for specific text queries within the entire SARC archive,
- Supporting additional formats: ASB, AINB, Tag.Product, etc. ![Alt text](preview/p2.png "")
- Drag-and-drop: simply drag the file into the window area to open it (dragging multiple files will open only the first one).

## Zstd
All `*.zs` files are automatically compressed or decompressed. If you wish to save the file without compression, click `Save as` then simply remove the `.zs` extension from the file path.

# Supported Formats
- ASB
- AINB
- Tag.Product
- BYML
- SARC
- AAMP
- MSBT
- RESTBL (RSTB)
- esetb.byml (editing everything except the PTCL file section is supported)
- JSON/YAML and other plaintext formats

To save the file as plaintext, click `Save as` and choose one of the extensions: json, yaml, yml, txt.

# Contributors
- [NiceneNerd](https://github.com/NiceneNerd): [roead](https://github.com/NiceneNerd/roead), part of the [msbt](https://github.com/NiceneNerd/msyt) library (used only for Big Endian msbt) and the [RESTBL library](https://github.com/NiceneNerd/restbl)
- [jordanbtucker](https://github.com/jordanbtucker): [msbt c++ library](https://github.com/EPD-Libraries/msbt) (used for Little Endian msbt)
- [dt-12345](https://github.com/dt-12345): [AINB](https://github.com/dt-12345/ainb.git) and [ASB](https://github.com/dt-12345/asb.git) parsers

# Known Issues
- .szs files can be edited and saved, but for some reason, games like Super Mario Odyssey won't boot with those files (due to an issue with roead::Yaz0::compress). Reopening the file in Switch Toolbox and saving it again fixes this issue.

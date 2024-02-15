# msyt

A human readable and editable format for `msbt` files.

msyt is a YAML format specification for `msbt` files, allowing easy plaintext reading and writing.
This repo houses a binary used to interface with `msyt` files.

---

msyt is unique amongst its `msbt`-editing peers in that it does not require any `msbt` files after
the initial export. Once an `msyt` file is created, it can be used to create an `msbt` file.

## Usage

### Exporting

Use the `export` subcommand to create `msyt` files from `msbt` files.

#### Examples

See complete usage:
`msyt export --help`

Output `msyt` files next to their `msbt` counterparts:  
`msyt export path/to/file.msbt a/different/file.msbt`

Output all `msyt` files next to their `msbt` counterparts recursively in a directory:  
`msyt export -d path/to/dir/containing/msbt/files`

Output `msyt` files generated from files in `some/dir` into `another/dir`:  
`msyt export -o another/dir -d some/dir`

### Creating

Use the `create` subcommand to create `msbt` files from `msyt` files.

#### Examples

See complete usage:  
`msyt create --help`

Create `msbt` files next to `msyt` files:  
`msyt create path/to/file.msyt a/different/file.msyt`

Create `msbt` files next to all `msyt` files recursively in a directory:  
`msyt create -d path/to/dir/containing/msyt/files`

Create `msbt` files in `some/dir` from files in `another/dir`:  
`msyt create -o another/dir -d some/dir`

### Importing

Use the `import` subcommand to import a `msyt` into an existing `msbt`.

Note that this will keep the section order and label order. This will *not* add new labels and will not update attributes.

#### Examples

See complete usage:  
`msyt import --help`

Import `msyt` files into the `msbt` files adjacent to them:  
`msyt import path/to/file.msyt another/path/to/file.msyt`

Import `msyt` files adjacent to `msbt` files recursively in a directory:  
`msyt import -d some/dir`

Import `msyt` files adjacent to `msbt` files recursively in a directory, outputting the result in `another/dir`:  
`msyt import -o another/dir -d some/dir`

## Building

```shell
# from repo root
cargo build

# with optimisations
cargo build --release
```

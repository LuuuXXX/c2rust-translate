# c2rust-translate

A command-line tool for translating C code to Rust.

## Installation

Build from source:

```bash
cd tools/c2rust-translate
cargo build --release
```

The binary will be available at `target/release/c2rust-translate`.

## Usage

### Syntax

```bash
c2rust-translate --feature <FEATURE> <FILE>...
```

### Arguments

- `--feature <FEATURE>`: Feature name for the translation (required)
- `<FILE>...`: One or more C source files to translate (required)

### Examples

**Translate a single C file:**
```bash
c2rust-translate --feature myfeature myfile.c
```

**Translate multiple files:**
```bash
c2rust-translate --feature myfeature file1.c file2.c file3.c
```

### Help

Get help information:
```bash
c2rust-translate --help
```

## Development

### Running Tests

```bash
cargo test
```

### Building

```bash
cargo build
```

For release builds:
```bash
cargo build --release
```

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

### Basic Syntax

```bash
c2rust-translate translate [OPTIONS] <FILE>...
```

### Options

- `--feature <FEATURE>`: Optional feature name for the translation

### Examples

**Translate a single C file:**
```bash
c2rust-translate translate myfile.c
```

**Translate with a feature flag:**
```bash
c2rust-translate translate --feature myfeature myfile.c
```

**Translate multiple files:**
```bash
c2rust-translate translate file1.c file2.c file3.c
```

**Translate multiple files with a feature:**
```bash
c2rust-translate translate --feature myfeature file1.c file2.c
```

### Help

Get help information:
```bash
c2rust-translate --help
c2rust-translate translate --help
```

## Command Structure

The tool uses a subcommand structure:
- **Main command**: `c2rust-translate`
- **Subcommand**: `translate` - Performs the C to Rust translation
- **Option**: `--feature` - Optional feature name for the translation
- **Arguments**: One or more C source files to translate

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

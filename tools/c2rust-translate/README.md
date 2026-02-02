# c2rust-translate

A command-line tool for translating C code to Rust with automated orchestration support.

## Installation

Build from source:

```bash
cd tools/c2rust-translate
cargo build --release
```

The binary will be available at `target/release/c2rust-translate`.

## Usage

The tool provides two modes of operation:

### 1. Simple Translation Mode

Translate individual C files directly.

**Syntax:**
```bash
c2rust-translate translate [OPTIONS] <FILE>...
```

**Options:**
- `--feature <FEATURE>`: Optional feature name for the translation

**Examples:**

Translate a single C file:
```bash
c2rust-translate translate myfile.c
```

Translate with a feature flag:
```bash
c2rust-translate translate --feature myfeature myfile.c
```

Translate multiple files:
```bash
c2rust-translate translate file1.c file2.c file3.c
```

### 2. Auto-Orchestration Mode

Automatically scan, translate, build, and test C to Rust conversions.

**Syntax:**
```bash
c2rust-translate auto [OPTIONS]
```

**Options:**
- `-p, --path <PATH>`: Path to the project directory (defaults to current directory)
- `-f, --feature <FEATURE>`: Feature name for the translation
- `--ld-preload <LD_PRELOAD>`: Path to the mixed build library for LD_PRELOAD
- `--skip-checks`: Skip environment checks

**Examples:**

Run auto-orchestration in current directory:
```bash
c2rust-translate auto --feature myfeature
```

Run in a specific directory:
```bash
c2rust-translate auto --path /path/to/project --feature myfeature
```

Run with custom LD_PRELOAD:
```bash
c2rust-translate auto --feature myfeature --ld-preload /path/to/lib.so
```

### Auto Mode Features

The auto-orchestration mode:
- Scans for empty Rust files that need translation
- Translates corresponding C files using the specified feature
- Runs code analysis with `code-analyse`
- Executes clean/build/test pipeline with proper environment setup
- Provides error reporting for manual intervention when needed

### Requirements (Auto Mode)

The following tools must be available in your PATH:
- `c2rust-translate`: For translating C code to Rust
- `c2rust-config`: For retrieving build configuration
- `code-analyse`: For analyzing the translated code
- `git`: For version control operations

### Project Structure (Auto Mode)

The tool expects the following structure:

```
project-root/
  .c2rust/
    config.toml          # Configuration file with build/test commands
  feature-dir/
    file.c               # C source files
    file.rs              # Empty Rust files to be filled
```

### Configuration (Auto Mode)

The tool reads configuration from `.c2rust/config.toml`. Example:

```toml
[clean]
dir = "build"
command = "make clean"

[build]
dir = "."
command = "make"

[test]
dir = "."
command = "make test"
```

## Help

Get help information:
```bash
c2rust-translate --help
c2rust-translate translate --help
c2rust-translate auto --help
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

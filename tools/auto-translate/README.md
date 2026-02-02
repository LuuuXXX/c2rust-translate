# auto-translate

Automated C to Rust translation orchestration tool for the c2rust-translate project.

## Overview

`auto-translate` is a command-line tool that automates the process of translating C code to Rust, including:
- Scanning for empty Rust files that need translation
- Translating corresponding C files using `c2rust-translate`
- Running code analysis with `code-analyse`
- Building and testing the translated code
- Handling the full compilation and fixing pipeline

## Installation

Build the tool from source:

```bash
cd tools/auto-translate
cargo build --release
```

The binary will be available at `target/release/auto-translate`.

## Usage

```bash
auto-translate [OPTIONS]
```

### Options

- `-p, --path <PATH>`: Path to the project directory (defaults to current directory)
- `--ld-preload <LD_PRELOAD>`: Path to the mixed build library for LD_PRELOAD
- `--skip-checks`: Skip environment checks

### Examples

Run in the current directory:
```bash
auto-translate
```

Run in a specific directory:
```bash
auto-translate --path /path/to/project
```

Run with custom LD_PRELOAD path:
```bash
auto-translate --ld-preload /path/to/lib.so
```

## Requirements

The following tools must be available in your PATH:
- `c2rust`: For translating C code to Rust (command: `c2rust translate --feature <filename>`)
- `c2rust-config`: For retrieving build configuration
- `code-analyse`: For analyzing the translated code
- `git`: For version control operations

## Project Structure

The tool expects the following structure:

```
project-root/
  .c2rust/
    config.toml          # Configuration file with build/test commands
  feature-dir/
    file.c               # C source files
    file.rs              # Empty Rust files to be filled
```

## Configuration

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

## How It Works

1. **Environment Check**: Verifies that all required tools are available
2. **Project Discovery**: Finds the project root by locating the `.c2rust` directory
3. **Configuration Loading**: Reads build/test configuration from `config.toml`
4. **File Scanning**: Identifies empty `.rs` files that need translation
5. **Translation Pipeline**: For each empty Rust file:
   - Finds the corresponding C file
   - Runs `c2rust-translate` to generate Rust code
   - Runs `code-analyse` to analyze the result
   - Executes the clean/build/test pipeline
6. **Error Handling**: Stops on errors and prompts for manual intervention

## Error Handling

If any step fails, the tool will:
- Display the error message
- Indicate which command failed
- Exit with an error code
- Prompt the user to fix the issue manually and rerun

## Module Structure

- `main.rs`: CLI entry point and main orchestration logic
- `lib.rs`: Library entry point
- `error.rs`: Custom error types using `thiserror`
- `commands.rs`: External command execution utilities
- `config.rs`: Configuration file parsing and management
- `env.rs`: Environment checking and tool validation
- `file_scanner.rs`: File system scanning utilities
- `git.rs`: Git operations (for future use)
- `compiler.rs`: Compilation checking and error handling
- `translator.rs`: Translation process control and build execution

## Testing

Run the test suite:

```bash
cargo test
```

## License

See the main project LICENSE file.

# c2rust-translate

A Rust tool for automating the translation of C code to Rust using the c2rust framework.

## Features

- Automated C to Rust translation workflow
- Support for feature-based translation with `--feature` flag
- Automatic initialization of Rust project structure
- Integration with translation tools (`translate_and_fix.py`)
- Automatic build error detection and fixing
- Git-based version control integration
- Code analysis integration (`code-analyse`)
- Hybrid build testing support

## Installation

Build from source:

```bash
cargo build --release
```

The binary will be available at `target/release/c2rust-translate`.

## Usage

### Translate a Feature

```bash
c2rust-translate translate --feature <feature_name>
```

This command will:

1. **Initialize** - Check if the `<feature_name>/rust` directory exists, and initialize it if needed
2. **Scan** - Find all empty `.rs` files in the rust directory
3. **Translate** - For each empty `.rs` file:
   - Determine the type (variable or function) based on filename prefix (`var_` or `fun_`)
   - Translate the corresponding `.c` file to Rust
   - Build the project and fix any compilation errors
   - Commit the changes with git
   - Update code analysis
   - Run hybrid build tests

### Workflow Details

#### File Naming Convention

- `var_*.rs` - Variable declarations
- `fun_*.rs` - Function definitions

Each `.rs` file should have a corresponding `.c` file with the same name.

#### Required Tools

The following tools must be available in your PATH:

- `code-analyse` - For code analysis and initialization
- `translate_and_fix.py` - Python script for translation and error fixing
- `c2rust-config` - For configuration management (optional)
- `c2rust-clean`, `c2rust-build`, `c2rust-test` - For hybrid build testing (optional)

#### Translation Tool Usage

The tool calls `translate_and_fix.py` with the following arguments:

```bash
# For translation
python translate_and_fix.py --config config.toml --type <var|fn> --code <c_file> --output <rs_file>

# For fixing errors
python translate_and_fix.py --config config.toml --type <var|fn> --error <error_file> --output <rs_file>
```

#### Code Analysis Tool Usage

```bash
# Initialize
code-analyse --init --feature <feature_name>

# Update
code-analyse --update --feature <feature_name>
```

#### Hybrid Build Environment Variables

When running the build command, the following environment variables are set:

- `C2RUST_FEATURE_ROOT=<feature_path>` - Root directory of the feature

## Example

```bash
# Translate a feature called "my_feature"
c2rust-translate translate --feature my_feature
```

This will process all empty `.rs` files in `my_feature/rust/` directory.

## Error Handling

- If the rust directory initialization fails, the tool will exit with an error
- If a corresponding `.c` file is missing, the tool will warn and skip the file
- If translation or error fixing fails, the tool will exit with an error
- If hybrid build tests fail, the tool will warn but continue

## Git Integration

The tool automatically commits changes at these points:

1. After initializing the rust directory
2. After successfully translating each file
3. After updating code analysis

Commit messages follow the format:
- `"Initialize <feature> rust directory"`
- `"Translate <feature> from C to Rust"`
- `"Update code analysis for <feature>"`

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]
# c2rust-translate

## Automated C to Rust Translation and Hybrid Build Tool

This repository contains tools for automated C to Rust translation and hybrid build processes.

### Main Tool: auto_translate.py

`auto_translate.py` is an automated tool that orchestrates the C to Rust translation process, including:
- Initializing the feature directory structure
- Scanning for empty .rs files that need translation
- Translating C code to Rust using LLM-based translation
- Fixing compilation errors automatically
- Running code analysis updates
- Executing build and test commands with hybrid build support

### Prerequisites

The tool requires the following commands to be available:

1. **code-analyse**: For initializing and updating code analysis
   - `code-analyse --init --feature <feature>`
   - `code-analyse --update --feature <feature>`

2. **Python 3** with the following modules:
   - `toml` (install with: `pip install toml`)

3. **Rust toolchain**:
   - `cargo` for building Rust code

4. **Git**: For version control

5. Optional tools (used if available):
   - `c2rust-config`: For retrieving build commands
   - `c2rust-clean`, `c2rust-build`, `c2rust-test`: For executing build pipeline

### Usage

#### Basic Usage

```bash
python3 auto_translate.py --feature <feature_name>
```

#### With Custom Project Root

```bash
python3 auto_translate.py --feature <feature_name> --project-root /path/to/project
```

### Translation Tool (translate_and_fix.py)

Located in `tools/translate_and_fix/`, this is the core LLM-based translation tool.

#### Variable Translation

```bash
python translate_and_fix.py --config config.toml --type var --code code.c --output output.rs
```

#### Function Translation

```bash
python translate_and_fix.py --config config.toml --type fn --code code.c --output output.rs
```

#### Syntax Fixing

```bash
python translate_and_fix.py --config config.toml --type fix --code code.rs --output output.rs --error error.txt
```

### How It Works

1. **Initialization Phase**:
   - Checks if `<feature>/rust` directory exists
   - Runs `code-analyse --init` if needed
   - Commits initialization results

2. **Main Loop**:
   - Performs initial `cargo build`
   - Scans for empty `.rs` files
   - For each empty file:
     - Determines file type from prefix (`var_` or `fun_`)
     - Verifies corresponding C file exists
     - Translates C code to Rust
     - Fixes compilation errors iteratively
     - Commits successful translations
     - Updates code analysis
     - Executes build/test commands

3. **Hybrid Build**:
   - Build commands are executed with environment variables:
     - `LD_PRELOAD`: Path to hybrid build library
     - `C2RUST_FEATURE_ROOT`: Path to feature directory

### Configuration

Configuration is stored in `tools/translate_and_fix/config.toml`:

- **LLM settings**: Model, API key, temperature, etc.
- **Feature-specific settings**: Build commands, directories, etc.

### Error Handling

The tool includes robust error handling:
- Prompts user for input when project corruption is detected
- Limits fix iterations to prevent infinite loops
- Provides clear error messages and logging
- Commits work incrementally for safety

### File Naming Convention

- Files prefixed with `var_` are treated as variable definitions
- Files prefixed with `fun_` are treated as function definitions
- Corresponding C files must have the same base name with `.c` extension

### Examples

Process a feature named "myfeature":

```bash
python3 auto_translate.py --feature myfeature
```

This will:
1. Check/create `myfeature/rust` directory
2. Find all empty `.rs` files in the rust directory
3. Translate corresponding `.c` files
4. Fix any compilation errors
5. Run tests and builds
6. Commit each successful translation

### Environment Variables

- `HYBRID_BUILD_LIB`: Path to hybrid build library (optional)
- `C2RUST_FEATURE_ROOT`: Set automatically by the tool

### Logging

The tool provides detailed logging with INFO, WARNING, and ERROR levels:
- `[INFO]`: General progress information
- `[WARNING]`: Non-fatal issues
- `[ERROR]`: Fatal errors that cause the tool to exit

### Troubleshooting

**"code-analyse tool not found"**
- Ensure code-analyse is installed and in your PATH

**"Translation failed"**
- Check that the LLM API key is configured correctly in config.toml
- Verify the C source file contains valid code

**"Max fix iterations reached"**
- The automatic fix loop has exceeded the retry limit
- Manual intervention may be required to fix the compilation errors

**"Corresponding C file not found"**
- Ensure the .rs file has a corresponding .c file with the same base name
- You may need to run `code-analyse --init` to regenerate the project structure
# Enhanced User Interaction Interface

This document describes the enhanced user interaction features implemented in the c2rust-translate tool.

## Overview

The tool now provides a more intuitive and flexible user interaction experience with:
- Side-by-side code comparison display
- Context-specific interactive prompts
- Auto-accept mode for batch processing
- Consistent UI across all interactive scenarios

## New Features

### 1. Code Comparison Display

The tool now displays C and Rust code side-by-side in a formatted comparison view:

```
═══════════════════════════════════════════════════════════════════
                  C vs Rust Code Comparison                        
═══════════════════════════════════════════════════════════════════
┌─────── C Source Code ────────┬─────── Rust Code ────────────────┐
│ 1 int add(int a, int b) {    │ 1 pub fn add(a: i32, b: i32)    │
│ 2     return a + b;          │ 2     -> i32 {                  │
│ 3 }                          │ 3     a + b                      │
│                              │ 4 }                              │
└──────────────────────────────┴──────────────────────────────────┘

═══════════════════════════════════════════════════════════════════
                        Test Result                                
═══════════════════════════════════════════════════════════════════
✓ All tests passed
```

### 2. Interactive Prompts Based on Context

#### Scenario 1: Compilation Success with Passing Tests

When compilation succeeds and all tests pass, you get 4 options:

```
✓ Compilation and tests successful!

What would you like to do?

Available options:
  1. Accept this code (will be committed)
  2. Auto-accept all subsequent translations
  3. Manual fix (edit the file with VIM)
  4. Exit (abort the translation process)

Enter your choice (1/2/3/4):
```

**Option 1 - Accept**: Accepts the current translation and commits it.

**Option 2 - Auto-accept**: Enables auto-accept mode for the current session, automatically accepting all future successful translations without prompting. Useful for batch processing.

**Option 3 - Manual Fix**: Opens the Rust file in VIM for manual editing, then rebuilds and retests.

**Option 4 - Exit**: Aborts the translation process.

#### Scenario 2: Test Failure

When tests fail after successful compilation, you get 3 options:

```
⚠ Tests failed - What would you like to do?

Available options:
  1. Add fix suggestion for AI to modify
  2. Manual fix (edit the file with VIM)
  3. Exit (abort the translation process)

Enter your choice (1/2/3):
```

**Option 1 - Add Suggestion**: Prompts you to enter a fix suggestion that will be used by the AI to modify the code.

**Option 2 - Manual Fix**: Opens the file in VIM for manual editing.

**Option 3 - Exit**: Aborts the translation process.

#### Scenario 3: Compilation Failure (Max Retries Reached)

When compilation fails after reaching the maximum number of fix attempts, you get 3 options:

```
⚠ Compilation failed - What would you like to do?

Available options:
  1. Add fix suggestion for AI to modify
  2. Manual fix (edit the file with VIM)
  3. Exit (abort the translation process)

Enter your choice (1/2/3):
```

The options work the same as in the test failure scenario.

### 3. Auto-Accept Mode

Auto-accept mode allows you to process multiple files without manual intervention:

- Enabled by selecting option 2 when tests pass
- Once enabled, all future successful translations are automatically accepted
- Particularly useful for batch processing large codebases
- Mode is session-based (resets when you restart the tool)

### 4. Improved Error Context

All interactive prompts now show:
- File locations (C source and Rust target)
- Side-by-side code comparison
- Build or test error messages
- Clear result indicators (✓ for success, ✗ for failure)

## Implementation Details

### New Modules

- **`src/diff_display.rs`**: Handles side-by-side code comparison display
- Enhanced **`src/interaction.rs`**: New enums and prompts for different scenarios

### New Enums

```rust
// For compilation success with passing tests
pub enum CompileSuccessChoice {
    Accept,
    AutoAccept,
    ManualFix,
    Exit,
}

// For test or compilation failures
pub enum FailureChoice {
    AddSuggestion,
    ManualFix,
    Exit,
}
```

### Key Functions

- `prompt_compile_success_choice()`: 4-option prompt for test success
- `prompt_test_failure_choice()`: 3-option prompt for test failure
- `prompt_compile_failure_choice()`: 3-option prompt for compilation failure
- `display_code_comparison()`: Side-by-side code display
- `is_auto_accept_mode()`, `enable_auto_accept_mode()`: Auto-accept mode management

## Backward Compatibility

The enhanced UI maintains backward compatibility:
- Existing `UserChoice` enum is preserved for compatibility
- Old prompts still work where needed
- All new features are additive, not replacing existing functionality

## Testing

Run the test suite to verify the implementation:

```bash
cargo test
```

All 60+ unit tests pass, ensuring:
- Enum variants work correctly
- Auto-accept mode state management functions properly
- Code comparison display handles edge cases
- File path display works correctly

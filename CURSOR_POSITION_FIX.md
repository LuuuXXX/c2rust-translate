# Cursor Position Fix for CJK Characters

## Problem Description

When users were inputting suggestions with mixed Chinese and English characters, the cursor position became misaligned and did not correctly correspond to the actual text position. This was particularly noticeable when:

- Typing mixed Chinese/English text like "Hello世界"
- Using arrow keys to navigate through the text
- Using backspace or delete keys to edit the text
- The cursor would appear several positions away from where it should be

## Root Cause

The issue was in the `inquire` library version 0.7.5, which had a bug in its cursor positioning logic for wide characters (CJK characters). CJK characters typically occupy 2 display columns in the terminal, while ASCII characters occupy 1 column. The library was not correctly accounting for this difference when calculating cursor positions.

## Solution

Updated the `inquire` library from version 0.7.5 to 0.9.x (currently 0.9.3).

### Key Changes

1. **Cargo.toml**: Updated `inquire` dependency from `"0.7"` to `"0.9"`
2. **Cargo.lock**: Updated all transitive dependencies accordingly

### Why This Fixes the Issue

The `inquire` library version 0.8.0 (released 2025-09-14) included a specific fix for this issue:

> "Fix incorrect cursor placement when inputting CJK characters."

This fix properly handles the display width calculation for:
- Multi-byte UTF-8 characters (Chinese, Japanese, Korean characters)
- Single-byte ASCII characters (English letters, numbers)
- Mixed text with both character types
- Special characters and emojis

### Dependencies Updated

| Package | Old Version | New Version |
|---------|-------------|-------------|
| inquire | 0.7.5 | 0.9.x (currently 0.9.3) |
| crossterm | 0.25.0 | 0.29.0 |
| unicode-width | 0.1.14 | 0.2.2 |

## Testing

### Automated Tests

All existing tests continue to pass:
```bash
cargo test
```

Result: **All tests passed**

### Manual Testing

We've provided an example program to manually test the cursor positioning:

```bash
cargo run --example test_cursor_position
```

Try typing various combinations:
- Pure English: "Hello World"
- Pure Chinese: "你好世界"
- Mixed: "Hello世界"
- Complex: "中文English混合123"
- Emojis: "test🎉混合"

Verify that:
- ✓ Cursor stays aligned with visible text
- ✓ Arrow keys navigate correctly
- ✓ Backspace and delete work at the right position
- ✓ Text editing feels natural and intuitive

### Security Verification

Dependencies can be verified using:
```bash
cargo audit
```

At the time of this update, no known security vulnerabilities were found in the GitHub Advisory Database for the updated dependencies.

## Impact Assessment

### Affected Components

The following components use `inquire::Text` for user input and benefit from this fix:

1. **src/interaction.rs**:
   - `prompt_suggestion()` - Fix suggestion input with CJK characters
   
2. **src/file_scanner.rs**:
   - `prompt_file_selection()` - File selection input with CJK paths

### Backward Compatibility

This update is fully backward compatible:
- ✓ All existing APIs remain unchanged
- ✓ No breaking changes in the code
- ✓ All tests pass without modification
- ✓ No changes required to calling code

### Performance

No significant performance impact observed in local testing:
- Build time: Similar to previous versions (measured on a developer laptop; exact times will vary by machine)
- Test execution: Similar to previous versions (measured on a developer laptop; exact times will vary by machine)
- Runtime behavior: No noticeable changes in interactive prompts during manual testing

## Acceptance Criteria Status

- ✅ Cursor position correctly aligns with text when typing mixed Chinese and English characters
- ✅ No regression in cursor behavior for pure English or pure Chinese input
- ✅ Solution handles edge cases (wide characters, multi-byte sequences)
- ✅ All existing tests pass
- ✅ No security vulnerabilities introduced

## Additional Benefits

By updating to inquire 0.9.x, we also gain:

1. **Better multi-line handling**: Fixed bug where inputs spanning 3+ lines would break text rendering
2. **Improved autocomplete**: Fixed autocomplete suggestions not being updated after a suggestion is accepted
3. **Better user experience**: Multiple UX improvements for prompts
4. **Modern dependencies**: Updated to latest versions of crossterm and unicode-width

## References

- [inquire CHANGELOG](https://github.com/mikaelmello/inquire/blob/main/CHANGELOG.md)
- [inquire v0.8.0 Release Notes](https://github.com/mikaelmello/inquire/releases/tag/v0.8.0)
- [inquire v0.9.3 Release Notes](https://github.com/mikaelmello/inquire/releases/tag/v0.9.3)
- [GitHub Advisory Database](https://github.com/advisories)

## Related Files

- `Cargo.toml` - Dependency specification
- `Cargo.lock` - Locked dependency versions
- `src/interaction.rs` - User interaction with text input
- `src/file_scanner.rs` - File selection with text input
- `examples/test_cursor_position.rs` - Manual test example

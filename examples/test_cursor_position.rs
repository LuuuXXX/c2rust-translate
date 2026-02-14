/// Example program to test cursor positioning with CJK characters
///
/// This example demonstrates that the cursor positioning issue with mixed
/// Chinese and English characters has been fixed by upgrading inquire from
/// v0.7.5 to 0.9.
///
/// To run this example:
/// ```bash
/// cargo run --example test_cursor_position
/// ```
///
/// Try entering mixed text like:
/// - "Hello世界"
/// - "中文English混合"
/// - "测试123test"
///
/// The cursor should remain properly aligned with the visible text.

use inquire::Text;

fn main() {
    println!("========================================");
    println!("Testing Cursor Position with CJK Characters");
    println!("========================================");
    println!();
    println!("This test validates that cursor positioning works correctly");
    println!("when mixing Chinese and English characters.");
    println!();
    println!("Try typing:");
    println!("  - Mixed text: Hello世界");
    println!("  - Chinese+English: 中文English混合");
    println!("  - Numbers+CJK: 测试123test");
    println!();
    println!("Use arrow keys, backspace, and delete to edit.");
    println!("The cursor should stay aligned with the text.");
    println!();

    // Test 1: Basic text input with mixed characters
    let result = Text::new("Enter mixed Chinese/English text:")
        .with_help_message("Try typing: Hello世界 or 中文English")
        .prompt();

    match result {
        Ok(text) => {
            println!("✓ You entered: {}", text);
            println!("✓ Text length (chars): {}", text.chars().count());
            println!("✓ Text length (bytes): {}", text.len());
        }
        Err(e) => println!("Error: {}", e),
    }

    println!();
    println!("========================================");
    println!("Test Complete!");
    println!("========================================");
}

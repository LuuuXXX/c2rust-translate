//! Code comparison display utilities for showing C and Rust code side-by-side

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// Display C and Rust code side-by-side with test/build results
pub fn display_code_comparison(
    c_file: &Path,
    rust_file: &Path,
    result_message: &str,
    result_type: ResultType,
) -> Result<()> {
    println!("│");
    println!("{}", "═══════════════════════════════════════════════════════════════════".bright_cyan());
    println!("{}", "                  C vs Rust Code Comparison                        ".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════════════════════".bright_cyan());
    
    // Read file contents
    let c_content = std::fs::read_to_string(c_file)
        .with_context(|| format!("Failed to read C file: {}", c_file.display()))?;
    let rust_content = std::fs::read_to_string(rust_file)
        .with_context(|| format!("Failed to read Rust file: {}", rust_file.display()))?;
    
    let c_lines: Vec<&str> = c_content.lines().collect();
    let rust_lines: Vec<&str> = rust_content.lines().collect();
    
    // Display header
    // Row format: "│ {:3} {:<26}│ {:3} {:<31}│"
    // C side: space(1) + line_num(3) + space(1) + content(26) = 31 chars
    // Rust side: space(1) + line_num(3) + space(1) + content(31) = 36 chars
    println!("┌{:─<31}┬{:─<36}┐", "─ C Source Code ", "─ Rust Code ─");
    
    // Display lines side by side
    let max_lines = std::cmp::max(c_lines.len(), rust_lines.len());
    for i in 0..max_lines {
        let c_line = c_lines.get(i).unwrap_or(&"");
        let rust_line = rust_lines.get(i).unwrap_or(&"");
        
        // Truncate lines if too long to fit in the column (using character count for UTF-8 safety)
        // Column widths: C=26 chars, Rust=31 chars
        // Reserve 3 chars for "..." suffix
        let c_display = if c_line.chars().count() > 26 {
            let truncated: String = c_line.chars().take(23).collect();
            format!("{}...", truncated)
        } else {
            c_line.to_string()
        };
        
        let rust_display = if rust_line.chars().count() > 31 {
            let truncated: String = rust_line.chars().take(28).collect();
            format!("{}...", truncated)
        } else {
            rust_line.to_string()
        };
        
        println!(
            "│ {:3} {:<26}│ {:3} {:<31}│",
            format!("{}", i + 1).dimmed(),
            c_display,
            format!("{}", i + 1).dimmed(),
            rust_display
        );
    }
    
    println!("└{:─<31}┴{:─<36}┘", "", "");
    
    // Display result section
    display_result_section(result_message, result_type);
    
    Ok(())
}

/// Display test or build result section
fn display_result_section(message: &str, result_type: ResultType) {
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════════════".bright_cyan());
    
    let header = match result_type {
        ResultType::TestPass => "                        Test Result                                ".bright_green().bold(),
        ResultType::TestFail => "                        Test Result                                ".bright_red().bold(),
        ResultType::BuildSuccess => "                       Build Result                               ".bright_green().bold(),
        ResultType::BuildFail => "                       Build Result                               ".bright_red().bold(),
    };
    
    println!("{}", header);
    println!("{}", "═══════════════════════════════════════════════════════════════════".bright_cyan());
    
    // Format message with appropriate color
    let formatted_message = match result_type {
        ResultType::TestPass | ResultType::BuildSuccess => message.bright_green(),
        ResultType::TestFail | ResultType::BuildFail => message.bright_red(),
    };
    
    println!("{}", formatted_message);
    println!();
}

/// Type of result being displayed
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ResultType {
    TestPass,
    TestFail,
    BuildSuccess,
    BuildFail,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_display_code_comparison_basic() {
        // Create temporary C file
        let mut c_file = NamedTempFile::new().unwrap();
        writeln!(c_file, "int add(int a, int b) {{").unwrap();
        writeln!(c_file, "    return a + b;").unwrap();
        writeln!(c_file, "}}").unwrap();
        c_file.flush().unwrap();
        
        // Create temporary Rust file
        let mut rust_file = NamedTempFile::new().unwrap();
        writeln!(rust_file, "pub fn add(a: i32, b: i32)").unwrap();
        writeln!(rust_file, "    -> i32 {{").unwrap();
        writeln!(rust_file, "    a + b").unwrap();
        writeln!(rust_file, "}}").unwrap();
        rust_file.flush().unwrap();
        
        // Test that display doesn't panic
        let result = display_code_comparison(
            c_file.path(),
            rust_file.path(),
            "✓ All tests passed (3/3)",
            ResultType::TestPass
        );
        
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_result_type_variants() {
        // Just verify enum variants exist
        let _ = ResultType::TestPass;
        let _ = ResultType::TestFail;
        let _ = ResultType::BuildSuccess;
        let _ = ResultType::BuildFail;
    }
}

//! 代码比较显示工具，用于并排显示 C 和 Rust 代码

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// 并排显示 C 和 Rust 代码及测试/构建结果
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
    
    // 读取文件内容
    let c_content = std::fs::read_to_string(c_file)
        .with_context(|| format!("Failed to read C file: {}", c_file.display()))?;
    let rust_content = std::fs::read_to_string(rust_file)
        .with_context(|| format!("Failed to read Rust file: {}", rust_file.display()))?;
    
    let c_lines: Vec<&str> = c_content.lines().collect();
    let rust_lines: Vec<&str> = rust_content.lines().collect();
    
    // 显示表头
    // 行格式："│ {:3} {:<26}│ {:3} {:<31}│"
    // C 侧：空格(1) + 行号(3) + 空格(1) + 内容(26) = 31 字符
    // Rust 侧：空格(1) + 行号(3) + 空格(1) + 内容(31) = 36 字符
    println!("┌{:─<31}┬{:─<36}┐", "─ C Source Code ", "─ Rust Code ─");
    
    // 并排显示行
    let max_lines = std::cmp::max(c_lines.len(), rust_lines.len());
    for i in 0..max_lines {
        let c_line = c_lines.get(i).unwrap_or(&"");
        let rust_line = rust_lines.get(i).unwrap_or(&"");
        
        // 如果行太长而无法放入列中，则截断（使用字符计数以确保 UTF-8 安全）
        // 列宽：C=26 字符，Rust=31 字符
        // 为 "..." 后缀保留 3 个字符
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
    
    // 显示结果部分
    display_result_section(result_message, result_type);
    
    Ok(())
}

/// 显示测试或构建结果部分
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
    
    // 用适当的颜色格式化消息
    let formatted_message = match result_type {
        ResultType::TestPass | ResultType::BuildSuccess => message.bright_green(),
        ResultType::TestFail | ResultType::BuildFail => message.bright_red(),
    };
    
    println!("{}", formatted_message);
    println!();
}

/// 显示的结果类型
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
        // 创建临时 C 文件
        let mut c_file = NamedTempFile::new().unwrap();
        writeln!(c_file, "int add(int a, int b) {{").unwrap();
        writeln!(c_file, "    return a + b;").unwrap();
        writeln!(c_file, "}}").unwrap();
        c_file.flush().unwrap();
        
        // 创建临时 Rust 文件
        let mut rust_file = NamedTempFile::new().unwrap();
        writeln!(rust_file, "pub fn add(a: i32, b: i32)").unwrap();
        writeln!(rust_file, "    -> i32 {{").unwrap();
        writeln!(rust_file, "    a + b").unwrap();
        writeln!(rust_file, "}}").unwrap();
        rust_file.flush().unwrap();
        
        // 测试显示不会引发 panic
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
        // 仅验证枚举变体是否存在
        let _ = ResultType::TestPass;
        let _ = ResultType::TestFail;
        let _ = ResultType::BuildSuccess;
        let _ = ResultType::BuildFail;
    }
}

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
    println!("{}", "═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════".bright_cyan());
    println!("{}", "                                                                                    C vs Rust Code Comparison                                                                                                      ".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════".bright_cyan());
    
    // 读取文件内容
    let c_content = std::fs::read_to_string(c_file)
        .with_context(|| format!("Failed to read C file: {}", c_file.display()))?;
    let rust_content = std::fs::read_to_string(rust_file)
        .with_context(|| format!("Failed to read Rust file: {}", rust_file.display()))?;
    
    let c_lines: Vec<&str> = c_content.lines().collect();
    let rust_lines: Vec<&str> = rust_content.lines().collect();
    
    // 显示表头
    // 行格式："│ {:3} {:<90}│ {:3} {:<110}│"
    // C 侧：空格(1) + 行号(3) + 空格(1) + 内容(90) = 95 字符
    // Rust 侧：空格(1) + 行号(3) + 空格(1) + 内容(110) = 115 字符
    println!("┌{:─<95}┬{:─<115}┐", "─ C Source Code ", "─ Rust Code ─");
    
    // 并排显示行
    let max_lines = std::cmp::max(c_lines.len(), rust_lines.len());
    for i in 0..max_lines {
        let c_line = c_lines.get(i).unwrap_or(&"");
        let rust_line = rust_lines.get(i).unwrap_or(&"");
        
        // 如果行太长而无法放入列中，则换行显示
        // 列宽：C=90 字符，Rust=110 字符
        let c_wrapped = wrap_line(c_line, 90);
        let rust_wrapped = wrap_line(rust_line, 110);
        
        let max_wrapped_lines = std::cmp::max(c_wrapped.len(), rust_wrapped.len());
        
        for j in 0..max_wrapped_lines {
            let c_display = c_wrapped.get(j).map(|s| s.as_str()).unwrap_or("");
            let rust_display = rust_wrapped.get(j).map(|s| s.as_str()).unwrap_or("");
            
            // 第一行显示行号，后续换行不显示行号
            let c_line_num = if j == 0 {
                format!("{}", i + 1).dimmed().to_string()
            } else {
                "   ".to_string()
            };
            
            let rust_line_num = if j == 0 {
                format!("{}", i + 1).dimmed().to_string()
            } else {
                "   ".to_string()
            };
            
            println!(
                "│ {} {:<90}│ {} {:<110}│",
                c_line_num,
                c_display,
                rust_line_num,
                rust_display
            );
        }
    }
    
    println!("└{:─<95}┴{:─<115}┘", "", "");
    
    // 显示结果部分
    display_result_section(result_message, result_type);
    
    Ok(())
}

/// 将长行按指定宽度换行
/// 
/// # Arguments
/// * `line` - 要换行的文本行
/// * `width` - 每行的最大字符宽度
/// 
/// # Returns
/// 换行后的字符串向量
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let char_count = line.chars().count();
    
    if char_count <= width {
        return vec![line.to_string()];
    }
    
    let mut wrapped_lines = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut start = 0;
    
    while start < char_count {
        let end = std::cmp::min(start + width, char_count);
        let segment: String = chars[start..end].iter().collect();
        wrapped_lines.push(segment);
        start = end;
    }
    
    wrapped_lines
}

/// 显示测试或构建结果部分
fn display_result_section(message: &str, result_type: ResultType) {
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════".bright_cyan());
    
    let header = match result_type {
        ResultType::TestPass => "                                                                                      Test Result                                                                                                        ".bright_green().bold(),
        ResultType::TestFail => "                                                                                      Test Result                                                                                                        ".bright_red().bold(),
        ResultType::BuildSuccess => "                                                                                     Build Result                                                                                                       ".bright_green().bold(),
        ResultType::BuildFail => "                                                                                     Build Result                                                                                                       ".bright_red().bold(),
    };
    
    println!("{}", header);
    println!("{}", "═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════".bright_cyan());
    
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
    
    #[test]
    fn test_wrap_line_short() {
        let line = "short line";
        let wrapped = wrap_line(line, 20);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], "short line");
    }
    
    #[test]
    fn test_wrap_line_exact_width() {
        let line = "exactly twenty chars";
        let wrapped = wrap_line(line, 20);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], "exactly twenty chars");
    }
    
    #[test]
    fn test_wrap_line_long() {
        let line = "This is a very long line that needs to be wrapped into multiple lines";
        let wrapped = wrap_line(line, 20);
        assert!(wrapped.len() > 1);
        // 验证每行不超过指定宽度
        for segment in &wrapped {
            assert!(segment.chars().count() <= 20);
        }
    }
    
    #[test]
    fn test_wrap_line_utf8() {
        let line = "这是一个很长的中文字符串需要换行显示";
        let wrapped = wrap_line(line, 10);
        assert!(wrapped.len() > 1);
        for segment in &wrapped {
            assert!(segment.chars().count() <= 10);
        }
    }
}

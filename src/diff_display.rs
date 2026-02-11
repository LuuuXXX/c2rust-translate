//! 代码比较显示工具，用于并排显示 C 和 Rust 代码

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;
use terminal_size::{Width, terminal_size};

// 默认列宽常量（当无法检测终端大小时使用）
const DEFAULT_C_COLUMN_WIDTH: usize = 95;
const DEFAULT_RUST_COLUMN_WIDTH: usize = 95;
const LINE_NUM_WIDTH: usize = 3;
const CONTINUATION_MARKER: &str = "   ";
// 终端宽度的最小要求
const MIN_TERMINAL_WIDTH: usize = 80;
// 分隔符和边框占用的字符数
const SEPARATOR_WIDTH: usize = 5; // "│", "│", "│" plus spaces

/// 获取适配终端大小的列宽
/// 
/// 根据当前终端宽度动态计算 C 和 Rust 代码列的宽度
/// 
/// # Returns
/// 返回 (c_column_width, rust_column_width) 元组
fn get_adaptive_column_widths() -> (usize, usize) {
    if let Some((Width(terminal_width), _)) = terminal_size() {
        let term_width = terminal_width as usize;
        
        // 如果终端太小，使用默认值
        if term_width < MIN_TERMINAL_WIDTH {
            return (DEFAULT_C_COLUMN_WIDTH, DEFAULT_RUST_COLUMN_WIDTH);
        }
        
        // 计算可用于代码显示的宽度
        // 格式："│ num code │ num code │"
        // 需要减去：行号列(2个，各4个字符) + 分隔符(3个) + 边框(2个)
        let line_num_space = (LINE_NUM_WIDTH + 1) * 2; // 两侧的行号和空格
        let borders_and_separators = SEPARATOR_WIDTH;
        let available_width = term_width.saturating_sub(line_num_space + borders_and_separators);
        
        // 将可用宽度平均分配给两列
        let column_width = available_width / 2;
        
        // 确保列宽不会太小
        let min_column_width = 40;
        if column_width < min_column_width {
            return (DEFAULT_C_COLUMN_WIDTH, DEFAULT_RUST_COLUMN_WIDTH);
        }
        
        (column_width, column_width)
    } else {
        // 无法检测终端大小，使用默认值
        (DEFAULT_C_COLUMN_WIDTH, DEFAULT_RUST_COLUMN_WIDTH)
    }
}

/// 并排显示 C 和 Rust 代码及测试/构建结果
pub fn display_code_comparison(
    c_file: &Path,
    rust_file: &Path,
    result_message: &str,
    result_type: ResultType,
) -> Result<()> {
    // 获取适配终端大小的列宽
    let (c_column_width, rust_column_width) = get_adaptive_column_widths();
    
    println!("│");
    
    // 根据计算出的列宽动态生成分隔线
    let total_width = (LINE_NUM_WIDTH + 1 + c_column_width + 1) + (LINE_NUM_WIDTH + 1 + rust_column_width + 1) + 3;
    println!("{}", "═".repeat(total_width).bright_cyan());
    
    // 居中显示标题
    let title = "C vs Rust Code Comparison";
    let padding = (total_width.saturating_sub(title.len())) / 2;
    println!("{}{}{}", " ".repeat(padding), title.bright_cyan().bold(), " ".repeat(total_width - padding - title.len()));
    println!("{}", "═".repeat(total_width).bright_cyan());
    
    // 读取文件内容
    let c_content = std::fs::read_to_string(c_file)
        .with_context(|| format!("Failed to read C file: {}", c_file.display()))?;
    let rust_content = std::fs::read_to_string(rust_file)
        .with_context(|| format!("Failed to read Rust file: {}", rust_file.display()))?;
    
    let c_lines: Vec<&str> = c_content.lines().collect();
    let rust_lines: Vec<&str> = rust_content.lines().collect();
    
    // 显示表头
    let c_total_width = LINE_NUM_WIDTH + 1 + c_column_width + 1;
    let rust_total_width = LINE_NUM_WIDTH + 1 + rust_column_width + 1;
    println!("┌{:─<width1$}┬{:─<width2$}┐", "─ C Source Code ", "─ Rust Code ─", width1 = c_total_width, width2 = rust_total_width);
    
    // 并排显示行
    let max_lines = std::cmp::max(c_lines.len(), rust_lines.len());
    for i in 0..max_lines {
        let c_line = c_lines.get(i).unwrap_or(&"");
        let rust_line = rust_lines.get(i).unwrap_or(&"");
        
        // 如果行太长而无法放入列中，则换行显示
        let c_wrapped = wrap_line(c_line, c_column_width);
        let rust_wrapped = wrap_line(rust_line, rust_column_width);
        
        let max_wrapped_lines = std::cmp::max(c_wrapped.len(), rust_wrapped.len());
        
        for j in 0..max_wrapped_lines {
            let c_display = c_wrapped.get(j).map(|s| s.as_str()).unwrap_or("");
            let rust_display = rust_wrapped.get(j).map(|s| s.as_str()).unwrap_or("");
            
            // 第一行显示行号，后续换行不显示行号
            let c_line_num = format_line_number(j, i + 1);
            let rust_line_num = format_line_number(j, i + 1);
            
            println!(
                "│ {} {:<c_width$}│ {} {:<r_width$}│",
                c_line_num,
                c_display,
                rust_line_num,
                rust_display,
                c_width = c_column_width,
                r_width = rust_column_width
            );
        }
    }
    
    let c_total_width = LINE_NUM_WIDTH + 1 + c_column_width + 1;
    let rust_total_width = LINE_NUM_WIDTH + 1 + rust_column_width + 1;
    println!("└{:─<width1$}┴{:─<width2$}┘", "", "", width1 = c_total_width, width2 = rust_total_width);
    
    // 显示结果部分
    display_result_section(result_message, result_type, total_width);
    
    Ok(())
}

/// 格式化行号显示
/// 
/// # Arguments
/// * `wrap_index` - 当前行的换行索引（0 表示第一行）
/// * `line_number` - 源代码的行号
/// 
/// # Returns
/// 格式化后的行号字符串，第一行显示行号，后续行显示空格
fn format_line_number(wrap_index: usize, line_number: usize) -> String {
    if wrap_index == 0 {
        format!("{:>width$}", line_number, width = LINE_NUM_WIDTH).dimmed().to_string()
    } else {
        CONTINUATION_MARKER.to_string()
    }
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
fn display_result_section(message: &str, result_type: ResultType, total_width: usize) {
    println!();
    println!("{}", "═".repeat(total_width).bright_cyan());
    
    let title = match result_type {
        ResultType::TestPass | ResultType::TestFail => "Test Result",
        ResultType::BuildSuccess | ResultType::BuildFail => "Build Result",
    };
    
    let color = match result_type {
        ResultType::TestPass | ResultType::BuildSuccess => title.bright_green().bold(),
        ResultType::TestFail | ResultType::BuildFail => title.bright_red().bold(),
    };
    
    // 居中显示标题
    let padding = (total_width.saturating_sub(title.len())) / 2;
    println!("{}{}{}", " ".repeat(padding), color, " ".repeat(total_width - padding - title.len()));
    println!("{}", "═".repeat(total_width).bright_cyan());
    
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
    
    #[test]
    fn test_get_adaptive_column_widths() {
        // 测试获取自适应列宽不会 panic
        let (c_width, rust_width) = get_adaptive_column_widths();
        
        // 列宽应该是合理的值
        assert!(c_width >= 40 || c_width == DEFAULT_C_COLUMN_WIDTH);
        assert!(rust_width >= 40 || rust_width == DEFAULT_RUST_COLUMN_WIDTH);
        
        // 两列宽度应该相同（平均分配）
        assert_eq!(c_width, rust_width);
    }
}

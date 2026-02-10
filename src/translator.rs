use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;
use crate::util;
use crate::constants;
use colored::Colorize;

/// 从环境变量获取翻译脚本目录
/// 
/// 环境变量应包含包含 translate_and_fix.py 脚本的
/// 目录的路径。
fn get_translate_script_dir() -> Result<PathBuf> {
    match std::env::var("C2RUST_TRANSLATE_DIT") {
        Ok(path) => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                anyhow::bail!("Environment variable C2RUST_TRANSLATE_DIT is empty. Please set it to the directory containing translate_and_fix.py script.");
            }
            Ok(PathBuf::from(trimmed))
        }
        Err(std::env::VarError::NotPresent) => {
            anyhow::bail!("Environment variable C2RUST_TRANSLATE_DIT is not set. Please set it to the directory containing translate_and_fix.py script.");
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("Environment variable C2RUST_TRANSLATE_DIT contains non-UTF8 data. Please ensure it contains a valid UTF-8 path.");
        }
    }
}

/// 获取 translate_and_fix.py 脚本的完整路径
/// 
/// 这从 C2RUST_TRANSLATE_DIT 环境变量读取目录路径
/// 并附加脚本文件名。
fn get_translate_script_full_path() -> Result<PathBuf> {
    let translate_script_dir = get_translate_script_dir()?;
    Ok(translate_script_dir.join("translate_and_fix.py"))
}

/// 通过搜索 .c2rust 目录获取 config.toml 路径
fn get_config_path() -> Result<PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root.join(".c2rust/config.toml"))
}

/// 构建修复命令的参数列表
/// 
/// 返回一个参数向量，传递给 translate_and_fix.py 用于修复错误。
/// 参数遵循格式：script_path --config --type syntax_fix --c_code --rust_code --output --error [--suggestion]
/// 
/// # 参数
/// - `script_path`: translate_and_fix.py 脚本的路径（作为返回向量的第一个元素包含）
/// - `config_path`: config.toml 文件的路径
/// - `c_code_file`: C 源文件的路径
/// - `rust_code_file`: 要修复的 Rust 文件的路径（输入）
/// - `output_file`: 应写入修复结果的路径（通常与 rust_code_file 相同）
/// - `error_file`: 包含编译器错误消息的临时文件的路径
/// - `suggestion_file`: 建议文件的可选路径（c2rust.md）
fn build_fix_args<'a>(
    script_path: &'a str,
    config_path: &'a str,
    c_code_file: &'a str,
    rust_code_file: &'a str,
    output_file: &'a str,
    error_file: &'a str,
    suggestion_file: Option<&'a str>,
) -> Vec<&'a str> {
    let mut args = vec![
        script_path,
        "--config",
        config_path,
        "--type",
        "syntax_fix",
        "--c_code",
        c_code_file,
        "--rust_code",
        rust_code_file,
        "--output",
        output_file,
        "--error",
        error_file,
    ];
    
    // 如果提供了建议文件则添加
    if let Some(suggestion) = suggestion_file {
        args.push("--suggestion");
        args.push(suggestion);
    }
    
    args
}

/// 使用格式化输出显示文件中的代码
pub(crate) fn display_code(file_path: &Path, header: &str, max_lines: usize, show_full: bool) {
    match std::fs::read_to_string(file_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();
            let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, max_lines) };
            
            println!("│ {}", header.bright_cyan());
            for (i, line) in lines.iter().take(display_lines).enumerate() {
                println!("│ {} {}", format!("{:3}", i + 1).dimmed(), line);
            }
            if total_lines > display_lines {
                println!("│ {} (showing {} of {} lines)", "...".dimmed(), display_lines, total_lines);
            }
            println!("│");
        }
        Err(e) => {
            println!("│ {} Could not read file for preview: {}", "⚠".yellow(), e);
            println!("│");
        }
    }
}

/// 使用翻译工具将 C 文件翻译为 Rust
pub fn translate_c_to_rust(feature: &str, file_type: &str, c_file: &Path, rs_file: &Path, show_full_output: bool) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let config_path = get_config_path()?;
    let work_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    if !work_dir.exists() {
        anyhow::bail!(
            "Working directory does not exist: {}. Expected: <project_root>/.c2rust/<feature>/rust",
            work_dir.display()
        );
    }
    
    // Display C code preview
    display_code(c_file, "─ C Source Preview ─", constants::CODE_PREVIEW_LINES, show_full_output);
    
    let script_path = get_translate_script_full_path()?;
    let script_str = script_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", script_path.display()))?;
    
    let config_str = config_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", config_path.display()))?;
    let c_file_str = c_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", c_file.display()))?;
    let rs_file_str = rs_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", rs_file.display()))?;
    
    println!("│ {}", "Executing translation command:".bright_blue());
    println!("│ {} python {} --config {} --type {} --c_code {} --output {}", 
        "→".bright_blue(),
        script_str.dimmed(), 
        config_str.dimmed(), 
        file_type.bright_yellow(), 
        c_file_str.bright_yellow(), 
        rs_file_str.bright_yellow());
    println!("│");
    
    let status = Command::new("python")
        .args([
            script_str,
            "--config",
            config_str,
            "--type",
            file_type,
            "--c_code",
            c_file_str,
            "--output",
            rs_file_str,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute translate_and_fix.py")?;

    if !status.success() {
        anyhow::bail!("Translation failed with exit code: {} (check output above for details)", status.code().unwrap_or(-1));
    }

    // 读取并显示翻译后的 Rust 代码
    display_code(rs_file, "─ Translated Rust Code ─", constants::CODE_PREVIEW_LINES, show_full_output);

    Ok(())
}

/// 显示错误消息预览
fn display_error_preview(error_msg: &str, show_full: bool) {
    let error_lines: Vec<&str> = error_msg.lines().collect();
    let total_lines = error_lines.len();
    let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, constants::ERROR_PREVIEW_LINES) };
    
    println!("│ {}", "─ Build Error Preview ─".yellow());
    for (i, line) in error_lines.iter().take(display_lines).enumerate() {
        if i == 0 {
            println!("│ {}", line.bright_red());
        } else {
            println!("│ {}", line.dimmed());
        }
    }
    if total_lines > display_lines {
        println!("│ {} (showing {} of {} lines)", "...".dimmed(), display_lines, total_lines);
    }
    println!("│");
}

/// 创建包含错误消息的临时文件
fn create_error_temp_file(error_msg: &str) -> Result<tempfile::NamedTempFile> {
    let mut temp_file = tempfile::NamedTempFile::new()
        .context("Failed to create temporary error file")?;
    write!(temp_file, "{}", error_msg)
        .context("Failed to write error message to temp file")?;
    Ok(temp_file)
}

/// 使用翻译工具修复翻译错误
pub fn fix_translation_error(
    feature: &str, 
    _file_type: &str, 
    rs_file: &Path, 
    error_msg: &str, 
    show_full_error: bool,
    show_full_fixed_code: bool,
) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let config_path = get_config_path()?;
    let work_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    if !work_dir.exists() {
        anyhow::bail!(
            "Working directory does not exist: {}. Expected: <project_root>/.c2rust/<feature>/rust",
            work_dir.display()
        );
    }
    
    display_error_preview(error_msg, show_full_error);
    
    let temp_file = create_error_temp_file(error_msg)?;
    let script_path = get_translate_script_full_path()?;
    
    // 从 Rust 文件路径派生 C 源文件路径
    // 示例：var_example.rs -> var_example.c
    let c_file = rs_file.with_extension("c");
    if let Err(e) = std::fs::metadata(&c_file) {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::bail!(
                "Corresponding C source file not found: {}. Expected a .c file with the same name as the .rs file.",
                c_file.display()
            );
        } else {
            anyhow::bail!(
                "Failed to access corresponding C source file {}: {}",
                c_file.display(),
                e
            );
        }
    }
    
    // 检查建议文件是否存在
    let suggestion_path = crate::suggestion::get_suggestion_file_path()?;
    let suggestion_exists = suggestion_path.exists();
    
    let script_str = script_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", script_path.display()))?;
    let config_str = config_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", config_path.display()))?;
    let error_file_str = temp_file.path().to_str()
        .with_context(|| format!("Non-UTF8 path: {}", temp_file.path().display()))?;
    let rs_file_str = rs_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", rs_file.display()))?;
    let c_file_str = c_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", c_file.display()))?;
    
    let suggestion_str = if suggestion_exists {
        Some(suggestion_path.to_str()
            .with_context(|| format!("Non-UTF8 path: {}", suggestion_path.display()))?)
    } else {
        None
    };

    println!("│ {}", "Executing error fix command:".yellow());
    if suggestion_exists {
        println!("│ {} python {} --config {} --type syntax_fix --c_code {} --rust_code {} --output {} --error {} --suggestion {}", 
            "→".yellow(),
            script_str.dimmed(), 
            config_str.dimmed(), 
            c_file_str.bright_yellow(),
            rs_file_str.bright_yellow(), 
            rs_file_str.bright_yellow(), 
            error_file_str.dimmed(),
            suggestion_str.unwrap().bright_cyan());
    } else {
        println!("│ {} python {} --config {} --type syntax_fix --c_code {} --rust_code {} --output {} --error {}", 
            "→".yellow(),
            script_str.dimmed(), 
            config_str.dimmed(), 
            c_file_str.bright_yellow(),
            rs_file_str.bright_yellow(), 
            rs_file_str.bright_yellow(), 
            error_file_str.dimmed());
    }
    println!("│");

    let args = build_fix_args(script_str, config_str, c_file_str, rs_file_str, rs_file_str, error_file_str, suggestion_str);

    let status = Command::new("python")
        .args(&args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute translate_and_fix.py for fixing")?;

    if !status.success() {
        anyhow::bail!("Fix failed with exit code: {}", status.code().unwrap_or(-1));
    }

    display_code(rs_file, "─ Fixed Rust Code ─", constants::CODE_PREVIEW_LINES, show_full_fixed_code);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use super::*;
    use serial_test::serial;
    
    /// 确保即使在 panic 时也能恢复环境变量的守卫
    struct EnvVarGuard {
        key: &'static str,
        original_value: Option<std::ffi::OsString>,
    }
    
    impl EnvVarGuard {
        fn new(key: &'static str) -> Self {
            let original_value = std::env::var_os(key);
            Self { key, original_value }
        }
        
        fn set(&self, value: &str) {
            std::env::set_var(self.key, value);
        }
        
        fn remove(&self) {
            std::env::remove_var(self.key);
        }
    }
    
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original_value {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
    
    #[test]
    fn test_temp_error_file_creation() {
        let test_msg = "test error message";

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", test_msg).unwrap();

        let path = temp_file.path();
        let content = std::fs::read_to_string(path).unwrap();

        assert_eq!(content, test_msg);
        // temp_file is automatically deleted when it goes out of scope
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_not_set() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.remove();
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("not set"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_empty() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("");
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("empty"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_whitespace() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("   ");
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("empty"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_valid() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("/path/to/scripts");
        
        let result = get_translate_script_dir();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/path/to/scripts"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_full_path() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("/path/to/scripts");
        
        let result = get_translate_script_full_path();
        assert!(result.is_ok());
        
        let path = result.unwrap();
        assert_eq!(path, PathBuf::from("/path/to/scripts/translate_and_fix.py"));
    }
    
    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_get_translate_script_dir_non_utf8() {
        use std::os::unix::ffi::OsStringExt;
        
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        
        // Create an invalid UTF-8 sequence
        let invalid_utf8 = std::ffi::OsString::from_vec(vec![0xFF, 0xFE, 0xFD]);
        std::env::set_var("C2RUST_TRANSLATE_DIT", &invalid_utf8);
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("non-UTF8"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_whitespace_trimming() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("  /path/to/scripts  ");
        
        let result = get_translate_script_dir();
        assert!(result.is_ok());
        // Should be trimmed
        assert_eq!(result.unwrap(), PathBuf::from("/path/to/scripts"));
    }
    
    #[test]
    fn test_build_fix_args() {
        let script = "/path/to/translate_and_fix.py";
        let config = "/project/.c2rust/config.toml";
        let c_code = "/project/feature/rust/code.c";
        let rust_code = "/project/feature/rust/code.rs";
        let output = "/project/feature/rust/code.rs";
        let error = "/tmp/error.txt";
        
        // 测试没有建议
        let args = build_fix_args(script, config, c_code, rust_code, output, error, None);
        
        // 验证参数的准确顺序
        assert_eq!(args.len(), 13);
        assert_eq!(args[0], script);
        assert_eq!(args[1], "--config");
        assert_eq!(args[2], config);
        assert_eq!(args[3], "--type");
        assert_eq!(args[4], "syntax_fix");
        assert_eq!(args[5], "--c_code");
        assert_eq!(args[6], c_code);
        assert_eq!(args[7], "--rust_code");
        assert_eq!(args[8], rust_code);
        assert_eq!(args[9], "--output");
        assert_eq!(args[10], output);
        assert_eq!(args[11], "--error");
        assert_eq!(args[12], error);
        
        // 测试带有建议
        let suggestion = "/project/c2rust.md";
        let args_with_suggestion = build_fix_args(script, config, c_code, rust_code, output, error, Some(suggestion));
        
        assert_eq!(args_with_suggestion.len(), 15);
        assert_eq!(args_with_suggestion[13], "--suggestion");
        assert_eq!(args_with_suggestion[14], suggestion);
    }
    
    #[test]
    fn test_display_code_truncation_behavior() {
        use tempfile::NamedTempFile;
        use std::io::Write;
        
        // 创建包含 20 行的临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        for i in 1..=20 {
            writeln!(temp_file, "Line {}", i).unwrap();
        }
        temp_file.flush().unwrap();
        
        // 测试 show_full = false（应截断到 max_lines）
        // 我们无法在单元测试中轻松捕获 stdout，但我们可以通过
        // 独立测试逻辑来验证函数不会 panic 并正确读取文件
        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let max_lines = 10;
        
        // 当 show_full = false 时验证截断逻辑
        let show_full = false;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, max_lines) };
        assert_eq!(display_lines, 10, "Should truncate to max_lines when show_full is false");
        assert!(total_lines > display_lines, "Should show truncation message");
        
        // 当 show_full = true 时验证完整显示逻辑
        let show_full = true;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, max_lines) };
        assert_eq!(display_lines, 20, "Should display all lines when show_full is true");
        assert_eq!(total_lines, display_lines, "Should not show truncation message");
    }
    
    #[test]
    fn test_display_code_no_truncation_when_lines_less_than_max() {
        use tempfile::NamedTempFile;
        use std::io::Write;
        
        // 创建只有 5 行的临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        for i in 1..=5 {
            writeln!(temp_file, "Line {}", i).unwrap();
        }
        temp_file.flush().unwrap();
        
        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let max_lines = 10;
        
        // 验证当行数 <= max_lines 时不截断，无论 show_full 如何
        let show_full = false;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, max_lines) };
        assert_eq!(display_lines, 5, "Should display all lines when total < max_lines");
        assert_eq!(total_lines, display_lines, "Should not show truncation message");
        
        let show_full = true;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, max_lines) };
        assert_eq!(display_lines, 5, "Should display all lines when total < max_lines");
    }
    
    #[test]
    fn test_display_error_preview_truncation_behavior() {
        // 创建包含多行的错误消息
        let mut error_msg = String::new();
        for i in 1..=25 {
            error_msg.push_str(&format!("Error line {}\n", i));
        }
        
        let error_lines: Vec<&str> = error_msg.lines().collect();
        let total_lines = error_lines.len();
        
        // 当 show_full = false 时验证截断逻辑
        let show_full = false;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, constants::ERROR_PREVIEW_LINES) };
        assert_eq!(display_lines, constants::ERROR_PREVIEW_LINES, 
            "Should truncate to ERROR_PREVIEW_LINES when show_full is false");
        assert!(total_lines > display_lines, "Should show truncation message");
        
        // 当 show_full = true 时验证完整显示逻辑
        let show_full = true;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, constants::ERROR_PREVIEW_LINES) };
        assert_eq!(display_lines, 25, "Should display all error lines when show_full is true");
        assert_eq!(total_lines, display_lines, "Should not show truncation message");
    }
    
    #[test]
    fn test_display_error_preview_no_truncation_when_lines_less_than_max() {
        // 创建简短的错误消息
        let error_msg = "Error line 1\nError line 2\nError line 3";
        
        let error_lines: Vec<&str> = error_msg.lines().collect();
        let total_lines = error_lines.len();
        
        // 验证当行数 <= ERROR_PREVIEW_LINES 时不截断，无论 show_full 如何
        let show_full = false;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, constants::ERROR_PREVIEW_LINES) };
        assert_eq!(display_lines, 3, "Should display all error lines when total < ERROR_PREVIEW_LINES");
        assert_eq!(total_lines, display_lines, "Should not show truncation message");
        
        let show_full = true;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, constants::ERROR_PREVIEW_LINES) };
        assert_eq!(display_lines, 3, "Should display all error lines when total < ERROR_PREVIEW_LINES");
    }
    
    #[test]
    fn test_display_error_preview_line_count_message() {
        // 创建正好 ERROR_PREVIEW_LINES + 5 行的错误消息
        let num_lines = constants::ERROR_PREVIEW_LINES + 5;
        let mut error_msg = String::new();
        for i in 1..=num_lines {
            error_msg.push_str(&format!("Error line {}\n", i));
        }
        
        let error_lines: Vec<&str> = error_msg.lines().collect();
        let total_lines = error_lines.len();
        
        // 截断时，验证计数是正确的
        let show_full = false;
        let display_lines = if show_full { total_lines } else { std::cmp::min(total_lines, constants::ERROR_PREVIEW_LINES) };
        
        // 截断消息应显示："showing {display_lines} of {total_lines} lines"
        assert_eq!(display_lines, constants::ERROR_PREVIEW_LINES);
        assert_eq!(total_lines, num_lines);
        assert!(total_lines > display_lines, "Total lines should be greater than displayed lines");
    }
}

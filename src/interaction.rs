//! 用于提示和收集输入的用户交互工具

use anyhow::{Context, Result};
use colored::Colorize;
use inquire::{Select, Text};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// 全局自动接受模式标志
static AUTO_ACCEPT_MODE: AtomicBool = AtomicBool::new(false);

/// 检查是否启用了自动接受模式
pub fn is_auto_accept_mode() -> bool {
    AUTO_ACCEPT_MODE.load(Ordering::Relaxed)
}

/// 启用自动接受模式
pub fn enable_auto_accept_mode() {
    AUTO_ACCEPT_MODE.store(true, Ordering::Relaxed);
    println!(
        "│ {}",
        "✓ Auto-accept mode enabled. All future translations will be automatically accepted."
            .bright_green()
            .bold()
    );
}

/// 禁用自动接受模式（仅用于测试）
#[cfg(test)]
pub fn disable_auto_accept_mode() {
    AUTO_ACCEPT_MODE.store(false, Ordering::Relaxed);
}

/// 编译成功且测试通过时的用户选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompileSuccessChoice {
    Accept,
    AutoAccept,
    ManualFix,
    Exit,
}

/// 编译或测试失败时的用户选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FailureChoice {
    RetryDirectly, // 直接重试不输入建议
    AddSuggestion, // 添加建议后重试
    ManualFix,     // 手动修复
    Skip,          // 忽略本次失败，跳过当前步骤继续流程
    RetryBuild,    // 重试构建（不重新打开编辑器）
    Exit,          // 退出
}

/// 跳过文件后的用户选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkippedFilesChoice {
    ProcessNow,   // 现在处理跳过的文件
    ExitForLater, // 退出并稍后处理
}

/// 发现已有翻译进度时的用户选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContinueChoice {
    Continue, // 继续之前的进度
    Restart,  // 开始新的翻译会话
}

/// 统一的失败场景提示函数
///
/// 在失败时提示并返回 ManualFix/Skip/Exit（上下文仅用于展示提示信息）
pub fn prompt_failure_choice(context: &str) -> Result<FailureChoice> {
    println!("│");
    println!(
        "│ {}",
        format!("⚠ {} - 您想怎么做？", context).yellow().bold()
    );
    println!("│");

    let options = vec![
        "手动修复（使用 VIM 编辑文件）",
        "跳过（忽略失败继续）",
        "退出（中止流程）",
    ];

    let choice = Select::new("请选择处理方式:", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(FailureChoice::ManualFix),
        1 => Ok(FailureChoice::Skip),
        2 => Ok(FailureChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 手动修复后仍构建失败时的提示函数
///
/// 提供明确的"重试构建"语义，区别于通用的"跳过"选项
pub fn prompt_after_manual_fix_choice() -> Result<FailureChoice> {
    println!("│");
    println!(
        "│ {}",
        "⚠ 手动修复后构建/测试仍然失败 - 您想怎么做？"
            .yellow()
            .bold()
    );
    println!("│");

    let options = vec![
        "重试构建（使用当前修改，不重新打开编辑器）",
        "重新手动修复（再次打开 VIM）",
        "退出（中止流程）",
    ];

    let choice = Select::new("请选择处理方式:", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(FailureChoice::RetryBuild),
        1 => Ok(FailureChoice::ManualFix),
        2 => Ok(FailureChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}


/// 如果 require_input 为 true，用户必须提供非空输入
pub fn prompt_suggestion(require_input: bool) -> Result<Option<String>> {
    loop {
        println!("│");
        println!(
            "│ {}",
            "Please enter your fix suggestion:".bright_cyan().bold()
        );
        println!(
            "│ {}",
            "(The suggestion will be saved and used in the next fix attempt)".dimmed()
        );

        if !require_input {
            println!(
                "│ {}",
                "(Press Enter to skip entering a suggestion)".dimmed()
            );
        }

        println!("│");

        // Use inquire::Text instead of io::stdin().read_line() to properly handle terminal escape sequences
        // including Delete key (\x1b[3~), Backspace, arrow keys, etc.
        let prompt_text = "│ Suggestion: ";
        let text_input = Text::new(prompt_text)
            .with_help_message("Use Delete/Backspace to edit, Enter to submit")
            .prompt();

        let suggestion = match text_input {
            Ok(s) => s,
            Err(inquire::InquireError::OperationCanceled) => {
                // User pressed Ctrl+C or ESC: treat as cancellation and let caller decide what to do
                println!("│ {}", "Suggestion input canceled by user.".yellow());
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };

        let trimmed = suggestion.trim().to_string();

        if trimmed.is_empty() {
            if require_input {
                println!("│ {}", "Error: A suggestion is required to continue.".red());
                // 再次循环以重新提示而不是递归
                continue;
            } else {
                println!("│ {}", "No suggestion provided.".yellow());
                return Ok(None);
            }
        }

        println!(
            "│ {}",
            format!("✓ Suggestion recorded: {}", trimmed).bright_green()
        );
        return Ok(Some(trimmed));
    }
}

/// 在 vim 中打开文件进行手动编辑
pub fn open_in_vim(file_path: &Path) -> Result<()> {
    println!("│");
    println!(
        "│ {}",
        format!("Opening {} in vim...", file_path.display()).bright_cyan()
    );

    let status = Command::new("vim")
        .arg(file_path)
        .status()
        .context("Failed to open vim")?;

    if status.success() {
        println!("│ {}", "✓ File editing complete".bright_green());
    } else {
        println!("│ {}", "⚠ vim exited with non-zero status".yellow());
    }

    Ok(())
}

/// 为手动修复打开一个或多个文件
///
/// 如果只有一个文件，直接在 vim 中打开（保持原有行为）。
/// 如果有多个文件，展示文件选择列表供用户选择，然后打开所选文件。
pub fn open_files_for_manual_fix(files: &[std::path::PathBuf]) -> Result<()> {
    match files.len() {
        0 => anyhow::bail!("No files provided for manual fix"),
        1 => open_in_vim(&files[0]),
        _ => {
            println!("│");
            println!(
                "│ {}",
                format!(
                    "⚠ Error involves {} files. Select a file to edit:",
                    files.len()
                )
                .bright_yellow()
                .bold()
            );
            let selected = prompt_file_selection_for_edit(files)?;
            open_in_vim(&selected)
        }
    }
}

/// 将选项字符串（形如 "1: /path/to/file"）映射回 PathBuf 列表。
///
/// 使用 1-based 索引，任何无法解析或越界的项都返回错误。
#[cfg(test)]
pub fn map_selections_to_files(
    selections: &[String],
    files: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>> {
    let mut result = Vec::with_capacity(selections.len());
    for s in selections {
        let idx = s
            .split_once(": ")
            .and_then(|(idx_str, _)| idx_str.parse::<usize>().ok())
            .filter(|&i| i >= 1 && i <= files.len())
            .ok_or_else(|| anyhow::anyhow!("无法解析文件选项: {:?}", s))?;
        result.push(files[idx - 1].clone());
    }
    Ok(result)
}

/// 从形如 "1: /path/to/file" 的选项字符串中解析出 1-based 文件索引，并校验范围。
fn parse_file_option(selection: &str, file_count: usize) -> Result<usize> {
    let (idx_str, _) = selection
        .split_once(": ")
        .ok_or_else(|| anyhow::anyhow!("无法解析文件选项，缺少分隔符: {:?}", selection))?;
    let i: usize = idx_str
        .parse()
        .with_context(|| format!("无法将 {:?} 解析为索引数字", idx_str))?;
    if i < 1 || i > file_count {
        anyhow::bail!("文件索引 {} 超出范围 (有效范围: 1-{})", i, file_count);
    }
    Ok(i)
}

/// 提示用户选择要编辑的文件（1-based 编号展示）
pub fn prompt_file_selection_for_edit(
    files: &[std::path::PathBuf],
) -> Result<std::path::PathBuf> {
    println!("│ {}", "选择要编辑的文件:".bright_cyan().bold());
    println!("│ {}", "  （使用上下键选择，回车确认）".bright_blue());

    let options: Vec<String> = files
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{}: {}", i + 1, f.display()))
        .collect();

    let selection = inquire::Select::new("文件:", options)
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get file selection")?;

    let idx = parse_file_option(&selection, files.len())?;

    Ok(files[idx - 1].clone())
}

/// 显示多个文件路径
pub fn display_file_paths(c_file: Option<&Path>, rust_file: &Path) {
    println!("│");
    println!("│ {}", "File Locations:".bright_cyan().bold());

    if let Some(c_path) = c_file {
        println!("│   {} {}", "C file:   ".bright_white(), c_path.display());
    }

    println!(
        "│   {} {}",
        "Rust file:".bright_white(),
        rust_file.display()
    );
    println!("│");
}

/// 编译成功且测试通过时提示用户
pub fn prompt_compile_success_choice() -> Result<CompileSuccessChoice> {
    println!("│");
    println!(
        "│ {}",
        "✓ Compilation and tests successful!".bright_green().bold()
    );
    println!("│");

    let options = vec![
        "Accept this code (will be committed)",
        "Auto-accept all subsequent translations",
        "Manual fix (edit the file with VIM)",
        "Exit (abort the translation process)",
    ];

    let choice = Select::new("What would you like to do?", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(CompileSuccessChoice::Accept),
        1 => Ok(CompileSuccessChoice::AutoAccept),
        2 => Ok(CompileSuccessChoice::ManualFix),
        3 => Ok(CompileSuccessChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 测试失败时提示用户
pub fn prompt_test_failure_choice() -> Result<FailureChoice> {
    println!("│");
    println!(
        "│ {}",
        "⚠ Tests failed - What would you like to do?"
            .yellow()
            .bold()
    );
    println!("│");

    let options = vec![
        "Retry directly (⚠ Will clear .rs file, re-translate from C, and clear suggestions)",
        "Add fix suggestion for AI to modify",
        "Manual fix (edit the file with VIM)",
        "Exit (abort the translation process)",
    ];

    let choice = Select::new("Select an option:", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(FailureChoice::RetryDirectly),
        1 => Ok(FailureChoice::AddSuggestion),
        2 => Ok(FailureChoice::ManualFix),
        3 => Ok(FailureChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 在达到最大重试次数后编译失败时提示用户
pub fn prompt_compile_failure_choice() -> Result<FailureChoice> {
    println!("│");
    println!(
        "│ {}",
        "⚠ Compilation failed - What would you like to do?"
            .red()
            .bold()
    );
    println!("│");

    let options = vec![
        "Retry directly (⚠ Will clear .rs file, re-translate from C, and clear suggestions)",
        "Add fix suggestion for AI to modify",
        "Manual fix (edit the file with VIM)",
        "Skip this file (process later)",
        "Exit (abort the translation process)",
    ];

    let choice = Select::new("Select an option:", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(FailureChoice::RetryDirectly),
        1 => Ok(FailureChoice::AddSuggestion),
        2 => Ok(FailureChoice::ManualFix),
        3 => Ok(FailureChoice::Skip),
        4 => Ok(FailureChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 构建失败时提示用户
pub fn prompt_build_failure_choice() -> Result<FailureChoice> {
    println!("│");
    println!(
        "│ {}",
        "⚠ Build failed - What would you like to do?".red().bold()
    );
    println!("│");

    let options = vec![
        "Retry directly (⚠ Will clear .rs file, re-translate from C, and clear suggestions)",
        "Add fix suggestion for AI to modify",
        "Manual fix (edit the file with VIM)",
        "Exit (abort the translation process)",
    ];

    let choice = Select::new("Select an option:", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(FailureChoice::RetryDirectly),
        1 => Ok(FailureChoice::AddSuggestion),
        2 => Ok(FailureChoice::ManualFix),
        3 => Ok(FailureChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 在所有文件处理完成后提示用户如何处理跳过的文件
pub fn prompt_skipped_files_choice(skipped_files: &[String]) -> Result<SkippedFilesChoice> {
    println!(
        "\n{}",
        "┌─────────────────────────────────────────────┐".bright_cyan()
    );
    println!("{}", "│ Translation complete!".bright_cyan());
    println!("{}", "│".bright_cyan());
    println!("{}", "│ Some files were skipped:".bright_yellow());
    for (idx, file_name) in skipped_files.iter().enumerate() {
        println!(
            "{}",
            format!("│   {}. {}", idx + 1, file_name).bright_yellow()
        );
    }
    println!("{}", "│".bright_cyan());
    println!("{}", "│ Would you like to:".bright_cyan());
    println!(
        "{}",
        "└─────────────────────────────────────────────┘".bright_cyan()
    );

    let options = vec!["Process skipped files now", "Exit and process them later"];

    let choice = Select::new("Select an option:", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    let choice_index = options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?;

    match choice_index {
        0 => Ok(SkippedFilesChoice::ProcessNow),
        1 => Ok(SkippedFilesChoice::ExitForLater),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 询问用户是否继续之前的翻译进度或开始新的会话
pub fn prompt_continue_or_restart() -> Result<ContinueChoice> {
    let options = vec![
        "Continue previous progress (resume from where you left off)",
        "Start fresh (clear all progress)",
    ];

    let choice = Select::new("What would you like to do?", options.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get user selection")?;

    match options
        .iter()
        .position(|&o| o == choice)
        .context("Unexpected selection value")?
    {
        0 => Ok(ContinueChoice::Continue),
        1 => Ok(ContinueChoice::Restart),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_compile_success_choice_variants() {
        assert_eq!(CompileSuccessChoice::Accept, CompileSuccessChoice::Accept);
        assert_eq!(
            CompileSuccessChoice::AutoAccept,
            CompileSuccessChoice::AutoAccept
        );
        assert_eq!(
            CompileSuccessChoice::ManualFix,
            CompileSuccessChoice::ManualFix
        );
        assert_eq!(CompileSuccessChoice::Exit, CompileSuccessChoice::Exit);
        assert_ne!(CompileSuccessChoice::Accept, CompileSuccessChoice::Exit);
    }

    #[test]
    fn test_failure_choice_variants() {
        assert_eq!(FailureChoice::RetryDirectly, FailureChoice::RetryDirectly);
        assert_eq!(FailureChoice::AddSuggestion, FailureChoice::AddSuggestion);
        assert_eq!(FailureChoice::ManualFix, FailureChoice::ManualFix);
        assert_eq!(FailureChoice::Skip, FailureChoice::Skip);
        assert_eq!(FailureChoice::RetryBuild, FailureChoice::RetryBuild);
        assert_eq!(FailureChoice::Exit, FailureChoice::Exit);
        assert_ne!(FailureChoice::RetryDirectly, FailureChoice::Exit);
        assert_ne!(FailureChoice::AddSuggestion, FailureChoice::Exit);
        assert_ne!(FailureChoice::Skip, FailureChoice::Exit);
        assert_ne!(FailureChoice::RetryBuild, FailureChoice::Skip);
    }

    #[test]
    fn test_continue_choice_variants() {
        assert_eq!(ContinueChoice::Continue, ContinueChoice::Continue);
        assert_eq!(ContinueChoice::Restart, ContinueChoice::Restart);
        assert_ne!(ContinueChoice::Continue, ContinueChoice::Restart);
    }

    #[test]
    #[serial]
    fn test_auto_accept_mode() {
        // 测试前确保状态干净
        disable_auto_accept_mode();

        // 初始应该是禁用的
        assert!(!is_auto_accept_mode());

        // 启用它
        enable_auto_accept_mode();
        assert!(is_auto_accept_mode());

        // 禁用它
        disable_auto_accept_mode();
        assert!(!is_auto_accept_mode());

        // 清理 - 确保下次测试时禁用
        disable_auto_accept_mode();
    }

    #[test]
    fn test_map_selections_to_files_normal() {
        let files = vec![
            std::path::PathBuf::from("/a/foo.rs"),
            std::path::PathBuf::from("/b/bar.rs"),
            std::path::PathBuf::from("/c/baz.rs"),
        ];
        let selections = vec!["1: /a/foo.rs".to_string(), "3: /c/baz.rs".to_string()];
        let result = map_selections_to_files(&selections, &files).unwrap();
        assert_eq!(result, vec![files[0].clone(), files[2].clone()]);
    }

    #[test]
    fn test_map_selections_to_files_duplicate_paths() {
        // 两个不同索引但路径相同的文件都应正确回填
        let files = vec![
            std::path::PathBuf::from("/dup.rs"),
            std::path::PathBuf::from("/dup.rs"),
        ];
        let selections = vec!["1: /dup.rs".to_string(), "2: /dup.rs".to_string()];
        let result = map_selections_to_files(&selections, &files).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], files[0]);
        assert_eq!(result[1], files[1]);
    }

    #[test]
    fn test_map_selections_to_files_invalid_string() {
        let files = vec![std::path::PathBuf::from("/a/foo.rs")];
        let selections = vec!["no_colon_here".to_string()];
        assert!(map_selections_to_files(&selections, &files).is_err());
    }

    #[test]
    fn test_map_selections_to_files_zero_index() {
        // 0 is out of valid 1-based range
        let files = vec![std::path::PathBuf::from("/a/foo.rs")];
        let selections = vec!["0: /a/foo.rs".to_string()];
        assert!(map_selections_to_files(&selections, &files).is_err());
    }

    #[test]
    fn test_map_selections_to_files_out_of_bounds() {
        let files = vec![std::path::PathBuf::from("/a/foo.rs")];
        let selections = vec!["5: /a/foo.rs".to_string()];
        assert!(map_selections_to_files(&selections, &files).is_err());
    }

    #[test]
    fn test_map_selections_to_files_empty_selections() {
        let files = vec![std::path::PathBuf::from("/a/foo.rs")];
        let selections: Vec<String> = vec![];
        let result = map_selections_to_files(&selections, &files).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_open_files_for_manual_fix_empty_returns_error() {
        let files: Vec<std::path::PathBuf> = vec![];
        assert!(open_files_for_manual_fix(&files).is_err());
    }

    #[test]
    fn test_parse_file_option_valid() {
        assert_eq!(parse_file_option("1: /a/foo.rs", 3).unwrap(), 1);
        assert_eq!(parse_file_option("2: /b/bar.rs", 3).unwrap(), 2);
        assert_eq!(parse_file_option("3: /c/baz.rs", 3).unwrap(), 3);
    }

    #[test]
    fn test_parse_file_option_no_separator() {
        assert!(parse_file_option("no_colon_here", 3).is_err());
    }

    #[test]
    fn test_parse_file_option_non_numeric_index() {
        assert!(parse_file_option("x: /a/foo.rs", 3).is_err());
    }

    #[test]
    fn test_parse_file_option_zero_index() {
        assert!(parse_file_option("0: /a/foo.rs", 3).is_err());
    }

    #[test]
    fn test_parse_file_option_out_of_bounds() {
        assert!(parse_file_option("5: /a/foo.rs", 3).is_err());
    }
}

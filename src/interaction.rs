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
    Skip,          // 跳过当前文件
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
/// 根据上下文提供合适的选项
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
        0 => Ok(FailureChoice::Skip), // Skip = "retry build without reopening editor"
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

/// 提示用户选择要编辑的文件
pub fn prompt_file_selection_for_edit(
    files: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>> {
    println!("│ {}", "选择要编辑的文件:".bright_cyan().bold());
    println!("│ {}", "  （使用空格选择，回车确认）".bright_blue());

    let file_names: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();

    let selections = inquire::MultiSelect::new("文件:", file_names.clone())
        .with_vim_mode(true)
        .prompt()
        .context("Failed to get file selection")?;

    let selected_files: Vec<std::path::PathBuf> = selections
        .iter()
        .filter_map(|s| {
            file_names
                .iter()
                .position(|f| f == s)
                .map(|i| files[i].clone())
        })
        .collect();

    if selected_files.is_empty() {
        anyhow::bail!("未选择任何文件");
    }

    Ok(selected_files)
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
        assert_eq!(FailureChoice::Exit, FailureChoice::Exit);
        assert_ne!(FailureChoice::RetryDirectly, FailureChoice::Exit);
        assert_ne!(FailureChoice::AddSuggestion, FailureChoice::Exit);
        assert_ne!(FailureChoice::Skip, FailureChoice::Exit);
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
}

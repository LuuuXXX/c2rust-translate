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

/// 处理失败时的用户选择
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserChoice {
    Continue,
    ManualFix,
    Exit,
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
    Exit,          // 退出
}

/// 当达到最大尝试次数时提示用户选择
pub fn prompt_user_choice(failure_type: &str, require_suggestion: bool) -> Result<UserChoice> {
    println!("│");
    println!(
        "│ {}",
        format!("⚠ {} - What would you like to do?", failure_type)
            .yellow()
            .bold()
    );
    println!("│");

    let continue_text = if require_suggestion {
        "Continue trying (requires entering a fix suggestion)"
    } else {
        "Continue trying (optionally enter a fix suggestion)"
    };

    let options = vec![
        continue_text,
        "Manual fix (edit the file directly)",
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
        0 => Ok(UserChoice::Continue),
        1 => Ok(UserChoice::ManualFix),
        2 => Ok(UserChoice::Exit),
        _ => unreachable!("Invalid selection index"),
    }
}

/// 提示用户输入修复建议
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_user_choice_variants() {
        assert_eq!(UserChoice::Continue, UserChoice::Continue);
        assert_eq!(UserChoice::ManualFix, UserChoice::ManualFix);
        assert_eq!(UserChoice::Exit, UserChoice::Exit);
        assert_ne!(UserChoice::Continue, UserChoice::Exit);
    }

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
        assert_eq!(FailureChoice::Exit, FailureChoice::Exit);
        assert_ne!(FailureChoice::RetryDirectly, FailureChoice::Exit);
        assert_ne!(FailureChoice::AddSuggestion, FailureChoice::Exit);
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

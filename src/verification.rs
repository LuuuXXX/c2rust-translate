use crate::{analyzer, builder, diff_display, interaction, suggestion, translator};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// Signal type returned when the user chooses to skip the current file.
///
/// This type is used as an `anyhow::Error` payload so that callers can
/// distinguish a deliberate skip from a genuine build failure.
#[derive(Debug)]
pub struct SkipFileSignal;

impl std::fmt::Display for SkipFileSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "File skipped by user")
    }
}

impl std::error::Error for SkipFileSignal {}

/// Display warning message about retry directly operation
pub fn display_retry_directly_warning() {
    println!("│");
    println!("│ {}", "⚠ Warning: This will:".bright_yellow().bold());
    println!(
        "│ {}",
        "  • Clear the current .rs file content".bright_yellow()
    );
    println!(
        "│ {}",
        "  • Re-translate from C source completely".bright_yellow()
    );
    println!("│ {}", "  • Clear all previous suggestions".bright_yellow());
    println!("│");
}

/// Group messages (errors or warnings) by file and apply a fix to each affected file.
///
/// This is the shared logic used by both `build_and_fix_loop` (errors) and
/// `build_and_fix_warnings_loop` (warnings). `is_warning` controls whether
/// `apply_warning_fix` (true) or `apply_error_fix` (false) is called for each fix.
///
/// Returns the number of fixes applied in this call.
fn apply_fixes_for_messages<F>(
    message: &str,
    fallback_error: &anyhow::Error,
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
    show_full_output: bool,
    is_warning: bool,
) -> Result<usize>
where
    F: Fn(&str) -> String,
{
    let mut count = 0usize;

    let file_messages = match crate::error_handler::group_errors_by_file(message, feature) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "│ {}",
                format!("⚠ Failed to group messages by file: {}", e).yellow()
            );
            vec![]
        }
    };

    if !file_messages.is_empty() {
        if file_messages.len() > 1 && !is_warning {
            println!(
                "│ {}",
                format!(
                    "Found errors in {} file(s), fixing each in order...",
                    file_messages.len()
                )
                .bright_yellow()
            );
        }
        for (msg_file, file_msg) in &file_messages {
            let Some(file_stem) = msg_file.file_stem().and_then(|s| s.to_str()) else {
                println!(
                    "│ {}",
                    format!("⚠ Skipping file with invalid name: {}", msg_file.display()).yellow()
                );
                continue;
            };
            let (msg_file_type, _) =
                crate::file_scanner::extract_file_type(file_stem).unwrap_or((file_type, ""));
            let msg_error = anyhow::anyhow!("{}", file_msg);
            let msg_file_name = msg_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file_stem);
            let msg_format_progress = |op: &str| format!("Fixing {} - {}", msg_file_name, op);
            if is_warning {
                crate::apply_warning_fix(
                    feature,
                    msg_file_type,
                    msg_file,
                    &msg_error,
                    &msg_format_progress,
                    show_full_output,
                )?;
            } else {
                crate::apply_error_fix(
                    feature,
                    msg_file_type,
                    msg_file,
                    &msg_error,
                    &msg_format_progress,
                    show_full_output,
                )?;
            }
            count += 1;
        }
    } else {
        // Fall back to single-file fix
        if is_warning {
            crate::apply_warning_fix(
                feature,
                file_type,
                rs_file,
                fallback_error,
                format_progress,
                show_full_output,
            )?;
        } else {
            crate::apply_error_fix(
                feature,
                file_type,
                rs_file,
                fallback_error,
                format_progress,
                show_full_output,
            )?;
        }
        count += 1;
    }

    Ok(count)
}

/// 在循环中构建并修复错误
///
/// 返回 Ok((build_successful, fix_attempts, had_restart))：
/// - build_successful: true 如果构建成功
/// - fix_attempts: 本次循环中应用的修复次数
/// - had_restart: true 如果用户选择了 RetryDirectly
pub fn build_and_fix_loop<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    file_name: &str,
    format_progress: &F,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<(bool, usize, bool)>
where
    F: Fn(&str) -> String,
{
    let mut fix_attempts = 0usize;
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());
    for attempt in 1..=max_fix_attempts {
        println!("│");
        println!("│ {}", format_progress("Build").bright_magenta().bold());
        println!(
            "│ {}",
            format!(
                "Building Rust project (attempt {}/{})",
                attempt, max_fix_attempts
            )
            .bright_blue()
            .bold()
        );

        match builder::cargo_build(feature, show_full_output) {
            Ok(_) => {
                println!("│ {}", "✓ Build successful!".bright_green().bold());
                return Ok((true, fix_attempts, false));
            }
            Err(build_error) => {
                if attempt == max_fix_attempts {
                    let (build_successful, extra_fix_attempts, had_restart) =
                        handle_max_fix_attempts_reached(
                            build_error,
                            file_name,
                            rs_file,
                            is_last_attempt,
                            attempt_number,
                            max_fix_attempts,
                            feature,
                            file_type,
                            show_full_output,
                        )?;
                    return Ok((
                        build_successful,
                        fix_attempts + extra_fix_attempts,
                        had_restart,
                    ));
                } else {
                    // Apply fixes using the shared helper (error phase, is_warning=false)
                    fix_attempts += apply_fixes_for_messages(
                        &build_error.to_string(),
                        &build_error,
                        feature,
                        file_type,
                        rs_file,
                        format_progress,
                        show_full_output,
                        false,
                    )?;
                }
            }
        }

        println!("│ {}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("│ {}", "✓ Code analysis updated".bright_green());
    }

    Ok((false, fix_attempts, false))
}

/// 在循环中构建并修复警告（第二阶段）
///
/// 在所有错误都已修复后运行（build_and_fix_loop 成功后），
/// 此函数运行不带 -A warnings 的构建并修复剩余的警告。
///
/// 此函数为非致命性的：
/// - 如果修复超过 max_fix_attempts 次仍有剩余警告，继续并返回已应用的修复次数
/// - 如果警告阶段出现意外构建错误，记录日志后继续（不中断文件处理）
///
/// 返回 Ok(fix_attempts)：警告修复阶段中应用的修复次数
pub fn build_and_fix_warnings_loop<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    _file_name: &str,
    format_progress: &F,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<usize>
where
    F: Fn(&str) -> String,
{
    let mut fix_attempts = 0usize;
    for attempt in 1..=max_fix_attempts {
        println!("│");
        println!(
            "│ {}",
            format_progress("Warning Check").bright_magenta().bold()
        );
        println!(
            "│ {}",
            format!(
                "Checking for warnings (attempt {}/{})",
                attempt, max_fix_attempts
            )
            .bright_blue()
            .bold()
        );

        match builder::cargo_build_check_warnings(feature, show_full_output) {
            Ok(None) => {
                println!("│ {}", "✓ No warnings found!".bright_green().bold());
                return Ok(fix_attempts);
            }
            Ok(Some(warnings)) => {
                let warning_error = anyhow::anyhow!("{}", warnings);
                fix_attempts += apply_fixes_for_messages(
                    &warnings,
                    &warning_error,
                    feature,
                    file_type,
                    rs_file,
                    format_progress,
                    show_full_output,
                    true,
                )?;
            }
            Err(e) => {
                // Build failed during warning phase -- unexpected since errors were already fixed.
                // Treat as non-fatal: log and stop the warning loop but do not abort file processing.
                println!(
                    "│ {}",
                    format!("✗ Unexpected build error during warning phase: {}", e).red()
                );
                return Ok(fix_attempts);
            }
        }

        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
    }

    println!(
        "│ {}",
        "⚠ Maximum warning fix attempts reached, continuing with remaining warnings.".yellow()
    );
    Ok(fix_attempts)
}

/// 处理达到最大修复尝试次数的情况
///
/// 返回 (build_successful, extra_fix_attempts, had_restart)：
/// - Ok((true, _, _)) 如果处理应继续而不重试翻译
/// - Ok((false, _, had_restart)) 如果应重试翻译
fn handle_max_fix_attempts_reached(
    build_error: anyhow::Error,
    file_name: &str,
    rs_file: &Path,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    feature: &str,
    file_type: &str,
    show_full_output: bool,
) -> Result<(bool, usize, bool)> {
    println!("│");
    println!("│ {}", "⚠ Maximum fix attempts reached!".red().bold());
    println!(
        "│ {}",
        format!(
            "File {} still has build errors after {} fix attempts.",
            file_name, max_fix_attempts
        )
        .yellow()
    );

    // 显示代码比较和构建错误
    let c_file = rs_file.with_extension("c");

    // 显示文件位置
    interaction::display_file_paths(Some(&c_file), rs_file);

    // 使用差异显示进行更好的比较
    let error_message = format!("✗ Build Error:\n{}", build_error);
    if let Err(e) = diff_display::display_code_comparison(
        &c_file,
        rs_file,
        &error_message,
        diff_display::ResultType::BuildFail,
    ) {
        // 如果比较失败则回退到旧显示
        println!(
            "│ {}",
            format!("Failed to display comparison: {}", e).yellow()
        );
        println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
        translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);

        println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
        translator::display_code(rs_file, "─ Rust Code ─", usize::MAX, true);

        println!("│ {}", "═══ Build Error ═══".bright_red().bold());
        println!("│ {}", build_error);
    }

    // 使用新提示获取用户选择
    let choice = interaction::prompt_compile_failure_choice()?;

    match choice {
        interaction::FailureChoice::RetryDirectly => {
            handle_retry_directly(attempt_number, is_last_attempt)
        }
        interaction::FailureChoice::AddSuggestion => handle_add_suggestion(
            feature,
            file_type,
            rs_file,
            &build_error,
            is_last_attempt,
            attempt_number,
            file_name,
            max_fix_attempts,
            show_full_output,
        ),
        interaction::FailureChoice::ManualFix => handle_manual_fix(feature, file_type, rs_file),
        interaction::FailureChoice::Skip => {
            println!("│ {}", "You chose: Skip this file".bright_cyan());
            println!(
                "│ {}",
                "File will be skipped and can be processed later.".yellow()
            );
            Err(anyhow::Error::from(SkipFileSignal))
        }
        interaction::FailureChoice::Exit => Err(build_error).context(format!(
            "Build failed after {} fix attempts for file {}",
            max_fix_attempts, file_name
        )),
    }
}

/// 处理直接重试选项
fn handle_retry_directly(
    attempt_number: usize,
    is_last_attempt: bool,
) -> Result<(bool, usize, bool)> {
    use crate::util::MAX_TRANSLATION_ATTEMPTS;

    println!("│");
    println!(
        "│ {}",
        "You chose: Retry directly without suggestion".bright_cyan()
    );

    display_retry_directly_warning();

    // 清除旧建议
    suggestion::clear_suggestions()?;

    // 当这是最后一次翻译机会时，RetryDirectly 无法再次重新翻译，返回明确错误
    if is_last_attempt {
        println!(
            "│ {}",
            "✗ Cannot retry directly: this is the last translation attempt.".bright_red()
        );
        println!(
            "│ {}",
            "No more translation retries are available.".yellow()
        );
        anyhow::bail!("RetryDirectly selected on last translation attempt — no retries remaining");
    }

    // 重新翻译（清空并重新生成 rs 文件）
    let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
    println!(
        "│ {}",
        format!(
            "Retrying translation from scratch... ({} retries remaining)",
            remaining_retries
        )
        .bright_cyan()
    );
    println!(
        "│ {}",
        "Note: The translator will overwrite the existing file content.".bright_blue()
    );
    println!("│ {}", "✓ Retry scheduled".bright_green());
    Ok((false, 0, true)) // 发出重试信号，且使用了重来功能
}

/// 处理添加建议选项
fn handle_add_suggestion(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    _build_error: &anyhow::Error,
    is_last_attempt: bool,
    attempt_number: usize,
    file_name: &str,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<(bool, usize, bool)> {
    use crate::util::MAX_TRANSLATION_ATTEMPTS;

    println!("│");
    println!(
        "│ {}",
        "You chose: Add fix suggestion for AI to modify".bright_cyan()
    );

    // 在提示新建议之前清除旧建议
    suggestion::clear_suggestions()?;

    // 从用户获取必需的建议
    let suggestion_text = interaction::prompt_suggestion(true)?
        .ok_or_else(|| anyhow::anyhow!(
            "Suggestion is required for compilation failure but none was provided. \
             This may indicate an issue with the prompt_suggestion function when require_input=true."
        ))?;

    // 将建议保存到 suggestions.txt
    suggestion::append_suggestion(&suggestion_text)?;

    // 如果我们仍然可以重试翻译，则执行
    if !is_last_attempt {
        let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
        println!(
            "│ {}",
            format!(
                "Retrying translation from scratch... ({} retries remaining)",
                remaining_retries
            )
            .bright_cyan()
        );
        println!(
            "│ {}",
            "Note: The translator will overwrite the existing file content.".bright_blue()
        );
        println!("│ {}", "✓ Retry scheduled".bright_green());
        Ok((false, 0, false)) // 发出重试信号，未使用重来功能
    } else {
        // 没有更多翻译重试，但用户输入了新建议
        // 不清空 .rs 文件，而是用新建议重新开始完整的修复循环
        println!("│");
        println!("│ {}", "No translation retries remaining.".bright_yellow());
        println!(
            "│ {}",
            "Starting new fix-and-verify cycle with your suggestion...".bright_cyan()
        );
        println!(
            "│ {}",
            format!("(You will have {} fix attempts)", max_fix_attempts).bright_blue()
        );
        println!("│");

        // 调用 build_and_fix_loop 重新开始完整的修复循环
        // 注意：这里传入 is_last_attempt=true 表示这是最后一次翻译机会
        // 但修复循环本身会有完整的 max_fix_attempts 次机会
        // 第二个返回值是递归循环中消耗的 fix_attempts 次数，由调用方 process_rs_file 聚合统计。
        let (build_successful, recursive_fix_attempts, had_restart) =
            crate::verification::build_and_fix_loop(
                feature,
                file_type,
                rs_file,
                file_name,
                &|op: &str| format!("Suggestion-based fix - {}", op),
                true, // is_last_attempt: 翻译层面确实是最后一次了
                attempt_number,
                max_fix_attempts,
                show_full_output,
            )?;

        Ok((build_successful, recursive_fix_attempts, had_restart))
    }
}

/// 处理手动修复选项
fn handle_manual_fix(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
) -> Result<(bool, usize, bool)> {
    println!("│");
    println!("│ {}", "You chose: Manual fix".bright_cyan());

    // 尝试打开 vim
    match interaction::open_in_vim(rs_file) {
        Ok(_) => {
            // Vim 编辑后，重复尝试构建并允许用户决定是重试还是退出
            loop {
                println!("│");
                println!(
                    "│ {}",
                    "Vim editing completed. Running full build and test...".bright_blue()
                );

                // 手动编辑后执行完整构建流程
                match builder::run_full_build_and_test_interactive(feature, file_type, rs_file) {
                    Ok(_) => {
                        println!(
                            "│ {}",
                            "✓ All builds and tests passed after manual fix!"
                                .bright_green()
                                .bold()
                        );
                        return Ok((true, 0, false));
                    }
                    Err(e) => {
                        println!(
                            "│ {}",
                            "✗ Build or tests still failing after manual fix".red()
                        );

                        // 询问用户是否想再试一次
                        println!("│");
                        println!(
                            "│ {}",
                            "Build or tests still have errors. What would you like to do?".yellow()
                        );
                        let retry_choice =
                            interaction::prompt_user_choice("Build/tests still failing", false)?;

                        match retry_choice {
                            interaction::UserChoice::Continue => {
                                // 用户选择继续尝试，不再强制重新打开 Vim，直接在下一轮循环中重试构建和测试
                                println!(
                                    "│ {}",
                                    "Retrying build and tests without reopening the editor..."
                                        .bright_cyan()
                                );
                            }
                            interaction::UserChoice::ManualFix => {
                                println!(
                                    "│ {}",
                                    "Opening Vim again for another manual fix attempt..."
                                        .bright_cyan()
                                );
                                interaction::open_in_vim(rs_file)?;
                            }
                            interaction::UserChoice::Exit => {
                                return Err(e).context(format!(
                                    "Build or tests failed after manual fix for file {}",
                                    rs_file.display()
                                ));
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("│ {}", format!("Failed to open Vim: {}", e).red());
            Err(e).context(format!(
                "Failed to open file {} in Vim for manual editing",
                rs_file.display()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that apply_fixes_for_messages returns Err when the target Rust file
    /// does not exist within an otherwise valid project root and feature tree.
    #[test]
    #[serial_test::serial]
    fn test_apply_fixes_for_messages_nonexistent_file_returns_err() {
        use std::env;
        use tempfile::TempDir;

        // Set up a temporary project root with a valid .c2rust/<feature>/rust/src tree.
        let tmp = TempDir::new().unwrap();
        let orig_dir = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();

        let feature = "test_feature";
        let feature_src_dir = tmp
            .path()
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src");
        std::fs::create_dir_all(&feature_src_dir).unwrap();

        // Create the companion .c file so fix_translation_error passes its C-file
        // existence check, ensuring the error originates from the missing .rs file.
        std::fs::write(feature_src_dir.join("nonexistent.c"), "").unwrap();

        // Point rs_file at a path under the feature src dir that does NOT exist.
        let rs_file = feature_src_dir.join("nonexistent.rs");

        let result = apply_fixes_for_messages(
            "warning: unused\n  --> src/foo.rs:1:1",
            &anyhow::anyhow!("dummy"),
            feature,
            "var",
            &rs_file,
            &|op: &str| op.to_string(),
            false,
            true,
        );

        env::set_current_dir(orig_dir).unwrap();

        // With a valid project root and feature tree the error comes from the
        // missing target file, not from a missing project root.
        assert!(result.is_err());
    }

    /// Test that build_and_fix_warnings_loop returns Ok(0) when the build fails
    /// during the warning phase (e.g. the feature build directory does not exist).
    /// The loop should treat this as non-fatal and return Ok(0).
    #[test]
    #[serial_test::serial]
    fn test_build_and_fix_warnings_loop_build_failure_is_nonfatal() {
        use std::env;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let orig = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();

        // Create minimal .c2rust dir so find_project_root works, but do NOT create
        // the feature build directory so cargo_build_check_warnings will fail.
        std::fs::create_dir_all(tmp.path().join(".c2rust")).unwrap();

        // Use a path inside tmp so it is portable and clearly non-existent.
        let rs_file = tmp.path().join("var_foo.rs");

        let result = build_and_fix_warnings_loop(
            "nonexistent_feature",
            "var",
            &rs_file,
            "var_foo.rs",
            &|op: &str| op.to_string(),
            1, // max_fix_attempts
            false,
        );

        env::set_current_dir(orig).unwrap();

        // Build error during warning phase should be non-fatal → Ok(0)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}

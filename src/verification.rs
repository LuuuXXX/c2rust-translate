use crate::{builder, diff_display, interaction, suggestion, translator};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// Signal type returned when a file is skipped, either by the user interactively
/// or automatically (e.g., when `C2RUST_AUTO_RETRY_ON_MAX_FIX` is set and the
/// last translation attempt is reached).
///
/// This type is used as an `anyhow::Error` payload so that callers can
/// distinguish a deliberate or automatic skip from a genuine build failure.
#[derive(Debug)]
pub struct SkipFileSignal;

impl std::fmt::Display for SkipFileSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "File skipped")
    }
}

impl std::error::Error for SkipFileSignal {}

/// Signal type returned when a translation step fails (e.g. the Python translate
/// script exits non-zero), but the overall workflow should continue with the next
/// file rather than aborting.
///
/// Unlike [`SkipFileSignal`] (which represents a deliberate, user-chosen or
/// auto-triggered skip), this signal indicates a real failure.  Callers record
/// the file in [`crate::util::TranslationStats::translation_failed_files`] rather
/// than in `skipped_files`, so translation failures are reported separately from
/// intentional skips in the final statistics summary and are not re-offered to the
/// user in the skipped-files retry loop.
#[derive(Debug)]
pub struct TranslationFailedSignal;

impl std::fmt::Display for TranslationFailedSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Translation failed")
    }
}

impl std::error::Error for TranslationFailedSignal {}

/// Outcome of the automatic retry decision when `C2RUST_AUTO_RETRY_ON_MAX_FIX` is set.
#[derive(Debug, PartialEq)]
enum AutoRetryOutcome {
    /// Automatically retry translation from scratch (retries remain).
    Retry,
    /// Automatically skip the file (last translation attempt reached).
    Skip,
}

/// Determine the automatic retry outcome when `C2RUST_AUTO_RETRY_ON_MAX_FIX` is set.
///
/// Returns `Some(AutoRetryOutcome)` when the env var is enabled (truthy: `1`, `true`,
/// or `yes`, case-insensitive), or `None` when it is not enabled — including when the
/// var is absent, empty, or set to a non-truthy value — falling through to the
/// interactive prompt.
fn resolve_auto_retry_outcome(is_last_attempt: bool) -> Option<AutoRetryOutcome> {
    if !crate::config::should_auto_retry_on_max_fix_attempts() {
        return None;
    }
    if is_last_attempt {
        Some(AutoRetryOutcome::Skip)
    } else {
        Some(AutoRetryOutcome::Retry)
    }
}

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
                match crate::code_rewrite::apply_warning_fix(
                    feature,
                    msg_file_type,
                    msg_file,
                    &msg_error,
                    &msg_format_progress,
                    show_full_output,
                ) {
                    Ok(()) => count += 1,
                    Err(e) => {
                        println!(
                            "│ {}",
                            format!("⚠ Warning fix failed, continuing: {:#}", e).yellow()
                        );
                    }
                }
            } else {
                match crate::code_rewrite::apply_error_fix(
                    feature,
                    msg_file_type,
                    msg_file,
                    &msg_error,
                    &msg_format_progress,
                    show_full_output,
                ) {
                    Ok(()) => count += 1,
                    Err(e) => {
                        println!(
                            "│ {}",
                            format!("⚠ Error fix failed, continuing: {:#}", e).yellow()
                        );
                    }
                }
            }
        }
    } else {
        // Fall back to single-file fix
        if is_warning {
            match crate::code_rewrite::apply_warning_fix(
                feature,
                file_type,
                rs_file,
                fallback_error,
                format_progress,
                show_full_output,
            ) {
                Ok(()) => count += 1,
                Err(e) => {
                    println!(
                        "│ {}",
                        format!("⚠ Warning fix failed, continuing: {:#}", e).yellow()
                    );
                }
            }
        } else {
            match crate::code_rewrite::apply_error_fix(
                feature,
                file_type,
                rs_file,
                fallback_error,
                format_progress,
                show_full_output,
            ) {
                Ok(()) => count += 1,
                Err(e) => {
                    println!(
                        "│ {}",
                        format!("⚠ Error fix failed, continuing: {:#}", e).yellow()
                    );
                }
            }
        }
    }

    Ok(count)
}

/// 在循环中构建并修复错误
///
/// 返回 Ok((build_successful, fix_attempts, had_restart))：
/// - build_successful: true 如果构建成功
/// - fix_attempts: 本次循环中应用的修复次数
/// - had_restart: true 如果用户选择了 RetryDirectly
pub fn execute_code_error_check_with_fix_loop<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    file_name: &str,
    format_progress: &F,
    is_last_attempt: bool,
    attempt_number: usize,
    max_error_fix_attempts: usize,
    show_full_output: bool,
    skip_test: bool,
) -> Result<(bool, usize, bool)>
where
    F: Fn(&str) -> String,
{
    let mut fix_attempts = 0usize;
    println!("│ {}", "Updating code analysis...".bright_blue());
    builder::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());
    for attempt in 1..=max_error_fix_attempts {
        println!("│");
        println!("│ {}", format_progress("Check").bright_magenta().bold());
        println!(
            "│ {}",
            format!(
                "Checking Rust project (attempt {}/{})",
                attempt, max_error_fix_attempts
            )
            .bright_blue()
            .bold()
        );

        match builder::cargo_check(feature, true, show_full_output) {
            Ok(_) => {
                println!("│ {}", "✓ Check successful!".bright_green().bold());
                return Ok((true, fix_attempts, false));
            }
            Err(build_error) => {
                if attempt == max_error_fix_attempts {
                    let (build_successful, extra_fix_attempts, had_restart) =
                        handle_max_fix_attempts_reached(
                            build_error,
                            file_name,
                            rs_file,
                            is_last_attempt,
                            attempt_number,
                            max_error_fix_attempts,
                            feature,
                            file_type,
                            show_full_output,
                            skip_test,
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
        builder::update_code_analysis(feature)?;
        println!("│ {}", "✓ Code analysis updated".bright_green());
    }

    Ok((false, fix_attempts, false))
}

/// 在循环中检查并修复警告（第二阶段）
///
/// 在所有错误都已修复后运行（execute_code_error_check_with_fix_loop 成功后），
/// 此函数运行不带 -A warnings 的 cargo check 并修复剩余的警告。
///
/// 此函数为非致命性的：
/// - 如果修复超过 max_warning_fix_attempts 次仍有剩余警告，继续并返回已应用的修复次数
/// - 如果警告阶段出现意外检查错误，记录日志后继续（不中断文件处理）
///
/// 返回 Ok(fix_attempts)：警告修复阶段中应用的修复次数
pub fn execute_code_warning_check_with_fix_loop<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    _file_name: &str,
    format_progress: &F,
    max_warning_fix_attempts: usize,
    show_full_output: bool,
) -> Result<usize>
where
    F: Fn(&str) -> String,
{
    let mut fix_attempts = 0usize;
    for attempt in 1..=max_warning_fix_attempts {
        println!("│");
        println!(
            "│ {}",
            format_progress("Warning Check").bright_magenta().bold()
        );
        println!(
            "│ {}",
            format!(
                "Checking for warnings (attempt {}/{})",
                attempt, max_warning_fix_attempts
            )
            .bright_blue()
            .bold()
        );

        match builder::cargo_check(feature, false, show_full_output) {
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
                // Check failed during warning phase -- unexpected since errors were already fixed.
                // Treat as non-fatal: log and stop the warning loop but do not abort file processing.
                println!(
                    "│ {}",
                    format!("✗ Unexpected check error during warning phase: {}", e).red()
                );
                return Ok(fix_attempts);
            }
        }

        println!("│ {}", "Updating code analysis...".bright_blue());
        builder::update_code_analysis(feature)?;
        println!("│ {}", "✓ Code analysis updated".bright_green());
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
    max_error_fix_attempts: usize,
    feature: &str,
    file_type: &str,
    show_full_output: bool,
    skip_test: bool,
) -> Result<(bool, usize, bool)> {
    println!("│");
    println!("│ {}", "⚠ Maximum error-fix attempts reached!".red().bold());
    println!(
        "│ {}",
        format!(
            "File {} still has build errors after {} error-fix attempts.",
            file_name, max_error_fix_attempts
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

    // 当设置了 C2RUST_AUTO_RETRY_ON_MAX_FIX 时，根据是否还有重试机会，
    // 自动选择重新翻译（retries remaining）或跳过文件（last attempt），无需人工干预
    if let Some(outcome) = resolve_auto_retry_outcome(is_last_attempt) {
        match outcome {
            AutoRetryOutcome::Skip => {
                println!(
                    "│ {}",
                    "Auto-retry enabled (C2RUST_AUTO_RETRY_ON_MAX_FIX): last translation attempt reached, skipping file."
                        .bright_yellow()
                );
                return Err(anyhow::Error::from(SkipFileSignal));
            }
            AutoRetryOutcome::Retry => {
                println!(
                    "│ {}",
                    "Auto-retry enabled (C2RUST_AUTO_RETRY_ON_MAX_FIX): retrying translation automatically."
                        .bright_cyan()
                );
                return handle_retry_directly(attempt_number, is_last_attempt, true);
            }
        }
    }

    // 使用新提示获取用户选择
    let choice = interaction::prompt_compile_failure_choice()?;

    match choice {
        interaction::FailureChoice::RetryDirectly => {
            handle_retry_directly(attempt_number, is_last_attempt, false)
        }
        interaction::FailureChoice::AddSuggestion => handle_add_suggestion(
            feature,
            file_type,
            rs_file,
            &build_error,
            is_last_attempt,
            attempt_number,
            file_name,
            max_error_fix_attempts,
            show_full_output,
            skip_test,
        ),
        interaction::FailureChoice::ManualFix => {
            handle_manual_fix(feature, file_type, rs_file, &build_error, skip_test)
        }
        interaction::FailureChoice::Skip => {
            println!("│ {}", "You chose: Skip this file".bright_cyan());
            println!(
                "│ {}",
                "File will be skipped and can be processed later.".yellow()
            );
            Err(anyhow::Error::from(SkipFileSignal))
        }
        interaction::FailureChoice::Exit => Err(build_error).context(format!(
            "Build failed after {} error-fix attempts for file {}",
            max_error_fix_attempts, file_name
        )),
        interaction::FailureChoice::FixOtherFile => {
            unreachable!("FixOtherFile is not offered in this context")
        }
    }
}

/// 处理直接重试选项
fn handle_retry_directly(
    attempt_number: usize,
    is_last_attempt: bool,
    auto_triggered: bool,
) -> Result<(bool, usize, bool)> {
    use crate::util::MAX_TRANSLATION_ATTEMPTS;

    println!("│");
    if auto_triggered {
        println!(
            "│ {}",
            "Auto-selected: Retry directly (C2RUST_AUTO_RETRY_ON_MAX_FIX)".bright_cyan()
        );
    } else {
        println!(
            "│ {}",
            "You chose: Retry directly without suggestion".bright_cyan()
        );
    }

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
    max_error_fix_attempts: usize,
    show_full_output: bool,
    skip_test: bool,
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
            format!("(You will have {} error-fix attempts)", max_error_fix_attempts).bright_blue()
        );
        println!("│");

        // 调用 execute_code_error_check_with_fix_loop 重新开始完整的修复循环
        // 注意：这里传入 is_last_attempt=true 表示这是最后一次翻译机会
        // 但修复循环本身会有完整的 max_error_fix_attempts 次机会
        // 第二个返回值是递归循环中消耗的 fix_attempts 次数，由调用方 process_rs_file 聚合统计。
        let (build_successful, recursive_fix_attempts, had_restart) =
            crate::verification::execute_code_error_check_with_fix_loop(
                feature,
                file_type,
                rs_file,
                file_name,
                &|op: &str| format!("Suggestion-based fix - {}", op),
                true, // is_last_attempt: 翻译层面确实是最后一次了
                attempt_number,
                max_error_fix_attempts,
                show_full_output,
                skip_test,
            )?;

        Ok((build_successful, recursive_fix_attempts, had_restart))
    }
}

/// 从错误消息中提取手动修复所需的文件列表，rs_file 始终包含在内
fn collect_fix_files(
    feature: &str,
    rs_file: &Path,
    error: &anyhow::Error,
) -> Vec<std::path::PathBuf> {
    crate::builder::get_manual_fix_files(feature, rs_file, &error.to_string())
}

/// 处理手动修复选项
fn handle_manual_fix(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    build_error: &anyhow::Error,
    skip_test: bool,
) -> Result<(bool, usize, bool)> {
    println!("│");
    println!("│ {}", "You chose: Manual fix".bright_cyan());

    // 尝试打开文件（多文件时展示选择界面）
    match interaction::open_files_for_manual_fix(&collect_fix_files(feature, rs_file, build_error))
    {
        Ok(_) => {
            // Vim 编辑后，重复尝试构建并允许用户决定是重试还是退出
            loop {
                println!("│");
                println!(
                    "│ {}",
                    "Vim editing completed. Running full build and test...".bright_blue()
                );

                // 手动编辑后执行完整构建流程
                match builder::run_full_build_and_test_interactive(feature, file_type, rs_file, skip_test) {
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

                        // 从新的错误中提取涉及的文件列表
                        let fix_files = collect_fix_files(feature, rs_file, &e);
                        println!("│");
                        println!(
                            "│ {}",
                            format!("Found {} file(s) with errors:", fix_files.len())
                                .bright_yellow()
                                .bold()
                        );
                        for (idx, file) in fix_files.iter().enumerate() {
                            println!("│   {}. {}", idx + 1, file.display());
                        }

                        // 询问用户是否想再试一次
                        println!("│");
                        println!(
                            "│ {}",
                            "Build or tests still have errors. What would you like to do?".yellow()
                        );
                        let retry_choice = interaction::prompt_after_manual_fix_choice()?;

                        match retry_choice {
                            interaction::FailureChoice::ManualFix => {
                                println!(
                                    "│ {}",
                                    "Opening Vim again for another manual fix attempt..."
                                        .bright_cyan()
                                );
                                // 使用已提取的文件列表（不重新解析）
                                interaction::open_files_for_manual_fix(&fix_files)?;
                            }
                            interaction::FailureChoice::Exit => {
                                return Err(e).context(format!(
                                    "Build or tests failed after manual fix for file {}",
                                    rs_file.display()
                                ));
                            }
                            interaction::FailureChoice::FixOtherFile => {
                                println!(
                                    "│ {}",
                                    "Skipping current file to fix other files..."
                                        .bright_cyan()
                                );
                                return Err(anyhow::Error::from(SkipFileSignal));
                            }
                            interaction::FailureChoice::RetryDirectly
                            | interaction::FailureChoice::AddSuggestion
                            | interaction::FailureChoice::Skip => {
                                return Err(e).context(
                                    "手动修复处理中出现意外选项 - 此上下文仅支持 ManualFix、FixOtherFile 和 Exit",
                                );
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

    /// Save the current value of an environment variable and return a `scopeguard`
    /// that restores it (or removes it if it was absent) when dropped.
    fn env_guard(key: &'static str) -> impl Drop {
        let prior = std::env::var(key).ok();
        scopeguard::guard(prior, move |v| match v {
            Some(val) => std::env::set_var(key, val),
            None => std::env::remove_var(key),
        })
    }

    /// Test that apply_fixes_for_messages returns Ok(0) when the fix attempt fails
    /// because the translate script is unavailable (`C2RUST_TRANSLATE_DIR` is not set
    /// in the test environment).  The warning message points at `src/nonexistent.rs`,
    /// which exists on disk together with its companion `nonexistent.c` so that
    /// `group_errors_by_file` resolves the file and calls `apply_warning_fix` for it.
    /// `fix_translation_error` then fails deterministically when it tries to look up the
    /// translate-script path.  Fix failures are non-fatal: the function logs a warning
    /// and returns 0 fixes applied so the caller can continue without aborting the
    /// file-processing workflow.
    #[test]
    #[serial_test::serial]
    fn test_apply_fixes_for_messages_fix_failure_missing_translate_script_is_nonfatal() {
        use std::env;
        use tempfile::TempDir;

        // Explicitly unset C2RUST_TRANSLATE_DIR so the translate-script lookup fails
        // deterministically, regardless of whether the variable happens to be set in
        // the outer CI/developer environment.
        let _translate_dir_guard = env_guard("C2RUST_TRANSLATE_DIR");
        env::remove_var("C2RUST_TRANSLATE_DIR");

        // Set up a temporary project root with a valid .c2rust/<feature>/rust/src tree.
        let tmp = TempDir::new().unwrap();
        let orig_dir = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();
        let _restore = scopeguard::guard(orig_dir, |dir| {
            let _ = env::set_current_dir(dir);
        });

        let feature = "test_feature";
        let feature_src_dir = tmp
            .path()
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src");
        std::fs::create_dir_all(&feature_src_dir).unwrap();

        // Create both the target .rs file and its companion .c file so that
        // group_errors_by_file resolves the path from the warning message and
        // fix_translation_error passes the C-file existence check.  The fix then
        // fails deterministically when it looks up the translate-script path
        // (C2RUST_TRANSLATE_DIR is not set in the test environment), exercising
        // the non-fatal warning path.
        std::fs::write(feature_src_dir.join("nonexistent.rs"), "").unwrap();
        std::fs::write(feature_src_dir.join("nonexistent.c"), "").unwrap();

        // Point the warning message at the .rs file we just created so that
        // group_errors_by_file finds it and apply_warning_fix is called for it.
        let rs_file = feature_src_dir.join("nonexistent.rs");

        let result = apply_fixes_for_messages(
            "warning: unused\n  --> src/nonexistent.rs:1:1",
            &anyhow::anyhow!("dummy"),
            feature,
            "var",
            &rs_file,
            &|op: &str| op.to_string(),
            false,
            true,
        );

        // Fix failures are now non-fatal: the function logs a warning and returns
        // Ok(0) (zero fixes applied) instead of propagating the error.
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    /// Test that execute_code_warning_check_with_fix_loop returns Ok(0) when the build fails
    /// during the warning phase (e.g. the feature build directory does not exist).
    /// The loop should treat this as non-fatal and return Ok(0).
    #[test]
    #[serial_test::serial]
    fn test_execute_code_warning_check_with_fix_loop_build_failure_is_nonfatal() {
        use std::env;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let orig = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();

        // Create minimal .c2rust dir so find_project_root works, but do NOT create
        // the feature build directory so cargo_build will fail.
        std::fs::create_dir_all(tmp.path().join(".c2rust")).unwrap();

        // Use a path inside tmp so it is portable and clearly non-existent.
        let rs_file = tmp.path().join("var_foo.rs");

        let result = execute_code_warning_check_with_fix_loop(
            "nonexistent_feature",
            "var",
            &rs_file,
            "var_foo.rs",
            &|op: &str| op.to_string(),
            1, // max_warning_fix_attempts
            false,
        );

        env::set_current_dir(orig).unwrap();

        // Build error during warning phase should be non-fatal → Ok(0)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    /// collect_fix_files returns a list containing only rs_file when the feature
    /// name is invalid (parse_error_for_files returns Err).
    #[test]
    fn test_collect_fix_files_invalid_feature_falls_back_to_rs_file() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let rs_file = tmp.path().join("var_foo.rs");
        std::fs::write(&rs_file, "").unwrap();

        // "../bad" is rejected by validate_feature_name, triggering the Err branch
        let error = anyhow::anyhow!("dummy build error");
        let result = collect_fix_files("../bad", &rs_file, &error);

        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("var_foo.rs"));
    }

    /// collect_fix_files includes rs_file even when parse returns an empty list
    /// (no matching files in the error message).
    #[test]
    #[serial_test::serial]
    fn test_collect_fix_files_empty_parse_includes_rs_file() {
        use std::env;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let orig = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();
        let _restore = scopeguard::guard(orig, |dir| {
            let _ = env::set_current_dir(dir);
        });
        std::fs::create_dir_all(tmp.path().join(".c2rust")).unwrap();

        let feature = "test_feature";
        std::fs::create_dir_all(
            tmp.path()
                .join(".c2rust")
                .join(feature)
                .join("rust")
                .join("src"),
        )
        .unwrap();

        let rs_file = tmp
            .path()
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src")
            .join("var_foo.rs");
        std::fs::write(&rs_file, "").unwrap();

        // Error message references no files → parse returns empty
        let error = anyhow::anyhow!("no file references here");
        let result = collect_fix_files(feature, &rs_file, &error);

        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("var_foo.rs"));
    }

    /// collect_fix_files does not duplicate rs_file when it already appears in
    /// the parsed file list.
    #[test]
    #[serial_test::serial]
    fn test_collect_fix_files_no_duplicate_when_rs_file_already_present() {
        use std::env;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let orig = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();
        let _restore = scopeguard::guard(orig, |dir| {
            let _ = env::set_current_dir(dir);
        });

        std::fs::create_dir_all(tmp.path().join(".c2rust")).unwrap();

        let feature = "test_feature";
        let src_dir = tmp
            .path()
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let rs_file = src_dir.join("var_foo.rs");
        std::fs::write(&rs_file, "").unwrap();

        // Error message that references var_foo.rs so parse_error_for_files returns it
        let error_msg = "error[E0308]: mismatched types\n   --> src/var_foo.rs:1:1\n    |\n1   | x\n    | ^ error";
        let error = anyhow::anyhow!("{}", error_msg);
        let result = collect_fix_files(feature, &rs_file, &error);

        // rs_file must appear exactly once
        let count = result
            .iter()
            .filter(|f| f.ends_with("var_foo.rs"))
            .count();
        assert_eq!(count, 1, "var_foo.rs should appear exactly once, got: {result:?}");
    }

    /// collect_fix_files inserts rs_file at the front when it is not in the
    /// parsed list and multiple other files are present.
    #[test]
    #[serial_test::serial]
    fn test_collect_fix_files_rs_file_inserted_first_when_missing() {
        use std::env;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let orig = env::current_dir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();
        let _restore = scopeguard::guard(orig, |dir| {
            let _ = env::set_current_dir(dir);
        });

        std::fs::create_dir_all(tmp.path().join(".c2rust")).unwrap();

        let feature = "test_feature";
        let src_dir = tmp
            .path()
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // Create two files that appear in the error, and a third that is rs_file
        let file_a = src_dir.join("fun_a.rs");
        let file_b = src_dir.join("fun_b.rs");
        let rs_file = src_dir.join("var_main.rs");
        for f in [&file_a, &file_b, &rs_file] {
            std::fs::write(f, "").unwrap();
        }

        // Error references fun_a and fun_b but NOT var_main
        let error_msg = "error[E0308]: mismatched types\n   --> src/fun_a.rs:1:1\n    |\n1   | x\nerror[E0425]: unknown\n   --> src/fun_b.rs:2:1\n    |\n2   | y";
        let error = anyhow::anyhow!("{}", error_msg);
        let result = collect_fix_files(feature, &rs_file, &error);

        // rs_file should be present and at index 0
        assert!(
            result.iter().any(|f| f.ends_with("var_main.rs")),
            "var_main.rs should be in results: {result:?}"
        );
        assert!(
            result[0].ends_with("var_main.rs"),
            "var_main.rs should be first: {result:?}"
        );
    }

    /// When the env var is not set, `resolve_auto_retry_outcome` returns `None`
    /// regardless of whether this is the last attempt.
    #[test]
    #[serial_test::serial]
    fn test_resolve_auto_retry_outcome_env_unset_returns_none() {
        let _restore = env_guard("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        std::env::remove_var("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        assert_eq!(resolve_auto_retry_outcome(false), None);
        assert_eq!(resolve_auto_retry_outcome(true), None);
    }

    /// When the env var is set and there are retries remaining (not last attempt),
    /// `resolve_auto_retry_outcome` returns `Some(AutoRetryOutcome::Retry)`.
    #[test]
    #[serial_test::serial]
    fn test_resolve_auto_retry_outcome_env_set_not_last_returns_retry() {
        let _restore = env_guard("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        std::env::set_var("C2RUST_AUTO_RETRY_ON_MAX_FIX", "1");
        assert_eq!(resolve_auto_retry_outcome(false), Some(AutoRetryOutcome::Retry));
    }

    /// When the env var is set and this is the last attempt,
    /// `resolve_auto_retry_outcome` returns `Some(AutoRetryOutcome::Skip)`.
    #[test]
    #[serial_test::serial]
    fn test_resolve_auto_retry_outcome_env_set_last_attempt_returns_skip() {
        let _restore = env_guard("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        std::env::set_var("C2RUST_AUTO_RETRY_ON_MAX_FIX", "1");
        assert_eq!(resolve_auto_retry_outcome(true), Some(AutoRetryOutcome::Skip));
    }

    /// Accepted truthy values ("true", "yes") also trigger auto-retry.
    #[test]
    #[serial_test::serial]
    fn test_resolve_auto_retry_outcome_accepts_true_and_yes() {
        let _restore = env_guard("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        for val in &["true", "yes", "TRUE", "YES"] {
            std::env::set_var("C2RUST_AUTO_RETRY_ON_MAX_FIX", val);
            assert_eq!(resolve_auto_retry_outcome(false), Some(AutoRetryOutcome::Retry), "val={val}");
            assert_eq!(resolve_auto_retry_outcome(true), Some(AutoRetryOutcome::Skip), "val={val}");
        }
    }

    /// A non-truthy value leaves the behaviour interactive (`None`).
    #[test]
    #[serial_test::serial]
    fn test_resolve_auto_retry_outcome_non_truthy_returns_none() {
        let _restore = env_guard("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        std::env::set_var("C2RUST_AUTO_RETRY_ON_MAX_FIX", "0");
        assert_eq!(resolve_auto_retry_outcome(false), None);
    }
}

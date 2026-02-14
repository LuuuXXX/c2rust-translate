use crate::{analyzer, builder, diff_display, interaction, suggestion, translator};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// 在循环中构建并修复错误
///
/// 返回 Ok(true) 如果构建成功，Ok(false) 如果需要重试翻译
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
) -> Result<bool>
where
    F: Fn(&str) -> String,
{
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
                return Ok(true);
            }
            Err(build_error) => {
                if attempt == max_fix_attempts {
                    return handle_max_fix_attempts_reached(
                        build_error,
                        file_name,
                        rs_file,
                        is_last_attempt,
                        attempt_number,
                        max_fix_attempts,
                        feature,
                        file_type,
                    );
                } else {
                    // Use lib.rs apply_error_fix instead of local duplicate
                    crate::apply_error_fix(
                        feature,
                        file_type,
                        rs_file,
                        &build_error,
                        format_progress,
                        show_full_output,
                    )?;
                }
            }
        }

        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
    }

    Ok(false)
}

/// 处理达到最大修复尝试次数的情况
///
/// 返回:
/// - Ok(true) 如果处理应继续而不重试翻译
/// - Ok(false) 如果应重试翻译
fn handle_max_fix_attempts_reached(
    build_error: anyhow::Error,
    file_name: &str,
    rs_file: &Path,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    feature: &str,
    file_type: &str,
) -> Result<bool> {
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
        ),
        interaction::FailureChoice::ManualFix => handle_manual_fix(feature, file_type, rs_file),
        interaction::FailureChoice::Exit => Err(build_error).context(format!(
            "Build failed after {} fix attempts for file {}",
            max_fix_attempts, file_name
        )),
    }
}

/// 处理直接重试选项
fn handle_retry_directly(attempt_number: usize, is_last_attempt: bool) -> Result<bool> {
    use crate::util::MAX_TRANSLATION_ATTEMPTS;

    println!("│");
    println!(
        "│ {}",
        "You chose: Retry directly without suggestion".bright_cyan()
    );
    println!("│");
    println!(
        "│ {}",
        "⚠ Warning: This will:".bright_yellow().bold()
    );
    println!(
        "│ {}",
        "  • Clear the current .rs file content".bright_yellow()
    );
    println!(
        "│ {}",
        "  • Re-translate from C source completely".bright_yellow()
    );
    println!(
        "│ {}",
        "  • Clear all previous suggestions".bright_yellow()
    );
    println!("│");

    // 清除旧建议
    suggestion::clear_suggestions()?;

    // 重新翻译（清空并重新生成 rs 文件）
    let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
    if is_last_attempt {
        println!(
            "│ {}",
            "This is the last automatic retry attempt.".bright_yellow()
        );
        println!(
            "│ {}",
            "Retrying translation from scratch one final time...".bright_cyan()
        );
    } else {
        println!(
            "│ {}",
            format!(
                "Retrying translation from scratch... ({} retries remaining)",
                remaining_retries
            )
            .bright_cyan()
        );
    }
    println!(
        "│ {}",
        "Note: The translator will overwrite the existing file content.".bright_blue()
    );
    println!("│ {}", "✓ Retry scheduled".bright_green());
    Ok(false) // 发出重试信号
}

/// 处理添加建议选项
fn handle_add_suggestion(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    build_error: &anyhow::Error,
    is_last_attempt: bool,
    attempt_number: usize,
) -> Result<bool> {
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
        Ok(false) // 发出重试信号
    } else {
        // 没有更多翻译重试，但我们可以再次尝试修复
        println!(
            "│ {}",
            "No translation retries remaining, attempting fix with new suggestion..."
                .bright_yellow()
        );

        // 应用带有建议的修复
        let error_msg = format!("{:#}", build_error);
        translator::fix_translation_error(feature, file_type, rs_file, &error_msg, true, true)?;

        // 再试一次构建和测试
        println!("│");
        println!(
            "│ {}",
            "Running full build and test after applying fix..."
                .bright_blue()
                .bold()
        );
        match builder::run_full_build_and_test_interactive(feature, file_type, rs_file) {
            Ok(_) => Ok(true),
            Err(e) => {
                println!(
                    "│ {}",
                    "✗ Build or tests still failing after fix attempt".red()
                );
                Err(e).context(format!(
                    "Build or tests failed after fix with suggestion for file {}",
                    rs_file.display()
                ))
            }
        }
    }
}

/// 处理手动修复选项
fn handle_manual_fix(feature: &str, file_type: &str, rs_file: &Path) -> Result<bool> {
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
                        return Ok(true);
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

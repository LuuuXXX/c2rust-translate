use crate::{interaction, util};
use anyhow::{Context, Result};
use colored::Colorize;

/// 从错误信息中提取失败的 .rs 文件并打开编辑器
///
/// 支持多文件选择
fn open_failing_files_from_error(error_text: &str, feature: &str) -> Result<bool> {
    let failing_files = crate::error_handler::group_errors_by_file(error_text, feature)?
        .into_iter()
        .map(|(f, _)| f)
        .collect::<Vec<_>>();

    if failing_files.is_empty() {
        return Ok(false);
    }

    if failing_files.len() > 1 {
        println!("│");
        println!(
            "│ {}",
            format!("找到 {} 个包含错误的文件:", failing_files.len()).yellow()
        );
        for (i, f) in failing_files.iter().enumerate() {
            println!("│   {}. {}", i + 1, f.display());
        }
        println!("│");

        let selected_file = interaction::prompt_file_selection_for_edit(&failing_files)?;
        interaction::open_in_vim(&selected_file)?;
    } else {
        interaction::open_in_vim(&failing_files[0])?;
    }

    Ok(true)
}

/// Apply auto-fixes for errors or warnings found during initialization validation.
///
/// Reuses `apply_error_fix` / `apply_warning_fix` and `group_errors_by_file`
/// to apply the same fix logic as the file-processing phase, ensuring code reuse.
///
/// Returns the number of fixes applied.
fn apply_fixes_for_init(
    message: &str,
    feature: &str,
    show_full_output: bool,
    is_warning: bool,
) -> Result<usize> {
    let mut count = 0;

    let file_messages = match crate::error_handler::group_errors_by_file(message, feature) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "│ {}",
                format!("⚠ 无法按文件分组消息: {}", e).yellow()
            );
            return Ok(0);
        }
    };

    for (msg_file, file_msg) in &file_messages {
        let Some(file_stem) = msg_file.file_stem().and_then(|s| s.to_str()) else {
            println!(
                "│ {}",
                format!("⚠ 跳过无效文件名: {}", msg_file.display()).yellow()
            );
            continue;
        };
        let (msg_file_type, _) =
            crate::file_scanner::extract_file_type(file_stem).unwrap_or(("fn", ""));
        let msg_error = anyhow::anyhow!("{}", file_msg);
        let msg_file_name = msg_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file_stem);
        let msg_format_progress = |op: &str| format!("修复 {} - {}", msg_file_name, op);

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

    Ok(count)
}

/// 检查并初始化 feature 目录
///
/// 如果 rust 目录不存在，则初始化并提交
pub fn check_and_initialize_feature(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");

    let rust_dir_exists = match std::fs::metadata(&rust_dir) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                anyhow::bail!("Path exists but is not a directory: {}", rust_dir.display());
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(e).context(format!(
                "Failed to access rust directory at {}",
                rust_dir.display()
            ));
        }
    };

    if !rust_dir_exists {
        println!(
            "{}",
            "Feature directory does not exist. Initializing...".yellow()
        );
        crate::analyzer::initialize_feature(feature)?;

        match std::fs::metadata(&rust_dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    anyhow::bail!(
                        "Initialization created a file instead of a directory: {}",
                        rust_dir.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("Error: Failed to initialize rust directory");
            }
            Err(e) => {
                return Err(e).context(format!(
                    "Failed to verify initialized rust directory at {}",
                    rust_dir.display()
                ));
            }
        }

        crate::git::git_commit(
            &format!("Initialize {} feature directory", feature),
            feature,
        )?;

        println!(
            "{}",
            "✓ Feature directory initialized successfully".bright_green()
        );
    } else {
        println!(
            "{}",
            "Feature directory exists, continuing...".bright_cyan()
        );
    }

    Ok(())
}

/// 执行初始化验证
///
/// 在项目初始化后执行完整的代码检查：
/// - 阶段1：错误检查，支持自动修复和手动修复
/// - 阶段2：告警检查，支持自动修复和手动修复
pub fn execute_initial_verification(feature: &str, show_full_output: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!(
        "\n{}",
        "═══ 初始化验证（初始化后） ═══".bright_magenta().bold()
    );

    // 阶段1：错误检查 + 自动/手动修复循环
    match crate::common_tasks::execute_code_error_check(feature, show_full_output) {
        Ok(_) => {
            println!("{}", "✓ 初始化错误检查通过".bright_green().bold());
        }
        Err(mut last_error) => {
            loop {
                println!("{}", "✗ 初始化验证失败！".red().bold());
                println!();
                println!("{}", "错误详情:".red().bold());
                println!("{}", format!("{:#}", last_error).red());
                println!();

                // 先尝试自动修复
                let error_text = format!("{:#}", last_error);
                match apply_fixes_for_init(&error_text, feature, show_full_output, false) {
                    Ok(n) if n > 0 => {
                        match crate::common_tasks::execute_code_error_check(
                            feature,
                            show_full_output,
                        ) {
                            Ok(_) => {
                                println!("{}", "✓ 初始化错误检查通过".bright_green().bold());
                                break;
                            }
                            Err(e) => {
                                last_error = e;
                                continue;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        println!(
                            "│ {}",
                            format!("⚠ 自动修复失败: {}", e).yellow()
                        );
                    }
                }

                let choice = interaction::prompt_failure_choice("初始化验证失败")?;

                match choice {
                    interaction::FailureChoice::Skip => {
                        println!(
                            "│ {}",
                            "跳过初始化验证。在文件处理过程中可能会出现问题。".yellow()
                        );
                        return Ok(());
                    }
                    interaction::FailureChoice::ManualFix => {
                        let error_text = format!("{:#}", last_error);
                        if !open_failing_files_from_error(&error_text, feature)? {
                            println!(
                                "│ {}",
                                "无法识别要打开的特定文件。请检查上面的错误。".yellow()
                            );
                            return Err(last_error).context("初始化验证失败 - 未识别到特定文件");
                        }
                        // Re-run the check; on success break out, on failure loop again
                        match crate::common_tasks::execute_code_error_check(
                            feature,
                            show_full_output,
                        ) {
                            Ok(_) => {
                                println!(
                                    "{}",
                                    "✓ 初始化错误检查通过".bright_green().bold()
                                );
                                break;
                            }
                            Err(e) => {
                                last_error = e;
                                continue;
                            }
                        }
                    }
                    interaction::FailureChoice::Exit => {
                        return Err(last_error).context("初始化验证失败，用户选择退出");
                    }
                    interaction::FailureChoice::RetryDirectly
                    | interaction::FailureChoice::AddSuggestion
                    | interaction::FailureChoice::FixOtherFile => {
                        println!("│ {}", "此上下文不支持该选项，视为退出".yellow());
                        return Err(last_error).context("初始化验证失败");
                    }
                }
            }
        }
    }

    // 阶段2：告警检查 + 自动/手动修复循环
    println!("{}", "  → 执行告警检查...".bright_blue());
    match crate::common_tasks::execute_code_warning_check(feature, show_full_output) {
        Ok(_) => {}
        Err(mut last_warning) => {
            loop {
                println!("{}", "⚠ 初始化告警检查发现警告！".yellow().bold());
                println!();
                println!("{}", format!("{:#}", last_warning).yellow());
                println!();

                // 先尝试自动修复
                let warning_text = format!("{:#}", last_warning);
                match apply_fixes_for_init(&warning_text, feature, show_full_output, true) {
                    Ok(n) if n > 0 => {
                        match crate::common_tasks::execute_code_warning_check(
                            feature,
                            show_full_output,
                        ) {
                            Ok(_) => {
                                break;
                            }
                            Err(e) => {
                                last_warning = e;
                                continue;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        println!(
                            "│ {}",
                            format!("⚠ 告警自动修复失败: {}", e).yellow()
                        );
                    }
                }

                let choice = interaction::prompt_failure_choice("初始化告警检查")?;

                match choice {
                    interaction::FailureChoice::Skip => {
                        println!(
                            "│ {}",
                            "跳过告警检查。在文件处理过程中可能会出现问题。".yellow()
                        );
                        return Ok(());
                    }
                    interaction::FailureChoice::ManualFix => {
                        let warning_text = format!("{:#}", last_warning);
                        if !open_failing_files_from_error(&warning_text, feature)? {
                            println!(
                                "│ {}",
                                "无法识别要打开的特定文件。跳过告警检查。".yellow()
                            );
                            return Ok(());
                        }
                        // Re-run the check; on success break out, on failure loop again
                        match crate::common_tasks::execute_code_warning_check(
                            feature,
                            show_full_output,
                        ) {
                            Ok(_) => {
                                break;
                            }
                            Err(e) => {
                                last_warning = e;
                                continue;
                            }
                        }
                    }
                    interaction::FailureChoice::Exit => {
                        return Err(last_warning).context("初始化告警检查失败，用户选择退出");
                    }
                    interaction::FailureChoice::RetryDirectly
                    | interaction::FailureChoice::AddSuggestion
                    | interaction::FailureChoice::FixOtherFile => {
                        println!("│ {}", "此上下文不支持该选项，视为退出".yellow());
                        return Err(last_warning).context("初始化告警检查失败");
                    }
                }
            }
        }
    }

    println!("{}", "✓ 初始化验证完成并已提交".bright_green().bold());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_and_initialize_feature_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str) -> Result<()>,
        {
            let _ = f;
        }

        assert_signature(check_and_initialize_feature);
    }

    #[test]
    fn execute_initial_verification_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str, bool) -> Result<()>,
        {
            let _ = f;
        }

        assert_signature(execute_initial_verification);
    }
}

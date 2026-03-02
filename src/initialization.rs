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
/// 在项目初始化后执行一次完整的代码错误检查，确保项目基础状态正常
pub fn execute_initial_verification(feature: &str, show_full_output: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!(
        "\n{}",
        "═══ 初始化验证（初始化后） ═══".bright_magenta().bold()
    );

    match crate::common_tasks::execute_code_error_check(feature, show_full_output) {
        Ok(_) => {
            println!("{}", "✓ 初始化验证完成并已提交".bright_green().bold());
            Ok(())
        }
        Err(mut last_error) => {
            loop {
                println!("{}", "✗ 初始化验证失败！".red().bold());
                println!();
                println!("{}", "错误详情:".red().bold());
                println!("{}", format!("{:#}", last_error).red());
                println!();

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
                                    "✓ 初始化验证完成并已提交".bright_green().bold()
                                );
                                return Ok(());
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
                    | interaction::FailureChoice::RetryBuild
                    | interaction::FailureChoice::FixOtherFile => {
                        println!("│ {}", "此上下文不支持该选项，视为退出".yellow());
                        return Err(last_error).context("初始化验证失败");
                    }
                }
            }
        }
    }
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

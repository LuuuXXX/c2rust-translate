pub mod analyzer;
pub mod builder;
pub mod constants;
pub(crate) mod diff_display;
pub(crate) mod error_handler;
pub mod file_scanner;
pub mod git;
pub mod hybrid_build;
pub mod initialization;
pub(crate) mod interaction;
pub mod progress;
pub(crate) mod suggestion;
pub mod translator;
pub mod util;
pub mod verification;

use anyhow::{Context, Result};
use colored::Colorize;

/// 特性的主要翻译工作流
pub fn translate_feature(
    feature: &str,
    allow_all: bool,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    let msg = format!("Starting translation for feature: {}", feature);
    println!("{}", msg.bright_cyan().bold());

    // 步骤 1：查找项目根目录和初始化
    println!(
        "\n{}",
        "Step 1: Find Project Root and Initialize".bright_cyan().bold()
    );
    initialization::check_and_initialize_feature(feature)?;

    // 步骤 2：门禁验证
    initialization::run_gate_verification(feature, show_full_output)?;

    // 获取 rust 目录用于后续步骤
    let project_root = util::find_project_root()?;
    let rust_dir = project_root.join(".c2rust").join(feature).join("rust");

    // 步骤 3 & 4：扫描文件并初始化进度
    println!(
        "\n{}",
        "Step 3: Select Files to Translate".bright_cyan().bold()
    );

    // 计算总 .rs 文件数和已处理的文件数
    let total_rs_files = file_scanner::count_all_rs_files(&rust_dir)?;
    let initial_empty_count = file_scanner::find_empty_rs_files(&rust_dir)?.len();
    let already_processed = total_rs_files.saturating_sub(initial_empty_count);

    let mut progress_state =
        progress::ProgressState::with_initial_progress(total_rs_files, already_processed);

    println!(
        "\n{}",
        "Step 4: Initialize Project Progress".bright_cyan().bold()
    );
    let progress_percentage = if total_rs_files > 0 {
        (already_processed as f64 / total_rs_files as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "{} {:.1}% ({}/{} files processed)",
        "Current progress:".cyan(),
        progress_percentage,
        already_processed,
        total_rs_files
    );

    // 步骤 5：执行翻译所有待翻译文件
    println!(
        "\n{}",
        "Step 5: Execute Translation for All Files".bright_cyan().bold()
    );
    loop {
        // 5.1：扫描空的 .rs 文件（未处理的文件）
        let empty_rs_files = file_scanner::find_empty_rs_files(&rust_dir)?;

        if empty_rs_files.is_empty() {
            let msg = "✓ No empty .rs files found. Translation complete!";
            println!("\n{}", msg.bright_green().bold());
            break;
        }

        println!(
            "{}",
            format!(
                "Found {} empty .rs file(s) to process",
                empty_rs_files.len()
            )
            .cyan()
        );

        // 基于 allow_all 标志选择要处理的文件
        let selected_indices: Vec<usize> = if allow_all {
            // 不提示处理所有空文件
            (0..empty_rs_files.len()).collect()
        } else {
            // 提示用户选择文件
            let file_refs: Vec<_> = empty_rs_files.iter().collect();
            file_scanner::prompt_file_selection(&file_refs, &rust_dir)?
        };

        for &idx in selected_indices.iter() {
            let rs_file = &empty_rs_files[idx];
            // 获取当前进度位置（在循环迭代之间保持）
            let current_position = progress_state.get_current_position();
            let total_count = progress_state.get_total_count();

            // 获取文件名以供显示
            let file_name = rs_file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>");

            let progress_msg = format!(
                "[{}/{}] Processing {}",
                current_position, total_count, file_name
            );
            println!("\n{}", progress_msg.bright_magenta().bold());

            process_rs_file(
                feature,
                rs_file,
                file_name,
                current_position,
                total_count,
                max_fix_attempts,
                show_full_output,
            )?;

            // 标记此会话中已处理的文件
            progress_state.mark_processed();
        }
    }

    Ok(())
}

/// 通过翻译工作流处理单个 .rs 文件
fn process_rs_file(
    feature: &str,
    rs_file: &std::path::Path,
    file_name: &str,
    current_position: usize,
    total_count: usize,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    use constants::MAX_TRANSLATION_ATTEMPTS;

    for attempt_number in 1..=MAX_TRANSLATION_ATTEMPTS {
        let is_last_attempt = attempt_number == MAX_TRANSLATION_ATTEMPTS;

        print_attempt_header(attempt_number, rs_file);

        // 为重试尝试添加消息
        if attempt_number > 1 {
            println!(
                "│ {}",
                "Starting fresh translation (previous translation will be overwritten)..."
                    .bright_cyan()
            );
        }

        let (file_type, _name) = extract_and_validate_file_info(rs_file)?;
        check_c_file_exists(rs_file)?;

        let format_progress = |operation: &str| {
            format!(
                "[{}/{}] Processing {} - {}",
                current_position, total_count, file_name, operation
            )
        };

        // 将 C 翻译为 Rust
        translate_file(
            feature,
            file_type,
            rs_file,
            &format_progress,
            show_full_output,
        )?;

        // 构建并修复错误 - 使用 verification 模块
        let build_successful = verification::build_and_fix_loop(
            feature,
            file_type,
            rs_file,
            file_name,
            &format_progress,
            is_last_attempt,
            attempt_number,
            max_fix_attempts,
            show_full_output,
        )?;

        if build_successful {
            let processing_complete =
                complete_file_processing(feature, file_name, file_type, rs_file, &format_progress)?;
            if processing_complete {
                return Ok(());
            }
            // If processing_complete is false, retry translation (loop continues)
        }
    }

    anyhow::bail!("Unexpected: all retry attempts completed without resolution")
}

/// 打印当前尝试的标题
fn print_attempt_header(attempt_number: usize, rs_file: &std::path::Path) {
    if attempt_number > 1 {
        let retry_number = attempt_number - 1;
        let max_retries = constants::MAX_TRANSLATION_ATTEMPTS - 1;
        println!(
            "\n{}",
            format!(
                "┌─ Retry attempt {}/{}: {}",
                retry_number,
                max_retries,
                rs_file.display()
            )
            .bright_yellow()
            .bold()
        );
    } else {
        println!(
            "\n{}",
            format!("┌─ Processing file: {}", rs_file.display())
                .bright_white()
                .bold()
        );
    }
}

/// 提取文件类型和名称，打印信息
fn extract_and_validate_file_info(rs_file: &std::path::Path) -> Result<(&'static str, &str)> {
    let file_stem = rs_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?;

    let (file_type, name) = file_scanner::extract_file_type(file_stem)
        .ok_or_else(|| anyhow::anyhow!("Unknown file prefix: {}", file_stem))?;

    println!("│ {} {}", "File type:".cyan(), file_type.bright_yellow());
    println!("│ {} {}", "Name:".cyan(), name.bright_yellow());

    Ok((file_type, name))
}

/// 检查对应的 C 文件是否存在
fn check_c_file_exists(rs_file: &std::path::Path) -> Result<()> {
    use std::fs;

    let c_file = rs_file.with_extension("c");
    match fs::metadata(&c_file) {
        Ok(_) => {
            println!(
                "│ {} {}",
                "C source:".cyan(),
                c_file.display().to_string().bright_yellow()
            );
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "Corresponding C file not found for Rust file: {}",
                rs_file.display()
            );
        }
        Err(err) => Err(err).context(format!(
            "Failed to access corresponding C file for Rust file: {}",
            rs_file.display()
        )),
    }
}

/// 将 C 文件翻译为 Rust
fn translate_file<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    use std::fs;

    let c_file = rs_file.with_extension("c");

    println!("│");
    println!(
        "│ {}",
        format_progress("Translation").bright_magenta().bold()
    );
    println!(
        "│ {}",
        format!("Translating {} to Rust...", file_type)
            .bright_blue()
            .bold()
    );
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file, show_full_output)?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }
    println!(
        "│ {}",
        format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green()
    );

    Ok(())
}

/// 对文件应用错误修复
pub(crate) fn apply_error_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    build_error: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    use std::fs;

    println!(
        "│ {}",
        "⚠ Build failed, attempting to fix errors..."
            .yellow()
            .bold()
    );
    println!("│");
    println!("│ {}", format_progress("Fix").bright_magenta().bold());
    // 始终显示完整的修复代码，但尊重用户对错误预览的偏好
    translator::fix_translation_error(
        feature,
        file_type,
        rs_file,
        &build_error.to_string(),
        show_full_output, // 用户对错误预览的偏好
        true,             // 始终显示完整的修复代码
    )?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Fix failed: output file is empty");
    }
    println!("│ {}", "✓ Fix applied".bright_green());

    Ok(())
}

/// 完成文件处理（提交、分析、混合构建）
/// Completes file processing by running hybrid build tests and committing changes
///
/// Returns:
/// - Ok(true) if file processing completed successfully (continue to next file)
/// - Ok(false) if translation should be retried from scratch
/// - Err if an unrecoverable error occurred
fn complete_file_processing<F>(
    feature: &str,
    file_name: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    format_progress: &F,
) -> Result<bool>
where
    F: Fn(&str) -> String,
{
    // 在提交之前首先运行混合构建测试
    println!("│");
    println!(
        "│ {}",
        format_progress("Hybrid Build Tests")
            .bright_magenta()
            .bold()
    );
    println!("│ {}", "Running hybrid build tests...".bright_blue());

    // 预检查（与 run_hybrid_build_interactive 中相同）
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");

    if !config_path.exists() {
        eprintln!(
            "{}",
            format!("Error: Config file not found at {}", config_path.display()).red()
        );
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    // 继续之前检查 c2rust-config 是否可用
    let check_output = std::process::Command::new("c2rust-config")
        .arg("--help")
        .output();

    match check_output {
        Ok(output) => {
            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "{}",
                    format!(
                        "Error: c2rust-config --version failed.\nstdout:\n{}\nstderr:\n{}",
                        stdout, stderr
                    )
                    .red()
                );
                anyhow::bail!(
                    "c2rust-config is present but failed to run successfully, cannot run hybrid build tests"
                );
            }
        }
        Err(_) => {
            eprintln!("{}", "Error: c2rust-config not found".red());
            anyhow::bail!("c2rust-config not found, cannot run hybrid build tests");
        }
    }

    // 使用自定义处理运行测试以检测成功/失败
    builder::c2rust_clean(feature)?;

    match builder::c2rust_build(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Build successful".bright_green().bold());
        }
        Err(build_error) => {
            println!("│ {}", "✗ Build failed".red().bold());
            // Enter interactive build failure handling
            let processing_complete = builder::handle_build_failure_interactive(
                feature,
                file_type,
                rs_file,
                build_error,
            )?;
            if !processing_complete {
                // User chose to retry translation
                return Ok(false);
            }
        }
    }

    let test_result = builder::c2rust_test(feature);

    match test_result {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());

            // 如果不在自动接受模式下，显示代码比较和成功提示
            if !interaction::is_auto_accept_mode() {
                let c_file = rs_file.with_extension("c");

                // 显示文件位置
                interaction::display_file_paths(Some(&c_file), rs_file);

                // 使用差异显示进行更好的比较
                let success_message = "✓ All tests passed";
                if let Err(e) = diff_display::display_code_comparison(
                    &c_file,
                    rs_file,
                    success_message,
                    diff_display::ResultType::TestPass,
                ) {
                    // 如果比较失败则回退到简单消息
                    println!(
                        "│ {}",
                        format!("Failed to display comparison: {}", e).yellow()
                    );
                    println!("│ {}", success_message.bright_green().bold());
                }

                // 获取用户选择
                let choice = interaction::prompt_compile_success_choice()?;

                match choice {
                    interaction::CompileSuccessChoice::Accept => {
                        println!("│ {}", "You chose: Accept this code".bright_cyan());
                        // 继续提交
                    }
                    interaction::CompileSuccessChoice::AutoAccept => {
                        println!(
                            "│ {}",
                            "You chose: Auto-accept all subsequent translations".bright_cyan()
                        );
                        interaction::enable_auto_accept_mode();
                        // 继续提交
                    }
                    interaction::CompileSuccessChoice::ManualFix => {
                        println!("│ {}", "You chose: Manual fix".bright_cyan());

                        // 打开 vim 进行手动编辑
                        match interaction::open_in_vim(rs_file) {
                            Ok(_) => {
                                // 编辑后，执行完整构建和测试
                                println!(
                                    "│ {}",
                                    "Running full build and test after manual changes..."
                                        .bright_blue()
                                );
                                builder::run_full_build_and_test_interactive(
                                    feature, file_type, rs_file,
                                )?;
                                println!(
                                    "│ {}",
                                    "✓ All builds and tests pass after manual changes"
                                        .bright_green()
                                );
                                // 继续提交
                            }
                            Err(e) => {
                                return Err(e).context("Failed to open vim for manual editing");
                            }
                        }
                    }
                    interaction::CompileSuccessChoice::Exit => {
                        println!("│ {}", "You chose: Exit".yellow());
                        anyhow::bail!("User chose to exit after successful tests");
                    }
                }
            } else {
                println!(
                    "│ {}",
                    "Auto-accept mode: automatically accepting translation".bright_green()
                );
            }
        }
        Err(test_error) => {
            // 测试失败 - 使用交互式处理器
            let processing_complete =
                builder::handle_test_failure_interactive(feature, file_type, rs_file, test_error)?;
            if !processing_complete {
                // User chose to retry translation
                return Ok(false);
            }
        }
    }

    // 提交更改
    println!("│");
    println!("│ {}", format_progress("Commit").bright_magenta().bold());
    println!("│ {}", "Committing changes...".bright_blue());
    git::git_commit(
        &format!(
            "Translate {} from C to Rust (feature: {})",
            file_name, feature
        ),
        feature,
    )?;
    println!("│ {}", "✓ Changes committed".bright_green());

    // 更新代码分析
    println!("│");
    println!(
        "│ {}",
        format_progress("Update Analysis").bright_magenta().bold()
    );
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // 提交分析
    println!("│");
    println!(
        "│ {}",
        format_progress("Commit Analysis").bright_magenta().bold()
    );
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

    println!("{}", "└─ File processing complete".bright_white().bold());

    Ok(true)
}

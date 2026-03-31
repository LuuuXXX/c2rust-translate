use crate::analyzer;
use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::process::Command;
use std::time::Instant;

/// 统一的 cargo check 函数
///
/// # 参数
/// - `feature`: 特性名称
/// - `suppress_warnings`: true=抑制警告(-A warnings), false=显示警告
/// - `_show_full_output`: 保留参数，暂未实现，传入值不影响当前输出行为
///
/// # 返回
/// - `Ok(None)`: 检查成功且无警告（或警告被抑制）
/// - `Ok(Some(warnings))`: 检查成功但有警告
/// - `Err`: 检查失败
pub fn cargo_check(
    feature: &str,
    suppress_warnings: bool,
    _show_full_output: bool,
) -> Result<Option<String>> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust").join(feature).join("rust");

    let start_time = Instant::now();

    let mut cmd = Command::new("cargo");
    cmd.arg("check").current_dir(&build_dir);
    // Required because translated Rust code may use unstable (nightly-only) features.
    cmd.env("RUSTC_BOOTSTRAP", "1");

    if suppress_warnings {
        cmd.env("RUSTFLAGS", "-A warnings");
    }

    let output = cmd.output().context("Failed to execute cargo check")?;
    let duration = start_time.elapsed();

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        anyhow::bail!("Check error: {}", stderr);
    }

    println!(
        "  {} (took {:.2}s)",
        "Check completed".bright_green(),
        duration.as_secs_f64()
    );

    if !suppress_warnings {
        let has_warnings = stderr
            .lines()
            .any(|line| line.contains("warning[") || line.contains("warning:"));

        if has_warnings {
            return Ok(Some(stderr));
        }
    }

    Ok(None)
}

/// 内部辅助函数：执行 cargo build 以生成构建产物（如静态库）
///
/// 该函数用于需要真正构建产物（而非仅做类型检查）的场景，例如混合链接前的静态库构建。
pub(crate) fn cargo_build_internal(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust").join(feature).join("rust");
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(&build_dir)
        .env("RUSTC_BOOTSTRAP", "1")
        .env("RUSTFLAGS", "-A warnings")
        .output()
        .context("Failed to execute cargo build")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        anyhow::bail!("Build error: {}", stderr);
    }
    Ok(())
}

/// 从 c2rust-config 获取特定的配置值
pub(crate) fn get_config_value(key: &str, feature: &str) -> Result<String> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");

    let output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args(["config", "--make", "--feature", feature, "--list", key])
        .output()
        .with_context(|| format!("Failed to get {} from config", key))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to retrieve {}: {}", key, stderr);
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if value.is_empty() {
        anyhow::bail!("Empty {} value from config", key);
    }

    Ok(value)
}

/// 如果启用了 LD_PRELOAD，则设置混合构建环境变量
fn setup_hybrid_env(
    command: &mut Command,
    project_root: &std::path::Path,
    feature: &str,
    set_ld_preload: bool,
    build_target: Option<&str>,
) -> Option<std::path::PathBuf> {
    if !set_ld_preload {
        return None;
    }

    let hybrid_lib = env::var("C2RUST_HYBRID_BUILD_LIB").ok()?;
    let c2rust_dir = project_root.join(".c2rust");
    let feature_root_path = c2rust_dir.join(feature);
    let rust_lib_path = feature_root_path
        .join("rust")
        .join("target")
        .join("debug")
        .join("librust.a");

    command.env("LD_PRELOAD", &hybrid_lib);
    command.env("C2RUST_PROJECT_ROOT", project_root);
    command.env("C2RUST_FEATURE_ROOT", &feature_root_path);
    command.env("C2RUST_RUST_LIB", &rust_lib_path);

    // 如果提供了 build.target，则设置 C2RUST_LD_TARGET
    if let Some(target) = build_target {
        command.env("C2RUST_LD_TARGET", target);
    }

    Some(feature_root_path)
}

/// 打印命令执行详情
fn print_command_details(
    command_type: &str,
    parts: &[String],
    exec_dir: &std::path::Path,
    project_root: &std::path::Path,
    feature_root: Option<&std::path::PathBuf>,
    build_target: Option<&str>,
    set_ld_preload: bool,
) {
    let colored_label = match command_type {
        "build" => "│ → Executing build command:".bright_blue().to_string(),
        "test" => "│ → Executing test command:".bright_green().to_string(),
        "clean" => "│ → Executing clean command:".bright_red().to_string(),
        _ => format!("│ → Executing {} command:", command_type),
    };

    println!("{}", colored_label);
    print!("│   ");

    if set_ld_preload {
        if let Ok(hybrid_lib) = env::var("C2RUST_HYBRID_BUILD_LIB") {
            let rust_lib_path = feature_root
                .map(|f| {
                    f.join("rust")
                        .join("target")
                        .join("debug")
                        .join("librust.a")
                })
                .unwrap_or_default();

            print!("LD_PRELOAD={} ", shell_words::quote(&hybrid_lib).dimmed());
            if let Some(feature_root) = feature_root {
                print!(
                    "C2RUST_FEATURE_ROOT={} ",
                    shell_words::quote(&feature_root.display().to_string()).dimmed()
                );
            }
            print!(
                "C2RUST_PROJECT_ROOT={} ",
                shell_words::quote(&project_root.display().to_string()).dimmed()
            );
            print!(
                "C2RUST_RUST_LIB={} ",
                shell_words::quote(&rust_lib_path.display().to_string()).dimmed()
            );

            // 如果提供了 build.target，则显示 C2RUST_LD_TARGET
            if let Some(target) = build_target {
                print!("C2RUST_LD_TARGET={} ", shell_words::quote(target).dimmed());
            }
        }
    }

    println!("{}", shell_words::join(parts).bright_yellow());
    println!(
        "│   {}: {}",
        "Working directory".dimmed(),
        exec_dir.display()
    );
}

/// 在配置的目录中执行命令
/// 此函数被 hybrid_build 模块使用，因此是公开的
pub fn execute_command_in_dir_with_type(
    command_str: &str,
    dir_key: &str,
    feature: &str,
    set_ld_preload: bool,
    command_type: &str,
) -> Result<()> {
    util::validate_feature_name(feature)?;

    let dir_str = get_config_value(dir_key, feature)?;

    // 验证路径安全性
    if std::path::Path::new(&dir_str).is_absolute() {
        anyhow::bail!(
            "Directory path from config must be relative, got: {}",
            dir_str
        );
    }
    if dir_str.contains("..") {
        anyhow::bail!(
            "Directory path from config cannot contain '..', got: {}",
            dir_str
        );
    }

    let parts = shell_words::split(command_str)
        .with_context(|| format!("Failed to parse command: {}", command_str))?;

    if parts.is_empty() {
        return Ok(());
    }

    if parts[0].is_empty() {
        anyhow::bail!("Command cannot be empty");
    }

    let project_root = util::find_project_root()?;
    let exec_dir = project_root.join(&dir_str);

    if !exec_dir.exists() {
        anyhow::bail!("Directory does not exist: {}", exec_dir.display());
    } else if !exec_dir.is_dir() {
        anyhow::bail!("Path is not a directory: {}", exec_dir.display());
    }

    // 一次性获取 build.target 用于环境设置和打印
    // 区分"未设置"（检查为空的 Ok）和实际错误
    let build_target = match get_config_value("build.target", feature) {
        Ok(target) if !target.is_empty() => Some(target),
        Ok(_) => None, // 空值表示未设置
        Err(e) => {
            // 检查这是否只是"未找到键"错误还是真正的失败
            let err_str = e.to_string();
            if err_str.contains("Empty") || err_str.contains("not found") {
                None // 未设置键是可接受的
            } else {
                // 真正的配置错误 - 发出警告但继续
                eprintln!("Warning: Failed to read build.target from config: {}", e);
                None
            }
        }
    };

    let mut command = Command::new(&parts[0]);
    command.current_dir(&exec_dir);

    if parts.len() > 1 {
        command.args(&parts[1..]);
    }

    let feature_root = setup_hybrid_env(
        &mut command,
        &project_root,
        feature,
        set_ld_preload,
        build_target.as_deref(),
    );
    print_command_details(
        command_type,
        &parts,
        &exec_dir,
        &project_root,
        feature_root.as_ref(),
        build_target.as_deref(),
        set_ld_preload,
    );

    let start_time = Instant::now();
    let output = command
        .output()
        .with_context(|| format!("Failed to execute command: {}", command_str))?;
    let duration = start_time.elapsed();

    if !output.status.success() {
        print_command_failure(command_type, &output, duration);

        // 在 bail 消息中包含错误详情以便更好地调试
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_summary = stderr.lines().take(3).collect::<Vec<_>>().join("\n");

        if stderr_summary.is_empty() {
            anyhow::bail!("Command '{}' failed with non-zero exit status", command_str);
        } else {
            anyhow::bail!(
                "Command '{}' failed with non-zero exit status. Stderr (first lines):\n{}",
                command_str,
                stderr_summary
            );
        }
    }

    print_command_success(command_type, duration);
    Ok(())
}

/// 打印命令失败消息
fn print_command_failure(
    command_type: &str,
    output: &std::process::Output,
    duration: std::time::Duration,
) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!(
        "│ {} (took {:.2}s)",
        format!("✗ {} failed", command_type.to_uppercase())
            .bright_red()
            .bold(),
        duration.as_secs_f64()
    );

    if !stderr.is_empty() {
        eprintln!("stderr: {}", stderr);
    }
    if !stdout.is_empty() {
        println!("stdout: {}", stdout);
    }
}

/// 打印命令成功消息
fn print_command_success(command_type: &str, duration: std::time::Duration) {
    let success_msg = match command_type {
        "build" => format!(
            "│ {} (took {:.2}s)",
            "✓ Build successful".bright_green().bold(),
            duration.as_secs_f64()
        ),
        "test" => format!(
            "│ {} (took {:.2}s)",
            "✓ Test successful".bright_green().bold(),
            duration.as_secs_f64()
        ),
        "clean" => format!(
            "│ {} (took {:.2}s)",
            "✓ Clean successful".bright_green().bold(),
            duration.as_secs_f64()
        ),
        _ => format!(
            "│ ✓ {} successful (took {:.2}s)",
            command_type,
            duration.as_secs_f64()
        ),
    };
    println!("{}", success_msg);
}

/// 为给定特性运行清理命令
pub fn c2rust_clean(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    let clean_cmd = get_config_value("clean.cmd", feature)?;

    execute_command_in_dir_with_type(&clean_cmd, "clean.dir", feature, false, "clean")
}

/// 为给定特性运行构建命令
/// 如果设置了 C2RUST_HYBRID_BUILD_LIB，则自动检测并设置 LD_PRELOAD
pub fn c2rust_build(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    println!("{}", "Rebuilding Rust static library for hybrid link...".bright_blue());
    cargo_build_internal(feature)?;
    println!("{}", "✓ Rust static library refreshed".bright_green());

    let build_cmd = get_config_value("build.cmd", feature)?;

    execute_command_in_dir_with_type(&build_cmd, "build.dir", feature, true, "build")
}

/// 为给定特性运行测试命令
pub fn c2rust_test(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    let test_cmd = get_config_value("test.cmd", feature)?;

    execute_command_in_dir_with_type(&test_cmd, "test.dir", feature, false, "test")
}

/// 运行混合构建测试套件
/// 如果 c2rust-config 不可用，则报告错误并退出
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    run_hybrid_build_interactive(feature, None, None)
}

/// 通过交互式错误处理运行混合构建测试套件
/// 交互式错误处理需要 file_type 和 rs_file
pub fn run_hybrid_build_interactive(
    feature: &str,
    file_type: Option<&str>,
    rs_file: Option<&std::path::Path>,
) -> Result<()> {
    // 从配置获取构建命令
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
    let check_output = Command::new("c2rust-config").arg("--version").output();

    if check_output.is_err() {
        eprintln!("{}", "Error: c2rust-config not found".red());
        anyhow::bail!("c2rust-config not found, cannot run hybrid build tests");
    }

    // 执行命令
    println!("│ {}", "Running hybrid build tests...".bright_blue().bold());
    c2rust_clean(feature)?;

    // 通过交互式错误处理进行构建
    match c2rust_build(feature) {
        Ok(_) => {
            // 构建成功，继续测试
        }
        Err(build_error) => {
            // 仅当我们有文件上下文时才显示交互菜单
            if let (Some(ftype), Some(rfile)) = (file_type, rs_file) {
                let processing_complete =
                    // run_hybrid_build_interactive is not part of the translation loop,
                    // so skip_test=false: tests always run in this standalone context.
                    handle_build_failure_interactive(feature, ftype, rfile, build_error, false)?;
                if !processing_complete {
                    // User chose to retry translation - not supported in this context
                    // so treat it as a failure and return early
                    println!(
                        "│ {}",
                        "Note: Retry translation requested but not supported in this context"
                            .yellow()
                    );
                    return Err(anyhow::anyhow!(
                        "Hybrid build failed and retry translation is not supported in this context"
                    ));
                }
            } else {
                // 没有文件上下文，只返回错误
                return Err(build_error);
            }
        }
    }

    // 通过交互式错误处理进行测试
    match c2rust_test(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());
            Ok(())
        }
        Err(test_error) => {
            if crate::should_continue_on_test_error() {
                println!(
                    "│ {}",
                    format!(
                        "⚠ Tests failed (continuing due to C2RUST_TEST_CONTINUE_ON_ERROR): {:#}",
                        test_error
                    )
                    .yellow()
                );
                return Ok(());
            }
            // Only show interactive menu when we have file context
            if let (Some(ftype), Some(rfile)) = (file_type, rs_file) {
                let processing_complete =
                    // run_hybrid_build_interactive is not part of the translation loop,
                    // so skip_test=false: tests always run in this standalone context.
                    handle_test_failure_interactive(feature, ftype, rfile, test_error, false)?;
                if !processing_complete {
                    // User chose to retry translation - not supported in this context
                    // so treat it as a failure and return early
                    println!(
                        "│ {}",
                        "Note: Retry translation requested but not supported in this context"
                            .yellow()
                    );
                    return Err(anyhow::anyhow!(
                        "Hybrid build tests failed and retry translation is not supported in this context"
                    ));
                }
                Ok(())
            } else {
                // No file context: just return the error
                Err(test_error)
            }
        }
    }
}

/// 收集手动修复所需的文件列表
///
/// 解析错误消息以获取所有涉及的文件，确保 rs_file 始终包含在列表中。
/// 返回文件列表，rs_file 始终在第一位（如果不已存在）。
/// 解析失败时回退到只包含 rs_file 的列表。
pub(crate) fn get_manual_fix_files(
    feature: &str,
    rs_file: &std::path::Path,
    error_str: &str,
) -> Vec<std::path::PathBuf> {
    let mut files = match crate::error_handler::parse_error_for_files(error_str, feature) {
        Ok(parsed) => parsed,
        Err(parse_err) => {
            eprintln!(
                "[debug] Failed to parse error for related files (feature: {}): {parse_err}",
                feature
            );
            Vec::new()
        }
    };

    // 规范化 rs_file 以进行比较（parse_error_for_files 返回的路径也是规范化的）
    let canonical_rs = rs_file.canonicalize().ok();

    // 如果 rs_file 不在列表中，则添加到列表首位
    let already_present = match &canonical_rs {
        Some(c) => files.contains(c),
        None => files.iter().any(|f| f == rs_file),
    };

    if !already_present {
        let to_insert = canonical_rs
            .clone()
            .unwrap_or_else(|| rs_file.to_path_buf());
        files.insert(0, to_insert);
    }

    files
}

/// 交互式处理构建失败
/// Handles build failures in hybrid build phase interactively
///
/// Returns:
/// - Ok(true) if the build failure was resolved (continue processing)
/// - Ok(false) if translation should be retried from scratch
/// - Err if an unrecoverable error occurred
pub(crate) fn handle_build_failure_interactive(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    build_error: anyhow::Error,
    skip_test: bool,
) -> Result<bool> {
    use crate::diff_display;
    use crate::interaction;
    use crate::suggestion;

    println!("│");
    println!("│ {}", "⚠ Build failed!".red().bold());
    println!("│ {}", "The build process did not succeed.".yellow());

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
        use crate::translator;
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
    let choice = interaction::prompt_build_failure_choice()?;

    match choice {
        interaction::FailureChoice::RetryDirectly => {
            println!("│");
            println!(
                "│ {}",
                "You chose: Retry directly without suggestion".bright_cyan()
            );

            // 清除旧建议
            suggestion::clear_suggestions()?;

            println!("│ {}", "Retrying translation from scratch...".bright_cyan());
            println!(
                "│ {}",
                "Note: The translator will overwrite the existing file content.".bright_blue()
            );
            println!("│ {}", "✓ Retry scheduled".bright_green());

            // 返回 false 以信号重试翻译
            Ok(false)
        }
        interaction::FailureChoice::AddSuggestion => {
            println!("│");
            println!(
                "│ {}",
                "You chose: Add fix suggestion for AI to modify".bright_cyan()
            );

            // 跟踪重试中最新的构建错误以避免递归
            let mut current_error = build_error;

            loop {
                // 在提示新建议之前清除旧建议
                suggestion::clear_suggestions()?;

                // 对于构建失败，建议是必需的
                let suggestion_text = interaction::prompt_suggestion(true)?
                    .ok_or_else(|| anyhow::anyhow!(
                        "Suggestion is required for build failure but none was provided. \
                         This may indicate an issue with the prompt_suggestion function when require_input=true."
                    ))?;

                // 将建议保存到 suggestions.txt
                suggestion::append_suggestion(&suggestion_text)?;

                // 应用带有建议的修复
                println!("│");
                println!(
                    "│ {}",
                    "Applying fix based on your suggestion...".bright_blue()
                );

                let format_progress = |op: &str| format!("Fix for build failure - {}", op);
                crate::apply_error_fix(
                    feature,
                    file_type,
                    rs_file,
                    &current_error,
                    &format_progress,
                    true,
                )?;

                // 再次尝试构建和测试
                println!("│");
                println!(
                    "│ {}",
                    "Running full build and test...".bright_blue().bold()
                );

                match run_full_build_and_test_interactive(feature, file_type, rs_file, skip_test) {
                    Ok(_) => {
                        return Ok(true);
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Build or tests still failing".red());

                        // 使用最新失败更新 current_error
                        current_error = e;

                        // 询问用户是否想再试一次
                        println!("│");
                        println!(
                            "│ {}",
                            "Build or tests still have errors. What would you like to do?".yellow()
                        );
                        let retry_choice = interaction::prompt_build_failure_choice()?;

                        match retry_choice {
                            interaction::FailureChoice::RetryDirectly => {
                                println!("│ {}", "Switching to retry translation flow.".yellow());
                                suggestion::clear_suggestions()?;
                                return Ok(false);
                            }
                            interaction::FailureChoice::AddSuggestion => {
                                // 继续循环以使用新建议重试
                                continue;
                            }
                            interaction::FailureChoice::ManualFix => {
                                println!("│");
                                println!("│ {}", "You chose: Manually edit the code".bright_cyan());
                                println!("│ {}", "Opening vim for manual fixes...".bright_blue());

                                // 打开 vim 允许用户手动编辑代码（支持多文件选择）
                                let fix_files = get_manual_fix_files(
                                    feature,
                                    rs_file,
                                    &current_error.to_string(),
                                );
                                match interaction::open_files_for_manual_fix(&fix_files) {
                                    Ok(_) => {
                                        println!("│");
                                        println!(
                                            "│ {}",
                                            "Running full build and test after manual fix..."
                                                .bright_blue()
                                                .bold()
                                        );

                                        // 执行完整构建流程（包含 cargo_check）
                                        match run_full_build_and_test_interactive(
                                            feature, file_type, rs_file, skip_test,
                                        ) {
                                            Ok(_) => {
                                                return Ok(true);
                                            }
                                            Err(e) => {
                                                println!("│ {}", "✗ Build or tests still failing after manual fix".red());

                                                // 询问用户是否想再试一次
                                                println!("│");
                                                println!("│ {}", "Build or tests still have errors. What would you like to do?".yellow());
                                                let nested_retry_choice =
                                                    interaction::prompt_build_failure_choice()?;

                                                match nested_retry_choice {
                                                    interaction::FailureChoice::RetryDirectly => {
                                                        println!(
                                                            "│ {}",
                                                            "Switching to retry translation flow."
                                                                .yellow()
                                                        );
                                                        suggestion::clear_suggestions()?;
                                                        return Ok(false);
                                                    }
                                                    interaction::FailureChoice::AddSuggestion => {
                                                        // 更新 current_error 并继续外部循环以使用新建议重试
                                                        current_error = e;
                                                        continue;
                                                    }
                                                    interaction::FailureChoice::ManualFix => {
                                                        // 重新打开 vim
                                                        println!("│ {}", "Reopening Vim for another manual fix attempt...".bright_blue());
                                                        let fix_files = get_manual_fix_files(feature, rs_file, &e.to_string());
                                                        interaction::open_files_for_manual_fix(&fix_files)
                                                            .context("Failed to reopen vim for additional manual fix")?;
                                                        // 更新错误并继续外部循环以重新构建
                                                        current_error = e;
                                                        continue;
                                                    }
                                                    interaction::FailureChoice::Skip
                                                    | interaction::FailureChoice::FixOtherFile => {
                                                        unreachable!(
                                                            "Skip and FixOtherFile are not offered in this context"
                                                        )
                                                    }
                                                    interaction::FailureChoice::Exit => {
                                                        return Err(e).context("Build failed after manual fix and user chose to exit");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(open_err) => {
                                        println!(
                                            "│ {}",
                                            format!("Failed to open vim: {}", open_err).red()
                                        );
                                        println!(
                                            "│ {}",
                                            "Cannot continue manual fix flow; exiting.".yellow()
                                        );
                                        return Err(open_err).context(
                                            "Build failed and could not open vim for manual fix",
                                        );
                                    }
                                }
                            }
                            interaction::FailureChoice::Skip
                            | interaction::FailureChoice::FixOtherFile => {
                                unreachable!("Skip and FixOtherFile are not offered in this context")
                            }
                            interaction::FailureChoice::Exit => {
                                return Err(current_error)
                                    .context("Build failed and user chose to exit");
                            }
                        }
                    }
                }
            }
        }
        interaction::FailureChoice::ManualFix => {
            println!("│");
            println!("│ {}", "You chose: Manual fix".bright_cyan());

            // 尝试打开 vim
            let fix_files = get_manual_fix_files(feature, rs_file, &build_error.to_string());
            match interaction::open_files_for_manual_fix(&fix_files) {
                Ok(_) => {
                    loop {
                        println!("│");
                        println!(
                            "│ {}",
                            "Vim editing completed. Running full build and test...".bright_blue()
                        );

                        // Vim 编辑后尝试使用混合构建流程进行构建和测试
                        match run_full_build_and_test_interactive(feature, file_type, rs_file, skip_test) {
                            Ok(_) => {
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
                                    "Build or tests still have errors. What would you like to do?"
                                        .yellow()
                                );
                                let retry_choice = interaction::prompt_build_failure_choice()?;

                                match retry_choice {
                                    interaction::FailureChoice::RetryDirectly => {
                                        println!(
                                            "│ {}",
                                            "Switching to retry translation flow.".yellow()
                                        );
                                        suggestion::clear_suggestions()?;
                                        return Ok(false);
                                    }
                                    interaction::FailureChoice::ManualFix => {
                                        println!(
                                            "│ {}",
                                            "Reopening Vim for another manual fix attempt..."
                                                .bright_blue()
                                        );
                                        let fix_files = get_manual_fix_files(feature, rs_file, &e.to_string());
                                        interaction::open_files_for_manual_fix(&fix_files).context(
                                            "Failed to reopen vim for additional manual fix",
                                        )?;
                                        // Vim 关闭后，继续循环重新构建和重新测试
                                        continue;
                                    }
                                    interaction::FailureChoice::AddSuggestion => {
                                        println!(
                                            "│ {}",
                                            "Switching to suggestion-based fix flow.".yellow()
                                        );
                                        // 递归调用以进入基于建议的交互式修复流程
                                        return handle_build_failure_interactive(
                                            feature, file_type, rs_file, e, skip_test,
                                        );
                                    }
                                    interaction::FailureChoice::Skip
                                    | interaction::FailureChoice::FixOtherFile => {
                                        unreachable!("Skip and FixOtherFile are not offered in this context")
                                    }
                                    interaction::FailureChoice::Exit => {
                                        return Err(e).context(
                                            "Build failed after manual fix and user chose to exit",
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(e).context(format!(
                        "Build failed (original error: {}) and could not open vim",
                        build_error
                    ))
                }
            }
        }
        interaction::FailureChoice::Skip
        | interaction::FailureChoice::FixOtherFile => {
            unreachable!("Skip and FixOtherFile are not offered in this context")
        }
        interaction::FailureChoice::Exit => {
            println!("│");
            println!("│ {}", "You chose: Exit".yellow());
            println!("│ {}", "Exiting due to build failures.".yellow());
            Err(build_error).context("Build failed and user chose to exit")
        }
    }
}

/// 交互式处理测试失败
/// Handles test failures interactively
///
/// Returns:
/// - Ok(true) if the test failure was resolved (continue processing)
/// - Ok(false) if translation should be retried from scratch
/// - Err if an unrecoverable error occurred
pub(crate) fn handle_test_failure_interactive(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    test_error: anyhow::Error,
    skip_test: bool,
) -> Result<bool> {
    use crate::diff_display;
    use crate::interaction;
    use crate::suggestion;

    println!("│");
    println!("│ {}", "⚠ Hybrid build tests failed!".red().bold());
    println!("│ {}", "The test suite did not pass.".yellow());

    // 显示代码比较和测试错误
    let c_file = rs_file.with_extension("c");

    // 显示文件位置
    interaction::display_file_paths(Some(&c_file), rs_file);

    // 使用差异显示进行更好的比较
    let error_message = format!("✗ Test Error:\n{}", test_error);
    if let Err(e) = diff_display::display_code_comparison(
        &c_file,
        rs_file,
        &error_message,
        diff_display::ResultType::TestFail,
    ) {
        // 如果比较失败则回退到旧显示
        use crate::translator;
        println!(
            "│ {}",
            format!("Failed to display comparison: {}", e).yellow()
        );
        println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
        translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);

        println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
        translator::display_code(rs_file, "─ Rust Code ─", usize::MAX, true);

        println!("│ {}", "═══ Test Error ═══".bright_red().bold());
        println!("│ {}", test_error);
    }

    // 使用新提示获取用户选择
    let choice = interaction::prompt_test_failure_choice()?;

    match choice {
        interaction::FailureChoice::RetryDirectly => {
            println!("│");
            println!(
                "│ {}",
                "You chose: Retry directly without suggestion".bright_cyan()
            );

            crate::verification::display_retry_directly_warning();

            // 清除旧建议
            suggestion::clear_suggestions()?;

            println!("│ {}", "Retrying translation from scratch...".bright_cyan());
            println!(
                "│ {}",
                "Note: The translator will overwrite the existing file content.".bright_blue()
            );
            println!("│ {}", "✓ Retry scheduled".bright_green());

            // 返回 false 以信号重试翻译
            Ok(false)
        }
        interaction::FailureChoice::AddSuggestion => {
            println!("│");
            println!(
                "│ {}",
                "You chose: Add fix suggestion for AI to modify".bright_cyan()
            );

            // 跟踪重试中最新的测试错误以避免递归
            let mut current_error = test_error;

            loop {
                // 在提示新建议之前清除旧建议
                suggestion::clear_suggestions()?;

                // 对于测试失败，建议是必需的
                let suggestion_text = interaction::prompt_suggestion(true)?
                    .ok_or_else(|| anyhow::anyhow!(
                        "Suggestion is required for test failure but none was provided. \
                         This may indicate an issue with the prompt_suggestion function when require_input=true."
                    ))?;

                // 将建议保存到 suggestions.txt
                suggestion::append_suggestion(&suggestion_text)?;

                // 应用带有建议的修复
                println!("│");
                println!(
                    "│ {}",
                    "Applying fix based on your suggestion...".bright_blue()
                );

                let format_progress = |op: &str| format!("Fix for test failure - {}", op);
                crate::apply_error_fix(
                    feature,
                    file_type,
                    rs_file,
                    &current_error,
                    &format_progress,
                    true,
                )?;

                // 再次尝试构建和测试
                println!("│");
                println!(
                    "│ {}",
                    "Running full build and test...".bright_blue().bold()
                );

                match run_full_build_and_test_interactive(feature, file_type, rs_file, skip_test) {
                    Ok(_) => {
                        return Ok(true);
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Tests still failing".red());

                        // 使用最新失败更新 current_error
                        current_error = e;

                        // 询问用户是否想再试一次
                        println!("│");
                        println!(
                            "│ {}",
                            "Tests still have errors. What would you like to do?".yellow()
                        );
                        let retry_choice = interaction::prompt_test_failure_choice()?;

                        match retry_choice {
                            interaction::FailureChoice::RetryDirectly => {
                                println!("│ {}", "Switching to retry translation flow.".yellow());
                                suggestion::clear_suggestions()?;
                                return Ok(false);
                            }
                            interaction::FailureChoice::AddSuggestion => {
                                // 继续循环以使用新建议重试
                                continue;
                            }
                            interaction::FailureChoice::ManualFix => {
                                println!("│");
                                println!("│ {}", "You chose: Manually edit the code".bright_cyan());
                                println!("│ {}", "Opening vim for manual fixes...".bright_blue());

                                // 打开 vim 允许用户手动编辑代码
                                let fix_files = get_manual_fix_files(
                                    feature,
                                    rs_file,
                                    &current_error.to_string(),
                                );
                                match interaction::open_files_for_manual_fix(&fix_files) {
                                    Ok(_) => {
                                        println!("│");
                                        println!(
                                            "│ {}",
                                            "Running full build and test after manual fix..."
                                                .bright_blue()
                                                .bold()
                                        );

                                        match run_full_build_and_test_interactive(
                                            feature, file_type, rs_file, skip_test,
                                        ) {
                                            Ok(_) => {
                                                return Ok(true);
                                            }
                                            Err(e) => {
                                                println!(
                                                    "│ {}",
                                                    "✗ Tests still failing after manual fix".red()
                                                );
                                                // 更新 current_error 并继续外部循环
                                                current_error = e;
                                                continue;
                                            }
                                        }
                                    }
                                    Err(open_err) => {
                                        println!(
                                            "│ {}",
                                            format!("Failed to open vim: {}", open_err).red()
                                        );
                                        println!(
                                            "│ {}",
                                            "Cannot continue manual fix flow; exiting.".yellow()
                                        );
                                        return Err(open_err).context(
                                            "Tests failed and could not open vim for manual fix",
                                        );
                                    }
                                }
                            }
                            interaction::FailureChoice::Skip
                            | interaction::FailureChoice::FixOtherFile => {
                                unreachable!("Skip and FixOtherFile are not offered in this context")
                            }
                            interaction::FailureChoice::Exit => {
                                return Err(current_error)
                                    .context("Tests failed and user chose to exit");
                            }
                        }
                    }
                }
            }
        }
        interaction::FailureChoice::ManualFix => {
            println!("│");
            println!("│ {}", "You chose: Manual fix".bright_cyan());

            // 尝试打开 vim
            let fix_files = get_manual_fix_files(feature, rs_file, &test_error.to_string());
            match interaction::open_files_for_manual_fix(&fix_files) {
                Ok(_) => {
                    loop {
                        println!("│");
                        println!(
                            "│ {}",
                            "Vim editing completed. Running full build and test...".bright_blue()
                        );

                        // Vim 编辑后尝试使用混合构建流程进行构建和测试
                        match run_full_build_and_test_interactive(feature, file_type, rs_file, skip_test) {
                            Ok(_) => {
                                return Ok(true);
                            }
                            Err(e) => {
                                println!("│ {}", "✗ Tests still failing after manual fix".red());

                                // 询问用户是否想再试一次
                                println!("│");
                                println!(
                                    "│ {}",
                                    "Tests still have errors. What would you like to do?".yellow()
                                );
                                let retry_choice = interaction::prompt_test_failure_choice()?;

                                match retry_choice {
                                    interaction::FailureChoice::RetryDirectly => {
                                        println!(
                                            "│ {}",
                                            "Switching to retry translation flow.".yellow()
                                        );
                                        suggestion::clear_suggestions()?;
                                        return Ok(false);
                                    }
                                    interaction::FailureChoice::ManualFix => {
                                        println!(
                                            "│ {}",
                                            "Reopening Vim for another manual fix attempt..."
                                                .bright_blue()
                                        );
                                        let fix_files = get_manual_fix_files(feature, rs_file, &e.to_string());
                                        interaction::open_files_for_manual_fix(&fix_files).context(
                                            "Failed to reopen vim for additional manual fix",
                                        )?;
                                        // Vim 关闭后，继续循环重新构建和重新测试
                                        continue;
                                    }
                                    interaction::FailureChoice::AddSuggestion => {
                                        println!(
                                            "│ {}",
                                            "Switching to suggestion-based fix flow.".yellow()
                                        );
                                        return Err(e).context("Tests still failing after manual fix; user chose to add a suggestion");
                                    }
                                    interaction::FailureChoice::Skip
                                    | interaction::FailureChoice::FixOtherFile => {
                                        unreachable!("Skip and FixOtherFile are not offered in this context")
                                    }
                                    interaction::FailureChoice::Exit => {
                                        return Err(e).context(
                                            "Tests failed after manual fix and user chose to exit",
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(e).context(format!(
                        "Tests failed (original error: {}) and could not open vim",
                        test_error
                    ))
                }
            }
        }
        interaction::FailureChoice::Skip
        | interaction::FailureChoice::FixOtherFile => {
            unreachable!("Skip and FixOtherFile are not offered in this context")
        }
        interaction::FailureChoice::Exit => {
            println!("│");
            println!("│ {}", "You chose: Exit".yellow());
            println!("│ {}", "Exiting due to test failures.".yellow());
            Err(test_error).context("Tests failed and user chose to exit")
        }
    }
}

/// 执行完整的构建和测试流程
/// 顺序：c2rust_clean → c2rust_build → c2rust_test
/// 每个步骤内部均会调用 update_code_analysis；这是主流程中的标准验证流程
pub fn run_full_build_and_test(feature: &str) -> Result<()> {
    // This entry point always runs tests; skip_test=false.
    run_full_build_and_test_interactive(feature, "", std::path::Path::new(""), false)
}

/// 执行完整的构建和测试流程
/// 顺序：c2rust_clean → c2rust_build → c2rust_test
///
/// 三个步骤各自内部均会调用 `analyzer::update_code_analysis`。
/// 任何步骤失败时直接返回错误，并打印详细的错误信息。
/// 调用方负责处理错误并提供交互式修复选项（如需要）。
///
/// 参数 `_file_type` 和 `_rs_file` 保留用于 API 兼容性，当前未使用。
/// 参数 `skip_test` 为 true 时跳过测试阶段（clean、build、test 中的 test）。
pub fn run_full_build_and_test_interactive(
    feature: &str,
    _file_type: &str,
    _rs_file: &std::path::Path,
    skip_test: bool,
) -> Result<()> {
    println!("│");
    println!(
        "│ {}",
        "Running full build and test flow...".bright_blue().bold()
    );

    // 1. 清理混合构建环境（内部会更新代码分析）
    println!("│ {}", "→ Step 1/3: Cleaning hybrid build...".bright_blue());
    match c2rust_clean(feature) {
        Ok(_) => {
            println!("│ {}", "  ✓ Clean successful".bright_green());
        }
        Err(e) => {
            println!("│ {}", "  ✗ Clean failed".red());
            println!("│");
            println!("│ {}", "Error details:".red().bold());
            println!("│ {}", format!("{:#}", e).red());
            println!("│");
            return Err(e).context("Clean failed in full build flow");
        }
    }

    // 2. 混合构建（内部会更新代码分析 + 执行 cargo build 生成 librust.a）
    println!(
        "│ {}",
        "→ Step 2/3: Running hybrid build (C + Rust)...".bright_blue()
    );
    match c2rust_build(feature) {
        Ok(_) => {
            println!("│ {}", "  ✓ Hybrid build successful".bright_green());
        }
        Err(e) => {
            println!("│ {}", "  ✗ Hybrid build failed".red());
            println!("│");
            println!("│ {}", "Error details:".red().bold());
            println!("│ {}", format!("{:#}", e).red());
            println!("│");
            return Err(e).context("Hybrid build failed in full build flow");
        }
    }

    // 3. 运行测试（不调用交互式处理器以避免递归）
    if skip_test {
        println!(
            "│ {}",
            "⚠ Skipping test phase (test configuration not available)".yellow()
        );
    } else {
        println!("│ {}", "→ Step 3/3: Running tests...".bright_blue());
        match c2rust_test(feature) {
            Ok(_) => {
                println!("│ {}", "  ✓ All tests passed".bright_green().bold());
            }
            Err(e) => {
                println!("│ {}", "  ✗ Tests failed".red());
                println!("│");
                println!("│ {}", "Error details:".red().bold());
                println!("│ {}", format!("{:#}", e).red());
                println!("│");
                if crate::should_continue_on_test_error() {
                    println!(
                        "│ {}",
                        "⚠ Continuing despite test failure (C2RUST_TEST_CONTINUE_ON_ERROR is set)."
                            .yellow()
                    );
                } else {
                    return Err(e).context("Tests failed in full build flow");
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    /// Test that warning detection recognises `warning[code]:` patterns
    #[test]
    fn test_detect_warning_code_format() {
        let stderr = "warning[unused_variables]: unused variable `x`\n  --> src/foo.rs:5:9";
        let has = stderr
            .lines()
            .any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(has);
    }

    /// Test that warning detection recognises `warning:` patterns
    #[test]
    fn test_detect_warning_colon_format() {
        let stderr = "warning: unused import: `std::fmt`\n  --> src/bar.rs:1:5";
        let has = stderr
            .lines()
            .any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(has);
    }

    /// Test that any line containing "warning:" anywhere (e.g. continuation lines) is detected
    #[test]
    fn test_detect_warning_anywhere_in_line() {
        let stderr = "   = warning: this matches too broadly";
        let has = stderr
            .lines()
            .any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(has);
    }

    /// Test that clean build output (no warnings) returns false
    #[test]
    fn test_no_warnings_clean_output() {
        let stderr =
            "   Compiling myproject v0.1.0\n    Finished dev [unoptimized] target(s) in 1.23s";
        let has = stderr
            .lines()
            .any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(!has);
    }

    /// Test that error-only output is not flagged as having warnings
    #[test]
    fn test_no_warnings_in_error_output() {
        let stderr = "error[E0308]: mismatched types\n  --> src/main.rs:3:5";
        let has = stderr
            .lines()
            .any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(!has);
    }

    /// Test that get_manual_fix_files always includes the primary rs_file
    #[test]
    fn test_get_manual_fix_files_always_includes_rs_file() {
        // When parsing fails (e.g. invalid feature name), rs_file is returned
        let rs_file = std::path::Path::new("/nonexistent/fun_test.rs");
        let error_str = "error: some build error";
        let files = super::get_manual_fix_files("invalid/feature", rs_file, error_str);
        assert!(!files.is_empty(), "Result should not be empty");
        assert!(
            files.iter().any(|f| f == rs_file),
            "rs_file should always be in the result"
        );
    }

    /// Test that get_manual_fix_files does not duplicate rs_file
    #[test]
    #[serial_test::serial]
    fn test_get_manual_fix_files_no_duplicate_rs_file() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();
        let _restore = scopeguard::guard(original_dir, |dir| {
            let _ = env::set_current_dir(dir);
        });

        let feature = "test_feature";
        let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let rs_file_path = src_dir.join("fun_test.rs");
        fs::write(&rs_file_path, "// test").unwrap();

        // Error message referencing the same file
        let error_str = format!(
            "error[E0308]: mismatched types\n  --> src/fun_test.rs:10:5\n  |\n10 |     x\n"
        );

        let files =
            super::get_manual_fix_files(feature, &rs_file_path, &error_str);

        // rs_file should appear only once
        let canonical_rs = rs_file_path.canonicalize().ok();
        let count = files
            .iter()
            .filter(|f| {
                if let Some(ref c) = canonical_rs {
                    *f == c
                } else {
                    *f == &rs_file_path
                }
            })
            .count();
        assert_eq!(count, 1, "rs_file should appear exactly once in the result");
    }
}

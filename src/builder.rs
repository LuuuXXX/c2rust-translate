use anyhow::{Context, Result};
use std::env;
use std::process::Command;
use std::time::Instant;
use crate::util;
use colored::Colorize;
use crate::analyzer;

/// 在每个特性的 Rust 项目目录 `<feature>/rust` 中运行 `cargo build`
///
/// 每个特性在 `<feature>/rust` 下都有自己的 Rust 项目（有自己的
/// `Cargo.toml`、依赖项和构建产物），而不是共享单个
/// `.c2rust/` 目录。这避免了特性之间的冲突（例如，
/// 不同的依赖版本或特性标志），并允许每个特性独立地构建、
/// 测试和迭代。
/// 
/// 注意：`_show_full_output` 参数当前未使用，因为 cargo build 错误
/// 已经通过 bail! 宏完整显示。保留该参数是为了与其他
/// 显示函数保持 API 一致性以及未来可能的使用。
pub fn cargo_build(feature: &str, _show_full_output: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    let start_time = Instant::now();
    
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(&build_dir)
        .env("RUSTFLAGS", "-A warnings")
        .output()
        .context("Failed to execute cargo build")?;

    let duration = start_time.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Build error: {}", stderr);
    }
    
    println!("  {} (took {:.2}s)", "Build completed".bright_green(), duration.as_secs_f64());

    Ok(())
}

/// 从 c2rust-config 获取特定的配置值
fn get_config_value(key: &str, feature: &str) -> Result<String> {
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
    let rust_lib_path = feature_root_path.join("rust").join("target").join("debug").join("librust.a");

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
                .map(|f| f.join("rust").join("target").join("debug").join("librust.a"))
                .unwrap_or_default();
            
            print!("LD_PRELOAD={} ", shell_words::quote(&hybrid_lib).dimmed());
            if let Some(feature_root) = feature_root {
                print!("C2RUST_FEATURE_ROOT={} ", shell_words::quote(&feature_root.display().to_string()).dimmed());
            }
            print!("C2RUST_PROJECT_ROOT={} ", shell_words::quote(&project_root.display().to_string()).dimmed());
            print!("C2RUST_RUST_LIB={} ", shell_words::quote(&rust_lib_path.display().to_string()).dimmed());
            
            // 如果提供了 build.target，则显示 C2RUST_LD_TARGET
            if let Some(target) = build_target {
                print!("C2RUST_LD_TARGET={} ", shell_words::quote(target).dimmed());
            }
        }
    }
    
    println!("{}", shell_words::join(parts).bright_yellow());
    println!("│   {}: {}", "Working directory".dimmed(), exec_dir.display());
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
        anyhow::bail!("Directory path from config must be relative, got: {}", dir_str);
    }
    if dir_str.contains("..") {
        anyhow::bail!("Directory path from config cannot contain '..', got: {}", dir_str);
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
    
    let feature_root = setup_hybrid_env(&mut command, &project_root, feature, set_ld_preload, build_target.as_deref());
    print_command_details(command_type, &parts, &exec_dir, &project_root, feature_root.as_ref(), build_target.as_deref(), set_ld_preload);
    
    let start_time = Instant::now();
    let output = command.output()
        .with_context(|| format!("Failed to execute command: {}", command_str))?;
    let duration = start_time.elapsed();

    if !output.status.success() {
        print_command_failure(command_type, &output, duration);
        
        // 在 bail 消息中包含错误详情以便更好地调试
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_summary = stderr
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n");
        
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
fn print_command_failure(command_type: &str, output: &std::process::Output, duration: std::time::Duration) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    println!("│ {} (took {:.2}s)", 
        format!("✗ {} failed", command_type.to_uppercase()).bright_red().bold(), 
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
        "build" => format!("│ {} (took {:.2}s)", "✓ Build successful".bright_green().bold(), duration.as_secs_f64()),
        "test" => format!("│ {} (took {:.2}s)", "✓ Test successful".bright_green().bold(), duration.as_secs_f64()),
        "clean" => format!("│ {} (took {:.2}s)", "✓ Clean successful".bright_green().bold(), duration.as_secs_f64()),
        _ => format!("│ ✓ {} successful (took {:.2}s)", command_type, duration.as_secs_f64()),
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
    rs_file: Option<&std::path::Path>
) -> Result<()> {
    
    // 从配置获取构建命令
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");
    
    if !config_path.exists() {
        eprintln!("{}", format!("Error: Config file not found at {}", config_path.display()).red());
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    // 继续之前检查 c2rust-config 是否可用
    let check_output = Command::new("c2rust-config")
        .arg("--version")
        .output();
    
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
                let processing_complete = handle_build_failure_interactive(feature, ftype, rfile, build_error)?;
                if !processing_complete {
                    // User chose to retry translation - not supported in this context
                    // so treat it as a failure and return early
                    println!("│ {}", "Note: Retry translation requested but not supported in this context".yellow());
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
            // 仅当我们有文件上下文时才显示交互菜单
            if let (Some(ftype), Some(rfile)) = (file_type, rs_file) {
                let processing_complete = handle_test_failure_interactive(feature, ftype, rfile, test_error)?;
                if !processing_complete {
                    // User chose to retry translation - not supported in this context
                    // so treat it as a failure and return early
                    println!("│ {}", "Note: Retry translation requested but not supported in this context".yellow());
                    return Err(anyhow::anyhow!(
                        "Hybrid build tests failed and retry translation is not supported in this context"
                    ));
                }
                Ok(())
            } else {
                // 没有文件上下文，只返回错误
                Err(test_error)
            }
        }
    }
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
) -> Result<bool> {
    use crate::interaction;
    use crate::suggestion;
    use crate::diff_display;
    
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
        println!("│ {}", format!("Failed to display comparison: {}", e).yellow());
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
            println!("│ {}", "You chose: Retry directly without suggestion".bright_cyan());
            
            // 清除旧建议
            suggestion::clear_suggestions()?;
            
            println!("│ {}", "Retrying translation from scratch...".bright_cyan());
            println!("│ {}", "Note: The translator will overwrite the existing file content.".bright_blue());
            println!("│ {}", "✓ Retry scheduled".bright_green());
            
            // 返回 false 以信号重试翻译
            Ok(false)
        }
        interaction::FailureChoice::AddSuggestion => {
            println!("│");
            println!("│ {}", "You chose: Add fix suggestion for AI to modify".bright_cyan());
            
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
                println!("│ {}", "Applying fix based on your suggestion...".bright_blue());
                
                let format_progress = |op: &str| format!("Fix for build failure - {}", op);
                crate::apply_error_fix(feature, file_type, rs_file, &current_error, &format_progress, true)?;
                
                // 再次尝试构建和测试
                println!("│");
                println!("│ {}", "Running full build and test...".bright_blue().bold());
                
                match run_full_build_and_test_interactive(feature, file_type, rs_file) {
                    Ok(_) => {
                        return Ok(true);
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Build or tests still failing".red());
                        
                        // 使用最新失败更新 current_error
                        current_error = e;
                        
                        // 询问用户是否想再试一次
                        println!("│");
                        println!("│ {}", "Build or tests still have errors. What would you like to do?".yellow());
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
                                
                                // 打开 vim 允许用户手动编辑代码
                                match interaction::open_in_vim(rs_file) {
                                    Ok(_) => {
                                        println!("│");
                                        println!("│ {}", "Running full build and test after manual fix...".bright_blue().bold());
                                        
                                        // 执行完整构建流程（包含 cargo_build）
                                        match run_full_build_and_test_interactive(feature, file_type, rs_file) {
                                            Ok(_) => {
                                                return Ok(true);
                                            }
                                            Err(e) => {
                                                println!("│ {}", "✗ Build or tests still failing after manual fix".red());
                                                
                                                // 询问用户是否想再试一次
                                                println!("│");
                                                println!("│ {}", "Build or tests still have errors. What would you like to do?".yellow());
                                                let nested_retry_choice = interaction::prompt_build_failure_choice()?;
                                                
                                                match nested_retry_choice {
                                                    interaction::FailureChoice::RetryDirectly => {
                                                        println!("│ {}", "Switching to retry translation flow.".yellow());
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
                                                        interaction::open_in_vim(rs_file)
                                                            .context("Failed to reopen vim for additional manual fix")?;
                                                        // 更新错误并继续外部循环以重新构建
                                                        current_error = e;
                                                        continue;
                                                    }
                                                    interaction::FailureChoice::Exit => {
                                                        return Err(e).context("Build failed after manual fix and user chose to exit");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(open_err) => {
                                        println!("│ {}", format!("Failed to open vim: {}", open_err).red());
                                        println!("│ {}", "Cannot continue manual fix flow; exiting.".yellow());
                                        return Err(open_err).context("Build failed and could not open vim for manual fix");
                                    }
                                }
                            }
                            interaction::FailureChoice::Exit => {
                                return Err(current_error).context("Build failed and user chose to exit");
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
            match interaction::open_in_vim(rs_file) {
                Ok(_) => {
                    loop {
                        println!("│");
                        println!("│ {}", "Vim editing completed. Running full build and test...".bright_blue());
                        
                        // Vim 编辑后尝试使用混合构建流程进行构建和测试
                        match run_full_build_and_test_interactive(feature, file_type, rs_file) {
                            Ok(_) => {
                                return Ok(true);
                            }
                            Err(e) => {
                                println!("│ {}", "✗ Build or tests still failing after manual fix".red());
                                
                                // 询问用户是否想再试一次
                                println!("│");
                                println!("│ {}", "Build or tests still have errors. What would you like to do?".yellow());
                                let retry_choice = interaction::prompt_build_failure_choice()?;
                                
                                match retry_choice {
                                    interaction::FailureChoice::RetryDirectly => {
                                        println!("│ {}", "Switching to retry translation flow.".yellow());
                                        suggestion::clear_suggestions()?;
                                        return Ok(false);
                                    }
                                    interaction::FailureChoice::ManualFix => {
                                        println!("│ {}", "Reopening Vim for another manual fix attempt...".bright_blue());
                                        interaction::open_in_vim(rs_file)
                                            .context("Failed to reopen vim for additional manual fix")?;
                                        // Vim 关闭后，继续循环重新构建和重新测试
                                        continue;
                                    }
                                    interaction::FailureChoice::AddSuggestion => {
                                        println!("│ {}", "Switching to suggestion-based fix flow.".yellow());
                                        // 递归调用以进入基于建议的交互式修复流程
                                        return handle_build_failure_interactive(feature, file_type, rs_file, e);
                                    }
                                    interaction::FailureChoice::Exit => {
                                        return Err(e).context("Build failed after manual fix and user chose to exit");
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(e).context(format!("Build failed (original error: {}) and could not open vim", build_error))
                }
            }
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
) -> Result<bool> {
    use crate::interaction;
    use crate::suggestion;
    use crate::diff_display;
    
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
        println!("│ {}", format!("Failed to display comparison: {}", e).yellow());
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
            println!("│ {}", "You chose: Retry directly without suggestion".bright_cyan());
            
            // 清除旧建议
            suggestion::clear_suggestions()?;
            
            println!("│ {}", "Retrying translation from scratch...".bright_cyan());
            println!("│ {}", "Note: The translator will overwrite the existing file content.".bright_blue());
            println!("│ {}", "✓ Retry scheduled".bright_green());
            
            // 返回 false 以信号重试翻译
            Ok(false)
        }
        interaction::FailureChoice::AddSuggestion => {
            println!("│");
            println!("│ {}", "You chose: Add fix suggestion for AI to modify".bright_cyan());
            
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
                println!("│ {}", "Applying fix based on your suggestion...".bright_blue());
                
                let format_progress = |op: &str| format!("Fix for test failure - {}", op);
                crate::apply_error_fix(feature, file_type, rs_file, &current_error, &format_progress, true)?;
                
                // 再次尝试构建和测试
                println!("│");
                println!("│ {}", "Running full build and test...".bright_blue().bold());
                
                match run_full_build_and_test_interactive(feature, file_type, rs_file) {
                    Ok(_) => {
                        return Ok(true);
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Tests still failing".red());
                        
                        // 使用最新失败更新 current_error
                        current_error = e;
                        
                        // 询问用户是否想再试一次
                        println!("│");
                        println!("│ {}", "Tests still have errors. What would you like to do?".yellow());
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
                                match interaction::open_in_vim(rs_file) {
                                    Ok(_) => {
                                        println!("│");
                                        println!("│ {}", "Running full build and test after manual fix...".bright_blue().bold());
                                        
                                        match run_full_build_and_test_interactive(feature, file_type, rs_file) {
                                            Ok(_) => {
                                                return Ok(true);
                                            }
                                            Err(e) => {
                                                println!("│ {}", "✗ Tests still failing after manual fix".red());
                                                // 更新 current_error 并继续外部循环
                                                current_error = e;
                                                continue;
                                            }
                                        }
                                    }
                                    Err(open_err) => {
                                        println!("│ {}", format!("Failed to open vim: {}", open_err).red());
                                        println!("│ {}", "Cannot continue manual fix flow; exiting.".yellow());
                                        return Err(open_err).context("Tests failed and could not open vim for manual fix");
                                    }
                                }
                            }
                            interaction::FailureChoice::Exit => {
                                return Err(current_error).context("Tests failed and user chose to exit");
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
            match interaction::open_in_vim(rs_file) {
                Ok(_) => {
                    loop {
                        println!("│");
                        println!("│ {}", "Vim editing completed. Running full build and test...".bright_blue());
                        
                        // Vim 编辑后尝试使用混合构建流程进行构建和测试
                        match run_full_build_and_test_interactive(feature, file_type, rs_file) {
                            Ok(_) => {
                                return Ok(true);
                            }
                            Err(e) => {
                                println!("│ {}", "✗ Tests still failing after manual fix".red());
                                
                                // 询问用户是否想再试一次
                                println!("│");
                                println!("│ {}", "Tests still have errors. What would you like to do?".yellow());
                                let retry_choice = interaction::prompt_test_failure_choice()?;
                                
                                match retry_choice {
                                    interaction::FailureChoice::RetryDirectly => {
                                        println!("│ {}", "Switching to retry translation flow.".yellow());
                                        suggestion::clear_suggestions()?;
                                        return Ok(false);
                                    }
                                    interaction::FailureChoice::ManualFix => {
                                        println!("│ {}", "Reopening Vim for another manual fix attempt...".bright_blue());
                                        interaction::open_in_vim(rs_file)
                                            .context("Failed to reopen vim for additional manual fix")?;
                                        // Vim 关闭后，继续循环重新构建和重新测试
                                        continue;
                                    }
                                    interaction::FailureChoice::AddSuggestion => {
                                        println!("│ {}", "Switching to suggestion-based fix flow.".yellow());
                                        return Err(e).context("Tests still failing after manual fix; user chose to add a suggestion");
                                    }
                                    interaction::FailureChoice::Exit => {
                                        return Err(e).context("Tests failed after manual fix and user chose to exit");
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(e).context(format!("Tests failed (original error: {}) and could not open vim", test_error))
                }
            }
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
/// 顺序：cargo_build → c2rust_clean → c2rust_build → c2rust_test
/// 这是主流程中的标准验证流程
/// 
/// 注意：此函数会多次调用 `update_code_analysis`（在 clean、build、test 步骤中各一次），
/// 这会略微降低性能。未来可以优化为只更新一次分析。
pub fn run_full_build_and_test(feature: &str) -> Result<()> {
    println!("│");
    println!("│ {}", "Running full build and test flow...".bright_blue().bold());
    
    // 1. 先构建 Rust 代码
    println!("│ {}", "→ Step 1/4: Building Rust code (cargo build)...".bright_blue());
    cargo_build(feature, true)?;
    println!("│ {}", "  ✓ Rust build successful".bright_green());
    
    // 2. 清理混合构建环境
    println!("│ {}", "→ Step 2/4: Cleaning hybrid build...".bright_blue());
    c2rust_clean(feature)?;
    
    // 3. 混合构建
    println!("│ {}", "→ Step 3/4: Running hybrid build (C + Rust)...".bright_blue());
    c2rust_build(feature)?;
    println!("│ {}", "  ✓ Hybrid build successful".bright_green());
    
    // 4. 运行测试
    println!("│ {}", "→ Step 4/4: Running tests...".bright_blue());
    c2rust_test(feature)?;
    println!("│ {}", "  ✓ All tests passed".bright_green().bold());
    
    Ok(())
}

/// 执行完整的构建和测试流程
/// 顺序：cargo_build → c2rust_clean → c2rust_build → c2rust_test
/// 
/// 注意：此函数不提供交互式错误处理，任何步骤失败时都会直接返回错误。
/// 调用方负责处理错误并提供交互式修复选项（如需要）。
/// 
/// 参数 `_file_type` 和 `_rs_file` 保留用于 API 兼容性，当前未使用。
/// 
/// 性能提示：此函数会多次调用 `update_code_analysis`（在 clean、build、test 步骤中各一次），
/// 这会略微降低性能。未来可以优化为只更新一次分析。
pub fn run_full_build_and_test_interactive(
    feature: &str,
    _file_type: &str,
    _rs_file: &std::path::Path,
) -> Result<()> {
    println!("│");
    println!("│ {}", "Running full build and test flow...".bright_blue().bold());
    
    // 1. 先构建 Rust 代码
    println!("│ {}", "→ Step 1/4: Building Rust code (cargo build)...".bright_blue());
    match cargo_build(feature, true) {
        Ok(_) => {
            println!("│ {}", "  ✓ Rust build successful".bright_green());
        }
        Err(e) => {
            println!("│ {}", "  ✗ Rust build failed".red());
            // cargo_build 失败通常意味着翻译的 Rust 代码有问题
            // 这不应该发生在手动修复后，所以直接返回错误
            return Err(e).context("Rust build failed in full build flow");
        }
    }
    
    // 2. 清理混合构建环境
    println!("│ {}", "→ Step 2/4: Cleaning hybrid build...".bright_blue());
    c2rust_clean(feature)?;
    
    // 3. 混合构建（不调用交互式处理器以避免递归）
    println!("│ {}", "→ Step 3/4: Running hybrid build (C + Rust)...".bright_blue());
    c2rust_build(feature)?;
    println!("│ {}", "  ✓ Hybrid build successful".bright_green());
    
    // 4. 运行测试（不调用交互式处理器以避免递归）
    println!("│ {}", "→ Step 4/4: Running tests...".bright_blue());
    c2rust_test(feature)?;
    println!("│ {}", "  ✓ All tests passed".bright_green().bold());
    
    Ok(())
}

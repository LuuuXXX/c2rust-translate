use crate::analyzer;
use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::process::Command;
use std::time::Instant;

/// 混合构建命令类型
#[derive(Debug, Clone, Copy)]
pub enum HybridCommandType {
    Clean,
    Build,
    Test,
}

impl HybridCommandType {
    pub fn cmd_key(&self) -> &'static str {
        match self {
            Self::Clean => "clean.cmd",
            Self::Build => "build.cmd",
            Self::Test => "test.cmd",
        }
    }

    pub fn dir_key(&self) -> &'static str {
        match self {
            Self::Clean => "clean.dir",
            Self::Build => "build.dir",
            Self::Test => "test.dir",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Build => "build",
            Self::Test => "test",
        }
    }

    pub fn needs_ld_preload(&self) -> bool {
        matches!(self, Self::Build)
    }
}

/// 获取混合构建命令和目录
pub fn get_hybrid_build_command(
    feature: &str,
    command_type: HybridCommandType,
) -> Result<(String, String)> {
    util::validate_feature_name(feature)?;

    let cmd = get_config_value(command_type.cmd_key(), feature)?;
    let dir = get_config_value(command_type.dir_key(), feature)?;

    Ok((cmd, dir))
}

/// 执行混合构建命令
pub fn execute_hybrid_build_command(feature: &str, command_type: HybridCommandType) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    run_hybrid_command(feature, command_type)
}

/// 执行混合构建命令序列（clean + build + test），仅更新一次代码分析
pub fn execute_hybrid_build_sequence(feature: &str, skip_test: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    run_hybrid_command(feature, HybridCommandType::Clean)?;
    run_hybrid_command(feature, HybridCommandType::Build)?;
    if skip_test {
        println!(
            "{}",
            "⏭ 跳过测试阶段（测试配置不完整）".bright_yellow()
        );
        return Ok(());
    }
    run_hybrid_command(feature, HybridCommandType::Test)
}

/// 执行单个混合构建命令（不更新代码分析）
fn run_hybrid_command(feature: &str, command_type: HybridCommandType) -> Result<()> {
    let cmd = get_config_value(command_type.cmd_key(), feature)?;

    execute_command_in_dir_with_type(
        &cmd,
        command_type.dir_key(),
        feature,
        command_type.needs_ld_preload(),
        command_type.as_str(),
    )
}

// ============================================================================
// Functions moved from builder.rs
// ============================================================================

/// 从 c2rust-config 获取特定的配置值
pub fn get_config_value(key: &str, feature: &str) -> Result<String> {
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
pub fn execute_command_in_dir_with_type(
    command_str: &str,
    dir_key: &str,
    feature: &str,
    set_ld_preload: bool,
    command_type: &str,
) -> Result<()> {
    util::validate_feature_name(feature)?;

    let dir_str = get_config_value(dir_key, feature)?;

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

    let build_target = match get_config_value("build.target", feature) {
        Ok(target) if !target.is_empty() => Some(target),
        Ok(_) => None,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("Empty") || err_str.contains("not found") {
                None
            } else {
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

/// 为给定特性运行清理命令（不更新代码分析）
pub(crate) fn c2rust_clean_no_analysis(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    let clean_cmd = get_config_value("clean.cmd", feature)?;

    execute_command_in_dir_with_type(&clean_cmd, "clean.dir", feature, false, "clean")
}

/// 为给定特性运行构建命令
pub fn c2rust_build(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    println!("{}", "Rebuilding Rust static library for hybrid link...".bright_blue());
    crate::builder::cargo_build(feature, true, false)?;
    println!("{}", "✓ Rust static library refreshed".bright_green());

    let build_cmd = get_config_value("build.cmd", feature)?;

    execute_command_in_dir_with_type(&build_cmd, "build.dir", feature, true, "build")
}

/// 为给定特性运行构建命令（不更新代码分析）
pub(crate) fn c2rust_build_no_analysis(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Rebuilding Rust static library for hybrid link...".bright_blue());
    crate::builder::cargo_build(feature, true, false)?;
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

/// 为给定特性运行测试命令（不更新代码分析）
pub(crate) fn c2rust_test_no_analysis(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    let test_cmd = get_config_value("test.cmd", feature)?;

    execute_command_in_dir_with_type(&test_cmd, "test.dir", feature, false, "test")
}

/// 运行混合构建测试套件
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    run_hybrid_build_interactive(feature, None, None)
}

/// 通过交互式错误处理运行混合构建测试套件
pub fn run_hybrid_build_interactive(
    feature: &str,
    file_type: Option<&str>,
    rs_file: Option<&std::path::Path>,
) -> Result<()> {
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");

    if !config_path.exists() {
        eprintln!(
            "{}",
            format!("Error: Config file not found at {}", config_path.display()).red()
        );
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    let check_output = Command::new("c2rust-config").arg("--version").output();

    if check_output.is_err() {
        eprintln!("{}", "Error: c2rust-config not found".red());
        anyhow::bail!("c2rust-config not found, cannot run hybrid build tests");
    }

    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    println!("│ {}", "Running hybrid build tests...".bright_blue().bold());
    c2rust_clean_no_analysis(feature)?;

    match c2rust_build_no_analysis(feature) {
        Ok(_) => {}
        Err(build_error) => {
            if let (Some(ftype), Some(rfile)) = (file_type, rs_file) {
                let processing_complete = crate::workflow::handle_build_failure_interactive(
                    feature, ftype, rfile, build_error, false,
                )?;
                if !processing_complete {
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
                return Err(build_error);
            }
        }
    }

    match c2rust_test_no_analysis(feature) {
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
            if let (Some(ftype), Some(rfile)) = (file_type, rs_file) {
                let processing_complete = crate::workflow::handle_test_failure_interactive(
                    feature, ftype, rfile, test_error, false,
                )?;
                if !processing_complete {
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
                Err(test_error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_command_type_keys() {
        assert_eq!(HybridCommandType::Clean.cmd_key(), "clean.cmd");
        assert_eq!(HybridCommandType::Clean.dir_key(), "clean.dir");

        assert_eq!(HybridCommandType::Build.cmd_key(), "build.cmd");
        assert_eq!(HybridCommandType::Build.dir_key(), "build.dir");

        assert_eq!(HybridCommandType::Test.cmd_key(), "test.cmd");
        assert_eq!(HybridCommandType::Test.dir_key(), "test.dir");
    }

    #[test]
    fn test_hybrid_command_type_as_str() {
        assert_eq!(HybridCommandType::Clean.as_str(), "clean");
        assert_eq!(HybridCommandType::Build.as_str(), "build");
        assert_eq!(HybridCommandType::Test.as_str(), "test");
    }

    #[test]
    fn test_needs_ld_preload() {
        assert!(!HybridCommandType::Clean.needs_ld_preload());
        assert!(HybridCommandType::Build.needs_ld_preload());
        assert!(!HybridCommandType::Test.needs_ld_preload());
    }
}

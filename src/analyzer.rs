use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// Validates `feature`, then runs `code_analyse` with the given arguments.
/// `pre_feature_args` are placed before `--feature <feature>` and
/// `post_feature_args` are placed after, allowing callers to control CLI ordering precisely.
fn run_code_analyse(
    feature: &str,
    pre_feature_args: &[&str],
    post_feature_args: &[&str],
) -> Result<()> {
    util::validate_feature_name(feature)?;
    let project_root = util::find_project_root()?;

    let mut args: Vec<&str> = pre_feature_args.to_vec();
    args.extend_from_slice(&["--feature", feature]);
    args.extend_from_slice(post_feature_args);

    // Use debug formatting for an unambiguous representation (handles spaces/special chars).
    let args_display = format!("{:?}", args);

    println!("Running code_analyse {}", args_display);

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(&args)
        .output()
        .with_context(|| format!("Failed to execute code_analyse {}", args_display))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "code_analyse {} failed:\nstdout: {}\nstderr: {}",
            args_display,
            stdout,
            stderr
        );
    }

    Ok(())
}

/// 为功能初始化代码分析
pub fn initialize_feature(feature: &str) -> Result<()> {
    run_code_analyse(feature, &["--init"], &[])
}

/// 为功能更新代码分析
pub fn update_code_analysis(feature: &str) -> Result<()> {
    run_code_analyse(feature, &["--update"], &[])
}

/// 为功能更新代码分析，并标记构建成功（所有文件OK）
/// Invokes: code_analyse --update --feature <feature> --build-success
pub fn update_code_analysis_with_build_success(feature: &str) -> Result<()> {
    run_code_analyse(feature, &["--update"], &["--build-success"])
}

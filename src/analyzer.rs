use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// Validates `feature`, then runs `code_analyse` with the given extra arguments.
fn run_code_analyse(feature: &str, extra_args: &[&str]) -> Result<()> {
    util::validate_feature_name(feature)?;
    let project_root = util::find_project_root()?;

    let mut args = vec!["--feature", feature];
    args.extend_from_slice(extra_args);

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(&args)
        .output()
        .context("Failed to execute code_analyse")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code_analyse failed:\nstdout: {}\nstderr: {}", stdout, stderr);
    }

    Ok(())
}

/// 为功能初始化代码分析
pub fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code_analyse --init --feature {}", feature);
    run_code_analyse(feature, &["--init"])
}

/// 为功能更新代码分析
pub fn update_code_analysis(feature: &str) -> Result<()> {
    run_code_analyse(feature, &["--update"])
}

/// 为功能更新代码分析，并标记构建成功（所有文件OK）
pub fn update_code_analysis_with_build_success(feature: &str) -> Result<()> {
    run_code_analyse(feature, &["--update", "--build-success"])
}

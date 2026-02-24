use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// 为功能初始化代码分析
pub fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code_analyse --init --feature {}", feature);

    let project_root = util::find_project_root()?;

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(["--init", "--feature", feature])
        .output()
        .context("Failed to execute code_analyse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code_analyse failed: {}", stderr);
    }

    Ok(())
}

/// 为 feature 合并代码分析，将独立的 rs 文件合并为与 C 文件一一对应的大文件
pub fn merge_code_analysis(feature: &str) -> Result<()> {
    println!("Running code_analyse --merge --feature {}", feature);

    let project_root = util::find_project_root()?;

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(["--merge", "--feature", feature])
        .output()
        .context("Failed to execute code_analyse --merge")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code_analyse merge failed: {}", stderr);
    }

    Ok(())
}

/// 为功能更新代码分析
pub fn update_code_analysis(feature: &str) -> Result<()> {
    let project_root = util::find_project_root()?;

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(["--update", "--feature", feature])
        .output()
        .context("Failed to execute code_analyse --update")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code_analyse update failed: {}", stderr);
    }

    Ok(())
}

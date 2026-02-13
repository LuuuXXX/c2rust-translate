use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// 使用消息提交更改
/// 仅暂存 .c2rust/ 目录和特定功能目录，以避免提交无关的更改
pub fn git_commit(message: &str, feature: &str) -> Result<()> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");

    // 仅添加 .c2rust 目录和特定功能目录（而非所有功能）
    // 这可防止意外提交无关的本地修改
    // 路径相对于 .c2rust 目录（.c2rust/<feature>/rust/）
    let feature_rust_path = format!("{}/rust/", feature);
    let add_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["add", ".", &feature_rust_path])
        .output()
        .context("Failed to git add")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    // 从 .c2rust 目录提交
    let commit_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["commit", "-m", message])
        .output()
        .context("Failed to git commit")?;

    if !commit_output.status.success() {
        let stdout = String::from_utf8_lossy(&commit_output.stdout);
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        let combined_output = format!("{}{}", stdout, stderr);
        let exit_code = commit_output.status.code();

        // 如果没有可提交的内容也没关系（git 通常在这里以代码 1 退出）
        let is_nothing_to_commit =
            exit_code == Some(1) && combined_output.contains("nothing to commit");

        if !is_nothing_to_commit {
            anyhow::bail!(
                "git commit failed with exit code {:?}: {}",
                exit_code,
                combined_output
            );
        }
    }

    Ok(())
}

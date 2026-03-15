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

/// 对 .c2rust 仓库执行垃圾回收，压缩历史对象、缩减 .git 体积。
///
/// 保留所有 commit 历史，支持完整回退。
/// 建议在每翻译完 N 个文件（如10个）或整个 feature 翻译完成后调用。
///
/// - `--aggressive`: 更强力的 delta 压缩（耗时稍长，但效果最好）
/// - `--prune=now`:  立即清理所有不可达对象（而非等待默认的2周宽限期）
pub fn git_gc() -> Result<()> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");

    let gc_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["gc", "--aggressive", "--prune=now"])
        .output()
        .context("Failed to run git gc")?;

    if !gc_output.status.success() {
        let stderr = String::from_utf8_lossy(&gc_output.stderr);
        // gc 失败不是致命错误，仅打印警告，不中断主流程
        eprintln!("Warning: git gc failed: {}", stderr);
    }

    Ok(())
}

/// 清理 .c2rust 仓库的 reflog，释放 reflog 占用的空间。
///
/// 默认情况下 git 保留 90 天 reflog，此函数将其立即过期清理。
/// 建议在 git_gc() 之前调用，以使 gc 能清理更多不可达对象。
pub fn git_expire_reflog() -> Result<()> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");

    let output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["reflog", "expire", "--expire=now", "--all"])
        .output()
        .context("Failed to expire reflog")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: git reflog expire failed: {}", stderr);
    }

    Ok(())
}

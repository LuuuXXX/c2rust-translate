use anyhow::{Context, Result};
use std::process::Command;
use crate::util;

/// Commit changes with a message
/// Only stages .c2rust/ directory and the specific feature directory to avoid committing unrelated changes
pub fn git_commit(message: &str, feature: &str) -> Result<()> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    // Add only .c2rust directory and the specific feature directory (not all features)
    // This prevents accidentally committing unrelated local modifications
    let feature_rust_path = format!("{}/rust/", feature);
    let add_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(&["add", ".", &feature_rust_path])
        .output()
        .context("Failed to git add")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    // Commit from the project root
    let commit_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(&["commit", "-m", message])
        .output()
        .context("Failed to git commit")?;

    if !commit_output.status.success() {
        let stdout = String::from_utf8_lossy(&commit_output.stdout);
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        let combined_output = format!("{}{}", stdout, stderr);
        let exit_code = commit_output.status.code();

        // It's okay if there's nothing to commit (git typically exits with code 1 here)
        let is_nothing_to_commit = exit_code == Some(1) && combined_output.contains("nothing to commit");

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

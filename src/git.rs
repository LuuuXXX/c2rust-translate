use anyhow::{Context, Result};
use std::process::Command;

/// Commit changes with a message
pub fn git_commit(message: &str) -> Result<()> {
    // Add all changes
    let add_output = Command::new("git")
        .args(&["add", "."])
        .output()
        .context("Failed to git add")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        println!("Warning: git add failed: {}", stderr);
    }

    // Commit
    let commit_output = Command::new("git")
        .args(&["commit", "-m", message])
        .output()
        .context("Failed to git commit")?;

    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        // It's okay if there's nothing to commit
        if !stderr.contains("nothing to commit") {
            println!("Warning: git commit failed: {}", stderr);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests would require git repository setup
    // or mocking git commands
}

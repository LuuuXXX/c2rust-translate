use anyhow::{Context, Result};
use std::process::Command;
use crate::util;

/// Initialize code analysis for a feature
pub fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code-analyse --init --feature {}", feature);
    
    let project_root = util::find_project_root()?;
    
    let output = Command::new("code-analyse")
        .current_dir(&project_root)
        .args(&["--init", "--feature", feature])
        .output()
        .context("Failed to execute code-analyse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code-analyse failed: {}", stderr);
    }

    Ok(())
}

/// Update code analysis for a feature
pub fn update_code_analysis(feature: &str) -> Result<()> {
    let project_root = util::find_project_root()?;
    
    let output = Command::new("code-analyse")
        .current_dir(&project_root)
        .args(&["--update", "--feature", feature])
        .output()
        .context("Failed to execute code-analyse --update")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code-analyse update failed: {}", stderr);
    }

    Ok(())
}

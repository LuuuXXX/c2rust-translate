use anyhow::{Context, Result};
use std::process::Command;

/// Initialize code analysis for a feature
pub fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code-analyse --init --feature {}", feature);
    
    let output = Command::new("code-analyse")
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
    let output = Command::new("code-analyse")
        .args(&["--update", "--feature", feature])
        .output()
        .context("Failed to execute code-analyse --update")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code-analyse update failed: {}", stderr);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the code-analyse command
    // or integration tests with actual tool
}

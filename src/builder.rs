use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Run cargo build in the specified directory
pub fn cargo_build(rust_dir: &Path) -> Result<()> {
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(rust_dir)
        .output()
        .context("Failed to execute cargo build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Build error: {}", stderr);
    }

    Ok(())
}

/// Run c2rust command
pub fn run_c2rust_command(cmd_type: &str, feature: &str) -> Result<()> {
    let cmd_name = format!("c2rust-{}", cmd_type);
    
    let output = Command::new(&cmd_name)
        .args(&[cmd_type, "--", feature])
        .output()
        .with_context(|| format!("Failed to execute {}", cmd_name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: {} failed: {}", cmd_name, stderr);
        eprintln!("Please handle this manually");
    }

    Ok(())
}

/// Run hybrid build test suite
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    use std::path::PathBuf;

    // Get build commands from config
    let config_path = PathBuf::from(feature).join(".c2rust/config.toml");
    
    if !config_path.exists() {
        println!("Config file not found, skipping hybrid build tests");
        return Ok(());
    }

    // Get command list
    let output = Command::new("c2rust-config")
        .args(&["config", "--list", feature])
        .output()
        .context("Failed to get config list")?;

    if !output.status.success() {
        println!("Warning: Could not retrieve build commands");
        return Ok(());
    }

    // Execute clean, build, and test commands
    run_c2rust_command("clean", feature)?;
    run_c2rust_command("build", feature)?;
    run_c2rust_command("test", feature)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_command_name_generation() {
        let cmd_name = format!("c2rust-{}", "build");
        assert_eq!(cmd_name, "c2rust-build");
        
        let cmd_name = format!("c2rust-{}", "test");
        assert_eq!(cmd_name, "c2rust-test");
    }
}

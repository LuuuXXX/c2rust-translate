use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use crate::util;

/// Run cargo build in the .c2rust directory
pub fn cargo_build(_rust_dir: &Path) -> Result<()> {
    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust");
    
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(&build_dir)
        .output()
        .context("Failed to execute cargo build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Build error: {}", stderr);
    }

    Ok(())
}

/// Get a specific command from c2rust-config
fn get_c2rust_command(cmd_type: &str, feature: &str) -> Result<String> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    let output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args(&["config", "--make", "--feature", feature, "--list", cmd_type])
        .output()
        .with_context(|| format!("Failed to get {} command from config", cmd_type))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to retrieve {} command: {}", cmd_type, stderr);
    }

    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if command.is_empty() {
        anyhow::bail!("Empty {} command from config", cmd_type);
    }

    Ok(command)
}

/// Run c2rust command with the command from config
pub fn run_c2rust_command(cmd_type: &str, feature: &str) -> Result<()> {
    let cmd_name = format!("c2rust-{}", cmd_type);
    
    // Get the actual command from config
    let actual_command = get_c2rust_command(cmd_type, feature)?;
    
    // Split the command into parts for proper argument passing
    let parts: Vec<&str> = actual_command.split_whitespace().collect();
    
    let output = if parts.is_empty() {
        return Ok(()); // Nothing to execute
    } else if parts.len() == 1 {
        Command::new(&cmd_name)
            .args(&[cmd_type, "--", parts[0]])
            .output()
    } else {
        Command::new(&cmd_name)
            .arg(cmd_type)
            .arg("--")
            .args(&parts)
            .output()
    }.with_context(|| format!("Failed to execute {}", cmd_name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} failed: {}", cmd_name, stderr);
    }

    Ok(())
}

/// Run hybrid build test suite
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    // Get build commands from config
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");
    
    if !config_path.exists() {
        println!("Config file not found, skipping hybrid build tests");
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

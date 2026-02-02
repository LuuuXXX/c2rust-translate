use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the project root by searching upward for .c2rust directory
fn find_project_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()
        .context("Failed to get current directory")?;
    
    loop {
        let c2rust_dir = current.join(".c2rust");
        if c2rust_dir.exists() && c2rust_dir.is_dir() {
            return Ok(current);
        }
        
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => anyhow::bail!("Could not find .c2rust directory in any parent directory"),
        }
    }
}

/// Run cargo build in the .c2rust directory
pub fn cargo_build(_rust_dir: &Path) -> Result<()> {
    let project_root = find_project_root()?;
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
    let output = Command::new("c2rust-config")
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
    
    let output = Command::new(&cmd_name)
        .args(&[cmd_type, "--", &actual_command])
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
    // Get build commands from config
    let project_root = find_project_root()?;
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

use anyhow::{Context, Result};
use std::env;
use std::process::Command;
use crate::util;

/// Run `cargo build` in the per-feature Rust project directory at `<feature>/rust`.
///
/// Each feature has its own Rust project under `<feature>/rust` (with its own
/// `Cargo.toml`, dependencies, and build artifacts) rather than sharing a single
/// `.c2rust/` directory. This avoids conflicts between features (for example,
/// differing dependency versions or feature flags) and allows each feature to be built,
/// tested, and iterated on independently.
pub fn cargo_build(feature: &str) -> Result<()> {
    // Validate feature name to prevent path traversal attacks
    if feature.contains('/') || feature.contains('\\') || feature.contains("..") || feature.is_empty() {
        anyhow::bail!(
            "Invalid feature name '{}': must be a simple directory name without path separators or '..'",
            feature
        );
    }

    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(feature).join("rust");
    
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

/// Helper function to check if an error is a "command not found" error
fn is_command_not_found(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            io_err.kind() == std::io::ErrorKind::NotFound
        } else {
            false
        }
    })
}

/// Execute a c2rust command with the command from config
fn execute_c2rust_command(
    cmd_name: &str,
    cmd_type: &str,
    actual_command: &str,
    feature: &str,
    set_hybrid_env: bool,
) -> Result<()> {
    // Parse the command using shell-words to handle quoted arguments and spaces correctly
    let parts = shell_words::split(actual_command)
        .with_context(|| format!("Failed to parse command: {}", actual_command))?;
    
    // Ensure we run the c2rust-* command from the project .c2rust directory
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    let mut command = if parts.is_empty() {
        return Ok(()); // Nothing to execute
    } else if parts.len() == 1 {
        let mut cmd = Command::new(cmd_name);
        cmd.current_dir(&c2rust_dir)
            .args(&[cmd_type, "--", &parts[0]]);
        cmd
    } else {
        let mut cmd = Command::new(cmd_name);
        cmd.current_dir(&c2rust_dir)
            .arg(cmd_type)
            .arg("--")
            .args(&parts);
        cmd
    };
    
    // Set hybrid build environment variables if requested (only for build command)
    if set_hybrid_env {
        if let Ok(hybrid_lib) = env::var("C2RUST_HYBRID_BUILD_LIB") {
            let feature_root = c2rust_dir.join(feature);
            command.env("LD_PRELOAD", hybrid_lib);
            command.env("C2RUST_FEATURE_ROOT", feature_root);
        }
    }
    
    let output = command
        .output()
        .with_context(|| format!("Failed to execute {}", cmd_name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} failed: {}", cmd_name, stderr);
    }

    Ok(())
}

/// Run c2rust-clean command for a given feature
pub fn c2rust_clean(feature: &str) -> Result<()> {
    let actual_command = get_c2rust_command("clean", feature)?;
    execute_c2rust_command("c2rust-clean", "clean", &actual_command, feature, false)
}

/// Run c2rust-build command for a given feature
/// Automatically detects and sets hybrid build environment variables if C2RUST_HYBRID_BUILD_LIB is set
pub fn c2rust_build(feature: &str) -> Result<()> {
    let actual_command = get_c2rust_command("build", feature)?;
    execute_c2rust_command("c2rust-build", "build", &actual_command, feature, true)
}

/// Run c2rust-test command for a given feature
pub fn c2rust_test(feature: &str) -> Result<()> {
    let actual_command = get_c2rust_command("test", feature)?;
    execute_c2rust_command("c2rust-test", "test", &actual_command, feature, false)
}

/// Run hybrid build test suite
/// Gracefully skips if c2rust tools are not available
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    // Get build commands from config
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");
    
    if !config_path.exists() {
        println!("Config file not found, skipping hybrid build tests");
        return Ok(());
    }

    // Check if c2rust-config is available before proceeding
    let check_output = Command::new("c2rust-config")
        .arg("--version")
        .output();
    
    if check_output.is_err() {
        println!("c2rust-config not found, skipping hybrid build tests");
        return Ok(());
    }

    // Execute clean command
    match c2rust_clean(feature) {
        Ok(_) => {}
        Err(e) => {
            if is_command_not_found(&e) {
                println!("c2rust-clean not found, skipping hybrid build tests");
                return Ok(());
            }
            return Err(e);
        }
    }

    // Execute build command (with automatic hybrid environment support)
    match c2rust_build(feature) {
        Ok(_) => {}
        Err(e) => {
            if is_command_not_found(&e) {
                println!("c2rust-build not found, skipping hybrid build tests");
                return Ok(());
            }
            return Err(e);
        }
    }

    // Execute test command
    match c2rust_test(feature) {
        Ok(_) => {}
        Err(e) => {
            if is_command_not_found(&e) {
                println!("c2rust-test not found, skipping hybrid build tests");
                return Ok(());
            }
            return Err(e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    
    #[test]
    fn test_command_name_generation() {
        let cmd_name = format!("c2rust-{}", "build");
        assert_eq!(cmd_name, "c2rust-build");
        
        let cmd_name = format!("c2rust-{}", "test");
        assert_eq!(cmd_name, "c2rust-test");
    }
    
    #[test]
    fn test_is_command_not_found() {
        // Create an error that is a "command not found" error
        let not_found_err = io::Error::new(io::ErrorKind::NotFound, "command not found");
        let anyhow_err = anyhow::Error::from(not_found_err);
        assert!(is_command_not_found(&anyhow_err));
        
        // Create an error that is not a "command not found" error
        let other_err = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        let anyhow_err = anyhow::Error::from(other_err);
        assert!(!is_command_not_found(&anyhow_err));
        
        // Create a regular anyhow error (not from io::Error)
        let regular_err = anyhow::anyhow!("some error");
        assert!(!is_command_not_found(&regular_err));
    }
}

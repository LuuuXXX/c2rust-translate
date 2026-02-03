use anyhow::{Context, Result};
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

/// Run c2rust command with the command from config
/// 
/// Note: Currently does not set environment variables like LD_PRELOAD or C2RUST_FEATURE_ROOT.
/// If your hybrid build requires specific environment variables, you may need to set them
/// externally before running this tool.
pub fn run_c2rust_command(cmd_type: &str, feature: &str) -> Result<()> {
    let cmd_name = format!("c2rust-{}", cmd_type);
    
    // Get the actual command from config
    let actual_command = get_c2rust_command(cmd_type, feature)?;
    
    // Parse the command using shell-words to handle quoted arguments and spaces correctly
    let parts = shell_words::split(&actual_command)
        .with_context(|| format!("Failed to parse command: {}", actual_command))?;
    
    // Ensure we run the c2rust-* command from the project .c2rust directory
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    let output = if parts.is_empty() {
        return Ok(()); // Nothing to execute
    } else if parts.len() == 1 {
        Command::new(&cmd_name)
            .current_dir(&c2rust_dir)
            .args(&[cmd_type, "--", &parts[0]])
            .output()
    } else {
        Command::new(&cmd_name)
            .current_dir(&c2rust_dir)
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

    // Execute clean, build, and test commands
    // If any c2rust-* binary is missing, skip gracefully
    for cmd in &["clean", "build", "test"] {
        match run_c2rust_command(cmd, feature) {
            Ok(_) => {}
            Err(e) => {
                // Check if it's a "command not found" error by examining the error chain
                // for std::io::ErrorKind::NotFound
                let is_not_found = e.chain()
                    .any(|cause| {
                        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                            io_err.kind() == std::io::ErrorKind::NotFound
                        } else {
                            false
                        }
                    });
                
                if is_not_found {
                    println!("c2rust-{} not found, skipping hybrid build tests", cmd);
                    return Ok(());
                }
                // Otherwise propagate the error
                return Err(e);
            }
        }
    }

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

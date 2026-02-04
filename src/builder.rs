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
    validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust").join(feature).join("rust");
    
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

/// Get a specific config value from c2rust-config
fn get_config_value(key: &str) -> Result<String> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    let output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args(&["config", "--make", "--list", key])
        .output()
        .with_context(|| format!("Failed to get {} from config", key))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to retrieve {}: {}", key, stderr);
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if value.is_empty() {
        anyhow::bail!("Empty {} value from config", key);
    }

    Ok(value)
}

/// Validate feature name to prevent path traversal attacks
fn validate_feature_name(feature: &str) -> Result<()> {
    if feature.contains('/') || feature.contains('\\') || feature.contains("..") || feature.is_empty() {
        anyhow::bail!(
            "Invalid feature name '{}': must be a simple directory name without path separators or '..'",
            feature
        );
    }
    Ok(())
}

/// Execute a build/test/clean command directly in the build directory
fn execute_build_command(
    command_str: &str,
    feature: &str,
    set_ld_preload: bool,
) -> Result<()> {
    // Validate feature name to prevent path traversal (defense in depth)
    validate_feature_name(feature)?;
    
    // Get build directory from config
    let build_dir_str = get_config_value("build.dir")?;
    
    // Parse the command using shell-words to handle quoted arguments and spaces correctly
    let parts = shell_words::split(command_str)
        .with_context(|| format!("Failed to parse command: {}", command_str))?;
    
    if parts.is_empty() {
        return Ok(()); // Nothing to execute
    }
    
    // Ensure we execute the command in the build directory
    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(&build_dir_str);
    
    if !build_dir.exists() {
        anyhow::bail!("Build directory does not exist: {}", build_dir.display());
    }
    
    let mut command = Command::new(&parts[0]);
    command.current_dir(&build_dir);
    
    if parts.len() > 1 {
        command.args(&parts[1..]);
    }
    
    // Set LD_PRELOAD for build command if requested
    if set_ld_preload {
        if let Ok(hybrid_lib) = env::var("C2RUST_HYBRID_BUILD_LIB") {
            let c2rust_dir = project_root.join(".c2rust");
            let feature_root = c2rust_dir.join(feature);
            command.env("LD_PRELOAD", hybrid_lib);
            command.env("C2RUST_FEATURE_ROOT", feature_root);
        }
    }
    
    let output = command
        .output()
        .with_context(|| format!("Failed to execute command: {}", command_str))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Command failed: {}", stderr);
    }

    Ok(())
}

/// Run clean command for a given feature
pub fn c2rust_clean(feature: &str) -> Result<()> {
    validate_feature_name(feature)?;
    let build_cmd = get_config_value("build.cmd")?;
    
    // Construct clean command (typically "make clean", "cmake --build . --target clean", etc.)
    let clean_cmd = format!("{} clean", build_cmd);
    
    execute_build_command(&clean_cmd, feature, false)
}

/// Run build command for a given feature
/// Automatically detects and sets LD_PRELOAD if C2RUST_HYBRID_BUILD_LIB is set
pub fn c2rust_build(feature: &str) -> Result<()> {
    validate_feature_name(feature)?;
    let build_cmd = get_config_value("build.cmd")?;
    
    execute_build_command(&build_cmd, feature, true)
}

/// Run test command for a given feature
pub fn c2rust_test(feature: &str) -> Result<()> {
    validate_feature_name(feature)?;
    let build_cmd = get_config_value("build.cmd")?;
    
    // Construct test command (typically "make test", "cmake --build . --target test", etc.)
    let test_cmd = format!("{} test", build_cmd);
    
    execute_build_command(&test_cmd, feature, false)
}

/// Run hybrid build test suite
/// Reports error and exits if c2rust-config is not available
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    // Get build commands from config
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");
    
    if !config_path.exists() {
        eprintln!("Error: Config file not found at {}", config_path.display());
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    // Check if c2rust-config is available before proceeding
    let check_output = Command::new("c2rust-config")
        .arg("--version")
        .output();
    
    if check_output.is_err() {
        eprintln!("Error: c2rust-config not found");
        anyhow::bail!("c2rust-config not found, cannot run hybrid build tests");
    }

    // Execute commands
    c2rust_clean(feature)?;
    c2rust_build(feature)?;
    c2rust_test(feature)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_feature_name() {
        // Valid feature names
        assert!(validate_feature_name("valid_feature").is_ok());
        assert!(validate_feature_name("feature123").is_ok());
        assert!(validate_feature_name("my-feature").is_ok());
        
        // Invalid feature names with path separators
        assert!(validate_feature_name("invalid/feature").is_err());
        assert!(validate_feature_name("invalid\\feature").is_err());
        assert!(validate_feature_name("../feature").is_err());
        assert!(validate_feature_name("feature/..").is_err());
        assert!(validate_feature_name("").is_err());
    }
}

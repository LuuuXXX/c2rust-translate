use anyhow::{Context, Result};
use std::env;
use std::process::Command;
use std::time::Instant;
use crate::util;
use colored::Colorize;

/// Run `cargo build` in the per-feature Rust project directory at `<feature>/rust`.
///
/// Each feature has its own Rust project under `<feature>/rust` (with its own
/// `Cargo.toml`, dependencies, and build artifacts) rather than sharing a single
/// `.c2rust/` directory. This avoids conflicts between features (for example,
/// differing dependency versions or feature flags) and allows each feature to be built,
/// tested, and iterated on independently.
pub fn cargo_build(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    let start_time = Instant::now();
    
    let output = Command::new("cargo")
        .arg("build")
        // .args(["--message-format", "short"])
        .current_dir(&build_dir)
        .env("RUSTFLAGS", "-A warnings")
        .output()
        .context("Failed to execute cargo build")?;

    let duration = start_time.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Build error: {}", stderr);
    }
    
    println!("  {} (took {:.2}s)", "Build completed".bright_green(), duration.as_secs_f64());

    Ok(())
}

/// Get a specific config value from c2rust-config
fn get_config_value(key: &str, feature: &str) -> Result<String> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    let output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args(["config", "--make", "--feature", feature, "--list", key])
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

/// Execute a command in a configured directory
fn execute_command_in_dir(
    command_str: &str,
    dir_key: &str,
    feature: &str,
    set_ld_preload: bool,
    command_type: &str, // "build", "test", or "clean"
) -> Result<()> {
    // Validate feature name to prevent path traversal (defense in depth)
    util::validate_feature_name(feature)?;
    
    // Get directory from config using the specified key
    let dir_str = get_config_value(dir_key, feature)?;
    
    // Validate that dir_str is a relative path without path traversal
    if std::path::Path::new(&dir_str).is_absolute() {
        anyhow::bail!(
            "Directory path from config must be relative, got: {}",
            dir_str
        );
    }
    if dir_str.contains("..") {
        anyhow::bail!(
            "Directory path from config cannot contain '..', got: {}",
            dir_str
        );
    }
    
    // Parse the command using shell-words to handle quoted arguments and spaces correctly
    let parts = shell_words::split(command_str)
        .with_context(|| format!("Failed to parse command: {}", command_str))?;
    
    if parts.is_empty() {
        return Ok(()); // Nothing to execute
    }
    
    // Validate that the command is non-empty
    if parts[0].is_empty() {
        anyhow::bail!("Command cannot be empty");
    }
    
    // Ensure we execute the command in the correct directory
    let project_root = util::find_project_root()?;
    let exec_dir = project_root.join(&dir_str);
    
    if !exec_dir.exists() {
        anyhow::bail!("Directory does not exist: {}", exec_dir.display());
    } else if !exec_dir.is_dir() {
        anyhow::bail!("Path is not a directory: {}", exec_dir.display());
    }
    
    let mut command = Command::new(&parts[0]);
    command.current_dir(&exec_dir);
    
    if parts.len() > 1 {
        command.args(&parts[1..]);
    }
    
    // Set LD_PRELOAD for build command if requested
    let hybrid_lib = if set_ld_preload {
        env::var("C2RUST_HYBRID_BUILD_LIB").ok()
    } else {
        None
    };

    let c2rust_dir = project_root.join(".c2rust");
    let feature_root_path = c2rust_dir.join(feature);
    let rust_lib_path = feature_root_path.join("rust").join("target").join("debug").join("librust.a");

    let feature_root = if let Some(ref lib_path) = hybrid_lib {
        command.env("LD_PRELOAD", lib_path);
        command.env("C2RUST_PROJECT_ROOT", &project_root);
        command.env("C2RUST_FEATURE_ROOT", &feature_root_path);
        command.env("C2RUST_RUST_LIB", &rust_lib_path);
        Some(feature_root_path)
    } else {
        None
    };
    
    // Color code based on command type
    let colored_label = match command_type {
        "build" => "│ → Executing build command:".bright_blue().to_string(),
        "test" => "│ → Executing test command:".bright_green().to_string(),
        "clean" => "│ → Executing clean command:".bright_red().to_string(),
        _ => format!("│ → Executing {} command:", command_type),
    };
    
    println!("{}", colored_label);
    print!("│   ");
    if let Some(ref lib_path) = hybrid_lib {
        print!("LD_PRELOAD={} ", shell_words::quote(lib_path).dimmed());
        if let Some(ref feature_root) = feature_root {
            print!("C2RUST_FEATURE_ROOT={} ", shell_words::quote(&feature_root.display().to_string()).dimmed());
        }
        print!("C2RUST_PROJECT_ROOT={} ", shell_words::quote(&project_root.display().to_string()).dimmed());
        print!("C2RUST_RUST_LIB={} ", shell_words::quote(&rust_lib_path.display().to_string()).dimmed());
    }
    // Print the actual command that will be executed (after shell-words parsing)
    println!("{}", shell_words::join(&parts).bright_yellow());
    println!("│   {}: {}", "Working directory".dimmed(), exec_dir.display());
    
    let start_time = Instant::now();
    
    let output = command
        .output()
        .with_context(|| format!("Failed to execute command: {}", command_str))?;

    let duration = start_time.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        let error_details = if !stderr.is_empty() {
            format!("stderr: {}", stderr)
        } else if !stdout.is_empty() {
            format!("stdout: {}", stdout)
        } else {
            String::from("no output")
        };
        
        println!("│ {} (took {:.2}s)", format!("✗ {} failed", command_type.to_uppercase()).bright_red().bold(), duration.as_secs_f64());
        
        anyhow::bail!(
            "Command '{}' failed with {}: {}",
            command_str,
            output.status,
            error_details
        );
    }

    // Success message with timing
    let success_msg = match command_type {
        "build" => format!("│ {} (took {:.2}s)", "✓ Build successful".bright_green().bold(), duration.as_secs_f64()),
        "test" => format!("│ {} (took {:.2}s)", "✓ Test successful".bright_green().bold(), duration.as_secs_f64()),
        "clean" => format!("│ {} (took {:.2}s)", "✓ Clean successful".bright_green().bold(), duration.as_secs_f64()),
        _ => format!("│ ✓ {} successful (took {:.2}s)", command_type, duration.as_secs_f64()),
    };
    println!("{}", success_msg);

    Ok(())
}

/// Run clean command for a given feature
pub fn c2rust_clean(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let clean_cmd = get_config_value("clean.cmd", feature)?;
    
    execute_command_in_dir(&clean_cmd, "clean.dir", feature, false, "clean")
}

/// Run build command for a given feature
/// Automatically detects and sets LD_PRELOAD if C2RUST_HYBRID_BUILD_LIB is set
pub fn c2rust_build(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    let build_cmd = get_config_value("build.cmd", feature)?;
    
    execute_command_in_dir(&build_cmd, "build.dir", feature, true, "build")
}

/// Run test command for a given feature
pub fn c2rust_test(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let test_cmd = get_config_value("test.cmd", feature)?;
    
    execute_command_in_dir(&test_cmd, "test.dir", feature, false, "test")
}

/// Run hybrid build test suite
/// Reports error and exits if c2rust-config is not available
pub fn run_hybrid_build(feature: &str) -> Result<()> {
    // Get build commands from config
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");
    
    if !config_path.exists() {
        eprintln!("{}", format!("Error: Config file not found at {}", config_path.display()).red());
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    // Check if c2rust-config is available before proceeding
    let check_output = Command::new("c2rust-config")
        .arg("--version")
        .output();
    
    if check_output.is_err() {
        eprintln!("{}", "Error: c2rust-config not found".red());
        anyhow::bail!("c2rust-config not found, cannot run hybrid build tests");
    }

    // Execute commands
    println!("│ {}", "Running hybrid build tests...".bright_blue().bold());
    c2rust_clean(feature)?;
    c2rust_build(feature)?;
    c2rust_test(feature)?;
    println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());

    Ok(())
}

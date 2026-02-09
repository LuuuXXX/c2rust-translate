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
/// 
/// Note: The `_show_full_output` parameter is currently unused because cargo build errors
/// are already displayed in full via the bail! macro. The parameter is kept for API
/// consistency with other display functions and potential future use.
pub fn cargo_build(feature: &str, _show_full_output: bool) -> Result<()> {
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

/// Set hybrid build environment variables if LD_PRELOAD is enabled
fn setup_hybrid_env(
    command: &mut Command, 
    project_root: &std::path::Path, 
    feature: &str, 
    set_ld_preload: bool,
    build_target: Option<&str>,
) -> Option<std::path::PathBuf> {
    if !set_ld_preload {
        return None;
    }
    
    let hybrid_lib = env::var("C2RUST_HYBRID_BUILD_LIB").ok()?;
    let c2rust_dir = project_root.join(".c2rust");
    let feature_root_path = c2rust_dir.join(feature);
    let rust_lib_path = feature_root_path.join("rust").join("target").join("debug").join("librust.a");

    command.env("LD_PRELOAD", &hybrid_lib);
    command.env("C2RUST_PROJECT_ROOT", project_root);
    command.env("C2RUST_FEATURE_ROOT", &feature_root_path);
    command.env("C2RUST_RUST_LIB", &rust_lib_path);
    
    // Set C2RUST_LD_TARGET from build.target if provided
    if let Some(target) = build_target {
        command.env("C2RUST_LD_TARGET", target);
    }
    
    Some(feature_root_path)
}

/// Print command execution details
fn print_command_details(
    command_type: &str,
    parts: &[String],
    exec_dir: &std::path::Path,
    project_root: &std::path::Path,
    feature_root: Option<&std::path::PathBuf>,
    build_target: Option<&str>,
    set_ld_preload: bool,
) {
    let colored_label = match command_type {
        "build" => "│ → Executing build command:".bright_blue().to_string(),
        "test" => "│ → Executing test command:".bright_green().to_string(),
        "clean" => "│ → Executing clean command:".bright_red().to_string(),
        _ => format!("│ → Executing {} command:", command_type),
    };
    
    println!("{}", colored_label);
    print!("│   ");
    
    if set_ld_preload {
        if let Ok(hybrid_lib) = env::var("C2RUST_HYBRID_BUILD_LIB") {
            let rust_lib_path = feature_root
                .map(|f| f.join("rust").join("target").join("debug").join("librust.a"))
                .unwrap_or_default();
            
            print!("LD_PRELOAD={} ", shell_words::quote(&hybrid_lib).dimmed());
            if let Some(feature_root) = feature_root {
                print!("C2RUST_FEATURE_ROOT={} ", shell_words::quote(&feature_root.display().to_string()).dimmed());
            }
            print!("C2RUST_PROJECT_ROOT={} ", shell_words::quote(&project_root.display().to_string()).dimmed());
            print!("C2RUST_RUST_LIB={} ", shell_words::quote(&rust_lib_path.display().to_string()).dimmed());
            
            // Show C2RUST_LD_TARGET if build.target was provided
            if let Some(target) = build_target {
                print!("C2RUST_LD_TARGET={} ", shell_words::quote(target).dimmed());
            }
        }
    }
    
    println!("{}", shell_words::join(parts).bright_yellow());
    println!("│   {}: {}", "Working directory".dimmed(), exec_dir.display());
}

/// Execute a command in a configured directory
fn execute_command_in_dir(
    command_str: &str,
    dir_key: &str,
    feature: &str,
    set_ld_preload: bool,
    command_type: &str,
) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let dir_str = get_config_value(dir_key, feature)?;
    
    // Validate path safety
    if std::path::Path::new(&dir_str).is_absolute() {
        anyhow::bail!("Directory path from config must be relative, got: {}", dir_str);
    }
    if dir_str.contains("..") {
        anyhow::bail!("Directory path from config cannot contain '..', got: {}", dir_str);
    }
    
    let parts = shell_words::split(command_str)
        .with_context(|| format!("Failed to parse command: {}", command_str))?;
    
    if parts.is_empty() {
        return Ok(());
    }
    
    if parts[0].is_empty() {
        anyhow::bail!("Command cannot be empty");
    }
    
    let project_root = util::find_project_root()?;
    let exec_dir = project_root.join(&dir_str);
    
    if !exec_dir.exists() {
        anyhow::bail!("Directory does not exist: {}", exec_dir.display());
    } else if !exec_dir.is_dir() {
        anyhow::bail!("Path is not a directory: {}", exec_dir.display());
    }
    
    // Get build.target once for both env setup and printing
    // Distinguish between "not set" (Ok with empty check) vs actual errors
    let build_target = match get_config_value("build.target", feature) {
        Ok(target) if !target.is_empty() => Some(target),
        Ok(_) => None, // Empty value means not set
        Err(e) => {
            // Check if it's just a "key not found" error vs a real failure
            let err_str = e.to_string();
            if err_str.contains("Empty") || err_str.contains("not found") {
                None // Key not set is acceptable
            } else {
                // Real config error - emit warning but continue
                eprintln!("Warning: Failed to read build.target from config: {}", e);
                None
            }
        }
    };
    
    let mut command = Command::new(&parts[0]);
    command.current_dir(&exec_dir);
    
    if parts.len() > 1 {
        command.args(&parts[1..]);
    }
    
    let feature_root = setup_hybrid_env(&mut command, &project_root, feature, set_ld_preload, build_target.as_deref());
    print_command_details(command_type, &parts, &exec_dir, &project_root, feature_root.as_ref(), build_target.as_deref(), set_ld_preload);
    
    let start_time = Instant::now();
    let output = command.output()
        .with_context(|| format!("Failed to execute command: {}", command_str))?;
    let duration = start_time.elapsed();

    if !output.status.success() {
        print_command_failure(command_type, &output, duration);
        
        // Include error details in the bail message for better debugging
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_summary = stderr
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n");
        
        if stderr_summary.is_empty() {
            anyhow::bail!("Command '{}' failed with non-zero exit status", command_str);
        } else {
            anyhow::bail!(
                "Command '{}' failed with non-zero exit status. Stderr (first lines):\n{}",
                command_str,
                stderr_summary
            );
        }
    }

    print_command_success(command_type, duration);
    Ok(())
}

/// Print command failure message
fn print_command_failure(command_type: &str, output: &std::process::Output, duration: std::time::Duration) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    println!("│ {} (took {:.2}s)", 
        format!("✗ {} failed", command_type.to_uppercase()).bright_red().bold(), 
        duration.as_secs_f64()
    );
    
    if !stderr.is_empty() {
        eprintln!("stderr: {}", stderr);
    }
    if !stdout.is_empty() {
        println!("stdout: {}", stdout);
    }
}

/// Print command success message
fn print_command_success(command_type: &str, duration: std::time::Duration) {
    let success_msg = match command_type {
        "build" => format!("│ {} (took {:.2}s)", "✓ Build successful".bright_green().bold(), duration.as_secs_f64()),
        "test" => format!("│ {} (took {:.2}s)", "✓ Test successful".bright_green().bold(), duration.as_secs_f64()),
        "clean" => format!("│ {} (took {:.2}s)", "✓ Clean successful".bright_green().bold(), duration.as_secs_f64()),
        _ => format!("│ ✓ {} successful (took {:.2}s)", command_type, duration.as_secs_f64()),
    };
    println!("{}", success_msg);
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
    run_hybrid_build_interactive(feature, None, None)
}

/// Run hybrid build test suite with interactive error handling
/// file_type and rs_file are needed for interactive error handling
pub fn run_hybrid_build_interactive(
    feature: &str, 
    file_type: Option<&str>,
    rs_file: Option<&std::path::Path>
) -> Result<()> {
    
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
    
    // Test with interactive error handling
    match c2rust_test(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());
            Ok(())
        }
        Err(test_error) => {
            // Only show interactive menu if we have file context
            if let (Some(ftype), Some(rfile)) = (file_type, rs_file) {
                handle_test_failure_interactive(feature, ftype, rfile, test_error)
            } else {
                // No file context, just return the error
                Err(test_error)
            }
        }
    }
}

/// Handle test failure interactively
fn handle_test_failure_interactive(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    test_error: anyhow::Error,
) -> Result<()> {
    use crate::interaction;
    use crate::suggestion;
    use crate::translator;
    
    println!("│");
    println!("│ {}", "⚠ Hybrid build tests failed!".red().bold());
    println!("│ {}", "The test suite did not pass.".yellow());
    
    // Display full C and Rust code for user reference
    let c_file = rs_file.with_extension("c");
    
    // Show file locations
    interaction::display_file_paths(Some(&c_file), rs_file);
    
    // Display full code (always show full for interactive mode)
    println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
    translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);
    
    println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
    translator::display_code(rs_file, "─ Rust Code ─", usize::MAX, true);
    
    println!("│ {}", "═══ Test Error ═══".bright_red().bold());
    println!("│ {}", test_error);
    
    // Get user choice - for test failures, suggestion is REQUIRED for Continue
    let choice = interaction::prompt_user_choice("Test failure", true)?;
    
    match choice {
        interaction::UserChoice::Continue => {
            println!("│");
            println!("│ {}", "You chose: Continue trying with a new suggestion".bright_cyan());
            
            // For test failures, suggestion is REQUIRED
            let suggestion_text = interaction::prompt_suggestion(true)?
                .expect("Suggestion should be present when required");
            
            // Save suggestion to c2rust.md
            suggestion::append_suggestion(&suggestion_text)?;
            
            // Apply fix with the suggestion
            println!("│");
            println!("│ {}", "Applying fix based on your suggestion...".bright_blue());
            
            let format_progress = |op: &str| format!("Fix for test failure - {}", op);
            crate::apply_error_fix(feature, file_type, rs_file, &test_error, &format_progress, true)?;
            
            // Try to build and test again
            println!("│");
            println!("│ {}", "Rebuilding and retesting...".bright_blue().bold());
            
            cargo_build(feature, true)?;
            
            match c2rust_test(feature) {
                Ok(_) => {
                    println!("│ {}", "✓ Tests passed after applying fix!".bright_green().bold());
                    Ok(())
                }
                Err(e) => {
                    println!("│ {}", "✗ Tests still failing".red());
                    
                    // Ask if user wants to try again
                    println!("│");
                    println!("│ {}", "Tests still have errors. What would you like to do?".yellow());
                    let retry_choice = interaction::prompt_user_choice("Tests still failing", true)?;
                    
                    match retry_choice {
                        interaction::UserChoice::Continue | interaction::UserChoice::ManualFix => {
                            // Recursively handle again
                            handle_test_failure_interactive(feature, file_type, rs_file, e)
                        }
                        interaction::UserChoice::Exit => {
                            Err(e).context("Tests failed and user chose to exit")
                        }
                    }
                }
            }
        }
        interaction::UserChoice::ManualFix => {
            println!("│");
            println!("│ {}", "You chose: Manual fix".bright_cyan());
            
            // Try to open vim
            match interaction::open_in_vim(rs_file) {
                Ok(_) => {
                    println!("│");
                    println!("│ {}", "Vim editing completed. Rebuilding and retesting...".bright_blue());
                    
                    // Try building and testing after manual edit
                    cargo_build(feature, true)?;
                    
                    match c2rust_test(feature) {
                        Ok(_) => {
                            println!("│ {}", "✓ Tests passed after manual fix!".bright_green().bold());
                            Ok(())
                        }
                        Err(e) => {
                            println!("│ {}", "✗ Tests still failing after manual fix".red());
                            
                            // Ask if user wants to try again
                            println!("│");
                            println!("│ {}", "Tests still have errors. What would you like to do?".yellow());
                            let retry_choice = interaction::prompt_user_choice("Tests still failing", true)?;
                            
                            match retry_choice {
                                interaction::UserChoice::Continue | interaction::UserChoice::ManualFix => {
                                    // Recursively handle again
                                    handle_test_failure_interactive(feature, file_type, rs_file, e)
                                }
                                interaction::UserChoice::Exit => {
                                    Err(e).context("Tests failed after manual fix and user chose to exit")
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(test_error).context("Tests failed and could not open vim")
                }
            }
        }
        interaction::UserChoice::Exit => {
            println!("│");
            println!("│ {}", "You chose: Exit".yellow());
            println!("│ {}", "Skipping due to test failures.".yellow());
            Err(test_error).context("Tests failed and user chose to exit")
        }
    }
}

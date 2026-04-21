use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;
use std::time::Instant;

// Public re-exports
pub use crate::hybrid_build::{
    c2rust_build, c2rust_clean, c2rust_test,
    execute_command_in_dir_with_type, get_config_value,
    run_hybrid_build, run_hybrid_build_interactive,
};
pub use crate::workflow::run_full_build_and_test;

// Crate-internal re-exports
pub(crate) use crate::hybrid_build::{
    c2rust_build_no_analysis, c2rust_clean_no_analysis, c2rust_test_no_analysis,
};
pub(crate) use crate::workflow::{
    get_manual_fix_files, handle_build_failure_interactive, handle_test_failure_interactive,
    run_full_build_and_test_interactive,
};

fn run_cargo_subcommand(
    feature: &str,
    suppress_warnings: bool,
    subcommand: &str,
    exec_error_msg: &str,
    failure_label: &str,
    success_label: &str,
) -> Result<Option<String>> {
    util::validate_feature_name(feature)?;
    let project_root = util::find_project_root()?;
    let build_dir = project_root.join(".c2rust").join(feature).join("rust");
    let start_time = Instant::now();
    let mut cmd = Command::new("cargo");
    cmd.arg(subcommand).current_dir(&build_dir);
    cmd.env("RUSTC_BOOTSTRAP", "1");
    if suppress_warnings {
        cmd.env("RUSTFLAGS", "-A warnings");
    }
    let output = cmd.output().with_context(|| exec_error_msg.to_string())?;
    let duration = start_time.elapsed();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        anyhow::bail!("{}: {}", failure_label, stderr);
    }
    println!("  {} (took {:.2}s)", success_label.bright_green(), duration.as_secs_f64());
    if !suppress_warnings {
        let has_warnings = stderr.lines().any(|line| line.contains("warning[") || line.contains("warning:"));
        if has_warnings {
            return Ok(Some(stderr));
        }
    }
    Ok(None)
}

pub fn cargo_build(feature: &str, suppress_warnings: bool, _show_full_output: bool) -> Result<Option<String>> {
    run_cargo_subcommand(feature, suppress_warnings, "build", "Failed to execute cargo build", "Build error", "Build completed")
}

pub fn cargo_check(feature: &str, suppress_warnings: bool, _show_full_output: bool) -> Result<Option<String>> {
    run_cargo_subcommand(feature, suppress_warnings, "check", "Failed to execute cargo check", "Check error", "Check completed")
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_detect_warning_code_format() {
        let stderr = "warning[unused_variables]: unused variable `x`\n  --> src/foo.rs:5:9";
        let has = stderr.lines().any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(has);
    }
    #[test]
    fn test_detect_warning_colon_format() {
        let stderr = "warning: unused import: `std::fmt`\n  --> src/bar.rs:1:5";
        let has = stderr.lines().any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(has);
    }
    #[test]
    fn test_no_warnings_clean_output() {
        let stderr = "   Compiling myproject v0.1.0\n    Finished dev [unoptimized] target(s) in 1.23s";
        let has = stderr.lines().any(|l| l.contains("warning[") || l.contains("warning:"));
        assert!(!has);
    }
}

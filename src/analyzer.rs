use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// Shared helper: validates the feature name and runs `code_analyse` with the
/// assembled argument list: `<pre_args...> --feature <feature> <post_args...>`.
fn run_code_analyse(pre_args: &[&str], feature: &str, post_args: &[&str]) -> Result<()> {
    util::validate_feature_name(feature)?;
    let project_root = util::find_project_root()?;

    let mut args: Vec<&str> = pre_args.to_vec();
    args.push("--feature");
    args.push(feature);
    args.extend_from_slice(post_args);

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(&args)
        .output()
        .with_context(|| format!("Failed to execute code_analyse {:?}", args))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "code_analyse {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            stdout,
            stderr
        );
    }

    Ok(())
}

/// Initialize code analysis for a feature.
pub fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code_analyse --init --feature {}", feature);
    run_code_analyse(&["--init"], feature, &[])
}

/// Update code analysis for a feature.
pub fn update_code_analysis(feature: &str) -> Result<()> {
    run_code_analyse(&["--update"], feature, &[])
}

/// Notify code_analyse of build success after tests pass.
pub fn update_code_analysis_build_success(feature: &str) -> Result<()> {
    run_code_analyse(&["--update"], feature, &["--build-success"])
}

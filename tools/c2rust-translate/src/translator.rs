use std::path::Path;
use crate::error::{AutoTranslateError, Result};
use crate::commands::execute_command_checked;

/// Translate a C file to Rust using c2rust-translate
pub fn translate_c_to_rust(c_file: &Path, feature_name: &str) -> Result<()> {
    let filename = c_file.to_str()
        .ok_or_else(|| AutoTranslateError::TranslationFailed("Invalid file path".to_string()))?;
    
    let output = execute_command_checked(
        "c2rust-translate",
        &["translate", "--feature", feature_name, filename],
        None,
        &[]
    ).map_err(|e| AutoTranslateError::TranslationFailed(format!("Translation failed: {}", e)))?;
    
    println!("Translation output: {}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

/// Run code analysis on a file
pub fn analyze_code(filename: &str) -> Result<()> {
    let output = execute_command_checked(
        "code-analyse",
        &["--feature", filename],
        None,
        &[]
    ).map_err(|e| AutoTranslateError::TranslationFailed(format!("Code analysis failed: {}", e)))?;
    
    println!("Analysis output: {}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

/// Get configuration from c2rust-config
pub fn get_config_list(config_type: &str) -> Result<Vec<String>> {
    let output = execute_command_checked(
        "c2rust-config",
        &["config", "--list", config_type],
        None,
        &[]
    )?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let configs: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    
    Ok(configs)
}

/// Execute build with mixed compilation setup
pub fn execute_build(
    build_dir: &Path,
    build_command: &str,
    feature_root: &Path,
    ld_preload_path: &str,
) -> Result<()> {
    let feature_root_str = feature_root.to_str()
        .ok_or_else(|| AutoTranslateError::BuildFailed("Invalid feature root path".to_string()))?;
    
    let env_vars = [
        ("LD_PRELOAD", ld_preload_path),
        ("C2RUST_FEATURE_ROOT", feature_root_str),
    ];
    
    // Split command into program and args
    let parts: Vec<&str> = build_command.split_whitespace().collect();
    if parts.is_empty() {
        return Err(AutoTranslateError::BuildFailed("Empty build command".to_string()));
    }
    
    let program = parts[0];
    let args = &parts[1..];
    
    let output = execute_command_checked(
        program,
        args,
        Some(build_dir),
        &env_vars
    ).map_err(|e| AutoTranslateError::BuildFailed(format!("Build failed: {}", e)))?;
    
    println!("Build output: {}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

/// Execute tests
pub fn execute_test(
    test_dir: &Path,
    test_command: &str,
    feature_root: &Path,
    ld_preload_path: &str,
) -> Result<()> {
    let feature_root_str = feature_root.to_str()
        .ok_or_else(|| AutoTranslateError::TestFailed("Invalid feature root path".to_string()))?;
    
    let env_vars = [
        ("LD_PRELOAD", ld_preload_path),
        ("C2RUST_FEATURE_ROOT", feature_root_str),
    ];
    
    // Split command into program and args
    let parts: Vec<&str> = test_command.split_whitespace().collect();
    if parts.is_empty() {
        return Err(AutoTranslateError::TestFailed("Empty test command".to_string()));
    }
    
    let program = parts[0];
    let args = &parts[1..];
    
    let output = execute_command_checked(
        program,
        args,
        Some(test_dir),
        &env_vars
    ).map_err(|e| AutoTranslateError::TestFailed(format!("Test failed: {}", e)))?;
    
    println!("Test output: {}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

#[cfg(test)]
mod tests {
    // Empty test module - actual tests would require c2rust-config to be installed
}

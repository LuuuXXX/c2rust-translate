use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the config.toml path from C2RUST_PROJECT_ROOT environment variable
fn get_config_path() -> Result<PathBuf> {
    let project_root = std::env::var("C2RUST_PROJECT_ROOT")
        .context("C2RUST_PROJECT_ROOT environment variable not set")?;
    Ok(PathBuf::from(project_root).join(".c2rust/config.toml"))
}

/// Translate a C file to Rust using the translation tool
pub fn translate_c_to_rust(file_type: &str, c_file: &Path, rs_file: &Path) -> Result<()> {
    let config_path = get_config_path()?;
    
    let output = Command::new("python")
        .args(&[
            "translate_and_fix.py",
            "--config",
            config_path.to_str().unwrap(),
            "--type",
            file_type,
            "--code",
            c_file.to_str().unwrap(),
            "--output",
            rs_file.to_str().unwrap(),
        ])
        .output()
        .context("Failed to execute translate_and_fix.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Translation failed: {}", stderr);
    }

    Ok(())
}

/// Fix translation errors using the translation tool
pub fn fix_translation_error(file_type: &str, rs_file: &Path, error_msg: &str) -> Result<()> {
    let config_path = get_config_path()?;
    
    // Create a temporary file with error message
    let temp_dir = std::env::temp_dir();
    let error_file = temp_dir.join("build_error.txt");
    fs::write(&error_file, error_msg)?;

    let output = Command::new("python")
        .args(&[
            "translate_and_fix.py",
            "--config",
            config_path.to_str().unwrap(),
            "--type",
            file_type,
            "--error",
            error_file.to_str().unwrap(),
            "--output",
            rs_file.to_str().unwrap(),
        ])
        .output()
        .context("Failed to execute translate_and_fix.py for fixing")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Fix failed: {}", stderr);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_temp_error_file_creation() {
        use std::fs;
        
        let temp_dir = std::env::temp_dir();
        let error_file = temp_dir.join("test_build_error.txt");
        let test_msg = "test error message";
        
        fs::write(&error_file, test_msg).unwrap();
        let content = fs::read_to_string(&error_file).unwrap();
        
        assert_eq!(content, test_msg);
        fs::remove_file(&error_file).ok();
    }
}

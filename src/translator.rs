use anyhow::{Context, Result};
use std::fs;
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

/// Get the config.toml path by searching for .c2rust directory
fn get_config_path() -> Result<PathBuf> {
    let project_root = find_project_root()?;
    Ok(project_root.join(".c2rust/config.toml"))
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

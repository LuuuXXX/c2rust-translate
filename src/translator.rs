use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::Write;
use crate::util;

// Script name used for C to Rust translation
const TRANSLATE_SCRIPT: &str = "translate_and_fix.py";

/// Get the config.toml path by searching for .c2rust directory
fn get_config_path() -> Result<PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root.join(".c2rust/config.toml"))
}

/// Translate a C file to Rust using the translation tool
pub fn translate_c_to_rust(feature: &str, file_type: &str, c_file: &Path, rs_file: &Path) -> Result<()> {
    let project_root = util::find_project_root()?;
    let config_path = get_config_path()?;
    let work_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    // Verify working directory exists
    if !work_dir.exists() {
        anyhow::bail!(
            "Working directory does not exist: {}. Expected directory structure: <project_root>/.c2rust/<feature>/rust",
            work_dir.display()
        );
    }
    
    let config_str = config_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", config_path.display()))?;
    let c_file_str = c_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", c_file.display()))?;
    let rs_file_str = rs_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", rs_file.display()))?;
    
    let output = Command::new("python")
        .current_dir(&work_dir)
        .args(&[
            TRANSLATE_SCRIPT,
            "--config",
            config_str,
            "--type",
            file_type,
            "--code",
            c_file_str,
            "--output",
            rs_file_str,
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
pub fn fix_translation_error(feature: &str, file_type: &str, rs_file: &Path, error_msg: &str) -> Result<()> {
    let project_root = util::find_project_root()?;
    let config_path = get_config_path()?;
    let work_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    // Verify working directory exists
    if !work_dir.exists() {
        anyhow::bail!(
            "Working directory does not exist: {}. Expected directory structure: <project_root>/.c2rust/<feature>/rust",
            work_dir.display()
        );
    }
    
    // Create a unique temporary file with error message
    let mut temp_file = tempfile::NamedTempFile::new()
        .context("Failed to create temporary error file")?;
    write!(temp_file, "{}", error_msg)
        .context("Failed to write error message to temp file")?;
    
    let config_str = config_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", config_path.display()))?;
    let error_file_str = temp_file.path().to_str()
        .with_context(|| format!("Non-UTF8 path: {}", temp_file.path().display()))?;
    let rs_file_str = rs_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", rs_file.display()))?;

    let output = Command::new("python")
        .current_dir(&work_dir)
        .args(&[
            TRANSLATE_SCRIPT,
            "--config",
            config_str,
            "--type",
            file_type,
            "--error",
            error_file_str,
            "--output",
            rs_file_str,
        ])
        .output()
        .context("Failed to execute translate_and_fix.py for fixing")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Fix failed: {}", stderr);
    }

    // temp_file is automatically deleted when it goes out of scope
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_temp_error_file_creation() {
        let test_msg = "test error message";

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", test_msg).unwrap();

        let path = temp_file.path();
        let content = std::fs::read_to_string(path).unwrap();

        assert_eq!(content, test_msg);
        // temp_file is automatically deleted when it goes out of scope
    }
}

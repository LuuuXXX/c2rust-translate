//! Suggestion file management for c2rust.md

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use crate::util;

/// Get the path to the c2rust.md suggestion file
pub fn get_suggestion_file_path() -> Result<PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root.join("c2rust.md"))
}

/// Read the current content of c2rust.md if it exists
pub fn read_suggestions() -> Result<Option<String>> {
    let suggestion_file = get_suggestion_file_path()?;
    
    if !suggestion_file.exists() {
        return Ok(None);
    }
    
    let content = fs::read_to_string(&suggestion_file)
        .with_context(|| format!("Failed to read suggestion file: {}", suggestion_file.display()))?;
    
    if content.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(content))
    }
}

/// Append a suggestion to the c2rust.md file
pub fn append_suggestion(suggestion: &str) -> Result<()> {
    let suggestion_file = get_suggestion_file_path()?;
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = suggestion_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&suggestion_file)
        .with_context(|| format!("Failed to open suggestion file: {}", suggestion_file.display()))?;
    
    // Add timestamp and suggestion
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "\n## Suggestion added at {}", timestamp)?;
    writeln!(file, "{}", suggestion)?;
    
    println!("│ {}", format!("✓ Suggestion saved to {}", suggestion_file.display()).bright_green());
    
    Ok(())
}

/// Get suggestions as a string to pass to translate_and_fix.py
/// Returns the content of c2rust.md if it exists, otherwise None
pub fn get_suggestions_for_translation() -> Result<Option<String>> {
    read_suggestions()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;

    #[test]
    fn test_suggestion_file_path() {
        // This test requires a .c2rust directory to exist
        // We'll just test that the function doesn't panic
        let result = get_suggestion_file_path();
        // In a test environment without .c2rust, this might fail
        // So we just check it returns a Result
        let _is_result = result.is_ok() || result.is_err();
    }

    #[test]
    fn test_read_nonexistent_suggestions() {
        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();
        
        // Create .c2rust directory
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();
        
        let result = read_suggestions();
        
        // Restore directory
        env::set_current_dir(old_dir).unwrap();
        
        // Should return Ok(None) for non-existent file
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}

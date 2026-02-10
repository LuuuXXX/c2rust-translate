//! Suggestion file management for suggestions.txt

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use crate::util;

/// Get the path to the suggestions.txt suggestion file
pub fn get_suggestion_file_path() -> Result<PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root.join("suggestions.txt"))
}

/// Read the current content of suggestions.txt if it exists
#[cfg(test)]
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

/// Append a suggestion to the suggestions.txt file
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
    
    // Append suggestion in plain text format
    writeln!(file, "{}", suggestion)?;
    
    println!("│ {}", format!("✓ Suggestion saved to {}", suggestion_file.display()).bright_green());
    
    Ok(())
}

/// Clear all suggestions from the suggestions.txt file
/// This is useful when starting a fresh retry to avoid suggestion accumulation
pub fn clear_suggestions() -> Result<()> {
    let suggestion_file = get_suggestion_file_path()?;
    
    if suggestion_file.exists() {
        fs::remove_file(&suggestion_file)
            .with_context(|| format!("Failed to remove suggestion file: {}", suggestion_file.display()))?;
        println!("│ {}", "✓ Cleared previous suggestions for fresh retry".bright_yellow());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_suggestion_file_path() {
        // Create a temp directory to act as the project root
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // Create .c2rust directory inside the temp project root
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = get_suggestion_file_path();

        // Restore original working directory
        env::set_current_dir(old_dir).unwrap();

        // The path should be valid and point to suggestions.txt in the project root
        assert!(result.is_ok());
        let path = result.unwrap();
        assert_eq!(path.file_name().unwrap(), "suggestions.txt");
    }

    #[test]
    #[serial]
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

    #[test]
    #[serial]
    fn test_append_suggestion() {
        // Create a temp directory to act as the project root
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // Create .c2rust directory inside the temp project root
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // Append a suggestion
        let suggestion_text = "Use std::ffi::CStr instead of raw pointers";
        let result = append_suggestion(suggestion_text);
        assert!(result.is_ok());

        // Read back the file and verify content
        let suggestion_file = get_suggestion_file_path().unwrap();
        assert!(suggestion_file.exists());
        
        let content = fs::read_to_string(&suggestion_file).unwrap();
        assert!(content.contains(suggestion_text));
        // Plain text format - no timestamps
        assert!(!content.contains("## Suggestion added at"));

        // Append another suggestion
        let second_suggestion = "Ensure proper lifetime annotations";
        let result2 = append_suggestion(second_suggestion);
        assert!(result2.is_ok());

        // Verify both suggestions are present
        let content2 = fs::read_to_string(&suggestion_file).unwrap();
        assert!(content2.contains(suggestion_text));
        assert!(content2.contains(second_suggestion));
        
        // Plain text format - no timestamp headers
        assert!(!content2.contains("## Suggestion added at"));

        // Restore original working directory before temp_dir is dropped
        env::set_current_dir(&old_dir).unwrap();
    }
    
    #[test]
    #[serial]
    fn test_clear_suggestions() {
        // Create a temp directory to act as the project root
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // Create .c2rust directory inside the temp project root
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // First, create a suggestion file with some content
        let suggestion_text = "Test suggestion";
        let result = append_suggestion(suggestion_text);
        assert!(result.is_ok());
        
        let suggestion_file = get_suggestion_file_path().unwrap();
        assert!(suggestion_file.exists());

        // Now clear the suggestions
        let clear_result = clear_suggestions();
        assert!(clear_result.is_ok());

        // Verify the file no longer exists
        assert!(!suggestion_file.exists());

        // Clearing again should be a no-op and not error
        let clear_again = clear_suggestions();
        assert!(clear_again.is_ok());

        // Restore original working directory before temp_dir is dropped
        env::set_current_dir(&old_dir).unwrap();
    }
    
    #[test]
    #[serial]
    fn test_suggestion_workflow_with_retry() {
        // Simulate the retry workflow: add suggestion -> clear -> add new suggestion
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // First attempt - add a suggestion
        let first_suggestion = "First attempt: Use smart pointers";
        append_suggestion(first_suggestion).unwrap();
        
        let content1 = read_suggestions().unwrap();
        assert!(content1.is_some());
        assert!(content1.unwrap().contains(first_suggestion));

        // Retry - clear suggestions before retry
        clear_suggestions().unwrap();
        
        let content_after_clear = read_suggestions().unwrap();
        assert!(content_after_clear.is_none());

        // Second attempt - add a different suggestion
        let second_suggestion = "Second attempt: Use Option<T> for nullable values";
        append_suggestion(second_suggestion).unwrap();
        
        let content2 = read_suggestions().unwrap();
        assert!(content2.is_some());
        let final_content = content2.unwrap();
        
        // Should only contain second suggestion, not first
        assert!(final_content.contains(second_suggestion));
        assert!(!final_content.contains(first_suggestion));

        // Restore original working directory before temp_dir is dropped
        env::set_current_dir(&old_dir).unwrap();
    }
}

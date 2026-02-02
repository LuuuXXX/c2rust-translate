use std::path::Path;
use crate::error::{AutoTranslateError, Result};
use crate::commands::{execute_command_checked, execute_command};

/// Add a file to git staging area
pub fn git_add(file_path: &Path, repo_root: &Path) -> Result<()> {
    let file_str = file_path.to_str()
        .ok_or_else(|| AutoTranslateError::GitFailed("Invalid file path".to_string()))?;
    
    execute_command_checked("git", &["add", file_str], Some(repo_root), &[])
        .map_err(|e| AutoTranslateError::GitFailed(format!("Failed to add file: {}", e)))?;
    
    Ok(())
}

/// Commit changes with a message
pub fn git_commit(message: &str, repo_root: &Path) -> Result<()> {
    execute_command_checked("git", &["commit", "-m", message], Some(repo_root), &[])
        .map_err(|e| AutoTranslateError::GitFailed(format!("Failed to commit: {}", e)))?;
    
    Ok(())
}

/// Check if there are uncommitted changes
pub fn has_uncommitted_changes(repo_root: &Path) -> Result<bool> {
    let output = execute_command("git", &["status", "--porcelain"], Some(repo_root), &[])
        .map_err(|e| AutoTranslateError::GitFailed(format!("Failed to check status: {}", e)))?;
    
    Ok(!output.stdout.is_empty())
}

/// Get the current git branch name
pub fn get_current_branch(repo_root: &Path) -> Result<String> {
    let output = execute_command_checked(
        "git",
        &["rev-parse", "--abbrev-ref", "HEAD"],
        Some(repo_root),
        &[]
    )?;
    
    let branch = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();
    
    Ok(branch)
}

/// Create and checkout a new branch
pub fn create_branch(branch_name: &str, repo_root: &Path) -> Result<()> {
    execute_command_checked("git", &["checkout", "-b", branch_name], Some(repo_root), &[])
        .map_err(|e| AutoTranslateError::GitFailed(format!("Failed to create branch: {}", e)))?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_git_operations() {
        // Create a temporary git repository
        let temp_dir = std::env::temp_dir().join("test_git_repo");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        
        // Initialize git repo
        let output = Command::new("git")
            .args(&["init"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        
        if !output.status.success() {
            // Skip test if git init fails
            fs::remove_dir_all(temp_dir).ok();
            return;
        }
        
        // Configure git
        Command::new("git")
            .args(&["config", "user.email", "test@test.com"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        
        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        
        // Create an initial commit
        let test_file = temp_dir.join("test.txt");
        fs::write(&test_file, "test").unwrap();
        Command::new("git")
            .args(&["add", "."])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        
        // Test get_current_branch
        let branch = get_current_branch(&temp_dir);
        // Branch might be "main" or "master" depending on git version
        if let Ok(b) = branch {
            assert!(b == "main" || b == "master");
        }
        
        // Cleanup
        fs::remove_dir_all(temp_dir).ok();
    }
}

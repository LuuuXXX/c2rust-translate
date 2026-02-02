use anyhow::{Context, Result};
use std::path::PathBuf;

/// Find the project root by searching upward for .c2rust directory
pub fn find_project_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()
        .context("Failed to get current directory")?;
    
    loop {
        let c2rust_dir = current.join(".c2rust");
        
        // Use metadata to properly handle IO errors
        match std::fs::metadata(&c2rust_dir) {
            Ok(metadata) if metadata.is_dir() => {
                return Ok(current);
            }
            Ok(_) => {
                // .c2rust exists but is not a directory, continue searching
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // .c2rust doesn't exist, continue searching
            }
            Err(e) => {
                // Other IO error (permissions, etc.)
                return Err(e).with_context(|| {
                    format!("Failed to access .c2rust directory at {}", c2rust_dir.display())
                });
            }
        }
        
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => anyhow::bail!("Could not find .c2rust directory in any parent directory"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_project_root_from_nested_dir() {
        // Create a temporary directory structure:
        // temp/
        //   .c2rust/
        //   subdir1/
        //     subdir2/
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();
        
        let subdir1 = temp_dir.path().join("subdir1");
        let subdir2 = subdir1.join("subdir2");
        fs::create_dir_all(&subdir2).unwrap();
        
        // Change to nested subdirectory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&subdir2).unwrap();
        
        // Should find the .c2rust directory in the parent
        let result = find_project_root();
        
        // Restore original directory
        std::env::set_current_dir(&original_dir).unwrap();
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_find_project_root_not_found() {
        // Create a temporary directory without .c2rust
        let temp_dir = tempdir().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        
        // Change to the subdirectory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&subdir).unwrap();
        
        // Should fail to find .c2rust directory
        let result = find_project_root();
        
        // Restore original directory
        std::env::set_current_dir(&original_dir).unwrap();
        
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Could not find .c2rust"));
    }

    #[test]
    fn test_find_project_root_from_root_dir() {
        // Create a temporary directory with .c2rust at the root
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();
        
        // Change to the root directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();
        
        // Should find .c2rust in current directory
        let result = find_project_root();
        
        // Restore original directory
        std::env::set_current_dir(&original_dir).unwrap();
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), temp_dir.path());
    }
}

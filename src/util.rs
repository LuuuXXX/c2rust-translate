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

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Find the project root by searching upward for .c2rust directory
pub fn find_project_root() -> Result<PathBuf> {
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

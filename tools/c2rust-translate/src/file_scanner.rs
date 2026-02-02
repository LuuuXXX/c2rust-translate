use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use crate::error::{AutoTranslateError, Result};

/// Find all .rs files in the given directory
pub fn find_rust_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut rust_files = Vec::new();
    
    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            rust_files.push(path.to_path_buf());
        }
    }
    
    Ok(rust_files)
}

/// Find empty .rs files (files with no content or only whitespace)
pub fn find_empty_rust_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let rust_files = find_rust_files(dir)?;
    let mut empty_files = Vec::new();
    
    for file in rust_files {
        let content = fs::read_to_string(&file)
            .map_err(|e| AutoTranslateError::FileScanError(
                format!("Failed to read file {:?}: {}", file, e)
            ))?;
        
        if content.trim().is_empty() {
            empty_files.push(file);
        }
    }
    
    Ok(empty_files)
}

/// Get the corresponding C file path for a Rust file
pub fn get_c_file_for_rust(rust_file: &Path) -> Option<PathBuf> {
    let c_file = rust_file.with_extension("c");
    if c_file.exists() {
        Some(c_file)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, create_dir_all};
    use std::io::Write;

    #[test]
    fn test_find_rust_files() {
        let temp_dir = std::env::temp_dir().join("test_scanner");
        create_dir_all(&temp_dir).unwrap();
        
        // Create test files
        File::create(temp_dir.join("test1.rs")).unwrap();
        File::create(temp_dir.join("test2.rs")).unwrap();
        File::create(temp_dir.join("test.txt")).unwrap();
        
        let result = find_rust_files(&temp_dir).unwrap();
        assert_eq!(result.len(), 2);
        
        std::fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_find_empty_rust_files() {
        let temp_dir = std::env::temp_dir().join("test_empty");
        create_dir_all(&temp_dir).unwrap();
        
        // Create empty file
        File::create(temp_dir.join("empty.rs")).unwrap();
        
        // Create non-empty file
        let mut file = File::create(temp_dir.join("nonempty.rs")).unwrap();
        writeln!(file, "fn main() {{}}").unwrap();
        
        let result = find_empty_rust_files(&temp_dir).unwrap();
        assert_eq!(result.len(), 1);
        
        std::fs::remove_dir_all(temp_dir).ok();
    }
}

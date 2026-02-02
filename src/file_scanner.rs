use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Find all empty .rs files in the given directory
pub fn find_empty_rs_files(rust_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut empty_files = Vec::new();

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "rs") {
            let metadata = fs::metadata(path)?;
            if metadata.len() == 0 {
                empty_files.push(path.to_path_buf());
            }
        }
    }

    Ok(empty_files)
}

/// Extract file type from filename (var_ or fun_ prefix)
pub fn extract_file_type(filename: &str) -> Option<(&'static str, &str)> {
    if filename.starts_with("var_") {
        Some(("var", &filename[4..]))
    } else if filename.starts_with("fun_") {
        Some(("fn", &filename[4..]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_find_empty_rs_files() {
        // Create a unique temporary directory structure
        let temp_dir = tempdir().unwrap();

        // Create an empty .rs file
        let empty_file = temp_dir.path().join("var_test.rs");
        fs::File::create(&empty_file).unwrap();

        // Create a non-empty .rs file
        let non_empty_file = temp_dir.path().join("fun_test.rs");
        let mut file = fs::File::create(&non_empty_file).unwrap();
        file.write_all(b"fn test() {}").unwrap();

        // Test finding empty files
        let empty_files = find_empty_rs_files(temp_dir.path()).unwrap();
        assert_eq!(empty_files.len(), 1);
        assert!(empty_files[0].ends_with("var_test.rs"));

        // temp_dir is automatically deleted when it goes out of scope
    }

    #[test]
    fn test_extract_file_type_var() {
        let (file_type, name) = extract_file_type("var_counter").unwrap();
        assert_eq!(file_type, "var");
        assert_eq!(name, "counter");
    }

    #[test]
    fn test_extract_file_type_fun() {
        let (file_type, name) = extract_file_type("fun_calculate").unwrap();
        assert_eq!(file_type, "fn");
        assert_eq!(name, "calculate");
    }

    #[test]
    fn test_extract_file_type_invalid() {
        let result = extract_file_type("invalid_name");
        assert!(result.is_none());
    }

    #[test]
    fn test_path_construction() {
        let feature = "my_feature";
        let feature_path = PathBuf::from(feature);
        let rust_dir = feature_path.join("rust");
        
        assert_eq!(rust_dir.to_str().unwrap(), "my_feature/rust");
    }
}

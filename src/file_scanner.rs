use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Count all .rs files in the given directory (both empty and non-empty)
pub fn count_all_rs_files(rust_dir: &Path) -> Result<usize> {
    let mut count = 0;

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            count += 1;
        }
    }

    Ok(count)
}

/// Find all empty .rs files in the given directory
pub fn find_empty_rs_files(rust_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut empty_files = Vec::new();

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            let metadata = fs::metadata(path)?;
            if metadata.len() == 0 {
                empty_files.push(path.to_path_buf());
            }
        }
    }

    // Sort files alphabetically by path for consistent and predictable ordering
    empty_files.sort();

    Ok(empty_files)
}

/// Extract file type from filename (var_ or fun_ prefix)
pub fn extract_file_type(filename: &str) -> Option<(&'static str, &str)> {
    if let Some(stripped) = filename.strip_prefix("var_") {
        Some(("var", stripped))
    } else if let Some(stripped) = filename.strip_prefix("fun_") {
        Some(("fn", stripped))
    } else {
        None
    }
}

/// Parse user input for file selection.
/// Users provide 1-based indices; returns 0-based indices of selected files.
pub fn parse_file_selection(input: &str, total_files: usize) -> Result<Vec<usize>> {
    let input = input.trim();
    
    // Check for empty input after trimming
    if input.is_empty() {
        anyhow::bail!("No input provided. Please select at least one file.");
    }
    
    // Parse input
    if input.eq_ignore_ascii_case("all") {
        // Select all files
        return Ok((0..total_files).collect());
    }
    
    let mut selected_indices = Vec::new();
    
    // Split by comma and process each part
    for part in input.split(',') {
        let part = part.trim();
        
        if part.contains('-') {
            // Handle range (e.g., "1-3")
            let range_parts: Vec<&str> = part.split('-').collect();
            if range_parts.len() != 2 {
                anyhow::bail!("Invalid range format: {}. Expected format like '1-3'", part);
            }
            
            let start_str = range_parts[0].trim();
            let end_str = range_parts[1].trim();

            if start_str.is_empty() || end_str.is_empty() {
                anyhow::bail!(
                    "Invalid range format: ranges must have both start and end values (e.g., '1-3')"
                );
            }

            let start: usize = start_str.parse()
                .with_context(|| format!("Invalid number in range: {}", start_str))?;
            let end: usize = end_str.parse()
                .with_context(|| format!("Invalid number in range: {}", end_str))?;
            
            if start < 1 || end < 1 || start > total_files || end > total_files {
                anyhow::bail!("Range {}-{} is out of bounds (valid: 1-{})", start, end, total_files);
            }
            
            if start > end {
                anyhow::bail!("Invalid range: {} is greater than {}", start, end);
            }
            
            for i in start..=end {
                selected_indices.push(i - 1);
            }
        } else {
            // Handle single number
            let num: usize = part.parse()
                .with_context(|| format!("Invalid number: {}", part))?;
            
            if num < 1 || num > total_files {
                anyhow::bail!("Number {} is out of bounds (valid: 1-{})", num, total_files);
            }
            
            selected_indices.push(num - 1);
        }
    }
    
    // Remove duplicates and sort
    selected_indices.sort_unstable();
    selected_indices.dedup();
    
    if selected_indices.is_empty() {
        anyhow::bail!("No files selected");
    }
    
    Ok(selected_indices)
}

/// Prompt user to select files from a list
pub fn prompt_file_selection(files: &[&PathBuf], rust_dir: &Path) -> Result<Vec<usize>> {
    println!("\n{}", "Available files to process:".bright_cyan().bold());
    
    // Display files with index numbers and relative paths
    for (idx, file) in files.iter().enumerate() {
        let relative_path = file.strip_prefix(rust_dir)
            .unwrap_or(file);
        println!("  {}. {}", idx + 1, relative_path.display());
    }
    
    println!();
    println!("{}", "Select files to process:".bright_yellow());
    println!("  - Enter numbers separated by commas (e.g., 1,3,5)");
    println!("  - Enter ranges (e.g., 1-3,5)");
    println!("  - Enter 'all' to process all files");
    print!("\n{} ", "Your selection:".bright_green().bold());
    io::stdout().flush()?;
    
    // Read user input
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    parse_file_selection(&input, files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_count_all_rs_files() {
        use std::io::Write;
        
        // Create a temporary directory
        let temp_dir = tempdir().unwrap();

        // Create empty .rs files
        fs::File::create(temp_dir.path().join("var_test1.rs")).unwrap();
        fs::File::create(temp_dir.path().join("fun_test2.rs")).unwrap();

        // Create non-empty .rs files
        let mut file1 = fs::File::create(temp_dir.path().join("var_test3.rs")).unwrap();
        file1.write_all(b"pub static TEST: i32 = 42;").unwrap();
        
        let mut file2 = fs::File::create(temp_dir.path().join("fun_test4.rs")).unwrap();
        file2.write_all(b"fn test() {}").unwrap();

        // Create a non-.rs file (should not be counted)
        fs::File::create(temp_dir.path().join("test.txt")).unwrap();

        // Count all .rs files
        let total_count = count_all_rs_files(temp_dir.path()).unwrap();
        assert_eq!(total_count, 4); // Should count both empty and non-empty .rs files
        
        // Verify empty files count
        let empty_count = find_empty_rs_files(temp_dir.path()).unwrap().len();
        assert_eq!(empty_count, 2);
    }

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
    fn test_find_empty_rs_files_sorted() {
        // Create a temporary directory
        let temp_dir = tempdir().unwrap();

        // Create multiple empty .rs files in non-alphabetical order
        fs::File::create(temp_dir.path().join("var_zebra.rs")).unwrap();
        fs::File::create(temp_dir.path().join("fun_alpha.rs")).unwrap();
        fs::File::create(temp_dir.path().join("var_middle.rs")).unwrap();

        // Find empty files
        let empty_files = find_empty_rs_files(temp_dir.path()).unwrap();
        
        // Verify we found all 3 files
        assert_eq!(empty_files.len(), 3);
        
        // Verify files are sorted alphabetically
        assert_eq!(empty_files[0].file_name().unwrap(), "fun_alpha.rs");
        assert_eq!(empty_files[1].file_name().unwrap(), "var_middle.rs");
        assert_eq!(empty_files[2].file_name().unwrap(), "var_zebra.rs");
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
        
        // Compare paths as PathBuf instead of strings to work on Windows
        let expected = PathBuf::from("my_feature").join("rust");
        assert_eq!(rust_dir, expected);
    }

    #[test]
    fn test_parse_file_selection() {
        struct TestCase {
            input: &'static str,
            total_files: usize,
            expected: Result<Vec<usize>, &'static str>,
        }

        let test_cases = vec![
            // Success cases
            TestCase { input: "all", total_files: 5, expected: Ok(vec![0, 1, 2, 3, 4]) },
            TestCase { input: "ALL", total_files: 3, expected: Ok(vec![0, 1, 2]) },
            TestCase { input: "All", total_files: 3, expected: Ok(vec![0, 1, 2]) },
            TestCase { input: "3", total_files: 5, expected: Ok(vec![2]) },
            TestCase { input: "1,3,5", total_files: 5, expected: Ok(vec![0, 2, 4]) },
            TestCase { input: "2-4", total_files: 5, expected: Ok(vec![1, 2, 3]) },
            TestCase { input: "1,3-5,7", total_files: 10, expected: Ok(vec![0, 2, 3, 4, 6]) },
            TestCase { input: "1,2,1,3,2", total_files: 5, expected: Ok(vec![0, 1, 2]) },
            TestCase { input: " 1 , 3 , 5 ", total_files: 5, expected: Ok(vec![0, 2, 4]) },
            TestCase { input: " 2 - 4 ", total_files: 5, expected: Ok(vec![1, 2, 3]) },
            
            // Error cases
            TestCase { input: "6", total_files: 5, expected: Err("out of bounds") },
            TestCase { input: "1,6", total_files: 5, expected: Err("out of bounds") },
            TestCase { input: "5-2", total_files: 5, expected: Err("is greater than") },
            TestCase { input: "abc", total_files: 5, expected: Err("") },
            TestCase { input: "", total_files: 5, expected: Err("No input provided") },
            TestCase { input: "   ", total_files: 5, expected: Err("No input provided") },
            TestCase { input: "-3", total_files: 5, expected: Err("ranges must have both start and end values") },
            TestCase { input: "1-", total_files: 5, expected: Err("ranges must have both start and end values") },
            TestCase { input: "-", total_files: 5, expected: Err("ranges must have both start and end values") },
            TestCase { input: "0", total_files: 5, expected: Err("") },
            TestCase { input: "1-10", total_files: 5, expected: Err("") },
        ];

        for (i, tc) in test_cases.iter().enumerate() {
            let result = parse_file_selection(tc.input, tc.total_files);
            match &tc.expected {
                Ok(expected_vec) => {
                    assert!(result.is_ok(), "Test case #{}: expected Ok, got Err for input '{}'", i, tc.input);
                    assert_eq!(&result.unwrap(), expected_vec, 
                        "Test case #{}: mismatch for input '{}'", i, tc.input);
                }
                Err(expected_err) => {
                    assert!(result.is_err(), "Test case #{}: expected Err, got Ok for input '{}'", i, tc.input);
                    if !expected_err.is_empty() {
                        let err_msg = result.unwrap_err().to_string();
                        assert!(err_msg.contains(expected_err), 
                            "Test case #{}: error message '{}' doesn't contain '{}' for input '{}'", 
                            i, err_msg, expected_err, tc.input);
                    }
                }
            }
        }
    }
}

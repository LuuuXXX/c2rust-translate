use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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
    fn test_parse_file_selection_all() {
        let result = parse_file_selection("all", 5).unwrap();
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_parse_file_selection_all_case_insensitive() {
        let result = parse_file_selection("ALL", 3).unwrap();
        assert_eq!(result, vec![0, 1, 2]);
        
        let result = parse_file_selection("All", 3).unwrap();
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_parse_file_selection_single_number() {
        let result = parse_file_selection("3", 5).unwrap();
        assert_eq!(result, vec![2]); // 0-based index
    }

    #[test]
    fn test_parse_file_selection_multiple_numbers() {
        let result = parse_file_selection("1,3,5", 5).unwrap();
        assert_eq!(result, vec![0, 2, 4]); // 0-based indices
    }

    #[test]
    fn test_parse_file_selection_range() {
        let result = parse_file_selection("2-4", 5).unwrap();
        assert_eq!(result, vec![1, 2, 3]); // 0-based indices
    }

    #[test]
    fn test_parse_file_selection_mixed() {
        let result = parse_file_selection("1,3-5,7", 10).unwrap();
        assert_eq!(result, vec![0, 2, 3, 4, 6]); // 0-based indices
    }

    #[test]
    fn test_parse_file_selection_duplicates() {
        let result = parse_file_selection("1,2,1,3,2", 5).unwrap();
        assert_eq!(result, vec![0, 1, 2]); // Duplicates removed and sorted
    }

    #[test]
    fn test_parse_file_selection_whitespace() {
        let result = parse_file_selection(" 1 , 3 , 5 ", 5).unwrap();
        assert_eq!(result, vec![0, 2, 4]);
    }

    #[test]
    fn test_parse_file_selection_range_with_whitespace() {
        let result = parse_file_selection(" 2 - 4 ", 5).unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_file_selection_out_of_bounds() {
        let result = parse_file_selection("6", 5);
        assert!(result.is_err());
        
        let result = parse_file_selection("1,6", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_file_selection_invalid_range() {
        let result = parse_file_selection("5-2", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is greater than"));
    }

    #[test]
    fn test_parse_file_selection_invalid_format() {
        let result = parse_file_selection("abc", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_file_selection_empty() {
        // Empty string should fail with clear message
        let result = parse_file_selection("", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No input provided"));
    }

    #[test]
    fn test_parse_file_selection_whitespace_only() {
        // Whitespace-only input should fail with clear message
        let result = parse_file_selection("   ", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No input provided"));
    }

    #[test]
    fn test_parse_file_selection_malformed_range_missing_start() {
        // Range missing start value (e.g., "-3")
        let result = parse_file_selection("-3", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ranges must have both start and end values"));
    }

    #[test]
    fn test_parse_file_selection_malformed_range_missing_end() {
        // Range missing end value (e.g., "1-")
        let result = parse_file_selection("1-", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ranges must have both start and end values"));
    }

    #[test]
    fn test_parse_file_selection_malformed_range_both_missing() {
        // Range with both values missing (just "-")
        let result = parse_file_selection("-", 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ranges must have both start and end values"));
    }

    #[test]
    fn test_parse_file_selection_zero() {
        let result = parse_file_selection("0", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_file_selection_range_out_of_bounds() {
        let result = parse_file_selection("1-10", 5);
        assert!(result.is_err());
    }
}

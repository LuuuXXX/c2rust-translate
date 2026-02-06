pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod translator;
pub mod util;
pub mod progress;
pub mod logger;

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Parse user input for file selection.
/// Users provide 1-based indices; returns 0-based indices of selected files.
fn parse_file_selection(input: &str, total_files: usize) -> Result<Vec<usize>> {
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
fn prompt_file_selection(files: &[&PathBuf], rust_dir: &Path) -> Result<Vec<usize>> {
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

/// Main translation workflow for a feature
pub fn translate_feature(feature: &str, allow_all: bool) -> Result<()> {
    let msg = format!("Starting translation for feature: {}", feature);
    println!("{}", msg.bright_cyan().bold());
    logger::log_message(&msg);

    // Validate feature name to prevent path traversal attacks
    util::validate_feature_name(feature)?;

    // Find the project root first
    let project_root = util::find_project_root()?;
    
    // Step 1: Check if rust directory exists (with proper IO error handling)
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");

    let rust_dir_exists = match std::fs::metadata(&rust_dir) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                anyhow::bail!(
                    "Path exists but is not a directory: {}",
                    rust_dir.display()
                );
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(e).context(format!(
                "Failed to access rust directory at {}",
                rust_dir.display()
            ));
        }
    };

    if !rust_dir_exists {
        println!("{}", "Rust directory does not exist. Initializing...".yellow());
        analyzer::initialize_feature(feature)?;
        
        // Verify rust directory was created and is actually a directory
        match std::fs::metadata(&rust_dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    anyhow::bail!(
                        "Initialization created a file instead of a directory: {}",
                        rust_dir.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("Error: Failed to initialize rust directory");
            }
            Err(e) => {
                return Err(e).context(format!(
                    "Failed to verify initialized rust directory at {}",
                    rust_dir.display()
                ));
            }
        }
        
        // Commit the initialization
        git::git_commit(&format!("Initialize {} rust directory", feature), feature)?;
    }

    // Load or initialize progress state
    let mut progress_state = progress::ProgressState::load(feature)?;

    // Step 2: Main loop - process all empty .rs files
    loop {
        // Step 2.1: Try to build first
        println!("\n{}", "Building project...".bright_blue().bold());
        match builder::cargo_build(feature) {
            Ok(_) => {
                println!("{}", "✓ Build successful!".bright_green().bold());
            }
            Err(e) => {
                return Err(e).context("Translation workflow aborted due to build failure");
            }
        }

        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
            
        git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

        println!("{}", "Running hybrid build tests...".bright_blue());
        builder::run_hybrid_build(feature)?;
        
        // Step 2.2: Scan for empty .rs files
        let empty_rs_files = file_scanner::find_empty_rs_files(&rust_dir)?;
        
        if empty_rs_files.is_empty() {
            let msg = "✓ No empty .rs files found. Translation complete!";
            println!("\n{}", msg.bright_green().bold());
            logger::log_message(msg);
            break;
        }

        // Filter out already processed files
        let unprocessed_files: Vec<_> = empty_rs_files
            .iter()
            .filter(|f| !progress_state.is_processed(f, &rust_dir))
            .collect();
        
        if unprocessed_files.is_empty() {
            println!("{}", "All files have been processed already.".cyan());
            continue;
        }
        
        // Set total count for progress display (total unprocessed + already processed).
        // To maintain consistent progress across runs, never decrease the total count;
        // only update it if we observe more empty files than previously recorded.
        let current_total = progress_state.get_total_count();
        let new_total = std::cmp::max(current_total, empty_rs_files.len());
        if new_total != current_total {
            progress_state.set_total_count(new_total);
            progress_state.save(feature)?;
        }
        
        println!("{}", format!("Found {} empty .rs file(s) to process ({} already processed)", 
            unprocessed_files.len(), 
            empty_rs_files.len() - unprocessed_files.len()).cyan());

        // Select files to process based on allow_all flag
        // Use indices to avoid unnecessary cloning
        let selected_indices: Vec<usize> = if allow_all {
            // Process all unprocessed files without prompting
            (0..unprocessed_files.len()).collect()
        } else {
            // Prompt user to select files
            prompt_file_selection(&unprocessed_files, &rust_dir)?
        };

        for &idx in selected_indices.iter() {
            let rs_file = unprocessed_files[idx];
            // Get current progress position (persisted across runs)
            let current_position = progress_state.get_current_position();
            let total_count = progress_state.get_total_count();
            
            // Get file name for display
            let file_name = rs_file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>");
            
            let progress_msg = format!("[{}/{}] Processing {}", current_position, total_count, file_name);
            println!(
                "\n{}",
                progress_msg.bright_magenta().bold()
            );
            logger::log_message(&progress_msg);
            
            process_rs_file(feature, rs_file, file_name, current_position, total_count)?;
            
            // Mark file as processed and save progress
            progress_state.mark_processed(rs_file, &rust_dir)?;
            progress_state.save(feature)?;
        }
    }

    Ok(())
}

/// Process a single .rs file through the translation workflow
///
/// # Parameters
/// - `feature`: The feature name being translated
/// - `rs_file`: Path to the `.rs` file to process
/// - `file_name`: Display name of the file for progress messages
/// - `current_position`: Current file position in the overall progress
/// - `total_count`: Total number of files to process
fn process_rs_file(feature: &str, rs_file: &std::path::Path, file_name: &str, current_position: usize, total_count: usize) -> Result<()> {
    use std::fs;

    println!("\n{}", format!("┌─ Processing file: {}", rs_file.display()).bright_white().bold());

    // Step 2.2.1: Extract type from filename
    let file_stem = rs_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?;

    let (file_type, name) = file_scanner::extract_file_type(file_stem)
        .ok_or_else(|| anyhow::anyhow!("Unknown file prefix: {}", file_stem))?;

    println!("│ {} {}", "File type:".cyan(), file_type.bright_yellow());
    println!("│ {} {}", "Name:".cyan(), name.bright_yellow());

    // Step 2.2.2: Check if corresponding .c file exists (with proper IO error handling)
    let c_file = rs_file.with_extension("c");
    match fs::metadata(&c_file) {
        Ok(_) => {
            println!("│ {} {}", "C source:".cyan(), c_file.display().to_string().bright_yellow());
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "Corresponding C file not found for Rust file: {}",
                rs_file.display()
            );
        }
        Err(err) => {
            return Err(err).context(format!(
                "Failed to access corresponding C file for Rust file: {}",
                rs_file.display()
            ));
        }
    }

    // Helper function to format progress indicator
    let format_progress = |operation: &str| {
        format!("[{}/{}] Processing {} - {}", current_position, total_count, file_name, operation)
    };

    // Step 2.2.3: Call translation tool
    println!("│");
    println!("│ {}", format_progress("Translation").bright_magenta().bold());
    println!("│ {}", format!("Translating {} to Rust...", file_type).bright_blue().bold());
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file)?;

    // Step 2.2.4: Verify translation result
    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }
    println!("│ {}", format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green());

    // Step 2.2.5 & 2.2.6: Build and fix errors in a loop (max 10 attempts)
    const MAX_FIX_ATTEMPTS: usize = 10;
    for attempt in 1..=MAX_FIX_ATTEMPTS {
        println!("│");
        println!("│ {}", format_progress("Build").bright_magenta().bold());
        println!("│ {}", format!("Building Rust project (attempt {}/{})", attempt, MAX_FIX_ATTEMPTS).bright_blue().bold());
        match builder::cargo_build(feature) {
            Ok(_) => {
                println!("│ {}", "✓ Build successful!".bright_green().bold());
                break;
            }
            Err(build_error) => {
                if attempt == MAX_FIX_ATTEMPTS {
                    return Err(build_error).context(format!(
                        "Build failed after {} fix attempts for file {}",
                        MAX_FIX_ATTEMPTS,
                        rs_file.display()
                    ));
                }
                
                println!("│ {}", "⚠ Build failed, attempting to fix errors...".yellow().bold());
                
                // Try to fix the error
                println!("│");
                println!("│ {}", format_progress("Fix").bright_magenta().bold());
                translator::fix_translation_error(feature, file_type, rs_file, &build_error.to_string())?;

                // Verify fix result
                let metadata = fs::metadata(rs_file)?;
                if metadata.len() == 0 {
                    anyhow::bail!("Fix failed: output file is empty");
                }
                println!("│ {}", "✓ Fix applied".bright_green());
            }
        }
    }

    // Step 2.2.7: Save translation result with specific file in commit message
    println!("│");
    println!("│ {}", format_progress("Commit").bright_magenta().bold());
    println!("│ {}", "Committing changes...".bright_blue());
    git::git_commit(&format!(
        "Translate {} from C to Rust (feature: {})",
        file_name, feature
    ), feature)?;
    println!("│ {}", "✓ Changes committed".bright_green());

    // Step 2.2.8: Update code analysis
    println!("│");
    println!("│ {}", format_progress("Update Analysis").bright_magenta().bold());
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // Step 2.2.9: Save update result
    println!("│");
    println!("│ {}", format_progress("Commit Analysis").bright_magenta().bold());
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

    println!("│");
    println!("│ {}", format_progress("Hybrid Build Tests").bright_magenta().bold());
    println!("│ {}", "Running hybrid build tests...".bright_blue());
    builder::run_hybrid_build(feature)?;
    
    println!("{}", "└─ File processing complete".bright_white().bold());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

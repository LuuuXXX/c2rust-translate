//! Error handling utilities for parsing and handling test failures

use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::{builder, file_scanner, interaction, suggestion, translator, util};

/// Parse error message to extract Rust file paths
/// Returns a list of file paths found in the error message
/// Filters to only include files within the project
pub(crate) fn parse_error_for_files(error_msg: &str, feature: &str) -> Result<Vec<PathBuf>> {
    // Validate feature name to prevent path traversal
    util::validate_feature_name(feature)?;
    
    lazy_static::lazy_static! {
        static ref ERROR_PATH_RE: regex::Regex = 
            regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?")
                .expect("Failed to compile error path regex");
    }
    
    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");
    
    let mut file_paths = HashSet::new();
    
    for cap in ERROR_PATH_RE.captures_iter(error_msg) {
        if let Some(path_match) = cap.get(1) {
            let path_str = path_match.as_str();
            let path = PathBuf::from(path_str);
            
            // Try both as-is and as relative to rust_dir
            let candidates = vec![
                path.clone(),
                rust_dir.join(&path),
            ];
            
            for candidate in candidates {
                // Check if the file exists and is within our project
                if candidate.exists() && candidate.is_file() {
                    // Ensure the file is within the rust directory
                    if let Ok(canonical) = candidate.canonicalize() {
                        if let Ok(rust_canonical) = rust_dir.canonicalize() {
                            if canonical.starts_with(&rust_canonical) {
                                file_paths.insert(canonical);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Convert to Vec and sort for consistent ordering
    let mut result: Vec<PathBuf> = file_paths.into_iter().collect();
    result.sort();
    
    Ok(result)
}

/// Handle startup test failure when files can be located
pub(crate) fn handle_startup_test_failure_with_files(
    feature: &str,
    test_error: anyhow::Error,
    mut files: Vec<PathBuf>,
) -> Result<()> {
    let mut current_error = test_error;
    
    // Use a loop to handle files iteratively, avoiding deep recursion
    loop {
        if files.is_empty() {
            // No files to process, return the current error
            return Err(current_error).context("No files found to fix");
        }
        
        println!("│");
        println!("│ {}", format!("Found {} file(s) in error message:", files.len()).bright_cyan());
        for (idx, file) in files.iter().enumerate() {
            println!("│   {}. {}", idx + 1, file.display());
        }
        
        // Process each file found in the error
        for (idx, file) in files.iter().enumerate() {
            println!("│");
            let file_display_name = file.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            println!("│ {}", format!("═══ Processing file {}/{}: {} ═══", 
                idx + 1, files.len(), file_display_name).bright_cyan().bold());
            
            // Extract file type (var_ or fun_) from file stem
            let file_stem = file.file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid file stem")?;
            
        let (file_type, _) = file_scanner::extract_file_type(file_stem)
            .context(format!("Could not extract file type from filename: {}", file_display_name))?;
        
        // Display the C and Rust code
        let c_file = file.with_extension("c");
        
        if c_file.exists() {
            interaction::display_file_paths(Some(&c_file), file);
            
            println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
            translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);
        } else {
            interaction::display_file_paths(None, file);
        }
        
        println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
        translator::display_code(file, "─ Rust Code ─", usize::MAX, true);
        
        println!("│ {}", "═══ Test Error ═══".bright_red().bold());
        println!("│ {}", current_error);
        
        // Offer same choices as handle_max_fix_attempts_reached
        let choice = interaction::prompt_user_choice("Initial test failure", false)?;
        
        match choice {
            interaction::UserChoice::Continue => {
                println!("│");
                println!("│ {}", "You chose: Continue trying with a new suggestion".bright_cyan());
                
                // Clear old suggestions before prompting for new one
                suggestion::clear_suggestions()?;
                
                // Get optional suggestion from user
                if let Some(suggestion_text) = interaction::prompt_suggestion(false)? {
                    // Save suggestion to suggestions.txt
                    suggestion::append_suggestion(&suggestion_text)?;
                }
                
                // Apply fix with the suggestion
                let format_progress = |op: &str| format!("Fix startup test failure - {}", op);
                crate::apply_error_fix(feature, file_type, file, &current_error, &format_progress, true)?;
                
                // Try to build and test one more time
                println!("│");
                println!("│ {}", "Rebuilding and retesting...".bright_blue().bold());
                match builder::cargo_build(feature, true) {
                    Ok(_) => {
                        println!("│ {}", "✓ Build successful!".bright_green().bold());
                        
                        // Now try the full hybrid build test
                        match builder::run_hybrid_build(feature) {
                            Ok(_) => {
                                println!("│ {}", "✓ Hybrid build tests passed!".bright_green().bold());
                                // Hybrid build is now passing; stop further error handling
                                return Ok(());
                            }
                            Err(e) => {
                                println!("│ {}", "✗ Hybrid build tests still failing".red());
                                
                                // Try to parse the new error and see if there are more files
                                match parse_error_for_files(&e.to_string(), feature) {
                                    Ok(new_files) if !new_files.is_empty() => {
                                        println!("│ {}", "Found additional files in new error, will process them...".yellow());
                                        // Update files and error for the next iteration
                                        files = new_files;
                                        current_error = e;
                                        break; // Break inner loop to restart with new files
                                    }
                                    _ => {
                                        // No more files to process, return error
                                        return Err(e).context("Hybrid build tests failed after fix attempt");
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Build still failing after fix attempt".red());
                        return Err(e).context(format!(
                            "Build failed after fix for file {}",
                            file.display()
                        ));
                    }
                }
            }
            interaction::UserChoice::ManualFix => {
                println!("│");
                println!("│ {}", "You chose: Manual fix".bright_cyan());
                
                // Try to open vim
                match interaction::open_in_vim(file) {
                    Ok(_) => {
                        // After Vim editing, repeatedly try building and testing
                        loop {
                            println!("│");
                            println!("│ {}", "Vim editing completed. Rebuilding and retesting...".bright_blue());
                            
                            // Try building after manual edit
                            match builder::cargo_build(feature, true) {
                                Ok(_) => {
                                    println!("│ {}", "✓ Build successful!".bright_green().bold());
                                    
                                    // Now try the full hybrid build test
                                    match builder::run_hybrid_build(feature) {
                                        Ok(_) => {
                                            println!("│ {}", "✓ Hybrid build tests passed after manual fix!".bright_green().bold());
                                            // All tests have passed; exit the handler successfully
                                            return Ok(());
                                        }
                                        Err(e) => {
                                            println!("│ {}", "✗ Hybrid build tests still failing".red());
                                            
                                            // Ask if user wants to try again
                                            println!("│");
                                            println!("│ {}", "Tests still have errors. What would you like to do?".yellow());
                                            let retry_choice = interaction::prompt_user_choice("Tests still failing", false)?;
                                            
                                            match retry_choice {
                                                interaction::UserChoice::Continue => {
                                                    // Just retry the build with existing changes
                                                    continue;
                                                }
                                                interaction::UserChoice::ManualFix => {
                                                    println!("│ {}", "Reopening file in Vim for additional manual fixes...".bright_blue());
                                                    match interaction::open_in_vim(file) {
                                                        Ok(_) => {
                                                            // Loop will retry the build
                                                            continue;
                                                        }
                                                        Err(open_err) => {
                                                            println!("│ {}", format!("Failed to reopen vim: {}", open_err).red());
                                                            return Err(open_err).context(format!(
                                                                "Tests still failing and could not reopen vim for file {}",
                                                                file.display()
                                                            ));
                                                        }
                                                    }
                                                }
                                                interaction::UserChoice::Exit => {
                                                    return Err(e).context(format!(
                                                        "Tests failed after manual fix for file {}",
                                                        file.display()
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("│ {}", "✗ Build still failing after manual fix".red());
                                    
                                    // Ask if user wants to try again
                                    println!("│");
                                    println!("│ {}", "Build still has errors. What would you like to do?".yellow());
                                    let retry_choice = interaction::prompt_user_choice("Build still failing", false)?;
                                    
                                    match retry_choice {
                                        interaction::UserChoice::Continue => {
                                            // Continue: just retry the build with existing changes
                                            continue;
                                        }
                                        interaction::UserChoice::ManualFix => {
                                            println!("│ {}", "Reopening file in Vim for additional manual fixes...".bright_blue());
                                            match interaction::open_in_vim(file) {
                                                Ok(_) => {
                                                    // After additional manual fixes, loop will retry the build
                                                    continue;
                                                }
                                                Err(open_err) => {
                                                    println!("│ {}", format!("Failed to reopen vim: {}", open_err).red());
                                                    return Err(open_err).context(format!(
                                                        "Build still failing and could not reopen vim for file {}",
                                                        file.display()
                                                    ));
                                                }
                                            }
                                        }
                                        interaction::UserChoice::Exit => {
                                            return Err(e).context(format!(
                                                "Build failed after manual fix for file {}",
                                                file.display()
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("│ {}", format!("Failed to open vim: {}", e).red());
                        return Err(e).context(format!(
                            "Initial test failed and could not open vim for file {}",
                            file.display()
                        ));
                    }
                }
            }
            interaction::UserChoice::Exit => {
                println!("│");
                println!("│ {}", "You chose: Exit".yellow());
                return Err(current_error).context("User chose to exit during startup test failure handling");
            }
        }
        } // End of inner for loop
        
        // If we've processed all files without errors or early returns, we're done
        println!("│");
        println!("│ {}", "✓ All files processed successfully".bright_green().bold());
        return Ok(());
    } // End of outer loop
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_error_pattern_extraction() {
        // Test that we can extract file paths from error messages
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope";
        
        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re.captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"src/var_test.rs".to_string()));
        assert!(matches.contains(&"src/fun_helper.rs".to_string()));
    }
    
    #[test]
    fn test_parse_error_pattern_warnings() {
        // Test that we can extract file paths from warnings too
        let error_msg = "warning: unused variable: `x`
  --> src/var_counter.rs:5:9
   |
5  |     let x = 42;
   |         ^ help: if this is intentional, prefix it with an underscore: `_x`";
        
        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re.captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "src/var_counter.rs");
    }
    
    #[test]
    fn test_parse_error_multiple_files_same_error() {
        // Test multiple file references in a single error
        let error_msg = "error[E0308]: mismatched types
  --> src/var_main.rs:15:10
   |
15 |     foo(x);
   |          ^ expected `String`, found `i32`
   |
note: expected signature from here
  --> src/fun_foo.rs:3:1
   |
3  | fn foo(s: String) { }
   | ^^^^^^^^^^^^^^^^^";
        
        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re.captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        // Should find both files
        assert!(matches.len() >= 2);
        assert!(matches.contains(&"src/var_main.rs".to_string()));
        assert!(matches.contains(&"src/fun_foo.rs".to_string()));
    }
    
    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_with_real_directory() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;
        
        // Create a temporary directory structure
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        
        // Set current directory to temp directory so find_project_root can work
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();
        
        // Create .git directory so find_project_root succeeds
        fs::create_dir(project_root.join(".git")).unwrap();
        
        // Create feature directory structure
        let feature = "test_feature";
        let c2rust_dir = project_root.join(".c2rust");
        fs::create_dir_all(&c2rust_dir).unwrap();
        
        let feature_dir = c2rust_dir.join(feature);
        let rust_dir = feature_dir.join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        
        // Create test files
        let test_file1 = src_dir.join("var_test.rs");
        fs::write(&test_file1, "// test content").unwrap();
        
        let test_file2 = src_dir.join("fun_helper.rs");
        fs::write(&test_file2, "// helper content").unwrap();
        
        // Create a file outside the rust directory that should be filtered out
        let outside_file = c2rust_dir.join("outside.rs");
        fs::write(&outside_file, "// outside").unwrap();
        
        // Test error message with multiple files
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope
   
note: some note about outside file
  --> ../../outside.rs:1:1";
        
        let result = parse_error_for_files(error_msg, feature).unwrap();
        
        // Restore original directory
        env::set_current_dir(&original_dir).unwrap();
        
        // Should find exactly 2 files (not the outside.rs)
        assert_eq!(result.len(), 2);
        
        // Check that both files are present and canonical
        let canonical_file1 = test_file1.canonicalize().unwrap();
        let canonical_file2 = test_file2.canonicalize().unwrap();
        
        assert!(result.contains(&canonical_file1), "Should contain var_test.rs");
        assert!(result.contains(&canonical_file2), "Should contain fun_helper.rs");
        
        // Verify files are sorted
        assert!(result[0] < result[1], "Files should be sorted");
    }
    
    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_deduplication() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;
        
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();
        
        fs::create_dir(project_root.join(".git")).unwrap();
        
        let feature = "test_feature";
        let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        
        let test_file = src_dir.join("var_test.rs");
        fs::write(&test_file, "// test").unwrap();
        
        // Error message with the same file mentioned multiple times
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    
error[E0308]: another error
   --> src/var_test.rs:15:5
    
note: note about same file
   --> src/var_test.rs:20:1";
        
        let result = parse_error_for_files(error_msg, feature).unwrap();
        
        env::set_current_dir(&original_dir).unwrap();
        
        // Should only have 1 file despite multiple mentions
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("var_test.rs"));
    }
    
    #[test]
    fn test_parse_error_for_files_validates_feature_name() {
        // Test that invalid feature names are rejected
        let error_msg = "error: --> src/test.rs:1:1";
        
        // Feature names with path traversal should fail
        let result = parse_error_for_files(error_msg, "../bad");
        assert!(result.is_err(), "Should reject feature name with ..");
        
        let result = parse_error_for_files(error_msg, "good/bad");
        assert!(result.is_err(), "Should reject feature name with /");
    }
}

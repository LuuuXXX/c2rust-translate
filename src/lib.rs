pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod translator;
pub mod util;
pub mod progress;
pub mod logger;
pub mod constants;
pub mod target_selector;
pub(crate) mod interaction;
pub(crate) mod suggestion;

use anyhow::{Context, Result};
use colored::Colorize;

/// Main translation workflow for a feature
pub fn translate_feature(feature: &str, allow_all: bool, max_fix_attempts: usize, show_full_output: bool) -> Result<()> {
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

    // Step 1: Target artifact selection
    // Prompt user to select target artifact before processing files
    println!("\n{}", "Step 1: Select target artifact".bright_cyan().bold());
    let selected_target = target_selector::prompt_target_selection(feature)?;
    target_selector::store_target_in_config(feature, &selected_target)?;

    // Initialize progress state before the main loop
    // Count total .rs files and calculate how many have already been processed
    let total_rs_files = file_scanner::count_all_rs_files(&rust_dir)?;
    let initial_empty_count = file_scanner::find_empty_rs_files(&rust_dir)?.len();
    let already_processed = total_rs_files.saturating_sub(initial_empty_count);
    
    let mut progress_state = progress::ProgressState::with_initial_progress(
        total_rs_files,
        already_processed
    );

    // Step 2: Main loop - process all empty .rs files
    println!("\n{}", "Step 2: Translate C source files".bright_cyan().bold());
    loop {
        // Step 2.1: Try to build first
        println!("\n{}", "Building project...".bright_blue().bold());
        match builder::cargo_build(feature, show_full_output) {
            Ok(_) => {
                println!("{}", "✓ Build successful!".bright_green().bold());
            }
            Err(e) => {
                println!("{}", "✗ Initial build failed!".red().bold());
                println!("{}", "This may indicate issues with the project setup or previous translations.".yellow());
                
                // Offer interactive handling for startup build failure
                let choice = interaction::prompt_user_choice("Initial build failure", false)?;
                
                match choice {
                    interaction::UserChoice::Continue => {
                        println!("│ {}", "Continuing despite build failure. You can fix issues during file processing.".yellow());
                        // Continue with the workflow
                    }
                    interaction::UserChoice::ManualFix => {
                        println!("│ {}", "Please manually fix the build issues and run the tool again.".yellow());
                        return Err(e).context("Initial build failed and user chose manual fix");
                    }
                    interaction::UserChoice::Exit => {
                        return Err(e).context("Initial build failed and user chose to exit");
                    }
                }
            }
        }

        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
            
        git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

        println!("{}", "Running hybrid build tests...".bright_blue());
        match builder::run_hybrid_build(feature) {
            Ok(_) => {
                println!("{}", "✓ Hybrid build tests passed".bright_green());
            }
            Err(e) => {
                println!("{}", "✗ Initial hybrid build tests failed!".red().bold());
                
                // Try to parse error and locate files
                match parse_error_for_files(&e.to_string(), feature) {
                    Ok(files) if !files.is_empty() => {
                        // Found files, enter repair flow
                        println!("{}", "Attempting to automatically locate and fix files from error...".yellow());
                        handle_startup_test_failure_with_files(feature, &e, files)?;
                    }
                    _ => {
                        // Cannot locate files, offer basic choices
                        println!("{}", "Unable to automatically locate files from error.".yellow());
                        println!("{}", "This may indicate issues with the test environment or previous translations.".yellow());
                        
                        let choice = interaction::prompt_user_choice("Initial test failure", false)?;
                        
                        match choice {
                            interaction::UserChoice::Continue => {
                                println!("│ {}", "Continuing despite test failure. You can fix issues during file processing.".yellow());
                                // Continue with the workflow
                            }
                            interaction::UserChoice::ManualFix | interaction::UserChoice::Exit => {
                                return Err(e).context("Initial tests failed");
                            }
                        }
                    }
                }
            }
        }
        
        // Step 2.2: Scan for empty .rs files (unprocessed files)
        let empty_rs_files = file_scanner::find_empty_rs_files(&rust_dir)?;
        
        if empty_rs_files.is_empty() {
            let msg = "✓ No empty .rs files found. Translation complete!";
            println!("\n{}", msg.bright_green().bold());
            logger::log_message(msg);
            break;
        }
        
        println!("{}", format!("Found {} empty .rs file(s) to process", 
            empty_rs_files.len()).cyan());

        // Select files to process based on allow_all flag
        let selected_indices: Vec<usize> = if allow_all {
            // Process all empty files without prompting
            (0..empty_rs_files.len()).collect()
        } else {
            // Prompt user to select files
            let file_refs: Vec<_> = empty_rs_files.iter().collect();
            file_scanner::prompt_file_selection(&file_refs, &rust_dir)?
        };

        for &idx in selected_indices.iter() {
            let rs_file = &empty_rs_files[idx];
            // Get current progress position (persists across loop iterations)
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
            
            process_rs_file(feature, rs_file, file_name, current_position, total_count, max_fix_attempts, show_full_output)?;
            
            // Mark file as processed in this session
            progress_state.mark_processed();
        }
    }

    Ok(())
}

/// Process a single .rs file through the translation workflow
fn process_rs_file(feature: &str, rs_file: &std::path::Path, file_name: &str, current_position: usize, total_count: usize, max_fix_attempts: usize, show_full_output: bool) -> Result<()> {
    use constants::MAX_TRANSLATION_ATTEMPTS;
    
    for attempt_number in 1..=MAX_TRANSLATION_ATTEMPTS {
        let is_last_attempt = attempt_number == MAX_TRANSLATION_ATTEMPTS;
        
        print_attempt_header(attempt_number, rs_file);
        
        // Add message for retry attempts  
        if attempt_number > 1 {
            println!("│ {}", "Starting fresh translation (previous translation will be overwritten)...".bright_cyan());
        }
        
        let (file_type, _name) = extract_and_validate_file_info(rs_file)?;
        check_c_file_exists(rs_file)?;
        
        let format_progress = |operation: &str| {
            format!("[{}/{}] Processing {} - {}", current_position, total_count, file_name, operation)
        };
        
        // Translate C to Rust
        translate_file(feature, file_type, rs_file, &format_progress, show_full_output)?;
        
        // Build and fix errors
        let build_successful = build_and_fix_loop(
            feature, 
            file_type, 
            rs_file, 
            file_name, 
            &format_progress,
            is_last_attempt,
            attempt_number,
            max_fix_attempts,
            show_full_output
        )?;
        
        if build_successful {
            complete_file_processing(feature, file_name, file_type, rs_file, &format_progress)?;
            return Ok(());
        }
    }
    
    anyhow::bail!("Unexpected: all retry attempts completed without resolution")
}

/// Print header for current attempt
fn print_attempt_header(attempt_number: usize, rs_file: &std::path::Path) {
    if attempt_number > 1 {
        let retry_number = attempt_number - 1;
        let max_retries = constants::MAX_TRANSLATION_ATTEMPTS - 1;
        println!("\n{}", format!("┌─ Retry attempt {}/{}: {}", retry_number, max_retries, rs_file.display()).bright_yellow().bold());
    } else {
        println!("\n{}", format!("┌─ Processing file: {}", rs_file.display()).bright_white().bold());
    }
}

/// Extract file type and name, print info
fn extract_and_validate_file_info(rs_file: &std::path::Path) -> Result<(&'static str, &str)> {
    let file_stem = rs_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?;

    let (file_type, name) = file_scanner::extract_file_type(file_stem)
        .ok_or_else(|| anyhow::anyhow!("Unknown file prefix: {}", file_stem))?;

    println!("│ {} {}", "File type:".cyan(), file_type.bright_yellow());
    println!("│ {} {}", "Name:".cyan(), name.bright_yellow());
    
    Ok((file_type, name))
}

/// Check if corresponding C file exists
fn check_c_file_exists(rs_file: &std::path::Path) -> Result<()> {
    use std::fs;
    
    let c_file = rs_file.with_extension("c");
    match fs::metadata(&c_file) {
        Ok(_) => {
            println!("│ {} {}", "C source:".cyan(), c_file.display().to_string().bright_yellow());
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("Corresponding C file not found for Rust file: {}", rs_file.display());
        }
        Err(err) => {
            Err(err).context(format!("Failed to access corresponding C file for Rust file: {}", rs_file.display()))
        }
    }
}

/// Translate C file to Rust
fn translate_file<F>(feature: &str, file_type: &str, rs_file: &std::path::Path, format_progress: &F, show_full_output: bool) -> Result<()> 
where
    F: Fn(&str) -> String
{
    use std::fs;
    
    let c_file = rs_file.with_extension("c");
    
    println!("│");
    println!("│ {}", format_progress("Translation").bright_magenta().bold());
    println!("│ {}", format!("Translating {} to Rust...", file_type).bright_blue().bold());
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file, show_full_output)?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }
    println!("│ {}", format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green());
    
    Ok(())
}

/// Build and fix errors in a loop
fn build_and_fix_loop<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    file_name: &str,
    format_progress: &F,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<bool>
where
    F: Fn(&str) -> String
{
    
    for attempt in 1..=max_fix_attempts {
        println!("│");
        println!("│ {}", format_progress("Build").bright_magenta().bold());
        println!("│ {}", format!("Building Rust project (attempt {}/{})", attempt, max_fix_attempts).bright_blue().bold());
        
        match builder::cargo_build(feature, show_full_output) {
            Ok(_) => {
                println!("│ {}", "✓ Build successful!".bright_green().bold());
                return Ok(true);
            }
            Err(build_error) => {
                if attempt == max_fix_attempts {
                    return handle_max_fix_attempts_reached(
                        build_error,
                        file_name,
                        rs_file,
                        is_last_attempt,
                        attempt_number,
                        max_fix_attempts,
                        feature,
                        file_type,
                    );
                } else {
                    // Show full fixed code for visibility, respect user's error preview preference
                    apply_error_fix(feature, file_type, rs_file, &build_error, format_progress, show_full_output)?;
                }
            }
        }
    }
    
    Ok(false)
}

/// Parse error message to extract Rust file paths
/// Returns a list of file paths found in the error message
/// Filters to only include files within the project
fn parse_error_for_files(error_msg: &str, feature: &str) -> Result<Vec<std::path::PathBuf>> {
    use std::path::PathBuf;
    use std::collections::HashSet;
    
    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");
    
    // Common Rust compiler error patterns:
    // error[E0308]: ... --> path/to/file.rs:10:5
    // warning: ... --> path/to/file.rs:20:1
    // We'll look for paths ending with .rs followed by line:column
    let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?")
        .context("Failed to compile regex")?;
    
    let mut file_paths = HashSet::new();
    
    for cap in re.captures_iter(error_msg) {
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
fn handle_startup_test_failure_with_files(
    feature: &str,
    test_error: &anyhow::Error,
    files: Vec<std::path::PathBuf>,
) -> Result<()> {
    println!("│");
    println!("│ {}", format!("Found {} file(s) in error message:", files.len()).bright_cyan());
    for (idx, file) in files.iter().enumerate() {
        println!("│   {}. {}", idx + 1, file.display());
    }
    
    // Process each file found in the error
    for (idx, file) in files.iter().enumerate() {
        println!("│");
        println!("│ {}", format!("═══ Processing file {}/{}: {} ═══", 
            idx + 1, files.len(), file.file_name().unwrap_or_default().to_string_lossy()).bright_cyan().bold());
        
        // Extract file type (var_ or fun_)
        let file_name = file.file_name()
            .and_then(|n| n.to_str())
            .context("Invalid file name")?;
        
        let file_stem = file.file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid file stem")?;
            
        let (file_type, _) = file_scanner::extract_file_type(file_stem)
            .context(format!("Could not extract file type from filename: {}", file_name))?;
        
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
        println!("│ {}", test_error);
        
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
                apply_error_fix(feature, file_type, file, test_error, &format_progress, true)?;
                
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
                                // Continue to next file if any
                                continue;
                            }
                            Err(e) => {
                                println!("│ {}", "✗ Hybrid build tests still failing".red());
                                
                                // Try to parse the new error and see if there are more files
                                match parse_error_for_files(&e.to_string(), feature) {
                                    Ok(new_files) if !new_files.is_empty() => {
                                        println!("│ {}", "Found additional files in new error, will process them...".yellow());
                                        // Recursively handle the new files
                                        return handle_startup_test_failure_with_files(feature, &e, new_files);
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
                                            // Exit the loop and continue to next file
                                            break;
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
                anyhow::bail!("User chose to exit during startup test failure handling. Original error: {}", test_error);
            }
        }
    }
    
    println!("│");
    println!("│ {}", "✓ All files processed successfully".bright_green().bold());
    Ok(())
}

/// Handle the case when max fix attempts are reached
/// Returns Ok(true) if processing should continue without retrying translation, Ok(false) if translation should be retried
fn handle_max_fix_attempts_reached(
    build_error: anyhow::Error,
    file_name: &str,
    rs_file: &std::path::Path,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    feature: &str,
    file_type: &str,
) -> Result<bool> {
    use constants::MAX_TRANSLATION_ATTEMPTS;
    
    println!("│");
    println!("│ {}", "⚠ Maximum fix attempts reached!".red().bold());
    println!("│ {}", format!("File {} still has build errors after {} fix attempts.", file_name, max_fix_attempts).yellow());
    
    // Display full C and Rust code for user reference
    let c_file = rs_file.with_extension("c");
    
    // Show file locations
    interaction::display_file_paths(Some(&c_file), rs_file);
    
    // Display full code (always show full for interactive mode)
    println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
    translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);
    
    println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
    translator::display_code(rs_file, "─ Rust Code ─", usize::MAX, true);
    
    println!("│ {}", "═══ Build Error ═══".bright_red().bold());
    println!("│ {}", build_error);
    
    // Get user choice
    let choice = interaction::prompt_user_choice("Build failure", false)?;
    
    match choice {
        interaction::UserChoice::Continue => {
            println!("│");
            println!("│ {}", "You chose: Continue trying with a new suggestion".bright_cyan());
            
            // Clear old suggestions BEFORE prompting for new one
            // This prevents the bug where the new suggestion gets cleared on next retry
            suggestion::clear_suggestions()?;
            
            // Get optional suggestion from user
            if let Some(suggestion_text) = interaction::prompt_suggestion(false)? {
                // Save suggestion to suggestions.txt
                suggestion::append_suggestion(&suggestion_text)?;
            }
            
            // If we can still retry translation, do so
            if !is_last_attempt {
                let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
                println!("│ {}", format!("Retrying translation from scratch... ({} retries remaining)", remaining_retries).bright_cyan());
                println!("│ {}", "Note: The translator will overwrite the existing file content.".bright_blue());
                println!("│ {}", "✓ Retry scheduled".bright_green());
                Ok(false)// Signal retry
            } else {
                // No more translation retries, but we can try fix again
                println!("│ {}", "No translation retries remaining, attempting fix with new suggestion...".bright_yellow());
                
                // Apply fix with the suggestion
                let format_progress = |op: &str| format!("Fix with suggestion - {}", op);
                apply_error_fix(feature, file_type, rs_file, &build_error, &format_progress, true)?;
                
                // Try to build one more time
                println!("│");
                println!("│ {}", "Building with applied fix...".bright_blue().bold());
                match builder::cargo_build(feature, true) {
                    Ok(_) => {
                        println!("│ {}", "✓ Build successful after applying suggestion!".bright_green().bold());
                        Ok(true)
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Build still failing after fix attempt".red());
                        Err(e).context(format!(
                            "Build failed after fix with suggestion for file {}",
                            rs_file.display()
                        ))
                    }
                }
            }
        }
        interaction::UserChoice::ManualFix => {
            println!("│");
            println!("│ {}", "You chose: Manual fix".bright_cyan());
            
            // Try to open vim
            match interaction::open_in_vim(rs_file) {
                Ok(_) => {
                    // After Vim editing, repeatedly try building and allow the user
                    // to decide whether to retry or exit, using a loop to avoid recursion
                    loop {
                        println!("│");
                        println!("│ {}", "Vim editing completed. Attempting to build...".bright_blue());
                        
                        // Try building after manual edit
                        match builder::cargo_build(feature, true) {
                            Ok(_) => {
                                println!("│ {}", "✓ Build successful after manual fix!".bright_green().bold());
                                return Ok(true);
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
                                        match interaction::open_in_vim(rs_file) {
                                            Ok(_) => {
                                                // After additional manual fixes, loop will retry the build
                                                continue;
                                            }
                                            Err(open_err) => {
                                                println!("│ {}", format!("Failed to reopen vim: {}", open_err).red());
                                                println!("│ {}", "Cannot continue manual fix flow; exiting.".yellow());
                                                return Err(open_err).context(format!(
                                                    "Build still failing and could not reopen vim for file {}",
                                                    rs_file.display()
                                                ));
                                            }
                                        }
                                    }
                                    interaction::UserChoice::Exit => {
                                        return Err(e).context(format!(
                                            "Build failed after manual fix for file {}",
                                            rs_file.display()
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(e).context(format!(
                        "Build failed (original error: {}) and could not open vim for file {}",
                        build_error,
                        rs_file.display()
                    ))
                }
            }
        }
        interaction::UserChoice::Exit => {
            println!("│");
            println!("│ {}", "You chose: Exit".yellow());
            println!("│ {}", "Exiting due to build failures.".yellow());
            Err(build_error).context(format!(
                "Build failed after {} fix attempts for file {}. User chose to exit.",
                max_fix_attempts,
                rs_file.display()
            ))
        }
    }
}

/// Apply error fix to the file
pub(crate) fn apply_error_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    build_error: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String
{
    use std::fs;
    
    println!("│ {}", "⚠ Build failed, attempting to fix errors...".yellow().bold());
    println!("│");
    println!("│ {}", format_progress("Fix").bright_magenta().bold());
    // Always show full fixed code, but respect user's preference for error preview
    translator::fix_translation_error(
        feature, 
        file_type, 
        rs_file, 
        &build_error.to_string(), 
        show_full_output,  // User's preference for error preview
        true,              // Always show full fixed code
    )?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Fix failed: output file is empty");
    }
    println!("│ {}", "✓ Fix applied".bright_green());
    
    Ok(())
}

/// Complete file processing (commit, analyze, hybrid build)
fn complete_file_processing<F>(
    feature: &str, 
    file_name: &str, 
    file_type: &str,
    rs_file: &std::path::Path,
    format_progress: &F
) -> Result<()>
where
    F: Fn(&str) -> String
{
    println!("│");
    println!("│ {}", format_progress("Commit").bright_magenta().bold());
    println!("│ {}", "Committing changes...".bright_blue());
    git::git_commit(&format!("Translate {} from C to Rust (feature: {})", file_name, feature), feature)?;
    println!("│ {}", "✓ Changes committed".bright_green());

    println!("│");
    println!("│ {}", format_progress("Update Analysis").bright_magenta().bold());
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    println!("│");
    println!("│ {}", format_progress("Commit Analysis").bright_magenta().bold());
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

    println!("│");
    println!("│ {}", format_progress("Hybrid Build Tests").bright_magenta().bold());
    println!("│ {}", "Running hybrid build tests...".bright_blue());
    builder::run_hybrid_build_interactive(feature, Some(file_type), Some(rs_file))?;
    
    println!("{}", "└─ File processing complete".bright_white().bold());
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::tempdir;
    
    #[test]
    fn test_parse_error_for_files_basic() {
        // Create a temporary directory structure
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();
        
        // Create feature directory structure
        let feature = "test_feature";
        let c2rust_dir = project_root.join(".c2rust");
        fs::create_dir_all(&c2rust_dir).unwrap();
        
        let feature_dir = c2rust_dir.join(feature);
        let rust_dir = feature_dir.join("rust");
        fs::create_dir_all(&rust_dir).unwrap();
        
        // Create test files
        let test_file = rust_dir.join("var_test.rs");
        fs::write(&test_file, "// test content").unwrap();
        
        // Set the project root environment (this is a bit tricky - we'll need to work around this)
        // For now, just test the regex pattern
        let _error_msg = "error[E0308]: mismatched types
   --> var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`";
        
        // The function will need the project root context
        // Since we can't easily override that in tests, we'll test the regex logic separately
    }
    
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
}

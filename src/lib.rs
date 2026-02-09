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

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};
use std::process::Command;

/// Error recovery choices for interactive menu
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorRecoveryChoice {
    Continue,
    ManualFix,
    Exit,
}

/// Custom error type to signal user requested exit
#[derive(Debug)]
pub struct UserRequestedExit;

impl std::fmt::Display for UserRequestedExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "User requested exit")
    }
}

impl std::error::Error for UserRequestedExit {}

/// Truncate error message to a reasonable length for API calls.
///
/// Limits error messages to avoid overwhelming the translator API with
/// extremely long error outputs while preserving essential information.
///
/// # Arguments
/// * `error` - The error message to truncate
/// * `max_lines` - Maximum number of lines to keep (default: 50)
///
/// # Returns
/// Truncated error string with indication if truncation occurred
fn truncate_error_for_api(error: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = error.lines().collect();
    
    if lines.len() <= max_lines {
        return error.to_string();
    }
    
    let truncated_lines: Vec<&str> = lines.iter().take(max_lines).copied().collect();
    let remaining = lines.len() - max_lines;
    
    format!(
        "{}\n\n... ({} more lines truncated for brevity)",
        truncated_lines.join("\n"),
        remaining
    )
}

/// Open vim editor or display file path for manual editing.
///
/// This function first attempts to open the file in vim. If vim is not available
/// or fails to start, it falls back to displaying the absolute file path for manual
/// editing in another editor.
///
/// # Arguments
/// * `rs_file` - Path to the Rust source file to edit
///
/// # Behavior
/// - Tries to launch vim with the file
/// - If successful, waits for user to press Enter to continue
/// - If vim fails/unavailable, displays canonicalized file path
/// - Waits for user confirmation before returning
///
/// # Errors
/// Returns error if file path cannot be canonicalized or stdin read fails
fn manual_fix_file(rs_file: &std::path::Path) -> Result<()> {
    // 1. Try to use vim to open the file
    if let Ok(status) = Command::new("vim")
        .arg(rs_file)
        .status() 
    {
        if status.success() {
            println!("│ File edited. Press Enter to continue...");
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            return Ok(());
        }
    }
    
    // 2. If vim is not available, output file absolute path
    let absolute_path = match rs_file.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            // If canonicalize fails (file doesn't exist or other error),
            // fall back to using the current directory + relative path
            std::env::current_dir()?.join(rs_file)
        }
    };
    println!("│ Vim not available. Please manually edit the file at:");
    println!("│ {}", absolute_path.display());
    println!("│ Press Enter when done...");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(())
}

/// Get user suggestions for fixing test failures.
///
/// Prompts the user to enter multi-line suggestions for fixing a test failure.
/// Input continues until EOF is reached (Ctrl+D on Unix, Ctrl+Z on Windows).
///
/// # Returns
/// Trimmed string containing user's fix suggestions
///
/// # Errors
/// Returns error if stdin cannot be read or if user provides no suggestions
fn get_user_fix_suggestions() -> Result<String> {
    use std::io::Read;
    
    println!("│");
    println!("│ Please enter your suggestions for fixing the test failure:");
    println!("│ (Press Ctrl+D or Ctrl+Z on Windows when done)");
    print!("│ > ");
    io::stdout().flush()?;
    
    let mut suggestions = String::new();
    io::stdin().read_to_string(&mut suggestions)?;
    
    let trimmed = suggestions.trim();
    if trimmed.is_empty() {
        anyhow::bail!("No fix suggestions provided. Please enter suggestions or choose a different option.");
    }
    
    Ok(trimmed.to_string())
}

/// Prompt user with error recovery menu and handle their choice.
///
/// Displays an interactive menu with three options when a build or test failure occurs:
/// 1. Continue trying - Retry (for builds) or provide suggestions (for tests)
/// 2. Manual fix - Edit file with vim or display path for manual editing
/// 3. Exit - Quit the program
///
/// # Arguments
/// * `error_type` - Type of error, must be "Build" or "Test" (affects menu text)
/// * `file_name` - Name of the file being processed (used in error messages passed to caller)
/// * `rs_file` - Path to the Rust file (used for manual editing)
/// * `error_details` - Detailed error message to display to user
/// * `feature` - Feature name for build/test verification
/// * `show_full_output` - Whether to show full output during verification
///
/// # Returns
/// Tuple of (ErrorRecoveryChoice, Option<String>) where:
/// - ErrorRecoveryChoice indicates user's choice
/// - Option<String> contains user suggestions (only for Test + Continue choice)
///
/// # Behavior
/// - For option 2 (Manual fix), automatically verifies the fix by rebuilding/retesting
/// - If verification fails, re-displays the menu
/// - Loops until valid choice is made and (for manual fix) verification succeeds
/// - Option 3 (Exit) terminates the process immediately
///
/// # Errors
/// Returns error if stdin/stdout operations fail or verification encounters issues
fn prompt_error_recovery(
    error_type: &str,  // "Build" or "Test"
    _file_name: &str,
    rs_file: &std::path::Path,
    error_details: &str,
    feature: &str,
    show_full_output: bool,
) -> Result<(ErrorRecoveryChoice, Option<String>)> {
    loop {
        println!("│");
        println!("│ {}", format!("⚠ {} failed!", error_type).red().bold());
        println!("│ {}", error_details.yellow());
        println!("│");
        println!("│ Please choose how to proceed:");
        
        if error_type == "Build" {
            println!("│ 1. Continue trying - Retry translation from scratch");
        } else {
            println!("│ 1. Continue trying - Provide suggestions for fixing the test");
        }
        
        println!("│ 2. Manual fix - Edit the file manually");
        println!("│ 3. Exit - Quit the program");
        println!("│");
        
        print!("│ Enter your choice (1/2/3): ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim() {
            "1" => {
                // For test failures, get user suggestions
                if error_type == "Test" {
                    let suggestions = get_user_fix_suggestions()?;
                    return Ok((ErrorRecoveryChoice::Continue, Some(suggestions)));
                } else {
                    return Ok((ErrorRecoveryChoice::Continue, None));
                }
            }
            "2" => {
                manual_fix_file(rs_file)?;
                // After manual fix, verify by rebuilding/retesting
                match error_type {
                    "Build" => {
                        println!("│ {}", "Verifying fix...".bright_blue());
                        match builder::cargo_build(feature, show_full_output) {
                            Ok(_) => {
                                println!("│ {}", "✓ Build successful after manual fix!".bright_green().bold());
                                return Ok((ErrorRecoveryChoice::ManualFix, None));
                            }
                            Err(e) => {
                                println!("│ {}", "Build still fails. Please choose again.".yellow());
                                println!("│ Error: {}", truncate_error_for_api(&e.to_string(), 10));
                                continue;
                            }
                        }
                    }
                    "Test" => {
                        println!("│ {}", "Verifying fix...".bright_blue());
                        match builder::run_hybrid_build(feature) {
                            Ok(_) => {
                                println!("│ {}", "✓ Tests successful after manual fix!".bright_green().bold());
                                return Ok((ErrorRecoveryChoice::ManualFix, None));
                            }
                            Err(e) => {
                                println!("│ {}", "Tests still fail. Please choose again.".yellow());
                                println!("│ Error: {}", truncate_error_for_api(&e.to_string(), 10));
                                continue;
                            }
                        }
                    }
                    _ => return Ok((ErrorRecoveryChoice::ManualFix, None)),
                }
            }
            "3" => {
                return Ok((ErrorRecoveryChoice::Exit, None));
            }
            _ => {
                println!("│ {}", "Invalid choice. Please enter 1, 2, or 3.".yellow());
                continue;
            }
        }
    }
}

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
        
        // Handle initial build failures with error recovery menu
        loop {
            match builder::cargo_build(feature, show_full_output) {
                Ok(_) => {
                    println!("{}", "✓ Build successful!".bright_green().bold());
                    break;
                }
                Err(build_err) => {
                    // Include truncated error details in the menu
                    let error_details = format!(
                        "Initial build failed before processing files\n\n{}",
                        truncate_error_for_api(&build_err.to_string(), 20)
                    );
                    
                    // Use Cargo.toml as a representative file for project-level build errors
                    // since we don't have a specific .rs file being processed yet
                    let cargo_toml_path = rust_dir.join("Cargo.toml");
                    
                    let (choice, _) = prompt_error_recovery(
                        "Build",
                        "project",
                        &cargo_toml_path,
                        &error_details,
                        feature,
                        show_full_output,
                    )?;
                    
                    match choice {
                        ErrorRecoveryChoice::Continue => {
                            // Retry the build
                            println!("│ {}", "Retrying build...".bright_cyan());
                            continue;
                        }
                        ErrorRecoveryChoice::ManualFix => {
                            // Manual fix was successful, continue
                            println!("│ {}", "✓ Manual fix successful, continuing...".bright_green());
                            break;
                        }
                        ErrorRecoveryChoice::Exit => {
                            return Err(UserRequestedExit).context("User requested exit during initial build failure");
                        }
                    }
                }
            }
        }

        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
            
        git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

        println!("{}", "Running hybrid build tests...".bright_blue());
        
        // Handle initial test failures with error recovery menu
        loop {
            match builder::run_hybrid_build(feature) {
                Ok(_) => {
                    break;
                }
                Err(test_err) => {
                    // Include truncated error details in the menu
                    let error_details = format!(
                        "Initial test failed before processing files\n\n{}",
                        truncate_error_for_api(&test_err.to_string(), 20)
                    );
                    
                    // Use Cargo.toml as a representative file for project-level test errors
                    // since we don't have a specific .rs file being processed yet
                    let cargo_toml_path = rust_dir.join("Cargo.toml");
                    
                    // For initial tests, we don't have a specific file to fix,
                    // so we use "Build" type to avoid requesting suggestions
                    let (choice, _) = prompt_error_recovery(
                        "Build",  // Use "Build" type since we can't apply fixes to a specific file
                        "project",
                        &cargo_toml_path,
                        &error_details,
                        feature,
                        show_full_output,
                    )?;
                    
                    match choice {
                        ErrorRecoveryChoice::Continue => {
                            println!("│ {}", "Retrying tests...".bright_cyan());
                            continue;
                        }
                        ErrorRecoveryChoice::ManualFix => {
                            // Manual fix was successful, continue
                            println!("│ {}", "✓ Manual fix successful, continuing...".bright_green());
                            break;
                        }
                        ErrorRecoveryChoice::Exit => {
                            return Err(UserRequestedExit).context("User requested exit during initial test failure");
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
            complete_file_processing(feature, file_name, rs_file, &format_progress, show_full_output)?;
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
                        show_full_output,
                    );
                } else {
                    apply_error_fix(feature, file_type, rs_file, &build_error, format_progress, show_full_output)?;
                }
            }
        }
    }
    
    Ok(false)
}

/// Handle the case when max fix attempts are reached
fn handle_max_fix_attempts_reached(
    build_error: anyhow::Error,
    file_name: &str,
    rs_file: &std::path::Path,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    feature: &str,
    show_full_output: bool,
) -> Result<bool> {
    use constants::MAX_TRANSLATION_ATTEMPTS;
    
    let error_details = if !is_last_attempt {
        let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
        format!(
            "File {} still has build errors after {} fix attempts. ({} retries remaining)",
            file_name, max_fix_attempts, remaining_retries
        )
    } else {
        let total_retries = MAX_TRANSLATION_ATTEMPTS - 1;
        format!(
            "File {} still has build errors after {} fix attempts. All {} attempts exhausted (1 initial + {} retries).",
            file_name, max_fix_attempts, MAX_TRANSLATION_ATTEMPTS, total_retries
        )
    };
    
    let (choice, _) = prompt_error_recovery(
        "Build",
        file_name,
        rs_file,
        &error_details,
        feature,
        show_full_output,
    )?;
    
    match choice {
        ErrorRecoveryChoice::Continue => {
            if is_last_attempt {
                // Cannot retry anymore
                println!("│ {}", "Cannot retry - all attempts exhausted.".red());
                Err(build_error).context(format!(
                    "Build failed after {} fix attempts and all retries for file {}",
                    max_fix_attempts,
                    rs_file.display()
                ))
            } else {
                println!("│ {}", "Retrying translation...".bright_cyan());
                println!("│ {}", "Note: The translator will overwrite the existing file content.".bright_blue());
                println!("│ {}", "✓ Retry scheduled".bright_green());
                Ok(false) // Signal retry
            }
        }
        ErrorRecoveryChoice::ManualFix => {
            // Manual fix was successful (verified in prompt_error_recovery)
            println!("│ {}", "✓ Manual fix successful".bright_green());
            Ok(true) // Continue with build successful
        }
        ErrorRecoveryChoice::Exit => {
            return Err(UserRequestedExit).context(format!(
                "User requested exit during build error recovery for file {}",
                rs_file.display()
            ));
        }
    }
}

/// Apply error fix to the file
fn apply_error_fix<F>(
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
    translator::fix_translation_error(feature, file_type, rs_file, &build_error.to_string(), show_full_output)?;

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
    rs_file: &std::path::Path,
    format_progress: &F,
    show_full_output: bool,
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
    
    // Handle test failures with error recovery menu
    loop {
        match builder::run_hybrid_build(feature) {
            Ok(_) => {
                println!("{}", "└─ File processing complete".bright_white().bold());
                return Ok(());
            }
            Err(test_error) => {
                let error_details = format!("Test failed for file {}", file_name);
                let (choice, suggestions) = prompt_error_recovery(
                    "Test",
                    file_name,
                    rs_file,
                    &error_details,
                    feature,
                    show_full_output,
                )?;
                
                match choice {
                    ErrorRecoveryChoice::Continue => {
                        // Apply fix with user suggestions
                        if let Some(user_suggestions) = suggestions {
                            println!("│ {}", "Applying fix based on your suggestions...".bright_blue());
                            let (file_type, _) = extract_and_validate_file_info(rs_file)?;
                            
                            // Truncate test error to avoid overwhelming the API
                            let truncated_error = truncate_error_for_api(&test_error.to_string(), 50);
                            
                            // Combine test error and user suggestions
                            let fix_prompt = format!(
                                "Test Error:\n{}\n\nUser Suggestions:\n{}",
                                truncated_error,
                                user_suggestions
                            );
                            
                            translator::fix_translation_error(
                                feature,
                                file_type,
                                rs_file,
                                &fix_prompt,
                                show_full_output,
                            )?;
                            println!("│ {}", "✓ Fix applied, retrying tests...".bright_green());
                            
                            // Re-commit the fix
                            git::git_commit(
                                &format!("Fix test failures for {} based on user suggestions", file_name),
                                feature,
                            )?;
                            
                            // Continue loop to retry tests
                            continue;
                        } else {
                            // No suggestions provided (shouldn't happen for tests)
                            println!("│ {}", "No suggestions provided, retrying...".yellow());
                            continue;
                        }
                    }
                    ErrorRecoveryChoice::ManualFix => {
                        // Manual fix was successful (verified in prompt_error_recovery)
                        println!("│ {}", "✓ Manual fix successful".bright_green());
                        println!("{}", "└─ File processing complete".bright_white().bold());
                        return Ok(());
                    }
                    ErrorRecoveryChoice::Exit => {
                        return Err(UserRequestedExit).context(format!(
                            "User requested exit during test error recovery for file {}",
                            file_name
                        ));
                    }
                }
            }
        }
    }
}

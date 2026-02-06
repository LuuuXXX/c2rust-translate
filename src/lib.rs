pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod translator;
pub mod util;
pub mod progress;
pub mod logger;
pub mod constants;

use anyhow::{Context, Result};
use colored::Colorize;

/// Main translation workflow for a feature
pub fn translate_feature(feature: &str, allow_all: bool, max_fix_attempts: usize) -> Result<()> {
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
            break;
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
            file_scanner::prompt_file_selection(&unprocessed_files, &rust_dir)?
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
            
            process_rs_file(feature, rs_file, file_name, current_position, total_count, max_fix_attempts)?;
            
            // Mark file as processed and save progress
            progress_state.mark_processed(rs_file, &rust_dir)?;
            progress_state.save(feature)?;
        }
    }

    Ok(())
}

/// Process a single .rs file through the translation workflow
fn process_rs_file(feature: &str, rs_file: &std::path::Path, file_name: &str, current_position: usize, total_count: usize, max_fix_attempts: usize) -> Result<()> {
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
        translate_file(feature, file_type, rs_file, &format_progress)?;
        
        // Build and fix errors
        let build_successful = build_and_fix_loop(
            feature, 
            file_type, 
            rs_file, 
            file_name, 
            &format_progress,
            is_last_attempt,
            attempt_number,
            max_fix_attempts
        )?;
        
        if build_successful {
            complete_file_processing(feature, file_name, &format_progress)?;
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
fn translate_file<F>(feature: &str, file_type: &str, rs_file: &std::path::Path, format_progress: &F) -> Result<()> 
where
    F: Fn(&str) -> String
{
    use std::fs;
    
    let c_file = rs_file.with_extension("c");
    
    println!("│");
    println!("│ {}", format_progress("Translation").bright_magenta().bold());
    println!("│ {}", format!("Translating {} to Rust...", file_type).bright_blue().bold());
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file)?;

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
) -> Result<bool>
where
    F: Fn(&str) -> String
{
    
    for attempt in 1..=max_fix_attempts {
        println!("│");
        println!("│ {}", format_progress("Build").bright_magenta().bold());
        println!("│ {}", format!("Building Rust project (attempt {}/{})", attempt, max_fix_attempts).bright_blue().bold());
        
        match builder::cargo_build(feature) {
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
                        max_fix_attempts
                    );
                } else {
                    apply_error_fix(feature, file_type, rs_file, &build_error, format_progress)?;
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
) -> Result<bool> {
    use std::io::{self, Write};
    use constants::MAX_TRANSLATION_ATTEMPTS;
    
    println!("│");
    println!("│ {}", "⚠ Maximum fix attempts reached!".red().bold());
    println!("│ {}", format!("File {} still has build errors after {} fix attempts.", file_name, max_fix_attempts).yellow());
    println!("│");
    
    if !is_last_attempt {
        let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
        println!("│ {}", format!("Do you want to retry translating this file from scratch? ({} retries remaining)", remaining_retries).bright_yellow());
        println!("│ {} Type 'retry' to retry, or press Enter to skip:", "→".bright_yellow());
        
        loop {
            print!("│ ");
            io::stdout().flush()?;
            
            let mut user_input = String::new();
            io::stdin().read_line(&mut user_input)?;
            
            let input_trimmed = user_input.trim();
            if input_trimmed.eq_ignore_ascii_case("retry") {
                println!("│ {}", "Retrying translation...".bright_cyan());
                println!("│ {}", "Note: The translator will overwrite the existing file content.".bright_blue());
                println!("│ {}", "✓ Retry scheduled".bright_green());
                return Ok(false); // Signal retry
            } else if input_trimmed.is_empty() {
                // User pressed Enter to skip
                println!("│ {}", "Skipping file due to build errors.".yellow());
                return Err(build_error).context(format!(
                    "Build failed after {} fix attempts for file {}",
                    max_fix_attempts,
                    rs_file.display()
                ));
            } else {
                // Invalid input, prompt user to try again
                println!("│ {}", format!("Invalid input '{}'. Please type 'retry' to retry, or press Enter to skip.", input_trimmed).yellow());
                continue;
            }
        }
    } else {
        let total_retries = MAX_TRANSLATION_ATTEMPTS - 1;
        println!("│ {}", format!("All {} attempts exhausted (1 initial + {} retries). Cannot retry further.", MAX_TRANSLATION_ATTEMPTS, total_retries).red());
        Err(build_error).context(format!(
            "Build failed after {} fix attempts and {} retries for file {}",
            max_fix_attempts,
            total_retries,
            rs_file.display()
        ))
    }
}

/// Apply error fix to the file
fn apply_error_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    build_error: &anyhow::Error,
    format_progress: &F,
) -> Result<()>
where
    F: Fn(&str) -> String
{
    use std::fs;
    
    println!("│ {}", "⚠ Build failed, attempting to fix errors...".yellow().bold());
    println!("│");
    println!("│ {}", format_progress("Fix").bright_magenta().bold());
    translator::fix_translation_error(feature, file_type, rs_file, &build_error.to_string())?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Fix failed: output file is empty");
    }
    println!("│ {}", "✓ Fix applied".bright_green());
    
    Ok(())
}

/// Complete file processing (commit, analyze, hybrid build)
fn complete_file_processing<F>(feature: &str, file_name: &str, format_progress: &F) -> Result<()>
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
    builder::run_hybrid_build(feature)?;
    
    println!("{}", "└─ File processing complete".bright_white().bold());
    
    Ok(())
}

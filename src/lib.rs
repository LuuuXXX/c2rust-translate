//! C to Rust translation workflow orchestration
//!
//! This module provides the main translation workflow that coordinates initialization,
//! gate verification, file selection, and translation execution across multiple modules.

// Public modules - external API
pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod hybrid_build;
pub mod initialization;
pub mod translator;
pub mod util;
pub mod verification;

// Internal modules - implementation details
pub(crate) mod diff_display;
pub(crate) mod error_handler;
pub(crate) mod interaction;
pub(crate) mod suggestion;

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// Main translation workflow for a feature
///
/// Executes the complete C to Rust translation workflow in 5 steps:
/// 1. Find project root and initialize feature directory
/// 2. Run gate verification (cargo build, code analysis, hybrid build/test)
/// 3. Select files to translate (interactive or auto-all)
/// 4. Initialize progress tracking
/// 5. Execute translation loop for all selected files
///
/// # Arguments
/// * `feature` - Feature name (must not contain path separators)
/// * `allow_all` - If true, auto-process all files without prompting
/// * `max_fix_attempts` - Maximum number of error fix attempts per file
/// * `show_full_output` - If true, show complete code/error output without truncation
///
/// # Returns
/// * `Ok(())` - All translations completed successfully
/// * `Err` - Translation failed or user aborted
pub fn translate_feature(
    feature: &str,
    allow_all: bool,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    print_workflow_header(feature);

    // Step 1: Initialize feature directory
    step_1_initialize(feature)?;

    // Step 2: Run gate verification
    step_2_gate_verification(feature, show_full_output)?;

    // Step 3 & 4: Select files and initialize progress
    let (rust_dir, mut progress_state) = step_3_4_select_files_and_init_progress(feature)?;

    // Step 5: Execute translation loop
    step_5_execute_translation_loop(
        feature,
        &rust_dir,
        &mut progress_state,
        allow_all,
        max_fix_attempts,
        show_full_output,
    )?;

    Ok(())
}

// ============================================================================
// Workflow Step Functions
// ============================================================================

/// Print the workflow header
fn print_workflow_header(feature: &str) {
    let msg = format!("Starting translation for feature: {}", feature);
    println!("{}", msg.bright_cyan().bold());
}

/// Step 1: Find project root and initialize feature directory
fn step_1_initialize(feature: &str) -> Result<()> {
    println!(
        "\n{}",
        "Step 1: Find Project Root and Initialize"
            .bright_cyan()
            .bold()
    );
    initialization::check_and_initialize_feature(feature)
}

/// Step 2: Run gate verification
fn step_2_gate_verification(feature: &str, show_full_output: bool) -> Result<()> {
    initialization::run_gate_verification(feature, show_full_output)
}

/// Steps 3 & 4: Select files to translate and initialize progress tracking
fn step_3_4_select_files_and_init_progress(
    feature: &str,
) -> Result<(std::path::PathBuf, util::ProgressState)> {
    println!(
        "\n{}",
        "Step 3: Select Files to Translate".bright_cyan().bold()
    );

    // Get rust directory path
    let project_root = util::find_project_root()?;
    let rust_dir = project_root.join(".c2rust").join(feature).join("rust");

    // Calculate progress
    let total_rs_files = file_scanner::count_all_rs_files(&rust_dir)?;
    let initial_empty_count = file_scanner::find_empty_rs_files(&rust_dir)?.len();
    let already_processed = total_rs_files.saturating_sub(initial_empty_count);

    let progress_state =
        util::ProgressState::with_initial_progress(total_rs_files, already_processed);

    // Display progress
    print_progress_status(already_processed, total_rs_files);

    Ok((rust_dir, progress_state))
}

/// Print current progress status
fn print_progress_status(already_processed: usize, total_rs_files: usize) {
    println!(
        "\n{}",
        "Step 4: Initialize Project Progress".bright_cyan().bold()
    );
    
    let progress_percentage = if total_rs_files > 0 {
        (already_processed as f64 / total_rs_files as f64) * 100.0
    } else {
        0.0
    };
    
    println!(
        "{} {:.1}% ({}/{} files processed)",
        "Current progress:".cyan(),
        progress_percentage,
        already_processed,
        total_rs_files
    );
}

/// Step 5: Execute translation loop for all files
fn step_5_execute_translation_loop(
    feature: &str,
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    allow_all: bool,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    println!(
        "\n{}",
        "Step 5: Execute Translation for All Files"
            .bright_cyan()
            .bold()
    );

    loop {
        // Scan for empty .rs files
        let empty_rs_files = file_scanner::find_empty_rs_files(rust_dir)?;

        if empty_rs_files.is_empty() {
            print_completion_message();
            break;
        }

        print_files_found_message(empty_rs_files.len());

        // Select files to process
        let selected_indices = select_files_to_process(&empty_rs_files, rust_dir, allow_all)?;

        // Process each selected file
        process_selected_files(
            feature,
            &empty_rs_files,
            &selected_indices,
            progress_state,
            max_fix_attempts,
            show_full_output,
        )?;
    }

    Ok(())
}

// ============================================================================
// File Processing Functions
// ============================================================================

/// Select files to process based on allow_all flag
fn select_files_to_process(
    empty_rs_files: &[std::path::PathBuf],
    rust_dir: &Path,
    allow_all: bool,
) -> Result<Vec<usize>> {
    if allow_all {
        // Auto-process all files without prompting
        Ok((0..empty_rs_files.len()).collect())
    } else {
        // Prompt user for file selection
        let file_refs: Vec<_> = empty_rs_files.iter().collect();
        file_scanner::prompt_file_selection(&file_refs, rust_dir)
    }
}

/// Process all selected files
fn process_selected_files(
    feature: &str,
    empty_rs_files: &[std::path::PathBuf],
    selected_indices: &[usize],
    progress_state: &mut util::ProgressState,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    for &idx in selected_indices.iter() {
        let rs_file = &empty_rs_files[idx];
        let current_position = progress_state.get_current_position();
        let total_count = progress_state.get_total_count();

        let file_name = rs_file
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>");

        print_file_processing_header(current_position, total_count, file_name);

        process_rs_file(
            feature,
            rs_file,
            file_name,
            current_position,
            total_count,
            max_fix_attempts,
            show_full_output,
        )?;

        progress_state.mark_processed();
    }
    Ok(())
}

// ============================================================================
// Display Helper Functions
// ============================================================================

/// Print completion message
fn print_completion_message() {
    let msg = "✓ No empty .rs files found. Translation complete!";
    println!("\n{}", msg.bright_green().bold());
}

/// Print files found message
fn print_files_found_message(count: usize) {
    println!(
        "{}",
        format!("Found {} empty .rs file(s) to process", count).cyan()
    );
}

/// Print file processing header
fn print_file_processing_header(current_position: usize, total_count: usize, file_name: &str) {
    let progress_msg = format!(
        "[{}/{}] Processing {}",
        current_position, total_count, file_name
    );
    println!("\n{}", progress_msg.bright_magenta().bold());
}

// ============================================================================
// Single File Translation Workflow
// ============================================================================

/// Process a single .rs file through the translation workflow
///
/// Attempts translation up to MAX_TRANSLATION_ATTEMPTS times, with each attempt
/// including: translation → build → fix (if needed) → hybrid tests → commit
///
/// # Arguments
/// * `feature` - Feature name
/// * `rs_file` - Path to the .rs file to process
/// * `file_name` - Display name of the file
/// * `current_position` - Current file position in the overall workflow
/// * `total_count` - Total number of files to process
/// * `max_fix_attempts` - Maximum error fix attempts per translation
/// * `show_full_output` - Whether to show full output
///
/// # Returns
/// * `Ok(())` - File processed successfully
/// * `Err` - Processing failed after all retry attempts
fn process_rs_file(
    feature: &str,
    rs_file: &Path,
    file_name: &str,
    current_position: usize,
    total_count: usize,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    use util::MAX_TRANSLATION_ATTEMPTS;

    for attempt_number in 1..=MAX_TRANSLATION_ATTEMPTS {
        let is_last_attempt = attempt_number == MAX_TRANSLATION_ATTEMPTS;

        print_attempt_header(attempt_number, rs_file);

        if attempt_number > 1 {
            println!(
                "│ {}",
                "Starting fresh translation (previous translation will be overwritten)..."
                    .bright_cyan()
            );
        }

        // Extract file information and validate
        let (file_type, _name) = extract_and_validate_file_info(rs_file)?;
        check_c_file_exists(rs_file)?;

        // Create progress formatter
        let format_progress = |operation: &str| {
            format!(
                "[{}/{}] Processing {} - {}",
                current_position, total_count, file_name, operation
            )
        };

        // Translate C to Rust
        translate_file(feature, file_type, rs_file, &format_progress, show_full_output)?;

        // Build and fix errors
        let build_successful = verification::build_and_fix_loop(
            feature,
            file_type,
            rs_file,
            file_name,
            &format_progress,
            is_last_attempt,
            attempt_number,
            max_fix_attempts,
            show_full_output,
        )?;

        if build_successful {
            let processing_complete =
                complete_file_processing(feature, file_name, file_type, rs_file, &format_progress)?;
            if processing_complete {
                return Ok(());
            }
            // If not complete, retry translation (loop continues)
        }
    }

    anyhow::bail!("Unexpected: all retry attempts completed without resolution")
}

// ============================================================================
// File Processing Helper Functions
// ============================================================================

/// Print header for translation attempt
fn print_attempt_header(attempt_number: usize, rs_file: &Path) {
    if attempt_number > 1 {
        let retry_number = attempt_number - 1;
        let max_retries = util::MAX_TRANSLATION_ATTEMPTS - 1;
        println!(
            "\n{}",
            format!(
                "┌─ Retry attempt {}/{}: {}",
                retry_number, max_retries, rs_file.display()
            )
            .bright_yellow()
            .bold()
        );
    } else {
        println!(
            "\n{}",
            format!("┌─ Processing file: {}", rs_file.display())
                .bright_white()
                .bold()
        );
    }
}

/// Extract and validate file type information from filename
///
/// Returns (file_type, name) tuple where file_type is either "var" or "fun"
fn extract_and_validate_file_info(rs_file: &Path) -> Result<(&'static str, &str)> {
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

/// Check if corresponding C source file exists
fn check_c_file_exists(rs_file: &Path) -> Result<()> {
    let c_file = rs_file.with_extension("c");
    
    match std::fs::metadata(&c_file) {
        Ok(_) => {
            println!(
                "│ {} {}",
                "C source:".cyan(),
                c_file.display().to_string().bright_yellow()
            );
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "Corresponding C file not found for Rust file: {}",
                rs_file.display()
            );
        }
        Err(err) => Err(err).context(format!(
            "Failed to access corresponding C file for Rust file: {}",
            rs_file.display()
        )),
    }
}

// ============================================================================
// Translation and Error Fix Functions
// ============================================================================

/// Translate C source file to Rust
fn translate_file<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    let c_file = rs_file.with_extension("c");

    println!("│");
    println!(
        "│ {}",
        format_progress("Translation").bright_magenta().bold()
    );
    println!(
        "│ {}",
        format!("Translating {} to Rust...", file_type)
            .bright_blue()
            .bold()
    );
    
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file, show_full_output)?;

    // Verify translation produced output
    let metadata = std::fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }
    
    println!(
        "│ {}",
        format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green()
    );

    Ok(())
}

/// Apply error fix to translated file
pub(crate) fn apply_error_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    build_error: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    println!(
        "│ {}",
        "⚠ Build failed, attempting to fix errors..."
            .yellow()
            .bold()
    );
    println!("│");
    println!("│ {}", format_progress("Fix").bright_magenta().bold());
    
    // Fix translation error
    // Always show full fix code, but respect user preference for error preview
    translator::fix_translation_error(
        feature,
        file_type,
        rs_file,
        &build_error.to_string(),
        show_full_output, // User preference for error preview
        true,             // Always show full fix code
    )?;

    // Verify fix produced output
    let metadata = std::fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Fix failed: output file is empty");
    }
    
    println!("│ {}", "✓ Fix applied".bright_green());

    Ok(())
}

// ============================================================================
// File Completion and Finalization
// ============================================================================

/// Complete file processing by running hybrid build tests and committing changes
///
/// This function runs the final verification steps:
/// 1. Pre-check config and tools availability
/// 2. Run hybrid build clean/build/test
/// 3. Handle user interaction for successful tests
/// 4. Commit changes and update code analysis
///
/// # Returns
/// * `Ok(true)` - File processing completed successfully (continue to next file)
/// * `Ok(false)` - Translation should be retried from scratch
/// * `Err` - Unrecoverable error occurred
fn complete_file_processing<F>(
    feature: &str,
    file_name: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
) -> Result<bool>
where
    F: Fn(&str) -> String,
{
    println!("│");
    println!(
        "│ {}",
        format_progress("Hybrid Build Tests")
            .bright_magenta()
            .bold()
    );
    println!("│ {}", "Running hybrid build tests...".bright_blue());

    // Pre-check: Verify config and tools are available
    verify_hybrid_build_prerequisites()?;

    // Run hybrid build clean/build/test
    builder::c2rust_clean(feature)?;

    // Handle build
    if let Err(build_error) = builder::c2rust_build(feature) {
        println!("│ {}", "✗ Build failed".red().bold());
        let processing_complete = builder::handle_build_failure_interactive(
            feature,
            file_type,
            rs_file,
            build_error,
        )?;
        if !processing_complete {
            return Ok(false); // Retry translation
        }
    } else {
        println!("│ {}", "✓ Build successful".bright_green().bold());
    }

    // Handle test
    match builder::c2rust_test(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());
            handle_successful_tests(feature, file_name, file_type, rs_file, format_progress)?;
            Ok(true) // Processing complete
        }
        Err(test_error) => {
            let processing_complete =
                builder::handle_test_failure_interactive(feature, file_type, rs_file, test_error)?;
            Ok(processing_complete)
        }
    }
}

/// Verify prerequisites for hybrid build (config file and tools)
fn verify_hybrid_build_prerequisites() -> Result<()> {
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");

    if !config_path.exists() {
        eprintln!(
            "{}",
            format!("Error: Config file not found at {}", config_path.display()).red()
        );
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    // Check if c2rust-config is available
    let check_output = std::process::Command::new("c2rust-config")
        .arg("--help")
        .output();

    match check_output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "{}",
                format!(
                    "Error: c2rust-config failed to run.\nstdout:\n{}\nstderr:\n{}",
                    stdout, stderr
                )
                .red()
            );
            anyhow::bail!(
                "c2rust-config is present but failed to run successfully, cannot run hybrid build tests"
            )
        }
        Err(_) => {
            eprintln!("{}", "Error: c2rust-config not found".red());
            anyhow::bail!("c2rust-config not found, cannot run hybrid build tests")
        }
    }
}

/// Handle successful test completion with user interaction
fn handle_successful_tests<F>(
    feature: &str,
    file_name: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    // If in auto-accept mode, skip interaction
    if interaction::is_auto_accept_mode() {
        println!(
            "│ {}",
            "Auto-accept mode: automatically accepting translation".bright_green()
        );
        finalize_file_processing(feature, file_name, format_progress)?;
        return Ok(());
    }

    // Show code comparison and get user choice
    let c_file = rs_file.with_extension("c");
    interaction::display_file_paths(Some(&c_file), rs_file);

    let success_message = "✓ All tests passed";
    if let Err(e) = diff_display::display_code_comparison(
        &c_file,
        rs_file,
        success_message,
        diff_display::ResultType::TestPass,
    ) {
        println!(
            "│ {}",
            format!("Failed to display comparison: {}", e).yellow()
        );
        println!("│ {}", success_message.bright_green().bold());
    }

    let choice = interaction::prompt_compile_success_choice()?;

    match choice {
        interaction::CompileSuccessChoice::Accept => {
            println!("│ {}", "You chose: Accept this code".bright_cyan());
            finalize_file_processing(feature, file_name, format_progress)?;
        }
        interaction::CompileSuccessChoice::AutoAccept => {
            println!(
                "│ {}",
                "You chose: Auto-accept all subsequent translations".bright_cyan()
            );
            interaction::enable_auto_accept_mode();
            finalize_file_processing(feature, file_name, format_progress)?;
        }
        interaction::CompileSuccessChoice::ManualFix => {
            println!("│ {}", "You chose: Manual fix".bright_cyan());
            interaction::open_in_vim(rs_file)?;
            println!(
                "│ {}",
                "Running full build and test after manual changes...".bright_blue()
            );
            builder::run_full_build_and_test_interactive(feature, file_type, rs_file)?;
            println!(
                "│ {}",
                "✓ All builds and tests pass after manual changes".bright_green()
            );
            finalize_file_processing(feature, file_name, format_progress)?;
        }
        interaction::CompileSuccessChoice::Exit => {
            println!("│ {}", "You chose: Exit".yellow());
            anyhow::bail!("User chose to exit after successful tests");
        }
    }

    Ok(())
}

/// Finalize file processing: commit changes and update analysis
fn finalize_file_processing<F>(
    feature: &str,
    file_name: &str,
    format_progress: &F,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    // Commit changes
    println!("│");
    println!("│ {}", format_progress("Commit").bright_magenta().bold());
    println!("│ {}", "Committing changes...".bright_blue());
    git::git_commit(
        &format!(
            "Translate {} from C to Rust (feature: {})",
            file_name, feature
        ),
        feature,
    )?;
    println!("│ {}", "✓ Changes committed".bright_green());

    // Update code analysis
    println!("│");
    println!(
        "│ {}",
        format_progress("Update Analysis").bright_magenta().bold()
    );
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // Commit analysis
    println!("│");
    println!(
        "│ {}",
        format_progress("Commit Analysis").bright_magenta().bold()
    );
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

    println!(
        "{}",
        "└─ File processing complete".bright_white().bold()
    );

    Ok(())
}

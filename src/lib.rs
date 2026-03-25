//! C to Rust translation workflow orchestration
//!
//! This module provides the main translation workflow that coordinates initialization,
//! gate verification, file selection, and translation execution across multiple modules.

// Public modules - external API
pub mod analyzer;
pub mod builder;
pub mod common_tasks;
pub mod file_scanner;
pub mod git;
pub mod hybrid_build;
pub mod initialization;
pub mod merger;
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

/// Interval (in successfully processed files) at which periodic git GC is triggered.
/// Increasing this value reduces GC frequency; decreasing it compacts the repo more often.
const GIT_GC_INTERVAL: usize = 10;

/// Run periodic `git reflog expire` + `git gc` every [`GIT_GC_INTERVAL`] successfully
/// processed files to keep the `.c2rust/.git` directory from growing unbounded.
///
/// Should be called after every successful file translation regardless of which loop
/// produced it, so that long runs with many skipped-file retries also get periodic
/// compaction.  Both reflog expiry and GC failures are non-fatal (warnings only).
fn maybe_run_periodic_git_gc(progress_state: &util::ProgressState) {
    if progress_state.processed_count % GIT_GC_INTERVAL == 0
        && progress_state.processed_count > 0
    {
        git::git_expire_reflog();
        git::git_gc(false); // cheap periodic compaction, default prune grace period
    }
}

/// Main translation workflow for a feature
///
/// Executes the complete C to Rust translation workflow in 5 steps:
/// 1. Find project root and initialize feature directory
/// 2. Run gate verification (cargo build, code analysis, hybrid build/test)
/// 3. Scan for files to translate and initialize progress tracking
/// 4. Display current progress status
/// 5. Execute translation loop (select and process files interactively or auto-all)
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

    // Check test configuration before initial verification (step 2 also uses test-related commands)
    let skip_test = check_test_configuration(feature)?;

    // Step 2: Run initial verification
    step_2_initial_verification(feature, show_full_output, skip_test)?;

    // Step 2.5: Check and load previous translation stats
    let mut stats = step_2_5_load_or_create_stats(feature)?;

    // Step 3 & 4: Select files and initialize progress
    let (rust_dir, mut progress_state) = step_3_4_select_files_and_init_progress(feature, &stats)?;

    // Step 5: Execute translation loop
    let step5_result = step_5_execute_translation_loop(
        feature,
        &rust_dir,
        &mut progress_state,
        allow_all,
        max_fix_attempts,
        show_full_output,
        &mut stats,
        skip_test,
    );

    // Print summary even if step 5 fails, so progress is not lost
    if let Err(e) = step5_result {
        // Compact history even when translation aborts early.
        git::git_expire_reflog();
        git::git_gc(true);
        stats.print_summary();
        return Err(e);
    }

    // Run final aggressive GC after all translations complete to keep .git as compact as possible.
    git::git_expire_reflog();
    git::git_gc(true);
    stats.print_summary();
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

/// Step 2: Run initial verification
fn step_2_initial_verification(feature: &str, show_full_output: bool, skip_test: bool) -> Result<()> {
    initialization::execute_initial_verification(feature, show_full_output, skip_test)
}

/// Check test configuration in `.c2rust/config.toml`.
///
/// Returns `Ok(false)` if both `test.cmd` and `test.dir` are present and non-empty
/// (tests will run normally). Returns `Ok(true)` if the configuration is incomplete
/// and the user chose to continue without tests (skip_test=true). Returns `Err` if
/// the user chose to exit.
fn check_test_configuration(feature: &str) -> Result<bool> {
    let test_cmd = builder::get_config_value("test.cmd", feature);
    let test_dir = builder::get_config_value("test.dir", feature);

    if test_cmd.is_ok() && test_dir.is_ok() {
        // `get_config_value` returns Err for missing or empty values, so Ok(_) here
        // guarantees both keys are present and non-empty in the config file.
        return Ok(false);
    }

    // Configuration is incomplete: prompt the user
    let choice = interaction::prompt_test_config_missing_choice()?;

    match choice {
        interaction::TestConfigChoice::Exit => {
            anyhow::bail!(
                "User chose to exit to configure test settings in .c2rust/config.toml"
            );
        }
        interaction::TestConfigChoice::Continue => {
            println!(
                "{}",
                "✓ Continuing without test phase. Tests will be skipped."
                    .bright_yellow()
                    .bold()
            );
            Ok(true) // skip_test = true
        }
    }
}

/// Step 2.5: Check for existing stats file and load or create stats
fn step_2_5_load_or_create_stats(feature: &str) -> Result<util::TranslationStats> {
    match util::TranslationStats::load_from_file(feature)? {
        Some(existing_stats) => {
            println!(
                "\n{}",
                "Found previous translation progress!"
                    .bright_yellow()
                    .bold()
            );
            println!("Previous progress:");
            println!("  - Total files translated: {}", existing_stats.total_files);
            println!("  - Files skipped: {}", existing_stats.skipped_files.len());
            if !existing_stats.translation_failed_files.is_empty() {
                println!(
                    "  - Translation failures: {}",
                    existing_stats.translation_failed_files.len()
                );
            }

            let choice = interaction::prompt_continue_or_restart()?;

            match choice {
                interaction::ContinueChoice::Continue => {
                    println!("{}", "✓ Continuing previous progress...".bright_green());
                    Ok(existing_stats)
                }
                interaction::ContinueChoice::Restart => {
                    println!(
                        "{}",
                        "✓ Starting fresh translation session...".bright_cyan()
                    );
                    util::TranslationStats::clear_stats_file(feature)?;
                    Ok(util::TranslationStats::new())
                }
            }
        }
        None => {
            println!("{}", "Starting new translation session...".bright_cyan());
            Ok(util::TranslationStats::new())
        }
    }
}

/// Steps 3 & 4: Scan for files to translate and initialize progress tracking
fn step_3_4_select_files_and_init_progress(
    feature: &str,
    _stats: &util::TranslationStats,
) -> Result<(std::path::PathBuf, util::ProgressState)> {
    println!(
        "\n{}",
        "Step 3: Scan Files to Translate".bright_cyan().bold()
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
    stats: &mut util::TranslationStats,
    skip_test: bool,
) -> Result<()> {
    println!(
        "\n{}",
        "Step 5: Execute Translation for All Files"
            .bright_cyan()
            .bold()
    );

    // Tracks how many translations have completed since the last test run.
    // Shared across all iterations of the main loop and the skipped-files loop so
    // that the interval is counted consistently across the entire session.
    let mut translations_since_last_test: usize = 0;

    loop {
        // Scan for empty .rs files, then exclude any that have already been skipped
        // by the user or that previously failed to translate.  Skipped files are
        // offered again via handle_skipped_files_loop; translation-failed files are
        // reported at the end as unrecoverable and not re-offered.
        // Completed files are already excluded because successfully translated files are
        // non-empty on disk (the existing file-content-based resume mechanism).
        let all_empty_rs_files = file_scanner::find_empty_rs_files(rust_dir)?;
        let excluded_set: std::collections::HashSet<&str> = stats
            .skipped_files
            .iter()
            .chain(stats.translation_failed_files.iter())
            .map(|s| s.as_str())
            .collect();
        let empty_rs_files: Vec<_> = all_empty_rs_files
            .into_iter()
            .filter(|p| {
                let rel = p
                    .strip_prefix(rust_dir)
                    .ok()
                    .and_then(|r| r.to_str())
                    .unwrap_or("");
                !excluded_set.contains(rel)
            })
            .collect();

        if empty_rs_files.is_empty() {
            if stats.skipped_files.is_empty() {
                print_completion_message();
            }
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
            rust_dir,
            progress_state,
            max_fix_attempts,
            show_full_output,
            stats,
            skip_test,
            &mut translations_since_last_test,
        )?;
    }

    // Handle skipped files after the main translation loop
    handle_skipped_files_loop(
        feature,
        rust_dir,
        progress_state,
        max_fix_attempts,
        show_full_output,
        stats,
        skip_test,
        &mut translations_since_last_test,
    )?;

    // If C2RUST_TEST_INTERVAL > 1 and the total translation count was not a
    // multiple of the interval, the last few translations never got a test run.
    // Run one final test here to make sure every completed translation is
    // covered by at least one test pass.
    run_final_interval_test_if_needed(feature, skip_test, translations_since_last_test)?;

    Ok(())
}

/// After the main translation loop, repeatedly offer to process any skipped files.
///
/// If the user selects "Process skipped files now", all currently-skipped files are
/// processed.  If any of those are skipped again the loop repeats, giving the user
/// another chance.  Selecting "Exit and process them later" (or having no skipped
/// files at all) breaks out of the loop.
fn handle_skipped_files_loop(
    feature: &str,
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    max_fix_attempts: usize,
    show_full_output: bool,
    stats: &mut util::TranslationStats,
    skip_test: bool,
    translations_since_last_test: &mut usize,
) -> Result<()> {
    loop {
        if stats.skipped_files.is_empty() {
            break;
        }

        let choice = interaction::prompt_skipped_files_choice(&stats.skipped_files)?;

        match choice {
            interaction::SkippedFilesChoice::ProcessNow => {
                // Drain the skipped list so we can iterate and re-populate it with any
                // files that get skipped again during this pass.
                let files_to_process = std::mem::take(&mut stats.skipped_files);
                let total = files_to_process.len();
                let mut iter = files_to_process.into_iter().enumerate();
                while let Some((idx, file_name)) = iter.next() {
                    let rs_file = rust_dir.join(&file_name);
                    let pos = idx + 1;
                    print_file_processing_header(pos, total, &file_name);

                    let (_, skip_interval_test) =
                        compute_interval_test_decision(*translations_since_last_test);

                    match process_rs_file(
                        feature,
                        &rs_file,
                        &file_name,
                        pos,
                        total,
                        max_fix_attempts,
                        show_full_output,
                        stats,
                        skip_test,
                        skip_interval_test,
                    ) {
                        Err(e) => {
                            if e.downcast_ref::<verification::SkipFileSignal>().is_some()
                                || e.downcast_ref::<verification::TranslationFailedSignal>().is_some()
                            {
                                // File was re-skipped or translation failed; already recorded
                                // by process_rs_file into the appropriate list.
                                save_stats_or_warn(stats, feature);
                                continue;
                            }
                            // On real error, re-add the current and all remaining files so they are not lost.
                            stats.record_file_skipped(file_name);
                            for (_, remaining_file) in iter {
                                stats.record_file_skipped(remaining_file);
                            }
                            save_stats_or_warn(stats, feature);
                            return Err(e);
                        }
                        Ok(tests_ran) => {
                            // Mark file as processed. mark_processed() is capped at total_count
                            // so it cannot overflow even if this is a resumed session.
                            progress_state.mark_processed();
                            update_interval_counter(translations_since_last_test, tests_ran);
                            save_stats_or_warn(stats, feature);
                            maybe_run_periodic_git_gc(progress_state);
                        }
                    }
                }
                // Loop again: if any files were skipped during this pass they are now
                // in stats.skipped_files and the user will be prompted again.
            }
            interaction::SkippedFilesChoice::ExitForLater => break,
        }
    }

    Ok(())
}

// ============================================================================
// Final Interval Test
// ============================================================================

/// Run a final test pass after the translation loop completes, if needed.
///
/// When `C2RUST_TEST_INTERVAL > 1` the per-translation test is deferred until
/// every N-th translation.  If the total number of translations is not a
/// multiple of the interval the last few translations are never tested by the
/// per-file test.  This function ensures those translations are covered by
/// running one extra clean/build/test cycle at the end of the session.
///
/// The final test is skipped when:
/// * `skip_test` is `true` (test configuration is unavailable), or
/// * `translations_since_last_test == 0` (all translations already had a test).
fn run_final_interval_test_if_needed(
    feature: &str,
    skip_test: bool,
    translations_since_last_test: usize,
) -> Result<()> {
    if skip_test || translations_since_last_test == 0 {
        return Ok(());
    }

    println!(
        "\n{}",
        format!(
            "Running final test pass ({} translation(s) untested due to C2RUST_TEST_INTERVAL)…",
            translations_since_last_test
        )
        .bright_cyan()
        .bold()
    );

    verify_hybrid_build_prerequisites()?;
    builder::c2rust_clean(feature)?;

    if let Err(build_error) = builder::c2rust_build(feature) {
        println!("{}", "✗ Final build failed".red().bold());
        return Err(build_error);
    }
    println!("{}", "✓ Final build successful".bright_green().bold());

    match builder::c2rust_test(feature) {
        Ok(_) => {
            println!(
                "{}",
                "✓ Final hybrid build tests passed".bright_green().bold()
            );
            analyzer::update_code_analysis_build_success(feature)?;
        }
        Err(test_error) => {
            if should_continue_on_test_error() {
                println!(
                    "{}",
                    format!(
                        "⚠ Final tests failed (continuing due to C2RUST_TEST_CONTINUE_ON_ERROR): {:#}",
                        test_error
                    )
                    .yellow()
                );
            } else {
                return Err(test_error);
            }
        }
    }

    // Commit any analysis changes produced by clean/build/test above. The commit
    // is non-fatal: if it fails a warning is printed, the working tree may remain
    // dirty, and subsequent analysis commits may include extra unintended changes.
    git_commit_or_warn(
        &format!("Update code analysis after final interval test (feature: {})", feature),
        feature,
    );

    Ok(())
}

// ============================================================================
// File Processing Functions
// ============================================================================

/// Attempt a git commit and print a warning if it fails instead of propagating the error.
///
/// Git commit failures are non-fatal: the translation workflow continues even if
/// the commit cannot be recorded (e.g., git is misconfigured or the repo is locked).
///
/// Returns `true` if a new commit was actually created, `false` if there was nothing
/// to commit (no-op, no warning printed) or if the commit failed (warning printed).
fn git_commit_or_warn(message: &str, feature: &str) -> bool {
    match git::git_commit(message, feature) {
        Err(e) => {
            eprintln!(
                "{}",
                format!("⚠ Warning: git commit failed (continuing): {}", e).yellow()
            );
            false
        }
        Ok(committed) => committed,
    }
}

/// Save translation stats and print a warning if saving fails
fn save_stats_or_warn(stats: &util::TranslationStats, feature: &str) {
    if let Err(e) = stats.save_to_file(feature) {
        eprintln!(
            "{}",
            format!("⚠ Warning: Failed to save translation stats: {}", e).yellow()
        );
    }
}

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
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    max_fix_attempts: usize,
    show_full_output: bool,
    stats: &mut util::TranslationStats,
    skip_test: bool,
    translations_since_last_test: &mut usize,
) -> Result<()> {
    for &idx in selected_indices.iter() {
        let rs_file = &empty_rs_files[idx];
        let current_position = progress_state.get_current_position();
        let total_count = progress_state.get_total_count();

        let file_name = rs_file
            .strip_prefix(rust_dir)
            .ok()
            .and_then(|p| p.to_str())
            .unwrap_or_else(|| {
                rs_file
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<unknown>")
            });

        print_file_processing_header(current_position, total_count, file_name);

        let (_, skip_interval_test) =
            compute_interval_test_decision(*translations_since_last_test);

        match process_rs_file(
            feature,
            rs_file,
            file_name,
            current_position,
            total_count,
            max_fix_attempts,
            show_full_output,
            stats,
            skip_test,
            skip_interval_test,
        ) {
            Err(e) => {
                if e.downcast_ref::<verification::SkipFileSignal>().is_none()
                    && e.downcast_ref::<verification::TranslationFailedSignal>().is_none()
                {
                    return Err(e);
                }
                // File was skipped (deliberate) or translation failed (non-fatal).
                // Already recorded in process_rs_file. Don't mark as processed.
                // Save stats immediately so the outcome is persisted.
                save_stats_or_warn(stats, feature);
            }
            Ok(tests_ran) => {
                // Mark file as processed. mark_processed() is capped at total_count.
                progress_state.mark_processed();
                update_interval_counter(translations_since_last_test, tests_ran);
                // Save stats immediately after successful completion.
                save_stats_or_warn(stats, feature);
                maybe_run_periodic_git_gc(progress_state);
            }
        }
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
/// * `Ok(tests_ran)` - File processed successfully; `true` when the test suite executed
///   for this translation (either automatically or via a Manual Fix), `false` otherwise
/// * `Err` - Processing failed after all retry attempts
fn process_rs_file(
    feature: &str,
    rs_file: &Path,
    file_name: &str,
    current_position: usize,
    total_count: usize,
    max_fix_attempts: usize,
    show_full_output: bool,
    stats: &mut util::TranslationStats,
    skip_test: bool,
    skip_interval_test: bool,
) -> Result<bool> {
    use util::MAX_TRANSLATION_ATTEMPTS;

    let mut total_fix_attempts = 0usize;
    let mut had_restart = false;

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

        // Translate C to Rust.
        // Only `TranslationScriptFailedError` (translate script non-zero exit) is treated
        // as a non-fatal translation failure.  All other errors (missing project root,
        // invalid feature name, cannot execute Python, empty output, …) are infrastructure
        // problems and propagate to the caller with `?`.
        if let Err(e) = translate_file(
            feature,
            file_type,
            rs_file,
            &format_progress,
            show_full_output,
        ) {
            if e.downcast_ref::<translator::TranslationScriptFailedError>().is_some() {
                println!(
                    "│ {}",
                    format!("⚠ Translation failed: {:#}", e).yellow()
                );
                println!(
                    "│ {}",
                    format!("Skipping file due to translation failure: {}", file_name)
                        .bright_yellow()
                );
                stats.record_file_translation_failed(file_name.to_string());
                return Err(verification::TranslationFailedSignal.into());
            } else {
                return Err(e);
            }
        }

        // Phase 1: Build and fix errors (warnings suppressed via RUSTFLAGS="-A warnings")
        println!("│");
        println!(
            "│ {}",
            "Phase 1: Building and fixing errors..."
                .bright_blue()
                .bold()
        );
        let build_loop_result = verification::execute_code_error_check_with_fix_loop(
            feature,
            file_type,
            rs_file,
            file_name,
            &format_progress,
            is_last_attempt,
            attempt_number,
            max_fix_attempts,
            show_full_output,
            skip_test,
        );

        // Check if the user chose to skip this file
        if let Err(ref e) = build_loop_result {
            if e.downcast_ref::<verification::SkipFileSignal>().is_some() {
                println!(
                    "│ {}",
                    format!("Skipping file: {}", file_name).bright_yellow()
                );
                stats.record_file_skipped(file_name.to_string());
                return Err(verification::SkipFileSignal.into());
            }
        }

        let (build_successful, fix_attempts, did_restart) = build_loop_result?;

        // These counters are cumulative across all translation attempts for this file.
        // For example, if attempt 1 uses 5 fix attempts and attempt 2 uses 3, the recorded
        // total_fix_attempts will be 8, and had_restart will be true if any attempt restarted.
        total_fix_attempts += fix_attempts;
        had_restart |= did_restart;

        if build_successful {
            // Phase 2: Fix warnings after all errors are resolved
            // (skipped when C2RUST_PROCESS_WARNINGS=0 or =false)
            if should_process_warnings() {
                println!("│");
                println!(
                    "│ {}",
                    "Phase 2: Checking and fixing warnings..."
                        .bright_blue()
                        .bold()
                );
                let warning_fix_attempts = verification::execute_code_warning_check_with_fix_loop(
                    feature,
                    file_type,
                    rs_file,
                    file_name,
                    &format_progress,
                    max_fix_attempts,
                    show_full_output,
                )
                .unwrap_or_else(|e| {
                    println!(
                        "│ {}",
                        format!("⚠ Warning phase encountered an error: {}", e).yellow()
                    );
                    0
                });
                total_fix_attempts += warning_fix_attempts;
            } else {
                println!("│");
                println!(
                    "│ {}",
                    "Phase 2: Warning processing skipped (C2RUST_PROCESS_WARNINGS=0/false)."
                        .bright_yellow()
                );
            }

            let (processing_complete, tests_ran) =
                complete_file_processing(feature, file_name, file_type, rs_file, &format_progress, skip_test, skip_interval_test)?;
            if processing_complete {
                stats.record_file_completion(
                    file_name.to_string(),
                    attempt_number,
                    had_restart,
                    total_fix_attempts,
                );
                return Ok(tests_ran);
            }
            // If not complete, retry translation (loop continues)
        }
    }

    anyhow::bail!("Unexpected: all retry attempts completed without resolution")
}

// ============================================================================
// Environment-Variable Helpers
// ============================================================================

/// Returns `true` when warning processing is enabled (the default).
///
/// Set `C2RUST_PROCESS_WARNINGS=0` (or `false`) to skip Phase 2 (warning
/// detection and auto-fix) for every file processed in a run.
pub(crate) fn should_process_warnings() -> bool {
    match std::env::var("C2RUST_PROCESS_WARNINGS") {
        Ok(val) => {
            let val = val.trim();
            val != "0" && !val.eq_ignore_ascii_case("false")
        }
        Err(_) => true,
    }
}

/// Returns `true` when test failures should not interrupt the workflow.
///
/// Set `C2RUST_TEST_CONTINUE_ON_ERROR=1` (or `true`/`yes`) to treat
/// `c2rust_test` failures as non-fatal warnings, allowing subsequent tasks to
/// continue running instead of aborting.  By default (env var absent or set to
/// any other value) a test failure is fatal.
pub(crate) fn should_continue_on_test_error() -> bool {
    match std::env::var("C2RUST_TEST_CONTINUE_ON_ERROR") {
        Ok(val) => {
            let val = val.trim();
            val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes")
        }
        Err(_) => false,
    }
}

/// Returns `true` when the process should automatically retry translation upon reaching the
/// maximum number of fix attempts, without prompting the user.
///
/// Set `C2RUST_AUTO_RETRY_ON_MAX_FIX=1` (or `true`/`yes`) to automatically choose
/// "Retry directly" when fix attempts are exhausted, ensuring fully unattended runs.
/// When on the last translation attempt (no more retries available), the file is
/// automatically skipped instead so the overall process can continue.
/// By default (env var absent or set to any other value) the interactive prompt is shown.
pub(crate) fn should_auto_retry_on_max_fix_attempts() -> bool {
    match std::env::var("C2RUST_AUTO_RETRY_ON_MAX_FIX") {
        Ok(val) => {
            let val = val.trim();
            val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes")
        }
        Err(_) => false,
    }
}

/// Returns the test interval: run hybrid build tests once every N successful translations.
///
/// Set `C2RUST_TEST_INTERVAL=N` (a positive integer) to run tests only after every N-th
/// completed translation instead of after every single translation.  The default is `1`
/// (run tests after every translation), which preserves the existing behaviour.
///
/// Invalid values (zero, non-numeric, or empty) fall back to the default of `1`.
pub(crate) fn get_test_interval() -> usize {
    match std::env::var("C2RUST_TEST_INTERVAL") {
        Ok(val) => match val.trim().parse::<usize>() {
            Ok(n) if n > 0 => n,
            _ => 1,
        },
        Err(_) => 1,
    }
}

/// Determine whether to skip the test phase for the next translation based on the
/// current interval counter.
///
/// Returns `(should_run_test, skip_interval_test)`:
/// - `should_run_test` is `true` when the interval is reached (test should execute).
/// - `skip_interval_test` is the inverse of `should_run_test`.
fn compute_interval_test_decision(translations_since_last_test: usize) -> (bool, bool) {
    let interval = get_test_interval();
    let proposed_count = translations_since_last_test.saturating_add(1);
    let should_run_test = proposed_count % interval == 0;
    (should_run_test, !should_run_test)
}

/// Update the interval counter after a successful translation.
///
/// - Resets the counter to `0` when tests actually ran (`tests_ran == true`).
/// - Increments the counter by 1 (with saturation) when tests were not run.
fn update_interval_counter(translations_since_last_test: &mut usize, tests_ran: bool) {
    if tests_ran {
        *translations_since_last_test = 0;
    } else {
        *translations_since_last_test = translations_since_last_test.saturating_add(1);
    }
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
                retry_number,
                max_retries,
                rs_file.display()
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

/// Apply warning fix to translated file
///
/// Similar to `apply_error_fix` but used during Phase 2 (warning fixing).
/// The build has not failed -- warnings were surfaced by running without `-A warnings`.
pub(crate) fn apply_warning_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    warning_msg: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    println!(
        "│ {}",
        "⚠ Warnings detected, attempting to fix...".yellow().bold()
    );
    println!("│");
    println!(
        "│ {}",
        format_progress("Warning Fix").bright_magenta().bold()
    );

    // Fix using the same translation tool, passing warnings as the "error" message
    translator::fix_translation_error(
        feature,
        file_type,
        rs_file,
        &warning_msg.to_string(),
        show_full_output,
        true,
    )?;

    // Verify fix produced output
    let metadata = std::fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Warning fix failed: output file is empty");
    }

    println!("│ {}", "✓ Warning fix applied".bright_green());

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
/// * `Ok((true, tests_ran))` - File processing completed successfully; `tests_ran` is `true`
///   when the test suite executed for this translation (either automatically or via ManualFix).
/// * `Ok((false, false))` - Translation should be retried from scratch
/// * `Err` - Unrecoverable error occurred
fn complete_file_processing<F>(
    feature: &str,
    file_name: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
    skip_test: bool,
    skip_interval_test: bool,
) -> Result<(bool, bool)>
where
    F: Fn(&str) -> String,
{
    println!("│");
    // Choose the progress header. `skip_test` (config unavailable) takes priority over
    // `skip_interval_test` so the user sees the correct reason when both flags are true.
    if skip_test {
        println!(
            "│ {}",
            format_progress("Hybrid Build (tests skipped — config unavailable)")
                .bright_magenta()
                .bold()
        );
        println!(
            "│ {}",
            "Running clean/build only (test configuration not available)...".bright_blue()
        );
    } else if skip_interval_test {
        let interval = get_test_interval();
        println!(
            "│ {}",
            format_progress(&format!(
                "Hybrid Build (tests deferred by C2RUST_TEST_INTERVAL={})",
                interval
            ))
            .bright_magenta()
            .bold()
        );
        println!(
            "│ {}",
            format!(
                "Running clean/build only (tests deferred: every {} translations)...",
                interval
            )
            .bright_blue()
        );
    } else {
        println!(
            "│ {}",
            format_progress("Hybrid Build Tests")
                .bright_magenta()
                .bold()
        );
        println!("│ {}", "Running hybrid build tests...".bright_blue());
    }

    // Pre-check: Verify config and tools are available
    verify_hybrid_build_prerequisites()?;

    // Run hybrid build clean/build/test
    builder::c2rust_clean(feature)?;

    // Handle build
    if let Err(build_error) = builder::c2rust_build(feature) {
        println!("│ {}", "✗ Build failed".red().bold());
        let processing_complete =
            builder::handle_build_failure_interactive(feature, file_type, rs_file, build_error, skip_test)?;
        if !processing_complete {
            return Ok((false, false)); // Retry translation; tests did not run
        }
        // handle_build_failure_interactive succeeded: it called run_full_build_and_test_interactive
        // internally, which runs tests when !skip_test.  Return early so we don't fall into the
        // skip_interval_test branch and incorrectly report tests_ran=false.
        return Ok((true, !skip_test));
    } else {
        println!("│ {}", "✓ Build successful".bright_green().bold());
    }

    if skip_test {
        println!(
            "│ {}",
            "⚠ Skipping test phase (test configuration not available)".yellow()
        );
        let tests_ran = handle_successful_tests(feature, file_name, file_type, rs_file, format_progress, TestStatus::SkippedNoConfig)?;
        return Ok((true, tests_ran));
    }

    if skip_interval_test {
        let interval = get_test_interval();
        println!(
            "│ {}",
            format!(
                "⚠ Test phase deferred (C2RUST_TEST_INTERVAL={}: test runs every {} translations)",
                interval, interval
            )
            .yellow()
        );
        let tests_ran = handle_successful_tests(feature, file_name, file_type, rs_file, format_progress, TestStatus::DeferredByInterval)?;
        return Ok((true, tests_ran));
    }

    // Handle test
    match builder::c2rust_test(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());
            let tests_ran = handle_successful_tests(feature, file_name, file_type, rs_file, format_progress, TestStatus::Passed)?;
            Ok((true, tests_ran)) // Processing complete; tests ran
        }
        Err(test_error) => {
            if should_continue_on_test_error() {
                println!(
                    "│ {}",
                    format!(
                        "⚠ Tests failed (continuing due to C2RUST_TEST_CONTINUE_ON_ERROR): {:#}",
                        test_error
                    )
                    .yellow()
                );
                // tests_passed=false: tests ran but failed; we're only accepting because
                // C2RUST_TEST_CONTINUE_ON_ERROR is set — this must not emit --build-success.
                finalize_file_processing(feature, file_name, format_progress, false)?;
                // C2RUST_TEST_CONTINUE_ON_ERROR was set: tests ran (and failed) but we're
                // treating the failure as non-fatal and accepting the translation anyway.
                Ok((true, true))
            } else {
                let processing_complete = builder::handle_test_failure_interactive(
                    feature, file_type, rs_file, test_error, skip_test,
                )?;
                // Tests ran for this translation (they failed). If the user is retrying
                // (processing_complete=false), tests_ran doesn't influence the counter
                // (the caller won't update it on retry). If accepted (processing_complete=true),
                // reset the counter since tests did run—even though they failed.
                let tests_ran = processing_complete;
                Ok((processing_complete, tests_ran))
            }
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

/// Describes the outcome of the hybrid test phase, used to drive display and
/// user-interaction in `handle_successful_tests`.
enum TestStatus {
    /// Tests ran and passed.
    Passed,
    /// Tests were skipped because the test configuration / tooling is not available.
    SkippedNoConfig,
    /// Build succeeded but tests were deferred by `C2RUST_TEST_INTERVAL`.
    /// The user's Manual Fix choice may still run real tests.
    DeferredByInterval,
}

/// Handle successful test completion with user interaction.
///
/// Returns `Ok(true)` when tests actually ran for this translation (either
/// automatically or via a Manual Fix), and `Ok(false)` when they were skipped
/// or deferred without being run. Callers use this value to decide whether to
/// reset the `translations_since_last_test` interval counter.
fn handle_successful_tests<F>(
    feature: &str,
    file_name: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
    test_status: TestStatus,
) -> Result<bool>
where
    F: Fn(&str) -> String,
{
    // If in auto-accept mode, skip interaction
    if interaction::is_auto_accept_mode() {
        println!(
            "│ {}",
            "Auto-accept mode: automatically accepting translation".bright_green()
        );
        finalize_file_processing(
            feature,
            file_name,
            format_progress,
            matches!(test_status, TestStatus::Passed),
        )?;
        // In auto-accept mode we skip user interaction. Tests are considered to have
        // run only when the status is `Passed` (c2rust_test executed before this call).
        // `SkippedNoConfig` and `DeferredByInterval` both indicate tests did not run.
        return Ok(matches!(test_status, TestStatus::Passed));
    }

    // Show code comparison and get user choice
    let c_file = rs_file.with_extension("c");
    interaction::display_file_paths(Some(&c_file), rs_file);

    match test_status {
        TestStatus::SkippedNoConfig => {
            // Tests were skipped because the test config/tool is unavailable.
            println!(
                "│ {}",
                "Hybrid build completed with tests SKIPPED (results are not validated by tests)."
                    .yellow()
                    .bold()
            );
            if let Err(e) = diff_display::display_code_comparison(
                &c_file,
                rs_file,
                "⚠ Tests skipped (test configuration not available)",
                diff_display::ResultType::BuildFail,
            ) {
                println!(
                    "│ {}",
                    format!("Failed to display comparison: {}", e).yellow()
                );
            }

            let choice = interaction::prompt_build_success_tests_skipped_choice()?;

            match choice {
                interaction::CompileSuccessChoice::Accept => {
                    println!("│ {}", "You chose: Accept this code".bright_cyan());
                    finalize_file_processing(feature, file_name, format_progress, false)?;
                }
                interaction::CompileSuccessChoice::AutoAccept => {
                    println!(
                        "│ {}",
                        "You chose: Auto-accept all subsequent translations".bright_cyan()
                    );
                    interaction::enable_auto_accept_mode();
                    finalize_file_processing(feature, file_name, format_progress, false)?;
                }
                interaction::CompileSuccessChoice::ManualFix => {
                    println!("│ {}", "You chose: Manual fix".bright_cyan());
                    interaction::open_in_vim(rs_file)?;
                    println!(
                        "│ {}",
                        "Running full build after manual changes...".bright_blue()
                    );
                    // Tests remain skipped: config is still unavailable.
                    builder::run_full_build_and_test_interactive(feature, file_type, rs_file, true)?;
                    println!(
                        "│ {}",
                        "✓ Build passes after manual changes (tests skipped)".bright_green()
                    );
                    finalize_file_processing(feature, file_name, format_progress, false)?;
                }
                interaction::CompileSuccessChoice::Exit => {
                    println!("│ {}", "You chose: Exit".yellow());
                    anyhow::bail!("User chose to exit after successful build (tests skipped)");
                }
            }
            // Tests were never run in any choice of this path.
            Ok(false)
        }
        TestStatus::DeferredByInterval => {
            // Build passed but tests were deferred by C2RUST_TEST_INTERVAL.
            println!(
                "│ {}",
                "Hybrid build completed — tests deferred by interval (results not yet validated by tests)."
                    .yellow()
                    .bold()
            );
            if let Err(e) = diff_display::display_code_comparison(
                &c_file,
                rs_file,
                "⚠ Tests deferred by C2RUST_TEST_INTERVAL",
                diff_display::ResultType::BuildFail,
            ) {
                println!(
                    "│ {}",
                    format!("Failed to display comparison: {}", e).yellow()
                );
            }

            let choice = interaction::prompt_build_success_tests_deferred_choice()?;

            // Track whether tests actually ran (only true for ManualFix, which runs
            // run_full_build_and_test_interactive with skip_test=false).
            let tests_ran = match choice {
                interaction::CompileSuccessChoice::Accept => {
                    println!("│ {}", "You chose: Accept this code".bright_cyan());
                    finalize_file_processing(feature, file_name, format_progress, false)?;
                    false
                }
                interaction::CompileSuccessChoice::AutoAccept => {
                    println!(
                        "│ {}",
                        "You chose: Auto-accept all subsequent translations".bright_cyan()
                    );
                    interaction::enable_auto_accept_mode();
                    finalize_file_processing(feature, file_name, format_progress, false)?;
                    false
                }
                interaction::CompileSuccessChoice::ManualFix => {
                    println!("│ {}", "You chose: Manual fix".bright_cyan());
                    interaction::open_in_vim(rs_file)?;
                    println!(
                        "│ {}",
                        "Running full build and test after manual changes...".bright_blue()
                    );
                    // Config is available: run real tests during the manual-fix validation.
                    builder::run_full_build_and_test_interactive(feature, file_type, rs_file, false)?;
                    println!(
                        "│ {}",
                        "✓ All builds and tests pass after manual changes".bright_green()
                    );
                    finalize_file_processing(feature, file_name, format_progress, true)?;
                    true // tests actually ran
                }
                interaction::CompileSuccessChoice::Exit => {
                    println!("│ {}", "You chose: Exit".yellow());
                    anyhow::bail!(
                        "User chose to exit after successful build (tests deferred by interval)"
                    );
                }
            };
            Ok(tests_ran)
        }
        TestStatus::Passed => {
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
                    finalize_file_processing(feature, file_name, format_progress, true)?;
                }
                interaction::CompileSuccessChoice::AutoAccept => {
                    println!(
                        "│ {}",
                        "You chose: Auto-accept all subsequent translations".bright_cyan()
                    );
                    interaction::enable_auto_accept_mode();
                    finalize_file_processing(feature, file_name, format_progress, true)?;
                }
                interaction::CompileSuccessChoice::ManualFix => {
                    println!("│ {}", "You chose: Manual fix".bright_cyan());
                    interaction::open_in_vim(rs_file)?;
                    println!(
                        "│ {}",
                        "Running full build and test after manual changes...".bright_blue()
                    );
                    builder::run_full_build_and_test_interactive(feature, file_type, rs_file, false)?;
                    println!(
                        "│ {}",
                        "✓ All builds and tests pass after manual changes".bright_green()
                    );
                    finalize_file_processing(feature, file_name, format_progress, true)?;
                }
                interaction::CompileSuccessChoice::Exit => {
                    println!("│ {}", "You chose: Exit".yellow());
                    anyhow::bail!("User chose to exit after successful tests");
                }
            }
            // Tests ran (either automatically or via ManualFix, which also runs tests).
            Ok(true)
        }
    }
}

/// Finalize file processing: commit changes and update code analysis.
///
/// `tests_passed` must be `true` only when tests actually ran **and** passed for
/// this translation — it causes `--build-success` to be forwarded to `code_analyse`
/// so it can distinguish a verified translation from a skipped/deferred/failed one.
///
/// Pass `false` when:
/// - tests were skipped because the test configuration was unavailable (`SkippedNoConfig`)
/// - tests were deferred by `C2RUST_TEST_INTERVAL` and no manual re-run was triggered (`DeferredByInterval`)
/// - tests ran but failed and the caller is continuing due to `C2RUST_TEST_CONTINUE_ON_ERROR`
fn finalize_file_processing<F>(
    feature: &str,
    file_name: &str,
    format_progress: &F,
    tests_passed: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    // Commit changes
    println!("│");
    println!("│ {}", format_progress("Commit").bright_magenta().bold());
    println!("│ {}", "Committing changes...".bright_blue());
    if git_commit_or_warn(
        &format!(
            "Translate {} from C to Rust (feature: {})",
            file_name, feature
        ),
        feature,
    ) {
        println!("│ {}", "✓ Changes committed".bright_green());
    }

    // Update code analysis
    println!("│");
    println!(
        "│ {}",
        format_progress("Update Analysis").bright_magenta().bold()
    );
    println!("│ {}", "Updating code analysis...".bright_blue());
    if tests_passed {
        analyzer::update_code_analysis_build_success(feature)?;
    } else {
        analyzer::update_code_analysis(feature)?;
    }
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // Commit analysis
    println!("│");
    println!(
        "│ {}",
        format_progress("Commit Analysis").bright_magenta().bold()
    );
    git_commit_or_warn(&format!("Update code analysis for {}", feature), feature);

    println!("{}", "└─ File processing complete".bright_white().bold());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        prior: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, prior }
        }
        fn remove(key: &'static str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, prior }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prior {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_default() {
        let _guard = EnvGuard::remove("C2RUST_PROCESS_WARNINGS");
        assert!(should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_disabled_with_zero() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "0");
        assert!(!should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_disabled_with_false() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "false");
        assert!(!should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_disabled_with_false_uppercase() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "FALSE");
        assert!(!should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_enabled_with_one() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "1");
        assert!(should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_enabled_with_true() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "true");
        assert!(should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_default() {
        let _guard = EnvGuard::remove("C2RUST_TEST_CONTINUE_ON_ERROR");
        assert!(!should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_one() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "1");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_true() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "true");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_true_uppercase() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "TRUE");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_yes() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "yes");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_yes_uppercase() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "YES");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_disabled_with_zero() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "0");
        assert!(!should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_disabled_with_false() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "false");
        assert!(!should_continue_on_test_error());
    }

    // ========================================================================
    // should_auto_retry_on_max_fix_attempts Tests
    // ========================================================================

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_default() {
        let _guard = EnvGuard::remove("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        assert!(!should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_one() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "1");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_true() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "true");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_true_uppercase() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "TRUE");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_yes() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "yes");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_yes_uppercase() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "YES");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_disabled_with_zero() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "0");
        assert!(!should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_disabled_with_false() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "false");
        assert!(!should_auto_retry_on_max_fix_attempts());
    }

    // ========================================================================
    // get_test_interval Tests
    // ========================================================================

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_default() {
        let _guard = EnvGuard::remove("C2RUST_TEST_INTERVAL");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_explicit_one() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "1");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_five() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "5");
        assert_eq!(get_test_interval(), 5);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_large_value() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "100");
        assert_eq!(get_test_interval(), 100);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_zero_falls_back_to_default() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "0");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_invalid_falls_back_to_default() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "abc");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_empty_falls_back_to_default() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_whitespace_trimmed() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "  3  ");
        assert_eq!(get_test_interval(), 3);
    }

    // ========================================================================
    // compute_interval_test_decision Tests
    // ========================================================================

    #[test]
    #[serial_test::serial]
    fn test_compute_interval_decision_interval_1_always_runs() {
        // Interval=1: every translation should run the test.
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "1");
        for counter in 0..5 {
            let (should_run, skip) = compute_interval_test_decision(counter);
            assert!(should_run, "counter={}: expected test to run", counter);
            assert!(!skip, "counter={}: expected skip=false", counter);
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_compute_interval_decision_interval_3() {
        // Interval=3: test runs when proposed_count (counter+1) is a multiple of 3.
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "3");
        // counter=0 → proposed=1 → 1%3≠0 → skip
        let (should_run, skip) = compute_interval_test_decision(0);
        assert!(!should_run);
        assert!(skip);
        // counter=1 → proposed=2 → 2%3≠0 → skip
        let (should_run, skip) = compute_interval_test_decision(1);
        assert!(!should_run);
        assert!(skip);
        // counter=2 → proposed=3 → 3%3=0 → run
        let (should_run, skip) = compute_interval_test_decision(2);
        assert!(should_run);
        assert!(!skip);
        // counter=3 → proposed=4 → 4%3≠0 → skip (next cycle starts)
        let (should_run, skip) = compute_interval_test_decision(3);
        assert!(!should_run);
        assert!(skip);
        // counter=5 → proposed=6 → 6%3=0 → run
        let (should_run, skip) = compute_interval_test_decision(5);
        assert!(should_run);
        assert!(!skip);
    }

    #[test]
    #[serial_test::serial]
    fn test_compute_interval_decision_returns_inverse_pair() {
        // should_run and skip must always be exact inverses.
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "4");
        for counter in 0..12 {
            let (should_run, skip) = compute_interval_test_decision(counter);
            assert_eq!(should_run, !skip, "counter={}: should_run and skip are not inverses", counter);
        }
    }

    // ========================================================================
    // update_interval_counter Tests
    // ========================================================================

    #[test]
    fn test_update_interval_counter_resets_when_test_ran() {
        let mut counter = 4usize;
        update_interval_counter(&mut counter, true);
        assert_eq!(counter, 0);
    }

    #[test]
    fn test_update_interval_counter_increments_when_test_skipped() {
        let mut counter = 2usize;
        update_interval_counter(&mut counter, false);
        assert_eq!(counter, 3);
    }

    #[test]
    fn test_update_interval_counter_increments_from_zero() {
        let mut counter = 0usize;
        update_interval_counter(&mut counter, false);
        assert_eq!(counter, 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_full_interval_cycle_counter_behaviour() {
        // Simulates 4 successive translations with interval=3 and checks the full
        // sequence of decisions and counter updates.
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "3");
        let mut counter = 0usize;

        // Translation 1: proposed=1 → skip
        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(!should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 1);

        // Translation 2: proposed=2 → skip
        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(!should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 2);

        // Translation 3: proposed=3 → run → reset
        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 0);

        // Translation 4: proposed=1 → skip (new cycle)
        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(!should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 1);
    }

    // run_final_interval_test_if_needed logic tests
    // (We test the guard conditions; the actual builder calls require integration infra.)

    #[test]
    fn test_final_interval_test_skipped_when_skip_test_true() {
        // When skip_test=true the function should return Ok(()) immediately
        // regardless of the pending-translation count.
        let result = run_final_interval_test_if_needed("dummy_feature", true, 3);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_interval_test_skipped_when_no_pending_translations() {
        // When translations_since_last_test=0 (all translations already tested)
        // the function should return Ok(()) immediately.
        let result = run_final_interval_test_if_needed("dummy_feature", false, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_interval_test_skipped_when_both_skip_and_no_pending() {
        // Both guard conditions true: still Ok(()).
        let result = run_final_interval_test_if_needed("dummy_feature", true, 0);
        assert!(result.is_ok());
    }
}

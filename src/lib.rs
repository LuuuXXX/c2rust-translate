//! C to Rust translation workflow orchestration
//!
//! This module provides the main translation workflow that coordinates initialization,
//! gate verification, file selection, and translation execution across multiple modules.

// Public modules - external API
pub mod builder;
pub mod pipeline;
pub mod file_scanner;
pub mod git;
pub mod setup;
pub mod translator;
pub mod util;
pub mod verification;

// Internal modules - implementation details
pub(crate) mod code_rewrite;
pub(crate) mod config;
pub(crate) mod diff_display;
pub(crate) mod error_handler;
pub(crate) mod file_processor;
pub(crate) mod interaction;
pub(crate) mod suggestion;
pub(crate) mod workflow;

use anyhow::Result;
use colored::Colorize;

pub fn translate_feature(
    feature: &str,
    allow_all: bool,
    target_file: Option<&str>,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
    show_full_output: bool,
) -> Result<()> {
    workflow::print_workflow_header(feature);

    // Step 1: Initialize feature directory
    workflow::step_1_initialize(feature)?;

    // Capture whether the dedicated `.c2rust` repo was already dirty before any
    // verification side effects run. Resume snapshotting should reflect unfinished
    // user progress, not fresh analysis/build artifacts produced by step 2.
    let preexisting_resume_snapshot_needed = match git::git_has_uncommitted_changes() {
        Ok(is_dirty) => is_dirty,
        Err(e) => {
            eprintln!(
                "{}",
                format!(
                    "⚠ Warning: failed to inspect .c2rust working tree before verification: {}",
                    e
                )
                .yellow()
            );
            false
        }
    };

    // Check test configuration before initial verification (step 2 also uses test-related commands)
    let skip_test = workflow::check_test_configuration(feature)?;

    // Step 2: Run initial verification
    workflow::step_2_initial_verification(feature, show_full_output, skip_test)?;

    // Step 2.5: Check and load previous translation stats
    let mut stats = workflow::step_2_5_load_or_create_stats(
        feature,
        preexisting_resume_snapshot_needed,
        max_error_fix_attempts,
        max_warning_fix_attempts,
        show_full_output,
        skip_test,
    )?;

    if let Some(target_file) = target_file {
        workflow::prepare_target_file_rerun(feature, target_file, &mut stats)?;
    }

    // Step 3 & 4: Select files and initialize progress
    let (rust_dir, mut progress_state) =
        workflow::step_3_4_select_files_and_init_progress(feature, &stats, target_file)?;

    // Step 5: Execute translation loop
    let step5_result = workflow::step_5_execute_translation_loop(
        feature,
        &rust_dir,
        &mut progress_state,
        allow_all,
        target_file,
        max_error_fix_attempts,
        max_warning_fix_attempts,
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

/// Run feature initialization and project-level verification without entering
/// the translation loop.
pub fn verify_feature(feature: &str, show_full_output: bool) -> Result<()> {
    workflow::print_workflow_header(feature);

    workflow::step_1_initialize(feature)?;
    let skip_test = workflow::check_test_configuration(feature)?;
    workflow::step_2_initial_verification(feature, show_full_output, skip_test)?;

    Ok(())
}

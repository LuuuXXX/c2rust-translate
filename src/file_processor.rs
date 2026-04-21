use crate::{builder, code_rewrite, config, diff_display, file_scanner, interaction, translator, util, verification};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranslationInputMode {
    TranslateFromC,
    ReuseExistingRust,
}


pub(crate) fn process_rs_file(
    feature: &str,
    rs_file: &Path,
    file_name: &str,
    current_position: usize,
    total_count: usize,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
    show_full_output: bool,
    stats: &mut util::TranslationStats,
    skip_test: bool,
    skip_interval_test: bool,
    translation_mode: TranslationInputMode,
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
        if let Err(e) = run_translation_phase(
            feature,
            file_type,
            rs_file,
            &format_progress,
            show_full_output,
            translation_mode,
        ) {
            if e.downcast_ref::<translator::TranslationScriptFailedError>().is_some() {
                println!(
                    "│ {}",
                    format!("⚠ Translation failed: {:#}", e).yellow()
                );
                crate::workflow::stash_skipped_file_for_later(feature, file_name, rs_file)?;
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
            max_error_fix_attempts,
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
                crate::workflow::stash_skipped_file_for_later(feature, file_name, rs_file)?;
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
            // (skipped when C2RUST_PROCESS_WARNINGS=0/false or max_warning_fix_attempts=0)
            if config::should_process_warnings() && max_warning_fix_attempts > 0 {
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
                    max_warning_fix_attempts,
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
                let reason = if !config::should_process_warnings() {
                    "C2RUST_PROCESS_WARNINGS=0/false"
                } else {
                    "max_warning_fix_attempts=0"
                };
                println!(
                    "│ {}",
                    format!("Phase 2: Warning processing skipped ({}).", reason)
                        .bright_yellow()
                );
            }

            let (processing_complete, tests_ran) = match complete_file_processing(
                feature,
                file_name,
                file_type,
                rs_file,
                &format_progress,
                skip_test,
                skip_interval_test,
            ) {
                Ok(result) => result,
                Err(e) => {
                    if e.downcast_ref::<verification::SkipFileSignal>().is_some() {
                        crate::workflow::stash_skipped_file_for_later(feature, file_name, rs_file)?;
                        stats.record_file_skipped(file_name.to_string());
                        return Err(verification::SkipFileSignal.into());
                    }
                    return Err(e);
                }
            };
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

/// Revert a skipped/failed translation back to an empty placeholder file.
///
/// The workflow uses empty `fun_*.rs` / `var_*.rs` files to represent pending
/// work, but `cargo check` still compiles any non-empty file in the workspace.
/// If a skipped file keeps broken contents, the next file's verification phase
/// reports the stale error again. Truncating the file restores the placeholder
/// state so later files can proceed independently.
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
fn run_translation_phase<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
    show_full_output: bool,
    translation_mode: TranslationInputMode,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    match translation_mode {
        TranslationInputMode::TranslateFromC => {
            translate_file(feature, file_type, rs_file, format_progress, show_full_output)
        }
        TranslationInputMode::ReuseExistingRust => {
            println!("│");
            println!(
                "│ {}",
                format_progress("Reuse Existing Rust").bright_magenta().bold()
            );
            println!(
                "│ {}",
                "Reusing previously stashed Rust output for skipped-file recovery."
                    .bright_blue()
                    .bold()
            );

            let metadata = std::fs::metadata(rs_file).with_context(|| {
                format!(
                    "Skipped-file recovery expected existing Rust output, but file metadata could not be read: {}",
                    rs_file.display()
                )
            })?;
            if metadata.len() == 0 {
                anyhow::bail!(
                    "Skipped-file recovery expected existing Rust output, but the file is empty: {}",
                    rs_file.display()
                );
            }

            translator::display_code(
                rs_file,
                "─ Existing Rust Code Preview ─",
                util::CODE_PREVIEW_LINES,
                show_full_output,
            );
            println!(
                "│ {}",
                format!("✓ Reused existing Rust output ({} bytes)", metadata.len())
                    .bright_green()
            );
            Ok(())
        }
    }
}

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

    code_rewrite::try_collapse_exported_function_unsafe_regions(rs_file)?;
    code_rewrite::try_normalize_c_char_literal_ptrs(rs_file)?;

    println!(
        "│ {}",
        format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green()
    );

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
        let interval = config::get_test_interval();
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

    // 在整个序列开始前统一更新一次代码分析，避免 clean/build/test 各自重复更新
    println!("│ {}", "Updating code analysis...".bright_blue());
    builder::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // Run hybrid build clean/build/test
    builder::c2rust_clean_no_analysis(feature)?;

    // Handle build
    if let Err(build_error) = builder::c2rust_build_no_analysis(feature) {
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
        let interval = config::get_test_interval();
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
    match builder::c2rust_test_no_analysis(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());
            let tests_ran = handle_successful_tests(feature, file_name, file_type, rs_file, format_progress, TestStatus::Passed)?;
            Ok((true, tests_ran)) // Processing complete; tests ran
        }
        Err(test_error) => {
            if config::should_continue_on_test_error() {
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
pub(crate) fn verify_hybrid_build_prerequisites() -> Result<()> {
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
    if crate::workflow::git_commit_or_warn(
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
        builder::update_code_analysis_build_success(feature)?;
    } else {
        builder::update_code_analysis(feature)?;
    }
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // Commit analysis
    println!("│");
    println!(
        "│ {}",
        format_progress("Commit Analysis").bright_magenta().bold()
    );
    crate::workflow::git_commit_or_warn(&format!("Update code analysis for {}", feature), feature);

    println!("{}", "└─ File processing complete".bright_white().bold());

    Ok(())
}


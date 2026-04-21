use crate::{builder, config, file_processor, file_scanner, git, interaction, setup, util, verification};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

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
/// * `max_error_fix_attempts` - Maximum number of build-error fix attempts per file
/// * `max_warning_fix_attempts` - Maximum number of warning-fix attempts per file
/// * `show_full_output` - If true, show complete code/error output without truncation
///
/// # Returns
/// * `Ok(())` - All translations completed successfully
/// * `Err` - Translation failed or user aborted

pub(crate) fn print_workflow_header(feature: &str) {
    let msg = format!("Starting translation for feature: {}", feature);
    println!("{}", msg.bright_cyan().bold());
}

/// Step 1: Find project root and initialize feature directory
pub(crate) fn step_1_initialize(feature: &str) -> Result<()> {
    println!(
        "\n{}",
        "Step 1: Find Project Root and Initialize"
            .bright_cyan()
            .bold()
    );
    setup::check_and_initialize_feature(feature)
}

/// Step 2: Run initial verification
pub(crate) fn step_2_initial_verification(feature: &str, show_full_output: bool, skip_test: bool) -> Result<()> {
    setup::execute_initial_verification(feature, show_full_output, skip_test)
}

/// Check test configuration in `.c2rust/config.toml`.
///
/// Returns `Ok(false)` if both `test.cmd` and `test.dir` are present and non-empty
/// (tests will run normally). Returns `Ok(true)` if the configuration is incomplete
/// and the user chose to continue without tests (skip_test=true). Returns `Err` if
/// the user chose to exit.
pub(crate) fn check_test_configuration(feature: &str) -> Result<bool> {
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
pub(crate) fn step_2_5_load_or_create_stats(
    feature: &str,
    preexisting_resume_snapshot_needed: bool,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
    show_full_output: bool,
    skip_test: bool,
) -> Result<util::TranslationStats> {
    let project_root = util::find_project_root()?;
    let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
    let rust_src_dir = rust_dir.join("src");
    match util::TranslationStats::load_from_file(feature)? {
        Some(mut existing_stats) => {
            let normalized = existing_stats.normalize_file_keys();
            let reconciled =
                rust_src_dir.is_dir() && existing_stats.reconcile_with_workspace(&rust_src_dir)?;
            if normalized || reconciled {
                save_stats_or_warn(&existing_stats, feature);
            }
            loop {
                print_previous_progress_summary(&existing_stats);

                let choice =
                    interaction::prompt_continue_or_restart(!existing_stats.skipped_files.is_empty())?;

                match compute_resume_action(choice, feature, preexisting_resume_snapshot_needed) {
                    ResumeAction::Continue { snapshot_message } => {
                        if let Some(snapshot_message) = snapshot_message {
                            if git_commit_or_warn(&snapshot_message, feature) {
                                println!(
                                    "{}",
                                    "✓ Snapshotted uncommitted translation progress before resume."
                                        .bright_green()
                                );
                            }
                        }
                        println!("{}", "✓ Continuing previous progress...".bright_green());
                        return Ok(existing_stats);
                    }
                    ResumeAction::Restart => {
                        println!(
                            "{}",
                            "✓ Starting fresh translation session...".bright_cyan()
                        );
                        util::TranslationStats::clear_stats_file(feature)?;
                        return Ok(util::TranslationStats::new());
                    }
                    ResumeAction::FixSkippedFiles => {
                        println!(
                            "{}",
                            "Reprocessing skipped files from previous run...".bright_cyan()
                        );
                        let mut progress_state = build_progress_state(&rust_dir, None)?;
                        let mut translations_since_last_test = 0usize;
                        process_skipped_files_once(
                            feature,
                            &rust_dir,
                            &mut progress_state,
                            max_error_fix_attempts,
                            max_warning_fix_attempts,
                            show_full_output,
                            &mut existing_stats,
                            skip_test,
                            &mut translations_since_last_test,
                        )?;
                        run_final_interval_test_if_needed(
                            feature,
                            skip_test,
                            translations_since_last_test,
                        )?;
                        save_stats_or_warn(&existing_stats, feature);
                    }
                }
            }
        }
        None => {
            println!("{}", "Starting new translation session...".bright_cyan());
            let mut stats = util::TranslationStats::new();
            if rust_src_dir.is_dir() && stats.reconcile_with_workspace(&rust_src_dir)? {
                save_stats_or_warn(&stats, feature);
            }
            Ok(stats)
        }
    }
}

enum ResumeAction {
    Continue { snapshot_message: Option<String> },
    Restart,
    FixSkippedFiles,
}

fn compute_resume_action(
    choice: interaction::ContinueChoice,
    feature: &str,
    preexisting_resume_snapshot_needed: bool,
) -> ResumeAction {
    match choice {
        interaction::ContinueChoice::Continue => ResumeAction::Continue {
            snapshot_message: preexisting_resume_snapshot_needed.then(|| {
                format!(
                    "Snapshot unfinished translation progress before resume (feature: {})",
                    feature
                )
            }),
        },
        interaction::ContinueChoice::Restart => ResumeAction::Restart,
        interaction::ContinueChoice::FixSkippedFiles => ResumeAction::FixSkippedFiles,
    }
}

fn print_previous_progress_summary(stats: &util::TranslationStats) {
    println!(
        "\n{}",
        "Found previous translation progress!"
            .bright_yellow()
            .bold()
    );
    println!("Previous progress:");
    println!("  - Total files translated: {}", stats.total_files);
    println!("  - Files skipped: {}", stats.skipped_files.len());
    if !stats.translation_failed_files.is_empty() {
        println!(
            "  - Translation failures: {}",
            stats.translation_failed_files.len()
        );
    }
}

/// Steps 3 & 4: Scan for files to translate and initialize progress tracking
pub(crate) fn step_3_4_select_files_and_init_progress(
    feature: &str,
    _stats: &util::TranslationStats,
    target_file: Option<&str>,
) -> Result<(std::path::PathBuf, util::ProgressState)> {
    println!(
        "\n{}",
        "Step 3: Scan Files to Translate".bright_cyan().bold()
    );

    // Get rust directory path
    let project_root = util::find_project_root()?;
    let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
    let progress_state = build_progress_state(&rust_dir, target_file)?;
    let already_processed = progress_state.processed_count;
    let total_rs_files = progress_state.total_count;

    // Display progress
    print_progress_status(already_processed, total_rs_files);

    Ok((rust_dir, progress_state))
}

fn build_progress_state(rust_dir: &Path, target_file: Option<&str>) -> Result<util::ProgressState> {
    let (already_processed, total_rs_files) = if let Some(target_file) = target_file {
        let target_path = rust_dir.join(target_file);
        let exists = target_path.is_file();
        let is_empty = exists && std::fs::metadata(&target_path)?.len() == 0;
        let already_processed = if exists && !is_empty { 1 } else { 0 };
        (already_processed, 1)
    } else {
        let total_rs_files = file_scanner::count_all_rs_files(rust_dir)?;
        let initial_empty_count = file_scanner::find_empty_rs_files(rust_dir)?.len();
        let already_processed = total_rs_files.saturating_sub(initial_empty_count);
        (already_processed, total_rs_files)
    };

    Ok(util::ProgressState::with_initial_progress(
        total_rs_files,
        already_processed,
    ))
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
pub(crate) fn step_5_execute_translation_loop(
    feature: &str,
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    allow_all: bool,
    target_file: Option<&str>,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
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

        let empty_rs_files = filter_target_files(empty_rs_files, rust_dir, target_file)?;

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
            max_error_fix_attempts,
            max_warning_fix_attempts,
            show_full_output,
            stats,
            skip_test,
            &mut translations_since_last_test,
        )?;

        if target_file.is_some() {
            break;
        }
    }

    // Handle skipped files after the main translation loop
    if target_file.is_none() {
        handle_skipped_files_loop(
            feature,
            rust_dir,
            progress_state,
            max_error_fix_attempts,
            max_warning_fix_attempts,
            show_full_output,
            stats,
            skip_test,
            &mut translations_since_last_test,
        )?;
    }

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
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
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
            interaction::SkippedFilesChoice::ProcessNow => process_skipped_files_once(
                feature,
                rust_dir,
                progress_state,
                max_error_fix_attempts,
                max_warning_fix_attempts,
                show_full_output,
                stats,
                skip_test,
                translations_since_last_test,
            )?,
            interaction::SkippedFilesChoice::ExitForLater => break,
        }
    }

    Ok(())
}

fn process_skipped_files_once(
    feature: &str,
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
    show_full_output: bool,
    stats: &mut util::TranslationStats,
    skip_test: bool,
    translations_since_last_test: &mut usize,
) -> Result<()> {
    let failed_isolation = BackgroundFailedFileIsolation::activate(
        feature,
        rust_dir,
        &files_to_process_snapshot(&stats.skipped_files),
        stats,
    )?;
    let result = process_skipped_files_once_inner(
        feature,
        rust_dir,
        progress_state,
        max_error_fix_attempts,
        max_warning_fix_attempts,
        show_full_output,
        stats,
        skip_test,
        translations_since_last_test,
    );
    failed_isolation.restore()?;
    result
}

fn process_skipped_files_once_inner(
    feature: &str,
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
    show_full_output: bool,
    stats: &mut util::TranslationStats,
    skip_test: bool,
    translations_since_last_test: &mut usize,
) -> Result<()> {
    let files_to_process = std::mem::take(&mut stats.skipped_files);
    let total = files_to_process.len();
    for idx in 0..files_to_process.len() {
        let file_name = files_to_process[idx].clone();
        let rs_file = rust_dir.join(&file_name);
        let pos = idx + 1;
        print_file_processing_header(pos, total, &file_name);

        let (_, skip_interval_test) = config::compute_interval_test_decision(*translations_since_last_test);
        let translation_mode = prepare_skipped_file_for_retry(
            feature,
            rust_dir,
            &file_name,
            &files_to_process[idx..],
        )?;

        match file_processor::process_rs_file(
            feature,
            &rs_file,
            &file_name,
            pos,
            total,
            max_error_fix_attempts,
            max_warning_fix_attempts,
            show_full_output,
            stats,
            skip_test,
            skip_interval_test,
            translation_mode,
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
                for remaining_file in &files_to_process[idx + 1..] {
                    stats.record_file_skipped(remaining_file.clone());
                }
                save_stats_or_warn(stats, feature);
                return Err(e);
            }
            Ok(tests_ran) => {
                // Mark file as processed. mark_processed() is capped at total_count
                // so it cannot overflow even if this is a resumed session.
                clear_skipped_file_stash(feature, &file_name)?;
                progress_state.mark_processed();
                config::update_interval_counter(translations_since_last_test, tests_ran);
                save_stats_or_warn(stats, feature);
                maybe_run_periodic_git_gc(progress_state);
            }
        }
    }

    Ok(())
}

struct BackgroundFailedFileIsolation {
    feature: String,
    rust_dir: std::path::PathBuf,
    stashed_files: Vec<String>,
}

impl BackgroundFailedFileIsolation {
    fn activate(
        feature: &str,
        rust_dir: &Path,
        skipped_retry_files: &[String],
        stats: &util::TranslationStats,
    ) -> Result<Self> {
        let skipped_set: std::collections::HashSet<&str> =
            skipped_retry_files.iter().map(|item| item.as_str()).collect();
        let mut stashed_files = Vec::new();

        for file_name in &stats.translation_failed_files {
            if skipped_set.contains(file_name.as_str()) {
                continue;
            }
            let rs_file = rust_dir.join(file_name);
            if rust_file_has_content(&rs_file)? {
                stash_skipped_file_for_later(feature, file_name, &rs_file)?;
                stashed_files.push(file_name.clone());
            }
        }

        Ok(Self {
            feature: feature.to_string(),
            rust_dir: rust_dir.to_path_buf(),
            stashed_files,
        })
    }

    fn restore(self) -> Result<()> {
        for file_name in &self.stashed_files {
            let rs_file = self.rust_dir.join(file_name);
            restore_skipped_file_stash(&self.feature, file_name, &rs_file)?;
        }
        Ok(())
    }
}

fn files_to_process_snapshot(skipped_files: &[String]) -> Vec<String> {
    skipped_files.to_vec()
}

fn prepare_skipped_file_for_retry(
    feature: &str,
    rust_dir: &Path,
    current_file: &str,
    remaining_skipped_files: &[String],
) -> Result<file_processor::TranslationInputMode> {
    temporarily_clear_other_skipped_files(feature, rust_dir, current_file, remaining_skipped_files)?;
    let current_rs_file = rust_dir.join(current_file);
    let restored = restore_skipped_file_stash(feature, current_file, &current_rs_file)?;
    let current_has_content = rust_file_has_content(&current_rs_file)?;

    if restored || current_has_content {
        println!(
            "│ {}",
            "Using existing Rust output for skipped-file recovery; proceeding directly to check/fix."
                .bright_blue()
        );
        Ok(file_processor::TranslationInputMode::ReuseExistingRust)
    } else {
        println!(
            "│ {}",
            "Skipped file is empty; retranslating it from the C source before verification."
                .bright_blue()
        );
        Ok(file_processor::TranslationInputMode::TranslateFromC)
    }
}

fn temporarily_clear_other_skipped_files(
    feature: &str,
    rust_dir: &Path,
    current_file: &str,
    remaining_skipped_files: &[String],
) -> Result<()> {
    for file_name in remaining_skipped_files {
        if file_name == current_file {
            continue;
        }
        let rs_file = rust_dir.join(file_name);
        if rust_file_has_content(&rs_file)? {
            stash_skipped_file_for_later(feature, file_name, &rs_file)?;
        }
    }
    Ok(())
}

fn skipped_file_stash_root(feature: &str) -> Result<std::path::PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root
        .join(".c2rust")
        .join(feature)
        .join("tmp")
        .join("skipped-rust-stash"))
}

fn validate_relative_stats_path(file_name: &str) -> Result<std::path::PathBuf> {
    use std::path::{Component, Path, PathBuf};

    let path = Path::new(file_name);
    if path.is_absolute() {
        anyhow::bail!("Skipped file path must be relative, got: {}", file_name);
    }

    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => anyhow::bail!("Skipped file path must not contain path traversal: {}", file_name),
        }
    }

    if relative.as_os_str().is_empty() {
        anyhow::bail!("Skipped file path is empty: {}", file_name);
    }

    Ok(relative)
}

fn skipped_file_stash_path(feature: &str, file_name: &str) -> Result<std::path::PathBuf> {
    let stash_root = skipped_file_stash_root(feature)?;
    Ok(stash_root.join(validate_relative_stats_path(file_name)?))
}

fn rust_file_has_content(rs_file: &Path) -> Result<bool> {
    match std::fs::metadata(rs_file) {
        Ok(metadata) => Ok(metadata.len() > 0),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).context(format!(
            "Failed to inspect Rust file content state: {}",
            rs_file.display()
        )),
    }
}

pub(crate) fn stash_skipped_file_for_later(feature: &str, file_name: &str, rs_file: &Path) -> Result<()> {
    if !rust_file_has_content(rs_file)? {
        revert_failed_file_to_empty(rs_file)?;
        return Ok(());
    }

    let stash_path = skipped_file_stash_path(feature, file_name)?;
    if let Some(parent) = stash_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create skipped-file stash directory: {}", parent.display())
        })?;
    }

    let content = std::fs::read(rs_file)
        .with_context(|| format!("Failed to read Rust file for skipped stash: {}", rs_file.display()))?;
    std::fs::write(&stash_path, content).with_context(|| {
        format!(
            "Failed to write skipped-file stash for {} at {}",
            file_name,
            stash_path.display()
        )
    })?;
    revert_failed_file_to_empty(rs_file)?;
    Ok(())
}

fn restore_skipped_file_stash(feature: &str, file_name: &str, rs_file: &Path) -> Result<bool> {
    let stash_path = skipped_file_stash_path(feature, file_name)?;
    if !stash_path.is_file() {
        return Ok(false);
    }

    if rust_file_has_content(rs_file)? {
        return Ok(false);
    }

    let content = std::fs::read(&stash_path).with_context(|| {
        format!(
            "Failed to read skipped-file stash for {} from {}",
            file_name,
            stash_path.display()
        )
    })?;
    std::fs::write(rs_file, content).with_context(|| {
        format!(
            "Failed to restore skipped-file stash for {} into {}",
            file_name,
            rs_file.display()
        )
    })?;
    std::fs::remove_file(&stash_path).with_context(|| {
        format!(
            "Failed to remove skipped-file stash after restore: {}",
            stash_path.display()
        )
    })?;
    prune_empty_stash_dirs(feature, stash_path.parent())?;
    Ok(true)
}

fn clear_skipped_file_stash(feature: &str, file_name: &str) -> Result<()> {
    let stash_path = skipped_file_stash_path(feature, file_name)?;
    match std::fs::remove_file(&stash_path) {
        Ok(()) => prune_empty_stash_dirs(feature, stash_path.parent()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).context(format!(
            "Failed to remove skipped-file stash: {}",
            stash_path.display()
        )),
    }
}

fn prune_empty_stash_dirs(
    feature: &str,
    start_dir: Option<&Path>,
) -> Result<()> {
    let stash_root = skipped_file_stash_root(feature)?;
    let mut current = start_dir.map(|path| path.to_path_buf());

    while let Some(dir) = current {
        if dir == stash_root {
            break;
        }
        if std::fs::read_dir(&dir)
            .with_context(|| format!("Failed to read skipped-file stash dir: {}", dir.display()))?
            .next()
            .is_some()
        {
            break;
        }
        std::fs::remove_dir(&dir).with_context(|| {
            format!("Failed to remove empty skipped-file stash dir: {}", dir.display())
        })?;
        current = dir.parent().map(|parent| parent.to_path_buf());
    }

    if stash_root.is_dir()
        && std::fs::read_dir(&stash_root)
            .with_context(|| format!("Failed to read skipped-file stash root: {}", stash_root.display()))?
            .next()
            .is_none()
    {
        std::fs::remove_dir(&stash_root).with_context(|| {
            format!(
                "Failed to remove empty skipped-file stash root: {}",
                stash_root.display()
            )
        })?;
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

    file_processor::verify_hybrid_build_prerequisites()?;

    // 在整个序列开始前统一更新一次代码分析，避免 clean/build/test 各自重复更新
    println!("{}", "Updating code analysis...".bright_blue());
    builder::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    builder::c2rust_clean_no_analysis(feature)?;

    if let Err(build_error) = builder::c2rust_build_no_analysis(feature) {
        println!("{}", "✗ Final build failed".red().bold());
        return Err(build_error);
    }
    println!("{}", "✓ Final build successful".bright_green().bold());

    match builder::c2rust_test_no_analysis(feature) {
        Ok(_) => {
            println!(
                "{}",
                "✓ Final hybrid build tests passed".bright_green().bold()
            );
            builder::update_code_analysis_build_success(feature)?;
        }
        Err(test_error) => {
            if config::should_continue_on_test_error() {
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
pub(crate) fn git_commit_or_warn(message: &str, feature: &str) -> bool {
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

fn filter_target_files(
    empty_rs_files: Vec<std::path::PathBuf>,
    rust_dir: &Path,
    target_file: Option<&str>,
) -> Result<Vec<std::path::PathBuf>> {
    let Some(target_file) = target_file else {
        return Ok(empty_rs_files);
    };

    let filtered: Vec<_> = empty_rs_files
        .into_iter()
        .filter(|path| {
            path.strip_prefix(rust_dir)
                .ok()
                .and_then(|rel| rel.to_str())
                .map(|rel| rel == target_file)
                .unwrap_or(false)
        })
        .collect();

    if !filtered.is_empty() {
        return Ok(filtered);
    }

    let target_path = rust_dir.join(target_file);
    if !target_path.exists() {
        anyhow::bail!(
            "Target Rust file not found under feature workspace: {}",
            target_file
        );
    }

    anyhow::bail!(
        "Target Rust file is currently excluded from translation (likely skipped or marked translation-failed): {}",
        target_file
    );
}

pub(crate) fn prepare_target_file_rerun(
    feature: &str,
    target_file: &str,
    stats: &mut util::TranslationStats,
) -> Result<()> {
    let project_root = util::find_project_root()?;
    let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
    let target_path = rust_dir.join(target_file);

    stats.clear_target_history(target_file);
    save_stats_or_warn(stats, feature);

    if !target_path.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(&target_path)?;
    if metadata.len() == 0 {
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "Target file already has previous Rust output; clearing it for targeted rerun: {}",
            target_file
        )
        .bright_yellow()
    );
    std::fs::write(&target_path, "")
        .with_context(|| format!("Failed to clear target Rust file for rerun: {}", target_file))?;

    Ok(())
}

/// Process all selected files
fn process_selected_files(
    feature: &str,
    empty_rs_files: &[std::path::PathBuf],
    selected_indices: &[usize],
    rust_dir: &Path,
    progress_state: &mut util::ProgressState,
    max_error_fix_attempts: usize,
    max_warning_fix_attempts: usize,
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
            config::compute_interval_test_decision(*translations_since_last_test);

        match file_processor::process_rs_file(
            feature,
            rs_file,
            file_name,
            current_position,
            total_count,
            max_error_fix_attempts,
            max_warning_fix_attempts,
            show_full_output,
            stats,
            skip_test,
            skip_interval_test,
            file_processor::TranslationInputMode::TranslateFromC,
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
                config::update_interval_counter(translations_since_last_test, tests_ran);
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
/// * `max_error_fix_attempts` - Maximum number of build-error fix attempts per translation
/// * `max_warning_fix_attempts` - Maximum number of warning-fix attempts per translation
/// * `show_full_output` - Whether to show full output
///
/// # Returns
/// * `Ok(tests_ran)` - File processed successfully; `true` when the test suite executed
///   for this translation (either automatically or via a Manual Fix), `false` otherwise
/// * `Err` - Processing failed after all retry attempts
pub(crate) fn revert_failed_file_to_empty(rs_file: &Path) -> Result<()> {
    std::fs::write(rs_file, "").with_context(|| {
        format!(
            "Failed to revert skipped/failed translation to empty placeholder: {}",
            rs_file.display()
        )
    })
}

// ============================================================================
// File Processing Helper Functions
// ============================================================================

/// Print header for translation attempt

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{interaction, file_processor};
    use file_processor::TranslationInputMode;
    use serial_test::serial;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

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

    struct CurrentDirGuard {
        prior: PathBuf,
    }

    impl CurrentDirGuard {
        fn change_to(path: &Path) -> Self {
            let prior = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { prior }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.prior).unwrap();
        }
    }

    fn create_temp_feature_workspace(
        feature: &str,
    ) -> (tempfile::TempDir, CurrentDirGuard, PathBuf, PathBuf) {
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let feature_root = project_root.join(".c2rust").join(feature);
        let rust_dir = feature_root.join("rust");
        fs::create_dir_all(rust_dir.join("src")).unwrap();
        let guard = CurrentDirGuard::change_to(&project_root);
        (temp_dir, guard, feature_root, rust_dir)
    }


    #[test]
    fn test_compute_resume_action_skips_snapshot_when_preexisting_tree_is_clean() {
        let action = compute_resume_action(
            interaction::ContinueChoice::Continue,
            "default",
            false,
        );

        match action {
            ResumeAction::Continue { snapshot_message } => {
                assert!(snapshot_message.is_none());
            }
            ResumeAction::Restart | ResumeAction::FixSkippedFiles => {
                panic!("expected continue action")
            }
        }
    }

    #[test]
    fn test_compute_resume_action_requests_snapshot_when_preexisting_tree_is_dirty() {
        let action = compute_resume_action(
            interaction::ContinueChoice::Continue,
            "default",
            true,
        );

        match action {
            ResumeAction::Continue { snapshot_message } => {
                assert_eq!(
                    snapshot_message.as_deref(),
                    Some("Snapshot unfinished translation progress before resume (feature: default)")
                );
            }
            ResumeAction::Restart | ResumeAction::FixSkippedFiles => {
                panic!("expected continue action")
            }
        }
    }

    #[test]
    fn test_compute_resume_action_fix_skipped_files() {
        let action = compute_resume_action(
            interaction::ContinueChoice::FixSkippedFiles,
            "default",
            true,
        );

        match action {
            ResumeAction::FixSkippedFiles => {}
            ResumeAction::Continue { .. } | ResumeAction::Restart => {
                panic!("expected fix-skipped-files action")
            }
        }
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


    #[test]
    fn test_revert_failed_file_to_empty_clears_non_empty_file() {
        let temp_dir = tempdir().unwrap();
        let rs_file = temp_dir.path().join("fun_example.rs");
        fs::write(&rs_file, "pub fn broken() -> i32 { nope }\n").unwrap();

        revert_failed_file_to_empty(&rs_file).unwrap();

        let content = fs::read_to_string(&rs_file).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_revert_failed_file_to_empty_preserves_empty_file() {
        let temp_dir = tempdir().unwrap();
        let rs_file = temp_dir.path().join("fun_example.rs");
        fs::write(&rs_file, "").unwrap();

        revert_failed_file_to_empty(&rs_file).unwrap();

        let metadata = fs::metadata(&rs_file).unwrap();
        assert_eq!(metadata.len(), 0);
    }

    #[test]
    #[serial]
    fn test_stash_skipped_file_for_later_writes_stash_and_clears_source() {
        let (_temp_dir, _guard, feature_root, rust_dir) = create_temp_feature_workspace("default");
        let rs_file = rust_dir.join("src").join("fun_example.rs");
        fs::write(&rs_file, "pub fn kept() {}\n").unwrap();

        stash_skipped_file_for_later("default", "src/fun_example.rs", &rs_file).unwrap();

        assert_eq!(fs::metadata(&rs_file).unwrap().len(), 0);
        let stash_file = feature_root
            .join("tmp")
            .join("skipped-rust-stash")
            .join("src")
            .join("fun_example.rs");
        assert_eq!(fs::read_to_string(stash_file).unwrap(), "pub fn kept() {}\n");
    }

    #[test]
    #[serial]
    fn test_prepare_skipped_file_for_retry_restores_current_and_clears_other_files() {
        let (_temp_dir, _guard, feature_root, rust_dir) = create_temp_feature_workspace("default");
        let current = rust_dir.join("src").join("fun_current.rs");
        let other = rust_dir.join("src").join("fun_other.rs");
        fs::write(&current, "").unwrap();
        fs::write(&other, "pub fn other() {}\n").unwrap();

        let stash_file = feature_root
            .join("tmp")
            .join("skipped-rust-stash")
            .join("src")
            .join("fun_current.rs");
        fs::create_dir_all(stash_file.parent().unwrap()).unwrap();
        fs::write(&stash_file, "pub fn current() {}\n").unwrap();

        let mode = prepare_skipped_file_for_retry(
            "default",
            &rust_dir,
            "src/fun_current.rs",
            &[
                "src/fun_current.rs".to_string(),
                "src/fun_other.rs".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(mode, TranslationInputMode::ReuseExistingRust);
        assert_eq!(fs::read_to_string(&current).unwrap(), "pub fn current() {}\n");
        assert_eq!(fs::metadata(&other).unwrap().len(), 0);
        let other_stash = feature_root
            .join("tmp")
            .join("skipped-rust-stash")
            .join("src")
            .join("fun_other.rs");
        assert_eq!(fs::read_to_string(other_stash).unwrap(), "pub fn other() {}\n");
    }

    #[test]
    #[serial]
    fn test_prepare_skipped_file_for_retry_uses_c_translation_for_empty_current() {
        let (_temp_dir, _guard, _feature_root, rust_dir) = create_temp_feature_workspace("default");
        let current = rust_dir.join("src").join("fun_current.rs");
        fs::write(&current, "").unwrap();

        let mode = prepare_skipped_file_for_retry(
            "default",
            &rust_dir,
            "src/fun_current.rs",
            &["src/fun_current.rs".to_string()],
        )
        .unwrap();

        assert_eq!(mode, TranslationInputMode::TranslateFromC);
        assert_eq!(fs::metadata(&current).unwrap().len(), 0);
    }

    #[test]
    #[serial]
    fn test_background_failed_file_isolation_stashes_and_restores_failed_files() {
        let (_temp_dir, _guard, feature_root, rust_dir) = create_temp_feature_workspace("default");
        let failed = rust_dir.join("src").join("fun_failed.rs");
        fs::write(&failed, "pub fn failed() {}\n").unwrap();

        let mut stats = util::TranslationStats::new();
        stats.record_file_translation_failed("src/fun_failed.rs".to_string());

        let isolation = BackgroundFailedFileIsolation::activate(
            "default",
            &rust_dir,
            &["src/fun_current.rs".to_string()],
            &stats,
        )
        .unwrap();

        assert_eq!(fs::metadata(&failed).unwrap().len(), 0);
        let stash_file = feature_root
            .join("tmp")
            .join("skipped-rust-stash")
            .join("src")
            .join("fun_failed.rs");
        assert_eq!(fs::read_to_string(&stash_file).unwrap(), "pub fn failed() {}\n");

        isolation.restore().unwrap();

        assert_eq!(fs::read_to_string(&failed).unwrap(), "pub fn failed() {}\n");
        assert!(!stash_file.exists());
    }
}

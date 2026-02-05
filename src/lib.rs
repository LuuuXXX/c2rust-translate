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

/// Main translation workflow for a feature
pub fn translate_feature(feature: &str) -> Result<()> {
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
        
        // Translate type.h to type.rs
        let type_h_path = rust_dir.join("src").join("type.h");
        let type_rs_path = rust_dir.join("src").join("type.rs");
        
        if type_h_path.exists() {
            println!("{}", "Translating type.h to type.rs...".bright_blue());
            translator::translate_c_to_rust(feature, "type", &type_h_path, &type_rs_path)?;
            println!("{}", "✓ Type header translation complete".bright_green());
        }
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

        for rs_file in unprocessed_files.iter() {
            // Get current progress position (persisted across runs)
            let current_position = progress_state.get_current_position();
            let total_count = progress_state.get_total_count();
            
            let progress_msg = format!("═══ Progress: File ({}/{}) ═══", current_position, total_count);
            println!(
                "\n{}",
                progress_msg.bright_magenta().bold()
            );
            logger::log_message(&progress_msg);
            
            let processing_msg = format!("→ Processing: {}", rs_file.display());
            println!("{}", processing_msg.bright_cyan());
            logger::log_message(&processing_msg);
            
            process_rs_file(feature, rs_file)?;
            
            // Mark file as processed and save progress
            progress_state.mark_processed(rs_file, &rust_dir)?;
            progress_state.save(feature)?;
        }
    }

    Ok(())
}

/// Process a single .rs file through the translation workflow
fn process_rs_file(feature: &str, rs_file: &std::path::Path) -> Result<()> {
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

    // Step 2.2.3: Call translation tool
    println!("│");
    println!("│ {}", format!("Translating {} to Rust...", file_type).bright_blue().bold());
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file)?;

    // Step 2.2.4: Verify translation result
    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }
    println!("│ {}", format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green());

    // Step 2.2.5 & 2.2.6: Build and fix errors in a loop (max 5 attempts)
    const MAX_FIX_ATTEMPTS: usize = 5;
    for attempt in 1..=MAX_FIX_ATTEMPTS {
        println!("│");
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
    let rs_file_name = rs_file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");
    
    println!("│");
    println!("│ {}", "Committing changes...".bright_blue());
    git::git_commit(&format!(
        "Translate {} from C to Rust (feature: {})",
        rs_file_name, feature
    ), feature)?;
    println!("│ {}", "✓ Changes committed".bright_green());

    // Step 2.2.8: Update code analysis
    println!("│");
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // Step 2.2.9: Save update result
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

    println!("{}", "Running hybrid build tests...".bright_blue());
    builder::run_hybrid_build(feature)?;
    
    println!("{}", "└─ File processing complete".bright_white().bold());

    Ok(())
}

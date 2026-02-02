pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod translator;

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Main translation workflow for a feature
pub fn translate_feature(feature: &str) -> Result<()> {
    println!("Starting translation for feature: {}", feature);

    // Step 1: Check if rust directory exists
    let feature_path = PathBuf::from(feature);
    let rust_dir = feature_path.join("rust");

    if !rust_dir.exists() {
        println!("Rust directory does not exist. Initializing...");
        analyzer::initialize_feature(feature)?;
        
        // Verify rust directory was created
        if !rust_dir.exists() {
            anyhow::bail!("Error: Failed to initialize rust directory");
        }
        
        // Commit the initialization
        git::git_commit(&format!("Initialize {} rust directory", feature))?;
    }

    // Step 2: Main loop - process all empty .rs files
    loop {
        // Step 2.1: Try to build first
        println!("Building project...");
        if let Err(e) = builder::cargo_build(&rust_dir) {
            println!("Build failed: {}", e);
            // Handle build failures if needed
        }

        // Step 2.2: Scan for empty .rs files
        let empty_rs_files = file_scanner::find_empty_rs_files(&rust_dir)?;
        
        if empty_rs_files.is_empty() {
            println!("No empty .rs files found. Translation complete!");
            break;
        }

        println!("Found {} empty .rs file(s) to process", empty_rs_files.len());

        for rs_file in empty_rs_files {
            process_rs_file(feature, &rs_file, &rust_dir)?;
        }
    }

    Ok(())
}

/// Process a single .rs file through the translation workflow
fn process_rs_file(feature: &str, rs_file: &std::path::Path, rust_dir: &std::path::Path) -> Result<()> {
    use std::fs;

    println!("\nProcessing file: {}", rs_file.display());

    // Step 2.2.1: Extract type from filename
    let file_stem = rs_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?;

    let (file_type, _name) = file_scanner::extract_file_type(file_stem)
        .ok_or_else(|| anyhow::anyhow!("Unknown file prefix: {}", file_stem))?;

    println!("File type: {}", file_type);

    // Step 2.2.2: Check if corresponding .c file exists
    let c_file = rs_file.with_extension("c");
    if !c_file.exists() {
        eprintln!("Warning: Corresponding C file not found: {}", c_file.display());
        eprintln!("The project may be corrupted. Do you need to run 'code-analyse --init'?");
        return Ok(());
    }

    // Step 2.2.3: Call translation tool
    println!("Translating {} file...", file_type);
    translator::translate_c_to_rust(file_type, &c_file, rs_file)?;

    // Step 2.2.4: Verify translation result
    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }

    // Step 2.2.5 & 2.2.6: Build and fix errors in a loop
    loop {
        println!("Building after translation...");
        match builder::cargo_build(rust_dir) {
            Ok(_) => {
                println!("Build successful!");
                break;
            }
            Err(build_error) => {
                println!("Build failed, attempting to fix errors...");
                
                // Try to fix the error
                translator::fix_translation_error(file_type, rs_file, &build_error.to_string())?;

                // Verify fix result
                let metadata = fs::metadata(rs_file)?;
                if metadata.len() == 0 {
                    anyhow::bail!("Fix failed: output file is empty");
                }
            }
        }
    }

    // Step 2.2.7: Save translation result
    git::git_commit(&format!("Translate {} from C to Rust", feature))?;

    // Step 2.2.8: Update code analysis
    println!("Updating code analysis...");
    analyzer::update_code_analysis(feature)?;

    // Step 2.2.9: Save update result
    git::git_commit(&format!("Update code analysis for {}", feature))?;

    // Step 2.2.10 & 2.2.11: Hybrid build testing
    println!("Running hybrid build tests...");
    builder::run_hybrid_build(feature)?;

    Ok(())
}

pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod translator;
pub mod util;

use anyhow::{Context, Result};

/// Main translation workflow for a feature
pub fn translate_feature(feature: &str) -> Result<()> {
    println!("Starting translation for feature: {}", feature);

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
        println!("Rust directory does not exist. Initializing...");
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

    // Step 2: Main loop - process all empty .rs files
    loop {
        // Step 2.1: Try to build first
        println!("Building project...");
        match builder::cargo_build(feature) {
            Ok(_) => {
                println!("Build successful!");
            }
            Err(e) => {
                return Err(e).context("Translation workflow aborted due to build failure");
            }
        }

        // Step 2.2: Scan for empty .rs files
        let empty_rs_files = file_scanner::find_empty_rs_files(&rust_dir)?;
        
        if empty_rs_files.is_empty() {
            println!("No empty .rs files found. Translation complete!");
            break;
        }

        println!("Found {} empty .rs file(s) to process", empty_rs_files.len());

        for (index, rs_file) in empty_rs_files.iter().enumerate() {
            println!(
                "Progress: {}/{} - {}",
                index + 1,
                empty_rs_files.len(),
                rs_file.display()
            );
            println!("Updating code analysis...");
            analyzer::update_code_analysis(feature)?;
            println!("Running hybrid build tests...");
            builder::run_hybrid_build(feature)?;
            process_rs_file(feature, rs_file)?;
        }
    }

    Ok(())
}

/// Process a single .rs file through the translation workflow
fn process_rs_file(feature: &str, rs_file: &std::path::Path) -> Result<()> {
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

    // Step 2.2.2: Check if corresponding .c file exists (with proper IO error handling)
    let c_file = rs_file.with_extension("c");
    match fs::metadata(&c_file) {
        Ok(_) => {}
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
    println!("Translating {} file...", file_type);
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file)?;

    // Step 2.2.4: Verify translation result
    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }

    // Step 2.2.5 & 2.2.6: Build and fix errors in a loop (max 5 attempts)
    const MAX_FIX_ATTEMPTS: usize = 3;
    for attempt in 1..=MAX_FIX_ATTEMPTS {
        println!("Building Rust Project after translation (attempt {}/{})", attempt, MAX_FIX_ATTEMPTS);
        match builder::cargo_build(feature) {
            Ok(_) => {
                println!("Build successful!");
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
                
                println!("Build failed, attempting to fix errors...");
                
                // Try to fix the error
                translator::fix_translation_error(feature, file_type, rs_file, &build_error.to_string())?;

                // Verify fix result
                let metadata = fs::metadata(rs_file)?;
                if metadata.len() == 0 {
                    anyhow::bail!("Fix failed: output file is empty");
                }
            }
        }
    }

    // Step 2.2.7: Save translation result with specific file in commit message
    let rs_file_name = rs_file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");
    git::git_commit(&format!(
        "Translate {} from C to Rust (feature: {})",
        rs_file_name, feature
    ), feature)?;

    // Step 2.2.8: Update code analysis
    println!("Updating code analysis...");
    analyzer::update_code_analysis(feature)?;

    // Step 2.2.9: Save update result
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

    Ok(())
}

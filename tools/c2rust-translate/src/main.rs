use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod error;
mod commands;
mod config;
mod env;
mod file_scanner;
mod git;
mod compiler;
mod translator;

use error::Result;

#[derive(Parser)]
#[command(name = "c2rust-translate")]
#[command(about = "C to Rust translation tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Translate C code to Rust (simple mode)
    Translate {
        /// Feature name for the translation
        #[arg(long)]
        feature: Option<String>,
        
        /// Input file(s) to translate
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
    },
    
    /// Auto-orchestrate the full translation pipeline
    Auto {
        /// Path to the project directory (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,
        
        /// Feature name for the translation
        #[arg(short, long)]
        feature: Option<String>,
        
        /// Path to the mixed build library for LD_PRELOAD
        #[arg(long)]
        ld_preload: Option<String>,
        
        /// Skip environment checks
        #[arg(long)]
        skip_checks: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Translate { feature, files } => {
            handle_translate(feature.as_deref(), &files);
        }
        Commands::Auto { path, feature, ld_preload, skip_checks } => {
            if let Err(e) = handle_auto(path, feature, ld_preload, skip_checks) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn handle_translate(feature: Option<&str>, files: &[PathBuf]) {
    println!("c2rust-translate: Translating files...");
    
    if let Some(feature_name) = feature {
        println!("Feature: {}", feature_name);
    } else {
        println!("Feature: <not specified>");
    }
    
    for file in files {
        println!("Translating file: {}", file.display());
        // TODO: Implement actual translation logic
        // For now, just simulate the translation
        println!("  -> Translation would happen here");
    }
    
    println!("Translation complete!");
}

fn handle_auto(
    path: Option<PathBuf>,
    feature: Option<String>,
    ld_preload: Option<String>,
    skip_checks: bool,
) -> Result<()> {
    // Determine starting directory
    let start_dir = path.unwrap_or_else(|| PathBuf::from("."));
    
    println!("c2rust-translate: Auto-orchestration mode starting in: {:?}", start_dir);
    
    // Step 1: Check required tools
    if !skip_checks {
        println!("Checking required tools...");
        env::check_required_tools()?;
        println!("All required tools found.");
    }
    
    // Step 2: Find project root
    println!("Finding project root...");
    let project_root = env::find_project_root(&start_dir)?;
    println!("Project root found: {:?}", project_root);
    
    // Step 3: Load configuration
    println!("Loading configuration...");
    let (config, _) = config::Config::find_and_load(&project_root)?;
    println!("Configuration loaded successfully.");
    
    // Get feature name
    let feature_name = feature.as_deref().unwrap_or("default");
    println!("Using feature: {}", feature_name);
    
    // Step 4: Find empty Rust files
    println!("Scanning for empty Rust files...");
    let empty_files = file_scanner::find_empty_rust_files(&project_root)?;
    
    if empty_files.is_empty() {
        println!("No empty Rust files found. Nothing to do.");
        return Ok(());
    }
    
    println!("Found {} empty Rust files to process.", empty_files.len());
    
    // Step 5: Process each empty file
    for (idx, rust_file) in empty_files.iter().enumerate() {
        println!("\n=== Processing file {}/{}: {:?} ===", idx + 1, empty_files.len(), rust_file);
        
        // Find corresponding C file
        let c_file = match file_scanner::get_c_file_for_rust(rust_file) {
            Some(f) => f,
            None => {
                println!("Warning: No corresponding C file found for {:?}, skipping.", rust_file);
                continue;
            }
        };
        
        println!("Found corresponding C file: {:?}", c_file);
        
        // Step 6: Translate C to Rust
        println!("Translating C to Rust...");
        if let Err(e) = translator::translate_c_to_rust(&c_file, feature_name) {
            eprintln!("Translation failed: {}", e);
            eprintln!("Please fix the issue manually and rerun the tool.");
            return Err(e);
        }
        
        // Step 7: Analyze code
        println!("Analyzing code...");
        if let Err(e) = translator::analyze_code(feature_name) {
            eprintln!("Code analysis failed: {}", e);
            eprintln!("Please fix the issue manually and rerun the tool.");
            return Err(e);
        }
        
        // Step 8: Execute build pipeline
        println!("Executing build pipeline...");
        
        // Clean
        if let Some((clean_cmd, clean_dir)) = config.get_clean_command() {
            let clean_path = clean_dir
                .map(|d| project_root.join(d))
                .unwrap_or_else(|| project_root.clone());
            println!("Cleaning: {} in {:?}", clean_cmd, clean_path);
            
            let parts: Vec<&str> = clean_cmd.split_whitespace().collect();
            if !parts.is_empty() {
                commands::execute_command_checked(parts[0], &parts[1..], Some(&clean_path), &[])?;
            }
        }
        
        // Build
        if let Some((build_cmd, build_dir)) = config.get_build_command() {
            let build_path = build_dir
                .map(|d| project_root.join(d))
                .unwrap_or_else(|| project_root.clone());
            println!("Building: {} in {:?}", build_cmd, build_path);
            
            let ld_preload = ld_preload.as_deref().unwrap_or("");
            let feature_root = project_root.clone();
            
            if let Err(e) = translator::execute_build(
                &build_path,
                build_cmd,
                &feature_root,
                ld_preload
            ) {
                eprintln!("Build failed: {}", e);
                eprintln!("Please fix the issue manually and rerun the tool.");
                return Err(e);
            }
        }
        
        // Test
        if let Some((test_cmd, test_dir)) = config.get_test_command() {
            let test_path = test_dir
                .map(|d| project_root.join(d))
                .unwrap_or_else(|| project_root.clone());
            println!("Testing: {} in {:?}", test_cmd, test_path);
            
            let ld_preload = ld_preload.as_deref().unwrap_or("");
            let feature_root = project_root.clone();
            
            if let Err(e) = translator::execute_test(
                &test_path,
                test_cmd,
                &feature_root,
                ld_preload
            ) {
                eprintln!("Tests failed: {}", e);
                eprintln!("Please fix the issue manually and rerun the tool.");
                return Err(e);
            }
        }
        
        println!("Successfully processed: {:?}", rust_file);
    }
    
    println!("\n=== All files processed successfully! ===");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_translate() {
        // Test basic translate command with feature and file
        let result = Cli::try_parse_from(["c2rust-translate", "translate", "--feature", "myfeature", "test.c"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        match cli.command {
            Commands::Translate { feature, files } => {
                assert_eq!(feature, Some("myfeature".to_string()));
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].to_str().unwrap(), "test.c");
            }
            _ => panic!("Expected Translate command"),
        }
    }
    
    #[test]
    fn test_cli_translate_without_feature() {
        // Test translate command without feature flag
        let result = Cli::try_parse_from(["c2rust-translate", "translate", "file1.c", "file2.c"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        match cli.command {
            Commands::Translate { feature, files } => {
                assert_eq!(feature, None);
                assert_eq!(files.len(), 2);
            }
            _ => panic!("Expected Translate command"),
        }
    }
    
    #[test]
    fn test_cli_translate_requires_files() {
        // Test that translate command requires at least one file
        let result = Cli::try_parse_from(["c2rust-translate", "translate", "--feature", "myfeature"]);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_cli_auto_subcommand() {
        // Test auto subcommand parsing
        let result = Cli::try_parse_from(["c2rust-translate", "auto", "--feature", "myfeature", "--path", "/some/path"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        match cli.command {
            Commands::Auto { feature, path, .. } => {
                assert_eq!(feature, Some("myfeature".to_string()));
                assert_eq!(path, Some(PathBuf::from("/some/path")));
            }
            _ => panic!("Expected Auto command"),
        }
    }
}

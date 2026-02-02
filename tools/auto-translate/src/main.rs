use clap::Parser;
use std::path::PathBuf;
use auto_translate::{
    env, file_scanner, config, translator, Result, AutoTranslateError
};

#[derive(Parser, Debug)]
#[command(name = "auto-translate")]
#[command(about = "Automated C to Rust translation tool", long_about = None)]
struct Args {
    /// Path to the project directory (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,
    
    /// Path to the mixed build library for LD_PRELOAD
    #[arg(long)]
    ld_preload: Option<String>,
    
    /// Skip environment checks
    #[arg(long)]
    skip_checks: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Determine starting directory
    let start_dir = args.path.unwrap_or_else(|| std::env::current_dir().unwrap());
    
    println!("Auto-translate tool starting in: {:?}", start_dir);
    
    // Step 1: Check required tools
    if !args.skip_checks {
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
        
        // Get filename for feature flag
        let filename = c_file.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| AutoTranslateError::TranslationFailed(
                "Invalid filename".to_string()
            ))?;
        
        // Step 6: Translate C to Rust
        println!("Translating C to Rust...");
        if let Err(e) = translator::translate_c_to_rust(&c_file, filename) {
            eprintln!("Translation failed: {}", e);
            eprintln!("Please fix the issue manually and rerun the tool.");
            return Err(e);
        }
        
        // Step 7: Analyze code
        println!("Analyzing code...");
        if let Err(e) = translator::analyze_code(filename) {
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
                use auto_translate::commands::execute_command_checked;
                execute_command_checked(parts[0], &parts[1..], Some(&clean_path), &[])?;
            }
        }
        
        // Build
        if let Some((build_cmd, build_dir)) = config.get_build_command() {
            let build_path = build_dir
                .map(|d| project_root.join(d))
                .unwrap_or_else(|| project_root.clone());
            println!("Building: {} in {:?}", build_cmd, build_path);
            
            let ld_preload = args.ld_preload.as_deref().unwrap_or("");
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
            
            let ld_preload = args.ld_preload.as_deref().unwrap_or("");
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

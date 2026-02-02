use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "c2rust-translate")]
#[command(about = "A tool for translating C code to Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Translate C code to Rust for a specific feature
    Translate {
        /// Feature name
        #[arg(long)]
        feature: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Translate { feature } => translate_feature(&feature)?,
    }

    Ok(())
}

fn translate_feature(feature: &str) -> Result<()> {
    println!("Starting translation for feature: {}", feature);

    // Step 1: Check if rust directory exists
    let feature_path = PathBuf::from(feature);
    let rust_dir = feature_path.join("rust");

    if !rust_dir.exists() {
        println!("Rust directory does not exist. Initializing...");
        initialize_feature(feature)?;
        
        // Verify rust directory was created
        if !rust_dir.exists() {
            anyhow::bail!("Error: Failed to initialize rust directory");
        }
        
        // Commit the initialization
        git_commit(&format!("Initialize {} rust directory", feature))?;
    }

    // Step 2: Main loop - process all empty .rs files
    loop {
        // Step 2.1: Try to build first
        println!("Building project...");
        if let Err(e) = cargo_build(&rust_dir) {
            println!("Build failed: {}", e);
            // Handle build failures if needed
        }

        // Step 2.2: Scan for empty .rs files
        let empty_rs_files = find_empty_rs_files(&rust_dir)?;
        
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

fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code-analyse --feature {} --init", feature);
    
    let output = Command::new("code-analyse")
        .args(&["--feature", feature, "--init"])
        .output()
        .context("Failed to execute code-analyse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code-analyse failed: {}", stderr);
    }

    Ok(())
}

fn find_empty_rs_files(rust_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut empty_files = Vec::new();

    for entry in WalkDir::new(rust_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "rs") {
            let metadata = fs::metadata(path)?;
            if metadata.len() == 0 {
                empty_files.push(path.to_path_buf());
            }
        }
    }

    Ok(empty_files)
}

fn process_rs_file(feature: &str, rs_file: &Path, rust_dir: &Path) -> Result<()> {
    println!("\nProcessing file: {}", rs_file.display());

    // Step 2.2.1: Extract type from filename
    let file_stem = rs_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?;

    let (file_type, _name) = if file_stem.starts_with("var_") {
        ("var", &file_stem[4..])
    } else if file_stem.starts_with("fun_") {
        ("fn", &file_stem[4..])
    } else {
        anyhow::bail!("Unknown file prefix: {}", file_stem);
    };

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
    translate_c_to_rust(file_type, &c_file, rs_file)?;

    // Step 2.2.4: Verify translation result
    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }

    // Step 2.2.5 & 2.2.6: Build and fix errors in a loop
    loop {
        println!("Building after translation...");
        match cargo_build(rust_dir) {
            Ok(_) => {
                println!("Build successful!");
                break;
            }
            Err(build_error) => {
                println!("Build failed, attempting to fix errors...");
                
                // Try to fix the error
                fix_translation_error(file_type, rs_file, &build_error.to_string())?;

                // Verify fix result
                let metadata = fs::metadata(rs_file)?;
                if metadata.len() == 0 {
                    anyhow::bail!("Fix failed: output file is empty");
                }
            }
        }
    }

    // Step 2.2.7: Save translation result
    git_commit(&format!("Translate {} from C to Rust", feature))?;

    // Step 2.2.8: Update code analysis
    println!("Updating code analysis...");
    update_code_analysis(feature)?;

    // Step 2.2.9: Save update result
    git_commit(&format!("Update code analysis for {}", feature))?;

    // Step 2.2.10 & 2.2.11: Hybrid build testing
    println!("Running hybrid build tests...");
    run_hybrid_build(feature)?;

    Ok(())
}

fn translate_c_to_rust(file_type: &str, c_file: &Path, rs_file: &Path) -> Result<()> {
    let output = Command::new("python")
        .args(&[
            "translate_and_fix.py",
            "--config",
            "config.toml",
            "--type",
            file_type,
            "--code",
            c_file.to_str().unwrap(),
            "--output",
            rs_file.to_str().unwrap(),
        ])
        .output()
        .context("Failed to execute translate_and_fix.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Translation failed: {}", stderr);
    }

    Ok(())
}

fn fix_translation_error(file_type: &str, rs_file: &Path, error_msg: &str) -> Result<()> {
    // Create a temporary file with error message
    let error_file = "/tmp/build_error.txt";
    fs::write(error_file, error_msg)?;

    let output = Command::new("python")
        .args(&[
            "translate_and_fix.py",
            "--config",
            "config.toml",
            "--type",
            file_type,
            "--error",
            error_file,
            "--output",
            rs_file.to_str().unwrap(),
        ])
        .output()
        .context("Failed to execute translate_and_fix.py for fixing")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Fix failed: {}", stderr);
    }

    Ok(())
}

fn cargo_build(rust_dir: &Path) -> Result<()> {
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(rust_dir)
        .output()
        .context("Failed to execute cargo build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Build error: {}", stderr);
    }

    Ok(())
}

fn update_code_analysis(feature: &str) -> Result<()> {
    let output = Command::new("code-analyse")
        .args(&["--feature", feature, "--update"])
        .output()
        .context("Failed to execute code-analyse --update")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("code-analyse update failed: {}", stderr);
    }

    Ok(())
}

fn run_hybrid_build(feature: &str) -> Result<()> {
    // Get build commands from config
    let config_path = PathBuf::from(feature).join(".c2rust/config.toml");
    
    if !config_path.exists() {
        println!("Config file not found, skipping hybrid build tests");
        return Ok(());
    }

    // Get command list
    let output = Command::new("c2rust-config")
        .args(&["config", "--list", feature])
        .output()
        .context("Failed to get config list")?;

    if !output.status.success() {
        println!("Warning: Could not retrieve build commands");
        return Ok(());
    }

    // Execute clean, build, and test commands
    run_c2rust_command("clean", feature)?;
    run_c2rust_command_with_env("build", feature)?;
    run_c2rust_command("test", feature)?;

    Ok(())
}

fn run_c2rust_command(cmd_type: &str, feature: &str) -> Result<()> {
    let cmd_name = format!("c2rust-{}", cmd_type);
    
    let output = Command::new(&cmd_name)
        .args(&[cmd_type, "--", feature])
        .output()
        .with_context(|| format!("Failed to execute {}", cmd_name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: {} failed: {}", cmd_name, stderr);
        eprintln!("Please handle this manually");
    }

    Ok(())
}

fn run_c2rust_command_with_env(cmd_type: &str, feature: &str) -> Result<()> {
    let cmd_name = format!("c2rust-{}", cmd_type);
    
    // Set environment variables for hybrid build
    let feature_root = std::env::current_dir()?.join(feature);
    
    let output = Command::new(&cmd_name)
        .args(&[cmd_type, "--", feature])
        .env("C2RUST_FEATURE_ROOT", feature_root)
        // Note: LD_PRELOAD would need the actual library path
        .output()
        .with_context(|| format!("Failed to execute {}", cmd_name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: {} failed: {}", cmd_name, stderr);
        eprintln!("Please handle this manually");
    }

    Ok(())
}

fn git_commit(message: &str) -> Result<()> {
    // Add all changes
    let add_output = Command::new("git")
        .args(&["add", "."])
        .output()
        .context("Failed to git add")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        println!("Warning: git add failed: {}", stderr);
    }

    // Commit
    let commit_output = Command::new("git")
        .args(&["commit", "-m", message])
        .output()
        .context("Failed to git commit")?;

    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        // It's okay if there's nothing to commit
        if !stderr.contains("nothing to commit") {
            println!("Warning: git commit failed: {}", stderr);
        }
    }

    Ok(())
}

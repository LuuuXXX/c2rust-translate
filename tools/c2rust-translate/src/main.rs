use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "c2rust-translate")]
#[command(about = "C to Rust translation tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Translate C code to Rust
    Translate {
        /// Feature name for the translation
        #[arg(long)]
        feature: Option<String>,
        
        /// Input file(s) to translate
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Translate { feature, files } => {
            handle_translate(feature.as_deref(), files);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
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
        }
    }
    
    #[test]
    fn test_cli_without_feature() {
        // Test translate command without feature flag
        let result = Cli::try_parse_from(["c2rust-translate", "translate", "file1.c", "file2.c"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        match cli.command {
            Commands::Translate { feature, files } => {
                assert_eq!(feature, None);
                assert_eq!(files.len(), 2);
            }
        }
    }
    
    #[test]
    fn test_cli_requires_files() {
        // Test that translate command requires at least one file
        let result = Cli::try_parse_from(["c2rust-translate", "translate", "--feature", "myfeature"]);
        assert!(result.is_err());
    }
}

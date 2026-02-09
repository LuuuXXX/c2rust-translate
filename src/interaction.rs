//! User interaction utilities for prompting and collecting input

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

/// User choice for handling failures
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserChoice {
    Continue,
    ManualFix,
    Exit,
}

/// Prompt user for their choice when max attempts are reached
pub fn prompt_user_choice(failure_type: &str, require_suggestion: bool) -> Result<UserChoice> {
    println!("│");
    println!("│ {}", format!("⚠ {} - What would you like to do?", failure_type).yellow().bold());
    println!("│");
    println!("│ {}", "Available options:".bright_cyan());
    
    if require_suggestion {
        println!("│   {} Continue trying (requires entering a fix suggestion)", "1.".bright_white());
    } else {
        println!("│   {} Continue trying (optionally enter a fix suggestion)", "1.".bright_white());
    }
    
    println!("│   {} Manual fix (edit the file directly)", "2.".bright_white());
    println!("│   {} Exit (skip this file and continue with next, or exit completely)", "3.".bright_white());
    println!("│");
    
    loop {
        print!("│ {} ", "Enter your choice (1/2/3):".bright_yellow());
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim() {
            "1" => return Ok(UserChoice::Continue),
            "2" => return Ok(UserChoice::ManualFix),
            "3" => return Ok(UserChoice::Exit),
            _ => {
                println!("│ {}", format!("Invalid choice '{}'. Please enter 1, 2, or 3.", input.trim()).yellow());
            }
        }
    }
}

/// Prompt user to enter a fix suggestion
/// If require_input is true, user must provide non-empty input
pub fn prompt_suggestion(require_input: bool) -> Result<Option<String>> {
    println!("│");
    println!("│ {}", "Please enter your fix suggestion:".bright_cyan().bold());
    println!("│ {}", "(The suggestion will be saved and used in the next fix attempt)".dimmed());
    
    if !require_input {
        println!("│ {}", "(Press Enter to skip entering a suggestion)".dimmed());
    }
    
    println!("│");
    print!("│ {} ", "Suggestion:".bright_yellow());
    io::stdout().flush()?;
    
    let mut suggestion = String::new();
    io::stdin().read_line(&mut suggestion)?;
    
    let trimmed = suggestion.trim().to_string();
    
    if trimmed.is_empty() {
        if require_input {
            println!("│ {}", "Error: A suggestion is required to continue.".red());
            return prompt_suggestion(require_input);
        } else {
            println!("│ {}", "No suggestion provided.".yellow());
            return Ok(None);
        }
    }
    
    println!("│ {}", format!("✓ Suggestion recorded: {}", trimmed).bright_green());
    Ok(Some(trimmed))
}

/// Open a file in vim for manual editing
pub fn open_in_vim(file_path: &Path) -> Result<()> {
    println!("│");
    println!("│ {}", format!("Opening {} in vim...", file_path.display()).bright_cyan());
    
    let status = Command::new("vim")
        .arg(file_path)
        .status()
        .context("Failed to open vim")?;
    
    if status.success() {
        println!("│ {}", "✓ File editing complete".bright_green());
    } else {
        println!("│ {}", "⚠ vim exited with non-zero status".yellow());
    }
    
    Ok(())
}

/// Display the absolute path of a file
pub fn display_file_path(file_path: &Path, label: &str) {
    println!("│");
    println!("│ {}", format!("{}: {}", label, file_path.display()).bright_cyan().bold());
    println!("│");
}

/// Display multiple file paths
pub fn display_file_paths(c_file: Option<&Path>, rust_file: &Path) {
    println!("│");
    println!("│ {}", "File Locations:".bright_cyan().bold());
    
    if let Some(c_path) = c_file {
        println!("│   {} {}", "C file:   ".bright_white(), c_path.display());
    }
    
    println!("│   {} {}", "Rust file:".bright_white(), rust_file.display());
    println!("│");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_choice_variants() {
        assert_eq!(UserChoice::Continue, UserChoice::Continue);
        assert_eq!(UserChoice::ManualFix, UserChoice::ManualFix);
        assert_eq!(UserChoice::Exit, UserChoice::Exit);
        assert_ne!(UserChoice::Continue, UserChoice::Exit);
    }
}

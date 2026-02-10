//! User interaction utilities for prompting and collecting input

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global auto-accept mode flag
static AUTO_ACCEPT_MODE: AtomicBool = AtomicBool::new(false);

/// Check if auto-accept mode is enabled
pub fn is_auto_accept_mode() -> bool {
    AUTO_ACCEPT_MODE.load(Ordering::Relaxed)
}

/// Enable auto-accept mode
pub fn enable_auto_accept_mode() {
    AUTO_ACCEPT_MODE.store(true, Ordering::Relaxed);
    println!("│ {}", "✓ Auto-accept mode enabled. All future translations will be automatically accepted.".bright_green().bold());
}

/// Disable auto-accept mode (test only)
#[cfg(test)]
pub fn disable_auto_accept_mode() {
    AUTO_ACCEPT_MODE.store(false, Ordering::Relaxed);
}

/// User choice for handling failures
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserChoice {
    Continue,
    ManualFix,
    Exit,
}

/// User choice when compilation succeeds and tests pass
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompileSuccessChoice {
    Accept,
    AutoAccept,
    ManualFix,
    Exit,
}

/// User choice when compilation or tests fail
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FailureChoice {
    AddSuggestion,
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
    println!("│   {} Exit (abort the translation process)", "3.".bright_white());
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
    loop {
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
                // Loop again to re-prompt instead of recursing
                continue;
            } else {
                println!("│ {}", "No suggestion provided.".yellow());
                return Ok(None);
            }
        }
        
        println!("│ {}", format!("✓ Suggestion recorded: {}", trimmed).bright_green());
        return Ok(Some(trimmed));
    }
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

/// Prompt user when compilation succeeds and tests pass
pub fn prompt_compile_success_choice() -> Result<CompileSuccessChoice> {
    println!("│");
    println!("│ {}", "✓ Compilation and tests successful!".bright_green().bold());
    println!("│");
    println!("│ {}", "What would you like to do?".bright_cyan().bold());
    println!("│");
    println!("│ {}", "Available options:".bright_cyan());
    println!("│   {} Accept this code (will be committed)", "1.".bright_white());
    println!("│   {} Auto-accept all subsequent translations", "2.".bright_white());
    println!("│   {} Manual fix (edit the file with VIM)", "3.".bright_white());
    println!("│   {} Exit (abort the translation process)", "4.".bright_white());
    println!("│");
    
    loop {
        print!("│ {} ", "Enter your choice (1/2/3/4):".bright_yellow());
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim() {
            "1" => return Ok(CompileSuccessChoice::Accept),
            "2" => return Ok(CompileSuccessChoice::AutoAccept),
            "3" => return Ok(CompileSuccessChoice::ManualFix),
            "4" => return Ok(CompileSuccessChoice::Exit),
            _ => {
                println!("│ {}", format!("Invalid choice '{}'. Please enter 1, 2, 3, or 4.", input.trim()).yellow());
            }
        }
    }
}

/// Prompt user when tests fail
pub fn prompt_test_failure_choice() -> Result<FailureChoice> {
    println!("│");
    println!("│ {}", "⚠ Tests failed - What would you like to do?".yellow().bold());
    println!("│");
    println!("│ {}", "Available options:".bright_cyan());
    println!("│   {} Add fix suggestion for AI to modify", "1.".bright_white());
    println!("│   {} Manual fix (edit the file with VIM)", "2.".bright_white());
    println!("│   {} Exit (abort the translation process)", "3.".bright_white());
    println!("│");
    
    loop {
        print!("│ {} ", "Enter your choice (1/2/3):".bright_yellow());
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim() {
            "1" => return Ok(FailureChoice::AddSuggestion),
            "2" => return Ok(FailureChoice::ManualFix),
            "3" => return Ok(FailureChoice::Exit),
            _ => {
                println!("│ {}", format!("Invalid choice '{}'. Please enter 1, 2, or 3.", input.trim()).yellow());
            }
        }
    }
}

/// Prompt user when compilation fails after max retries
pub fn prompt_compile_failure_choice() -> Result<FailureChoice> {
    println!("│");
    println!("│ {}", "⚠ Compilation failed - What would you like to do?".red().bold());
    println!("│");
    println!("│ {}", "Available options:".bright_cyan());
    println!("│   {} Add fix suggestion for AI to modify", "1.".bright_white());
    println!("│   {} Manual fix (edit the file with VIM)", "2.".bright_white());
    println!("│   {} Exit (abort the translation process)", "3.".bright_white());
    println!("│");
    
    loop {
        print!("│ {} ", "Enter your choice (1/2/3):".bright_yellow());
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim() {
            "1" => return Ok(FailureChoice::AddSuggestion),
            "2" => return Ok(FailureChoice::ManualFix),
            "3" => return Ok(FailureChoice::Exit),
            _ => {
                println!("│ {}", format!("Invalid choice '{}'. Please enter 1, 2, or 3.", input.trim()).yellow());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_user_choice_variants() {
        assert_eq!(UserChoice::Continue, UserChoice::Continue);
        assert_eq!(UserChoice::ManualFix, UserChoice::ManualFix);
        assert_eq!(UserChoice::Exit, UserChoice::Exit);
        assert_ne!(UserChoice::Continue, UserChoice::Exit);
    }
    
    #[test]
    fn test_compile_success_choice_variants() {
        assert_eq!(CompileSuccessChoice::Accept, CompileSuccessChoice::Accept);
        assert_eq!(CompileSuccessChoice::AutoAccept, CompileSuccessChoice::AutoAccept);
        assert_eq!(CompileSuccessChoice::ManualFix, CompileSuccessChoice::ManualFix);
        assert_eq!(CompileSuccessChoice::Exit, CompileSuccessChoice::Exit);
        assert_ne!(CompileSuccessChoice::Accept, CompileSuccessChoice::Exit);
    }
    
    #[test]
    fn test_failure_choice_variants() {
        assert_eq!(FailureChoice::AddSuggestion, FailureChoice::AddSuggestion);
        assert_eq!(FailureChoice::ManualFix, FailureChoice::ManualFix);
        assert_eq!(FailureChoice::Exit, FailureChoice::Exit);
        assert_ne!(FailureChoice::AddSuggestion, FailureChoice::Exit);
    }
    
    #[test]
    #[serial]
    fn test_auto_accept_mode() {
        // Ensure clean state before test
        disable_auto_accept_mode();
        
        // Initially should be disabled
        assert!(!is_auto_accept_mode());
        
        // Enable it
        enable_auto_accept_mode();
        assert!(is_auto_accept_mode());
        
        // Disable it
        disable_auto_accept_mode();
        assert!(!is_auto_accept_mode());
        
        // Clean up - ensure disabled for next test
        disable_auto_accept_mode();
    }
}

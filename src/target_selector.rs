use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use crate::util;

/// Internal function to read targets list from a specific project root
/// Used for testing to avoid changing global working directory
fn read_targets_list_from_root(feature: &str, project_root: &Path) -> Result<Vec<String>> {
    util::validate_feature_name(feature)?;
    
    let c2rust_dir = project_root.join(".c2rust");
    let feature_path = c2rust_dir.join(feature);
    let targets_file = feature_path.join("c").join("targets.list");
    
    if !targets_file.exists() {
        anyhow::bail!(
            "targets.list file not found at {}",
            targets_file.display()
        );
    }
    
    let content = fs::read_to_string(&targets_file)
        .with_context(|| format!("Failed to read targets.list from {}", targets_file.display()))?;
    
    // Read targets line by line, deduplicate while preserving order
    let mut targets = Vec::new();
    let mut seen = std::collections::HashSet::new();
    
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        
        // Only add if not seen before (deduplication)
        if seen.insert(trimmed.to_string()) {
            targets.push(trimmed.to_string());
        }
    }
    
    if targets.is_empty() {
        anyhow::bail!("No valid targets found in targets.list");
    }
    
    Ok(targets)
}

/// Read target artifacts from targets.list file
/// Returns a deduplicated list of targets preserving file order
pub fn read_targets_list(feature: &str) -> Result<Vec<String>> {
    let project_root = util::find_project_root()?;
    read_targets_list_from_root(feature, &project_root)
}


/// Parse user input for target selection (1-based index)
/// Returns 0-based index of selected target
fn parse_target_selection(input: &str, total_targets: usize) -> Result<usize> {
    let input = input.trim();
    
    if input.is_empty() {
        anyhow::bail!("No input provided. Please select a target.");
    }
    
    let index: usize = input.parse()
        .with_context(|| format!("Invalid number: {}", input))?;
    
    if index < 1 || index > total_targets {
        anyhow::bail!(
            "Selection {} is out of bounds (valid: 1-{})",
            index,
            total_targets
        );
    }
    
    Ok(index - 1)
}

/// Prompt user to select a target from the list
pub fn prompt_target_selection(feature: &str) -> Result<String> {
    let targets = read_targets_list(feature)?;
    
    // If only one target, auto-select it
    if targets.len() == 1 {
        let target = &targets[0];
        println!(
            "\n{} {}",
            "Only one target available, auto-selecting:".bright_cyan(),
            target.bright_yellow()
        );
        return Ok(target.clone());
    }
    
    // Display available targets
    println!("\n{}", "Available target artifacts:".bright_cyan().bold());
    for (idx, target) in targets.iter().enumerate() {
        println!("  {}. {}", idx + 1, target.bright_yellow());
    }
    
    println!();
    println!("{}", "Select a target artifact to translate:".bright_yellow());
    println!("  - Enter the number of the target");
    print!("\n{} ", "Your selection:".bright_green().bold());
    io::stdout().flush()?;
    
    // Read user input
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let selected_idx = parse_target_selection(&input, targets.len())?;
    let selected_target = &targets[selected_idx];
    
    println!(
        "{} {}",
        "Selected target:".bright_green(),
        selected_target.bright_yellow().bold()
    );
    
    Ok(selected_target.clone())
}

/// Store selected target in config using c2rust-config
pub fn store_target_in_config(feature: &str, target: &str) -> Result<()> {
    use std::process::Command;
    
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    // Use c2rust-config to set build.target
    let output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args([
            "config",
            "--make",
            "--feature",
            feature,
            "--set",
            "build.target",
            target,
        ])
        .output()
        .context("Failed to execute c2rust-config to store target")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to store target in config: {}", stderr);
    }
    
    // Verify the value was actually persisted
    let verify_output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args([
            "config",
            "--make",
            "--feature",
            feature,
            "--list",
            "build.target",
        ])
        .output()
        .context("Failed to verify build.target in config")?;
    
    if !verify_output.status.success() {
        let stdout = String::from_utf8_lossy(&verify_output.stdout);
        let stderr = String::from_utf8_lossy(&verify_output.stderr);
        anyhow::bail!(
            "Failed to verify build.target was stored correctly (status: {}): stdout: {} stderr: {}",
            verify_output.status,
            stdout,
            stderr
        );
    }
    
    let stored_value = String::from_utf8_lossy(&verify_output.stdout).trim().to_string();
    if stored_value != target {
        anyhow::bail!(
            "build.target verification failed: expected '{}', got '{}'",
            target,
            stored_value
        );
    }
    
    println!(
        "{} {} = {}",
        "✓ Stored in config:".bright_green(),
        "build.target".cyan(),
        target.bright_yellow()
    );
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    
    #[test]
    fn test_read_targets_list_basic() {
        let temp_dir = tempdir().unwrap();
        
        // Create .c2rust/test_feature/c directory structure
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let targets_file = c_dir.join("targets.list");
        let mut file = fs::File::create(&targets_file).unwrap();
        writeln!(file, "target1").unwrap();
        writeln!(file, "target2").unwrap();
        writeln!(file, "target3").unwrap();
        
        // Use the internal function with explicit project root
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_ok());
        let targets = result.unwrap();
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], "target1");
        assert_eq!(targets[1], "target2");
        assert_eq!(targets[2], "target3");
    }
    
    #[test]
    fn test_read_targets_list_with_duplicates() {
        let temp_dir = tempdir().unwrap();
        
        // Create .c2rust/test_feature/c directory structure
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let targets_file = c_dir.join("targets.list");
        let mut file = fs::File::create(&targets_file).unwrap();
        writeln!(file, "target1").unwrap();
        writeln!(file, "target2").unwrap();
        writeln!(file, "target1").unwrap(); // duplicate
        writeln!(file, "target3").unwrap();
        writeln!(file, "target2").unwrap(); // duplicate
        
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_ok());
        let targets = result.unwrap();
        // Should only have 3 unique targets in order of first appearance
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], "target1");
        assert_eq!(targets[1], "target2");
        assert_eq!(targets[2], "target3");
    }
    
    #[test]
    fn test_read_targets_list_with_empty_lines_and_comments() {
        let temp_dir = tempdir().unwrap();
        
        // Create .c2rust/test_feature/c directory structure
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let targets_file = c_dir.join("targets.list");
        let mut file = fs::File::create(&targets_file).unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "target1").unwrap();
        writeln!(file, "").unwrap(); // empty line
        writeln!(file, "  target2  ").unwrap(); // with whitespace
        writeln!(file, "# Another comment").unwrap();
        writeln!(file, "target3").unwrap();
        
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_ok());
        let targets = result.unwrap();
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], "target1");
        assert_eq!(targets[1], "target2");
        assert_eq!(targets[2], "target3");
    }
    
    #[test]
    fn test_parse_target_selection_valid() {
        assert_eq!(parse_target_selection("1", 3).unwrap(), 0);
        assert_eq!(parse_target_selection("2", 3).unwrap(), 1);
        assert_eq!(parse_target_selection("3", 3).unwrap(), 2);
        assert_eq!(parse_target_selection("  2  ", 3).unwrap(), 1);
    }
    
    #[test]
    fn test_parse_target_selection_invalid() {
        assert!(parse_target_selection("0", 3).is_err());
        assert!(parse_target_selection("4", 3).is_err());
        assert!(parse_target_selection("abc", 3).is_err());
        assert!(parse_target_selection("", 3).is_err());
        assert!(parse_target_selection("  ", 3).is_err());
    }
    
    #[test]
    fn test_read_targets_list_file_not_found() {
        let temp_dir = tempdir().unwrap();
        
        // Create .c2rust but no targets.list
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("targets.list file not found"));
    }
    
    #[test]
    fn test_read_targets_list_empty_file() {
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        // Create empty targets.list
        fs::File::create(c_dir.join("targets.list")).unwrap();
        
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No valid targets found"));
    }
    
    #[test]
    fn test_read_targets_list_only_comments() {
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let targets_file = c_dir.join("targets.list");
        let mut file = fs::File::create(&targets_file).unwrap();
        writeln!(file, "# Comment 1").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "# Comment 2").unwrap();
        
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No valid targets found"));
    }
}

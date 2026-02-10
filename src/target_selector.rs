use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use crate::util;

/// 从特定项目根目录读取目标列表的内部函数
/// 用于测试，以避免更改全局工作目录
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
    
    // 逐行读取目标，在保留顺序的同时去重
    let mut targets = Vec::new();
    let mut seen = std::collections::HashSet::new();
    
    for line in content.lines() {
        let trimmed = line.trim();
        // 跳过空行和注释
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        
        // 仅在之前未见过时添加（去重）
        if seen.insert(trimmed.to_string()) {
            targets.push(trimmed.to_string());
        }
    }
    
    if targets.is_empty() {
        anyhow::bail!("No valid targets found in targets.list");
    }
    
    Ok(targets)
}

/// 从 targets.list 文件读取目标构件
/// 返回保留文件顺序的去重目标列表
pub fn read_targets_list(feature: &str) -> Result<Vec<String>> {
    let project_root = util::find_project_root()?;
    read_targets_list_from_root(feature, &project_root)
}


/// 解析用于目标选择的用户输入（基于 1 的索引）
/// 返回所选目标的基于 0 的索引
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

/// 提示用户从列表中选择一个目标
pub fn prompt_target_selection(feature: &str) -> Result<String> {
    let targets = read_targets_list(feature)?;
    
    // 如果只有一个目标，自动选择它
    if targets.len() == 1 {
        let target = &targets[0];
        println!(
            "\n{} {}",
            "Only one target available, auto-selecting:".bright_cyan(),
            target.bright_yellow()
        );
        return Ok(target.clone());
    }
    
    // 显示可用的目标
    println!("\n{}", "Available target artifacts:".bright_cyan().bold());
    for (idx, target) in targets.iter().enumerate() {
        println!("  {}. {}", idx + 1, target.bright_yellow());
    }
    
    println!();
    println!("{}", "Select a target artifact to translate:".bright_yellow());
    println!("  - Enter the number of the target");
    print!("\n{} ", "Your selection:".bright_green().bold());
    io::stdout().flush()?;
    
    // 读取用户输入
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

/// 使用 c2rust-config 将选定的目标存储在配置中
pub fn store_target_in_config(feature: &str, target: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    // 使用 c2rust-config 设置 build.target
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
    
    // 验证值实际上已被持久化
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
        
        // 使用带有显式项目根目录的内部函数
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
        
        // 创建 .c2rust/test_feature/c 目录结构
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let targets_file = c_dir.join("targets.list");
        let mut file = fs::File::create(&targets_file).unwrap();
        writeln!(file, "target1").unwrap();
        writeln!(file, "target2").unwrap();
        writeln!(file, "target1").unwrap(); // 重复
        writeln!(file, "target3").unwrap();
        writeln!(file, "target2").unwrap(); // 重复
        
        let result = read_targets_list_from_root("test_feature", temp_dir.path());
        assert!(result.is_ok());
        let targets = result.unwrap();
        // 应该只有 3 个唯一目标，按首次出现的顺序
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], "target1");
        assert_eq!(targets[1], "target2");
        assert_eq!(targets[2], "target3");
    }
    
    #[test]
    fn test_read_targets_list_with_empty_lines_and_comments() {
        let temp_dir = tempdir().unwrap();
        
        // 创建 .c2rust/test_feature/c 目录结构
        let c2rust_dir = temp_dir.path().join(".c2rust");
        let feature_dir = c2rust_dir.join("test_feature");
        let c_dir = feature_dir.join("c");
        fs::create_dir_all(&c_dir).unwrap();
        
        let targets_file = c_dir.join("targets.list");
        let mut file = fs::File::create(&targets_file).unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "target1").unwrap();
        writeln!(file, "").unwrap(); // 空行
        writeln!(file, "  target2  ").unwrap(); // 带空格
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
        
        // 创建 .c2rust 但没有 targets.list
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
        
        // 创建空的 targets.list
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

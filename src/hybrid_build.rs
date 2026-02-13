use anyhow::{Context, Result};
use std::process::Command;
use crate::util;
use crate::analyzer;
use colored::Colorize;

/// 混合构建命令类型
#[derive(Debug, Clone, Copy)]
pub enum HybridCommandType {
    Clean,
    Build,
    Test,
}

impl HybridCommandType {
    /// 获取命令类型对应的配置键（命令）
    pub fn cmd_key(&self) -> &'static str {
        match self {
            Self::Clean => "clean.cmd",
            Self::Build => "build.cmd",
            Self::Test => "test.cmd",
        }
    }

    /// 获取命令类型对应的配置键（目录）
    pub fn dir_key(&self) -> &'static str {
        match self {
            Self::Clean => "clean.dir",
            Self::Build => "build.dir",
            Self::Test => "test.dir",
        }
    }

    /// 获取命令类型的字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Build => "build",
            Self::Test => "test",
        }
    }

    /// 是否需要设置 LD_PRELOAD
    pub fn needs_ld_preload(&self) -> bool {
        matches!(self, Self::Build)
    }
}

/// 获取混合构建命令和目录
/// 
/// # 参数
/// - `feature`: 特性名称
/// - `command_type`: 命令类型（clean, build, test）
/// 
/// # 返回
/// - `(cmd, dir)`: 命令字符串和工作目录
pub fn get_hybrid_build_command(feature: &str, command_type: HybridCommandType) -> Result<(String, String)> {
    util::validate_feature_name(feature)?;
    
    let cmd = get_config_value(command_type.cmd_key(), feature)?;
    let dir = get_config_value(command_type.dir_key(), feature)?;
    
    Ok((cmd, dir))
}

/// 执行混合构建命令
/// 
/// # 参数
/// - `feature`: 特性名称
/// - `command_type`: 命令类型
/// 
/// # 返回
/// - `Ok(())`: 命令执行成功
/// - `Err`: 命令执行失败
pub fn execute_hybrid_build_command(feature: &str, command_type: HybridCommandType) -> Result<()> {
    util::validate_feature_name(feature)?;

    // 首先更新代码分析
    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());
    
    let (cmd, _dir) = get_hybrid_build_command(feature, command_type)?;
    
    crate::builder::execute_command_in_dir_with_type(
        &cmd,
        command_type.dir_key(),
        feature,
        command_type.needs_ld_preload(),
        command_type.as_str()
    )
}

/// 从 c2rust-config 获取特定的配置值（内部辅助函数）
fn get_config_value(key: &str, feature: &str) -> Result<String> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");
    
    let output = Command::new("c2rust-config")
        .current_dir(&c2rust_dir)
        .args(["config", "--make", "--feature", feature, "--list", key])
        .output()
        .with_context(|| format!("Failed to get {} from config", key))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to retrieve {}: {}", key, stderr);
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if value.is_empty() {
        anyhow::bail!("Empty {} value from config", key);
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_command_type_keys() {
        assert_eq!(HybridCommandType::Clean.cmd_key(), "clean.cmd");
        assert_eq!(HybridCommandType::Clean.dir_key(), "clean.dir");
        
        assert_eq!(HybridCommandType::Build.cmd_key(), "build.cmd");
        assert_eq!(HybridCommandType::Build.dir_key(), "build.dir");
        
        assert_eq!(HybridCommandType::Test.cmd_key(), "test.cmd");
        assert_eq!(HybridCommandType::Test.dir_key(), "test.dir");
    }

    #[test]
    fn test_hybrid_command_type_as_str() {
        assert_eq!(HybridCommandType::Clean.as_str(), "clean");
        assert_eq!(HybridCommandType::Build.as_str(), "build");
        assert_eq!(HybridCommandType::Test.as_str(), "test");
    }

    #[test]
    fn test_needs_ld_preload() {
        assert!(!HybridCommandType::Clean.needs_ld_preload());
        assert!(HybridCommandType::Build.needs_ld_preload());
        assert!(!HybridCommandType::Test.needs_ld_preload());
    }
}

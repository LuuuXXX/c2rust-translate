use crate::analyzer;
use crate::util;
use anyhow::Result;
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
pub fn get_hybrid_build_command(
    feature: &str,
    command_type: HybridCommandType,
) -> Result<(String, String)> {
    util::validate_feature_name(feature)?;

    let cmd = crate::builder::get_config_value(command_type.cmd_key(), feature)?;
    let dir = crate::builder::get_config_value(command_type.dir_key(), feature)?;

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

    run_hybrid_command(feature, command_type)
}

/// 执行混合构建命令序列（clean + build + test），仅更新一次代码分析
///
/// 相比于对每个命令分别调用 `execute_hybrid_build_command`，此函数只执行一次
/// `analyzer::update_code_analysis`，避免重复分析开销。
///
/// 当 `skip_test` 为 `true` 时跳过测试阶段。
pub fn execute_hybrid_build_sequence(feature: &str, skip_test: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());

    // Clean and Build always run regardless of skip_test: they validate the build itself
    // and must succeed even when the test phase is skipped due to missing test configuration.
    run_hybrid_command(feature, HybridCommandType::Clean)?;
    run_hybrid_command(feature, HybridCommandType::Build)?;
    if skip_test {
        println!(
            "{}",
            "⏭ 跳过测试阶段（测试配置不完整）".bright_yellow()
        );
        return Ok(());
    }
    run_hybrid_command(feature, HybridCommandType::Test)
}

/// 执行单个混合构建命令（不更新代码分析）
fn run_hybrid_command(feature: &str, command_type: HybridCommandType) -> Result<()> {
    let cmd = crate::builder::get_config_value(command_type.cmd_key(), feature)?;

    crate::builder::execute_command_in_dir_with_type(
        &cmd,
        command_type.dir_key(),
        feature,
        command_type.needs_ld_preload(),
        command_type.as_str(),
    )
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

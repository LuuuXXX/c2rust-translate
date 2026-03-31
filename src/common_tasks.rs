//! 公共任务模块
//!
//! 包含需求中定义的4个公共任务：
//! 1. 执行代码错误检查
//! 2. 执行代码告警检查
//! 3. 执行混合构建检查
//! 4. 执行翻译任务

use crate::{builder, git, hybrid_build, translator};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// 公共任务1：执行代码错误检查
///
/// 流程：
/// 1. 执行 cargo build（抑制警告）
/// 2. 执行混合构建检查（内部包含代码分析更新）
/// 3. 提交到 git
///
/// 当 `skip_test` 为 `true` 时跳过混合构建中的测试阶段。
pub fn execute_code_error_check(feature: &str, show_full_output: bool, skip_test: bool) -> Result<()> {
    println!("{}", "执行代码错误检查...".bright_blue());

    println!("{}", "  → 构建中（抑制警告）...".bright_blue());
    builder::cargo_build(feature, true, show_full_output)?;
    println!("{}", "  ✓ 构建成功".bright_green());

    println!("{}", "  → 执行混合构建检查...".bright_blue());
    execute_hybrid_build_check(feature, skip_test)?;
    println!("{}", "  ✓ 混合构建检查通过".bright_green());

    git::git_commit(&format!("Code error check passed for {}", feature), feature)?;
    println!("{}", "✓ 代码错误检查完成".bright_green().bold());

    Ok(())
}

/// 公共任务2：执行代码告警检查
///
/// 流程：
/// 1. 执行 cargo build（显示警告）
/// 2. 执行混合构建检查（内部包含代码分析更新）
/// 3. 提交到 git
pub fn execute_code_warning_check(feature: &str, show_full_output: bool) -> Result<()> {
    println!("{}", "执行代码告警检查...".bright_blue());

    println!("{}", "  → 构建中（显示警告）...".bright_blue());
    match builder::cargo_build(feature, false, show_full_output)? {
        Some(warnings) => {
            println!("{}", "  ⚠ 构建有警告".yellow());
            anyhow::bail!("检测到构建警告:\n{}", warnings);
        }
        None => {
            println!("{}", "  ✓ 构建成功，无警告".bright_green());
        }
    }

    println!("{}", "  → 执行混合构建检查...".bright_blue());
    execute_hybrid_build_check(feature, false)?;
    println!("{}", "  ✓ 混合构建检查通过".bright_green());

    git::git_commit(
        &format!("Code warning check passed for {}", feature),
        feature,
    )?;
    println!("{}", "✓ 代码告警检查完成".bright_green().bold());

    Ok(())
}

/// 公共任务3：执行混合构建检查
///
/// 流程（仅执行一次代码分析）：
/// 1. 执行混合构建清除命令
/// 2. 执行混合构建构建命令
/// 3. 执行混合构建测试命令（当 `skip_test` 为 `true` 时跳过）
pub fn execute_hybrid_build_check(feature: &str, skip_test: bool) -> Result<()> {
    hybrid_build::execute_hybrid_build_sequence(feature, skip_test).context("混合构建检查失败")
}

/// 公共任务4：执行翻译任务
///
/// 调用 `translator::translate_c_to_rust` 执行 C 到 Rust 的翻译
pub fn execute_translation_task<F>(
    feature: &str,
    file_type: &str,
    rs_file: &Path,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String,
{
    let c_file = rs_file.with_extension("c");

    println!("│");
    println!(
        "│ {}",
        format_progress("Translation").bright_magenta().bold()
    );
    println!(
        "│ {}",
        format!("Translating {} to Rust...", file_type)
            .bright_blue()
            .bold()
    );

    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file, show_full_output)?;

    let metadata = std::fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }

    println!(
        "│ {}",
        format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_code_error_check_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str, bool, bool) -> Result<()>,
        {
            let _ = f;
        }
        assert_signature(execute_code_error_check);
    }

    #[test]
    fn test_execute_code_warning_check_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str, bool) -> Result<()>,
        {
            let _ = f;
        }
        assert_signature(execute_code_warning_check);
    }

    #[test]
    fn test_execute_hybrid_build_check_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str, bool) -> Result<()>,
        {
            let _ = f;
        }
        assert_signature(execute_hybrid_build_check);
    }
}

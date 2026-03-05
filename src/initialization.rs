use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;

/// 检查并初始化 feature 目录
///
/// 如果 rust 目录不存在，则初始化并提交
pub fn check_and_initialize_feature(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");

    let rust_dir_exists = match std::fs::metadata(&rust_dir) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                anyhow::bail!("Path exists but is not a directory: {}", rust_dir.display());
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(e).context(format!(
                "Failed to access rust directory at {}",
                rust_dir.display()
            ));
        }
    };

    if !rust_dir_exists {
        println!(
            "{}",
            "Feature directory does not exist. Initializing...".yellow()
        );
        crate::analyzer::initialize_feature(feature)?;

        match std::fs::metadata(&rust_dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    anyhow::bail!(
                        "Initialization created a file instead of a directory: {}",
                        rust_dir.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("Error: Failed to initialize rust directory");
            }
            Err(e) => {
                return Err(e).context(format!(
                    "Failed to verify initialized rust directory at {}",
                    rust_dir.display()
                ));
            }
        }

        crate::git::git_commit(
            &format!("Initialize {} feature directory", feature),
            feature,
        )?;

        println!(
            "{}",
            "✓ Feature directory initialized successfully".bright_green()
        );
    } else {
        println!(
            "{}",
            "Feature directory exists, continuing...".bright_cyan()
        );
    }

    Ok(())
}

/// 执行初始化验证
///
/// 在项目初始化后执行错误检查和修复、告警检查和修复，确保项目基础状态正常。
///
/// Phase 1：自动检查并修复构建错误（使用 execute_code_error_check_with_fix_loop）
/// Phase 2：自动检查并修复告警（使用 execute_code_warning_check_with_fix_loop，
///          可通过 C2RUST_PROCESS_WARNINGS=0/false 跳过）
pub fn execute_initial_verification(feature: &str, show_full_output: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!(
        "\n{}",
        "═══ 初始化验证（初始化后） ═══".bright_magenta().bold()
    );

    // 初始化验证不针对单个特定文件；
    // apply_fixes_for_messages 内部会通过 group_errors_by_file 识别实际出错的文件，
    // 以下占位值仅在无法识别具体文件时作为兜底。
    let max_fix_attempts = 10usize;
    let fallback_rs_file = std::path::Path::new("");
    let format_progress = |op: &str| format!("初始化验证 - {}", op);

    // Phase 1: 错误检查和修复循环
    println!("│");
    println!(
        "│ {}",
        "Phase 1: 错误检查和修复...".bright_blue().bold()
    );
    let build_loop_result = crate::verification::execute_code_error_check_with_fix_loop(
        feature,
        "",               // file_type 占位值：初始化验证非单文件场景，实际出错文件由构建输出识别
        fallback_rs_file,
        "初始化验证",
        &format_progress,
        true, // is_last_attempt：初始化阶段无翻译重试
        1,    // attempt_number：初始化阶段固定为第一次（也是唯一一次）尝试
        max_fix_attempts,
        show_full_output,
    );

    // 处理用户选择跳过的情况
    if let Err(ref e) = build_loop_result {
        if e.downcast_ref::<crate::verification::SkipFileSignal>().is_some() {
            println!(
                "│ {}",
                "跳过初始化验证。在文件处理过程中可能会出现问题。".yellow()
            );
            return Ok(());
        }
    }

    let (build_successful, _fix_attempts, _had_restart) = build_loop_result?;

    if !build_successful {
        return Err(anyhow::anyhow!("初始化验证失败：构建错误未能修复"));
    }

    // Phase 2: 告警检查和修复（可通过环境变量禁用）
    if crate::should_process_warnings() {
        println!("│");
        println!(
            "│ {}",
            "Phase 2: 告警检查和修复...".bright_blue().bold()
        );
        crate::verification::execute_code_warning_check_with_fix_loop(
            feature,
            "",               // file_type 占位值：同 Phase 1
            fallback_rs_file,
            "初始化验证",
            &format_progress,
            max_fix_attempts,
            show_full_output,
        )
        .unwrap_or_else(|e| {
            println!(
                "│ {}",
                format!("⚠ 告警修复阶段遇到错误: {}", e).yellow()
            );
            0
        });
    } else {
        println!("│");
        println!(
            "│ {}",
            "Phase 2: 告警处理已跳过 (C2RUST_PROCESS_WARNINGS=0/false)."
                .bright_yellow()
        );
    }

    // 执行混合构建检查并提交
    println!("{}", "  → 执行混合构建检查...".bright_blue());
    crate::common_tasks::execute_hybrid_build_check(feature)?;
    println!("{}", "  ✓ 混合构建检查通过".bright_green());

    crate::git::git_commit(
        &format!("Initial verification passed for {}", feature),
        feature,
    )?;

    println!("{}", "✓ 初始化验证完成并已提交".bright_green().bold());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_and_initialize_feature_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str) -> Result<()>,
        {
            let _ = f;
        }

        assert_signature(check_and_initialize_feature);
    }

    #[test]
    fn execute_initial_verification_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str, bool) -> Result<()>,
        {
            let _ = f;
        }

        assert_signature(execute_initial_verification);
    }
}

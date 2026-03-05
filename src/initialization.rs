use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;

/// 查找初始化验证中兜底用的 .rs 文件和对应的 file_type
///
/// 当构建输出无法识别具体出错文件时，apply_fixes_for_messages 需要一个真实存在的 .rs 文件作为兜底。
/// fix_translation_error 要求与 .rs 同名的 .c 伴随文件存在，因此 lib.rs/main.rs 不适合作为兜底。
///
/// 选择优先级：
/// 1. src 目录中带 var_/fun_ 前缀且存在同名 .c 文件的 .rs 文件（可正确推导 file_type）
/// 2. src 目录中任意存在同名 .c 文件的 .rs 文件
///
/// 返回 `(path, file_type)`，其中 file_type 由文件名前缀推导（var_/fun_），无法推导时为 ""。
/// 若找不到任何带 .c 伴随文件的 .rs 文件，则返回 None。
fn resolve_fallback_rs_file(feature: &str) -> Option<(std::path::PathBuf, &'static str)> {
    let project_root = util::find_project_root().ok()?;
    let src_dir = project_root
        .join(".c2rust")
        .join(feature)
        .join("rust")
        .join("src");

    // 两阶段扫描：
    // - best_typed：首个带 var_/fun_ 前缀且有同名 .c 文件的 .rs 文件（可正确推导 file_type）
    // - best_any：首个有同名 .c 文件的 .rs 文件（任意名称，file_type 为 ""）
    let mut best_typed: Option<(std::path::PathBuf, &'static str)> = None;
    let mut best_any: Option<std::path::PathBuf> = None;

    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !path.extension().map_or(false, |ext| ext == "rs") {
                continue;
            }
            // fix_translation_error 需要同名的 .c 文件存在
            if !path.with_extension("c").is_file() {
                continue;
            }
            if best_any.is_none() {
                best_any = Some(path.clone());
            }
            if best_typed.is_none() {
                if let Some((ft, _)) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|stem| crate::file_scanner::extract_file_type(stem))
                {
                    best_typed = Some((path.clone(), ft));
                }
            }
            // 已同时找到两种目标（typed 优先，any 作为备选）时可提前退出
            if best_typed.is_some() && best_any.is_some() {
                break;
            }
        }
    }

    best_typed.or_else(|| best_any.map(|p| (p, "")))
}

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
    // 以下兜底值仅在无法识别具体文件时使用。
    let max_fix_attempts = 10usize;
    let (fallback_rs_file, fallback_file_type) =
        match resolve_fallback_rs_file(feature) {
            Some(v) => v,
            None => {
                anyhow::bail!(
                    "初始化验证失败：在 .c2rust/{}/rust/src 中找不到带同名 .c 文件的 .rs 文件。\
                     请确认 feature 已完成初始化且源文件已生成。",
                    feature
                );
            }
        };
    let format_progress = |op: &str| format!("初始化验证 - {}", op);

    // Phase 1: 错误检查和修复循环
    println!("│");
    println!(
        "│ {}",
        "Phase 1: 错误检查和修复...".bright_blue().bold()
    );
    let build_loop_result = crate::verification::execute_code_error_check_with_fix_loop(
        feature,
        fallback_file_type, // 由兜底文件名推导的 file_type；实际出错文件由构建输出识别
        &fallback_rs_file,
        "初始化验证",
        &format_progress,
        false, // is_last_attempt：初始化阶段不存在翻译重试，传 false 以避免 RetryDirectly 报错
        1,     // attempt_number：初始化阶段固定为第一次（也是唯一一次）尝试
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

    let (build_successful, _fix_attempts, had_restart) = build_loop_result?;

    if had_restart {
        // 用户在初始化验证阶段选择了"直接重试（重新翻译）"，
        // 但初始化验证没有对应的翻译重试流程。
        return Err(anyhow::anyhow!(
            "初始化验证不支持重新翻译。请选择\u{201c}手动修复\u{201d}或\u{201c}跳过\u{201d}，\
             或重新运行并使用 `--show-full-output` 查看构建错误详情。"
        ));
    }

    if !build_successful {
        return Err(anyhow::anyhow!(
            "初始化验证失败：构建错误未能自动修复。请重新运行并使用 `--show-full-output` 选项（或查看构建日志）以查看最后一次构建错误的详细输出。"
        ));
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
            fallback_file_type, // 同 Phase 1
            &fallback_rs_file,
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

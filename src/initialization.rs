use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;

/// 扫描 `src_dir` 目录，查找带同名 `.c` 文件的 `.rs` 文件作为兜底。
///
/// 内部辅助函数，与文件系统无关，便于单元测试。
///
/// 选择优先级（按文件名排序后）：
/// 1. 带 `var_`/`fun_` 前缀且存在同名 `.c` 文件的 `.rs` 文件（可正确推导 `file_type`）
/// 2. 任意存在同名 `.c` 文件的 `.rs` 文件
///
/// `NotFound` 时返回 `Ok(None)`；其他 I/O 错误向上传播。
fn scan_for_fallback_rs_file(
    src_dir: &std::path::Path,
) -> Result<Option<(std::path::PathBuf, &'static str)>> {
    let entries = match std::fs::read_dir(src_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(anyhow::Error::from(e)
                .context(format!("Failed to read directory {}", src_dir.display())));
        }
    };

    // 收集所有带同名 .c 文件的 .rs 文件并排序，确保跨运行和跨平台结果稳定可复现
    let mut candidates: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().map_or(false, |ext| ext == "rs")
                && p.with_extension("c").is_file()
        })
        .collect();
    candidates.sort();

    // 两阶段选择：优先 var_/fun_ 前缀（可正确推导 file_type），次选首个带 .c 的 .rs
    let mut best_typed: Option<(std::path::PathBuf, &'static str)> = None;
    let mut best_any: Option<std::path::PathBuf> = None;

    for path in candidates {
        if best_any.is_none() {
            best_any = Some(path.clone());
        }
        if best_typed.is_none() {
            if let Some((ft, _)) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| crate::file_scanner::extract_file_type(stem))
            {
                best_typed = Some((path, ft));
                break;
            }
        }
    }

    Ok(best_typed.or_else(|| best_any.map(|p| (p, ""))))
}

/// 查找初始化验证中兜底用的 `.rs` 文件和对应的 `file_type`。
///
/// 在 `.c2rust/<feature>/rust/src` 中调用 [`scan_for_fallback_rs_file`]。
/// `find_project_root` 失败或目录 I/O 错误时向上传播；目录不存在时返回 `Ok(None)`。
fn resolve_fallback_rs_file(
    feature: &str,
) -> Result<Option<(std::path::PathBuf, &'static str)>> {
    let project_root = util::find_project_root()
        .context("Failed to find project root while resolving fallback .rs file")?;
    let src_dir = project_root
        .join(".c2rust")
        .join(feature)
        .join("rust")
        .join("src");
    scan_for_fallback_rs_file(&src_dir)
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
        match resolve_fallback_rs_file(feature)? {
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
        // build_successful=false 且 had_restart=false 时，用户在达到最大修复次数后
        // 选择了"添加建议"（AddSuggestion）并安排了翻译重试。
        // 初始化验证阶段没有对应的翻译重试流程来消费该建议，因此在此终止并提示用户。
        return Err(anyhow::anyhow!(
            "初始化验证失败：构建错误未能自动修复。\
             若已添加建议，请在正常文件翻译流程中使用；\
             请重新运行并使用 `--show-full-output` 选项（或查看构建日志）以查看最后一次构建错误的详细输出。"
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

    // ── scan_for_fallback_rs_file ──────────────────────────────────────

    /// Helper: create a file at `dir/name` (and its companion `.c` if requested).
    fn touch(dir: &std::path::Path, name: &str, with_c_companion: bool) {
        std::fs::write(dir.join(name), "").unwrap();
        if with_c_companion {
            let stem = std::path::Path::new(name)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            std::fs::write(dir.join(format!("{}.c", stem)), "").unwrap();
        }
    }

    #[test]
    fn scan_returns_none_for_missing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does_not_exist");
        let result = scan_for_fallback_rs_file(&missing).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_returns_none_for_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let result = scan_for_fallback_rs_file(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_returns_none_when_no_rs_has_c_companion() {
        let tmp = tempfile::tempdir().unwrap();
        // .rs without .c companion — should not be selected
        touch(tmp.path(), "var_foo.rs", false);
        touch(tmp.path(), "lib.rs", false);
        let result = scan_for_fallback_rs_file(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_prefers_typed_over_untyped() {
        let tmp = tempfile::tempdir().unwrap();
        // untyped first alphabetically, typed second
        touch(tmp.path(), "aaa.rs", true); // no var_/fun_ prefix
        touch(tmp.path(), "var_bar.rs", true); // typed
        let (path, file_type) = scan_for_fallback_rs_file(tmp.path())
            .unwrap()
            .expect("should find a fallback");
        assert_eq!(path.file_name().unwrap(), "var_bar.rs");
        assert_eq!(file_type, "var", "file_type should be 'var' for var_bar.rs");
    }

    #[test]
    fn scan_falls_back_to_any_when_no_typed_file() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "lib.rs", true); // has .c but no var_/fun_ prefix
        touch(tmp.path(), "other.rs", true);
        let (path, file_type) = scan_for_fallback_rs_file(tmp.path())
            .unwrap()
            .expect("should find a fallback");
        // sorted: lib.rs < other.rs → lib.rs selected
        assert_eq!(path.file_name().unwrap(), "lib.rs");
        assert_eq!(file_type, "");
    }

    #[test]
    fn scan_result_is_stable_regardless_of_creation_order() {
        // Create files in reverse alphabetical order to verify sorting
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "var_z.rs", true);
        touch(tmp.path(), "var_a.rs", true);
        touch(tmp.path(), "var_m.rs", true);
        let (path, _) = scan_for_fallback_rs_file(tmp.path())
            .unwrap()
            .expect("should find a fallback");
        // sorted: var_a.rs is first; but all are typed, so typed wins — and var_a.rs is first typed
        assert_eq!(path.file_name().unwrap(), "var_a.rs");
    }

    #[test]
    fn scan_ignores_rs_without_c_companion() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "var_no_c.rs", false); // no .c companion
        touch(tmp.path(), "other.rs", true); // has .c but untyped
        let (path, file_type) = scan_for_fallback_rs_file(tmp.path())
            .unwrap()
            .expect("should find a fallback");
        assert_eq!(path.file_name().unwrap(), "other.rs");
        assert_eq!(file_type, "");
    }
}

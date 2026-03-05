use crate::{interaction, util};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// 从错误信息中提取失败的 .rs 文件并打开编辑器
///
/// 支持多文件选择
fn open_failing_files_from_error(error_text: &str, feature: &str) -> Result<bool> {
    let failing_files = crate::error_handler::group_errors_by_file(error_text, feature)?
        .into_iter()
        .map(|(f, _)| f)
        .collect::<Vec<_>>();

    if failing_files.is_empty() {
        return Ok(false);
    }

    if failing_files.len() > 1 {
        println!("│");
        println!(
            "│ {}",
            format!("找到 {} 个包含错误的文件:", failing_files.len()).yellow()
        );
        for (i, f) in failing_files.iter().enumerate() {
            println!("│   {}. {}", i + 1, f.display());
        }
        println!("│");

        let selected_file = interaction::prompt_file_selection_for_edit(&failing_files)?;
        interaction::open_in_vim(&selected_file)?;
    } else {
        interaction::open_in_vim(&failing_files[0])?;
    }

    Ok(true)
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

/// 扫描目录查找用于修复循环的回退 rs 文件（内部可测试函数）
///
/// 优先选择有同名 .c 文件的 var_/fun_ 前缀 .rs 文件；
/// 其次选择任意有同名 .c 文件的 .rs 文件（排除 lib.rs 和 main.rs）。
/// 返回 `(rs_file_path, file_type, file_name)`。
fn scan_for_fallback_rs_file(
    src_dir: &Path,
) -> Result<Option<(PathBuf, &'static str, String)>> {
    let entries = match std::fs::read_dir(src_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(anyhow::Error::from(e))
                .context(format!("Failed to read directory: {}", src_dir.display()));
        }
    };

    let mut generic_fallback: Option<(PathBuf, &'static str, String)> = None;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Exclude lib.rs and main.rs — they are not valid targets for fix loops
        if file_name == "lib.rs" || file_name == "main.rs" {
            continue;
        }

        // Must have a companion .c file with the same stem
        let c_companion = path.with_extension("c");
        let c_meta = match std::fs::metadata(&c_companion) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "failed to read metadata for C companion file: {}",
                        c_companion.display()
                    )
                });
            }
        };
        if !c_meta.is_file() {
            continue;
        }

        // Prefer var_/fun_ prefixed files (return immediately when found)
        if file_name.starts_with("var_") {
            return Ok(Some((path, "var", file_name)));
        }
        if file_name.starts_with("fun_") {
            return Ok(Some((path, "fun", file_name)));
        }

        // Keep as non-prefixed fallback (first match wins); default file_type to "var" since
        // the fix loop uses it only as a hint for the AI prompt — the actual error grouping
        // by `error_handler::group_errors_by_file` overrides this when parsing the build output.
        if generic_fallback.is_none() {
            generic_fallback = Some((path, "var", file_name));
        }
    }

    Ok(generic_fallback)
}

/// 解析初始化验证修复循环的回退 rs 文件
///
/// 扫描 `.c2rust/<feature>/rust/src` 目录，返回适合用于修复循环的文件信息。
fn resolve_fallback_rs_file(
    feature: &str,
) -> Result<Option<(PathBuf, &'static str, String)>> {
    let project_root = util::find_project_root()?;
    let src_dir = project_root
        .join(".c2rust")
        .join(feature)
        .join("rust")
        .join("src");
    scan_for_fallback_rs_file(&src_dir)
}

/// 执行初始化验证
///
/// 在项目初始化后执行一次完整的代码检查，确保项目基础状态正常。
///
/// 流程：
/// 1. **第一阶段**：错误检查与自动修复（使用修复循环）
///    - 尝试自动修复构建错误
///    - 如果修复失败，提供 Skip/ManualFix/Exit 选项
/// 2. **第二阶段**：告警检查与自动修复（非致命）
///    - 只在错误全部修复后执行
///    - 修复失败不会中断整个流程
/// 3. 运行混合构建检查并提交变更
pub fn execute_initial_verification(feature: &str, show_full_output: bool) -> Result<()> {
    util::validate_feature_name(feature)?;

    println!(
        "\n{}",
        "═══ 初始化验证（初始化后） ═══".bright_magenta().bold()
    );

    const MAX_FIX_ATTEMPTS: usize = 3;
    let format_progress = |op: &str| format!("[初始化验证] {}", op);

    // Resolve a fallback rs_file for the fix loops
    let fallback = resolve_fallback_rs_file(feature)?;

    let build_successful = match &fallback {
        Some((rs_file, file_type, file_name)) => {
            // Phase 1: Error check with auto-fix loop
            println!(
                "{}",
                "Phase 1: 检查并修复错误...".bright_blue().bold()
            );
            let result = crate::verification::execute_code_error_check_with_fix_loop(
                feature,
                file_type,
                rs_file,
                file_name,
                &format_progress,
                // is_last_attempt=false: allow the fix loop to present and handle
                // "Retry directly" normally in this initialization context.
                // attempt_number=1: this is the first attempt for this verification.
                false, // is_last_attempt
                1,     // attempt_number
                MAX_FIX_ATTEMPTS,
                show_full_output,
            );
            match result {
                Ok((success, _, _)) => success,
                Err(e) => {
                    // SkipFileSignal means the user chose to skip this verification
                    if e.downcast_ref::<crate::verification::SkipFileSignal>().is_some() {
                        println!(
                            "│ {}",
                            "跳过初始化验证。在文件处理过程中可能会出现问题。".yellow()
                        );
                        return Ok(());
                    }
                    return Err(e).context("初始化验证失败");
                }
            }
        }
        None => {
            // No fallback file found — run an error-only build (no commit here; the single
            // hybrid build + commit runs after both phases complete below).
            // Use a labeled block so Phase 2 and the commit still run after Phase 1 succeeds.
            'phase1: {
                match crate::builder::cargo_build(feature, true, show_full_output) {
                    Ok(_) => break 'phase1 true,
                    Err(mut last_error) => {
                        loop {
                            println!("{}", "✗ 初始化验证失败！".red().bold());
                            println!();
                            println!("{}", "错误详情:".red().bold());
                            println!("{}", format!("{:#}", last_error).red());
                            println!();

                            let choice =
                                interaction::prompt_failure_choice("初始化验证失败")?;

                            match choice {
                                interaction::FailureChoice::Skip => {
                                    println!(
                                        "│ {}",
                                        "跳过初始化验证。在文件处理过程中可能会出现问题。"
                                            .yellow()
                                    );
                                    return Ok(());
                                }
                                interaction::FailureChoice::ManualFix => {
                                    let error_text = format!("{:#}", last_error);
                                    if !open_failing_files_from_error(&error_text, feature)? {
                                        println!(
                                            "│ {}",
                                            "无法识别要打开的特定文件。请检查上面的错误。"
                                                .yellow()
                                        );
                                        return Err(last_error)
                                            .context("初始化验证失败 - 未识别到特定文件");
                                    }
                                    // Re-run the build; on success break out, on failure loop
                                    match crate::builder::cargo_build(
                                        feature,
                                        true,
                                        show_full_output,
                                    ) {
                                        Ok(_) => break 'phase1 true,
                                        Err(e) => {
                                            last_error = e;
                                            continue;
                                        }
                                    }
                                }
                                interaction::FailureChoice::Exit => {
                                    return Err(last_error)
                                        .context("初始化验证失败，用户选择退出");
                                }
                                _ => unreachable!(
                                    "prompt_failure_choice only returns Skip/ManualFix/Exit"
                                ),
                            }
                        }
                    }
                }
            }
        }
    };

    if build_successful {
        // Phase 2: Warning check with auto-fix (non-fatal).
        // Respects the C2RUST_PROCESS_WARNINGS env-var — set to "0" or "false" to skip.
        if let Some((rs_file, file_type, file_name)) = &fallback {
            if crate::should_process_warnings() {
                println!(
                    "{}",
                    "Phase 2: 检查并修复警告...".bright_blue().bold()
                );
                crate::verification::execute_code_warning_check_with_fix_loop(
                    feature,
                    file_type,
                    rs_file,
                    file_name,
                    &format_progress,
                    MAX_FIX_ATTEMPTS,
                    show_full_output,
                )
                .unwrap_or_else(|e| {
                    println!(
                        "│ {}",
                        format!("⚠ 告警检查出现错误: {}", e).yellow()
                    );
                    0
                });
            } else {
                println!(
                    "{}",
                    "Phase 2: 告警处理已跳过 (C2RUST_PROCESS_WARNINGS=0/false)。"
                        .bright_yellow()
                );
            }
        } else {
            // No fallback file for auto-fix; run a basic (non-fatal) warning check.
            if crate::should_process_warnings() {
                println!(
                    "{}",
                    "Phase 2: 检查警告（无法自动修复）...".bright_blue().bold()
                );
                match crate::builder::cargo_build(feature, false, show_full_output) {
                    Ok(Some(warnings)) => {
                        println!(
                            "│ {}",
                            format!("⚠ 检测到告警（无法自动修复）:\n{}", warnings).yellow()
                        );
                    }
                    Ok(None) => {
                        println!("{}", "  ✓ 无告警".bright_green());
                    }
                    Err(e) => {
                        println!(
                            "│ {}",
                            format!("⚠ 告警检查出现错误: {}", e).yellow()
                        );
                    }
                }
            } else {
                println!(
                    "{}",
                    "Phase 2: 告警处理已跳过 (C2RUST_PROCESS_WARNINGS=0/false)。"
                        .bright_yellow()
                );
            }
        }

        // Run hybrid build check and commit
        println!("{}", "  → 执行混合构建检查...".bright_blue());
        crate::common_tasks::execute_hybrid_build_check(feature)?;
        println!("{}", "  ✓ 混合构建检查通过".bright_green());

        crate::git::git_commit(
            &format!("Initial verification passed for {}", feature),
            feature,
        )?;
        println!("{}", "✓ 初始化验证完成并已提交".bright_green().bold());
        Ok(())
    } else {
        // build_successful=false: fix loop exhausted all attempts without success
        Err(anyhow::anyhow!("初始化验证：构建在修复尝试后仍未成功"))
    }
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

    #[test]
    fn scan_for_fallback_rs_file_returns_none_for_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = scan_for_fallback_rs_file(tmp.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn scan_for_fallback_rs_file_returns_none_for_nonexistent_dir() {
        // Use a path under a TempDir that we never create, ensuring it does not exist
        let tmp = tempfile::TempDir::new().unwrap();
        let nonexistent_dir = tmp.path().join("definitely_missing");

        let result = scan_for_fallback_rs_file(&nonexistent_dir);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn scan_for_fallback_rs_file_requires_companion_c() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        // var_foo.rs without companion .c — should not be selected
        std::fs::write(dir.join("var_foo.rs"), "").unwrap();
        let result = scan_for_fallback_rs_file(dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_for_fallback_rs_file_returns_var_prefixed_with_companion() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("var_foo.rs"), "").unwrap();
        std::fs::write(dir.join("var_foo.c"), "").unwrap();
        let result = scan_for_fallback_rs_file(dir).unwrap().unwrap();
        assert_eq!(result.1, "var");
        assert!(result.2.starts_with("var_"));
    }

    #[test]
    fn scan_for_fallback_rs_file_returns_fun_prefixed_with_companion() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("fun_bar.rs"), "").unwrap();
        std::fs::write(dir.join("fun_bar.c"), "").unwrap();
        let result = scan_for_fallback_rs_file(dir).unwrap().unwrap();
        assert_eq!(result.1, "fun");
        assert!(result.2.starts_with("fun_"));
    }

    #[test]
    fn scan_for_fallback_rs_file_prefers_var_over_generic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        // generic file with companion .c
        std::fs::write(dir.join("other.rs"), "").unwrap();
        std::fs::write(dir.join("other.c"), "").unwrap();
        // var_-prefixed file with companion .c
        std::fs::write(dir.join("var_foo.rs"), "").unwrap();
        std::fs::write(dir.join("var_foo.c"), "").unwrap();
        let result = scan_for_fallback_rs_file(dir).unwrap().unwrap();
        assert_eq!(result.1, "var");
        assert!(result.2.starts_with("var_"));
    }

    #[test]
    fn scan_for_fallback_rs_file_excludes_lib_and_main() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        // lib.rs and main.rs with companion .c files — both should be excluded
        std::fs::write(dir.join("lib.rs"), "").unwrap();
        std::fs::write(dir.join("lib.c"), "").unwrap();
        std::fs::write(dir.join("main.rs"), "").unwrap();
        std::fs::write(dir.join("main.c"), "").unwrap();
        let result = scan_for_fallback_rs_file(dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_for_fallback_rs_file_falls_back_to_generic_when_no_prefixed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        // A non-prefixed file with companion .c (not lib/main)
        std::fs::write(dir.join("helper.rs"), "").unwrap();
        std::fs::write(dir.join("helper.c"), "").unwrap();
        let result = scan_for_fallback_rs_file(dir).unwrap().unwrap();
        assert_eq!(result.2, "helper.rs");
    }
}

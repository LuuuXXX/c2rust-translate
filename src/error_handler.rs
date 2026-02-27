//! 用于解析和处理测试失败的错误处理工具

use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::{builder, file_scanner, interaction, suggestion, translator, util};

/// 解析错误消息以提取 Rust 文件路径
/// 返回在错误消息中找到的文件路径列表
/// 过滤为仅包含项目内的文件
pub(crate) fn parse_error_for_files(error_msg: &str, feature: &str) -> Result<Vec<PathBuf>> {
    // 验证特性名称以防止路径遍历
    util::validate_feature_name(feature)?;

    lazy_static::lazy_static! {
        static ref ERROR_PATH_RE: regex::Regex =
            regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?")
                .expect("Failed to compile error path regex");
    }

    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");

    let mut file_paths = HashSet::new();

    for cap in ERROR_PATH_RE.captures_iter(error_msg) {
        if let Some(path_match) = cap.get(1) {
            let path_str = path_match.as_str();
            let path = PathBuf::from(path_str);

            // 尝试原样路径和相对于 rust_dir 的路径
            let candidates = vec![path.clone(), rust_dir.join(&path)];

            for candidate in candidates {
                // 检查文件是否存在且在我们的项目内
                if candidate.exists() && candidate.is_file() {
                    // 确保文件在 rust 目录内
                    if let Ok(canonical) = candidate.canonicalize() {
                        if let Ok(rust_canonical) = rust_dir.canonicalize() {
                            if canonical.starts_with(&rust_canonical) {
                                file_paths.insert(canonical);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // 转换为 Vec 并排序以保持一致的顺序
    let mut result: Vec<PathBuf> = file_paths.into_iter().collect();
    result.sort();

    Ok(result)
}

/// 解析错误消息以提取 Rust 文件路径，按首次出现的顺序返回
/// 与 parse_error_for_files 不同，该函数保留文件在错误信息中首次出现的顺序
pub(crate) fn parse_error_for_files_ordered(
    error_msg: &str,
    feature: &str,
) -> Result<Vec<PathBuf>> {
    util::validate_feature_name(feature)?;

    lazy_static::lazy_static! {
        static ref ERROR_PATH_RE: regex::Regex =
            regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?")
                .expect("Failed to compile error path regex");
    }

    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");

    // Canonicalize rust_dir once, before iterating over captured paths
    let rust_canonical = match rust_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return Ok(Vec::new()),
    };

    let mut seen = HashSet::new();
    let mut ordered_paths = Vec::new();

    for cap in ERROR_PATH_RE.captures_iter(error_msg) {
        if let Some(path_match) = cap.get(1) {
            let path_str = path_match.as_str();
            let path = PathBuf::from(path_str);

            // 尝试原样路径和相对于 rust_dir 的路径
            let candidates = vec![path.clone(), rust_dir.join(&path)];

            for candidate in candidates {
                if candidate.exists() && candidate.is_file() {
                    if let Ok(canonical) = candidate.canonicalize() {
                        if canonical.starts_with(&rust_canonical)
                            && seen.insert(canonical.clone())
                        {
                            ordered_paths.push(canonical);
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(ordered_paths)
}

/// 从错误消息中提取与特定文件相关的部分
/// 将错误按空行分割成块，返回包含该文件路径引用的块（使用最精确的路径后缀匹配）
/// 如果没有找到匹配的块，则返回完整的错误消息
pub(crate) fn extract_error_for_file(error_msg: &str, file_path: &std::path::Path) -> String {
    // Normalize line endings for cross-platform reliability (\r\n and \r -> \n)
    let normalized_msg = error_msg.replace("\r\n", "\n").replace('\r', "\n");

    // Build path suffix candidates (most-specific to least) from normal path components.
    // E.g. for /abs/root/rust/src/a/mod.rs → ["src/a/mod.rs", "a/mod.rs", "mod.rs"]
    // This avoids false matches when multiple files share the same basename (e.g. mod.rs).
    let components: Vec<&str> = file_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    if components.is_empty() {
        return normalized_msg;
    }

    // Choose the longest suffix that actually appears in a --> location line
    let match_pattern = (0..components.len())
        .map(|start| components[start..].join("/"))
        .find(|pattern| {
            normalized_msg
                .lines()
                .any(|line| line.contains("-->") && line.contains(pattern.as_str()))
        });

    let pattern = match match_pattern {
        Some(ref p) => p.as_str(),
        None => return normalized_msg,
    };

    // Split into blank-line-separated blocks and keep those whose --> line
    // references our file path pattern
    let blocks: Vec<&str> = normalized_msg.split("\n\n").collect();
    let relevant: Vec<&str> = blocks
        .iter()
        .filter(|block| {
            block
                .lines()
                .any(|line| line.contains("-->") && line.contains(pattern))
        })
        .copied()
        .collect();

    if relevant.is_empty() {
        normalized_msg
    } else {
        relevant.join("\n\n")
    }
}

/// 当可以定位文件时处理启动测试失败
#[allow(dead_code)]
pub(crate) fn handle_startup_test_failure_with_files(
    feature: &str,
    test_error: anyhow::Error,
    mut files: Vec<PathBuf>,
) -> Result<()> {
    let mut current_error = test_error;

    // 使用循环迭代处理文件，避免深度递归
    'outer: loop {
        if files.is_empty() {
            // 没有要处理的文件，返回当前错误
            return Err(current_error).context("No files found to fix");
        }

        println!("│");
        println!(
            "│ {}",
            format!("Found {} file(s) in error message:", files.len()).bright_cyan()
        );
        for (idx, file) in files.iter().enumerate() {
            println!("│   {}. {}", idx + 1, file.display());
        }

        // 处理第一个文件（每次外层循环迭代只处理一个文件）
        let file = &files[0];
        println!("│");
        let file_display_name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        println!(
            "│ {}",
            format!("═══ Processing file: {} ═══", file_display_name)
                .bright_cyan()
                .bold()
        );

        // 从文件主干提取文件类型（var_ 或 fun_）
        let file_stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid file stem")?;

        let (file_type, _) = file_scanner::extract_file_type(file_stem).context(format!(
            "Could not extract file type from filename: {}",
            file_display_name
        ))?;

        // 显示 C 和 Rust 代码
        let c_file = file.with_extension("c");

        if c_file.exists() {
            interaction::display_file_paths(Some(&c_file), file);

            println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
            translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);
        } else {
            interaction::display_file_paths(None, file);
        }

        println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
        translator::display_code(file, "─ Rust Code ─", usize::MAX, true);

        println!("│ {}", "═══ Test Error ═══".bright_red().bold());
        println!("│ {}", current_error);

        // 提供与 handle_max_fix_attempts_reached 相同的选择
        let choice = interaction::prompt_user_choice("Initial test failure", false)?;

        match choice {
            interaction::UserChoice::Continue => {
                println!("│");
                println!(
                    "│ {}",
                    "You chose: Continue trying with a new suggestion".bright_cyan()
                );

                // 在提示新建议之前清除旧建议
                suggestion::clear_suggestions()?;

                // 从用户获取可选建议
                if let Some(suggestion_text) = interaction::prompt_suggestion(false)? {
                    // 将建议保存到 suggestions.txt
                    suggestion::append_suggestion(&suggestion_text)?;
                }

                // 应用带有建议的修复
                let format_progress = |op: &str| format!("Fix startup test failure - {}", op);
                crate::apply_error_fix(
                    feature,
                    file_type,
                    file,
                    &current_error,
                    &format_progress,
                    true,
                )?;

                // 再次尝试构建和测试
                println!("│");
                println!(
                    "│ {}",
                    "Running full build and test flow...".bright_blue().bold()
                );
                match builder::run_full_build_and_test(feature) {
                    Ok(_) => {
                        // 全部通过，停止进一步的错误处理
                        return Ok(());
                    }
                    Err(e) => {
                        println!(
                            "│ {}",
                            "✗ Build or tests still failing after fix attempt".red()
                        );

                        // 尝试解析新错误并查看是否有更多文件
                        match parse_error_for_files(&e.to_string(), feature) {
                            Ok(new_files) if !new_files.is_empty() => {
                                println!(
                                    "│ {}",
                                    "Found additional files in new error, will process them..."
                                        .yellow()
                                );
                                // 更新文件和错误以进行下一次迭代
                                files = new_files;
                                current_error = e;
                                continue; // 重新开始循环以处理新文件
                            }
                            _ => {
                                // 没有更多文件需要处理，返回错误
                                return Err(e).context("Build or tests failed after fix attempt");
                            }
                        }
                    }
                }
            }
            interaction::UserChoice::ManualFix => {
                println!("│");
                println!("│ {}", "You chose: Manual fix".bright_cyan());

                // 尝试打开 vim
                match interaction::open_in_vim(file) {
                    Ok(_) => {
                        // Vim 编辑后，重复尝试构建和测试
                        loop {
                            println!("│");
                            println!(
                                "│ {}",
                                "Vim editing completed. Running full build and test flow..."
                                    .bright_blue()
                            );

                            // 手动编辑后执行完整构建流程
                            match builder::run_full_build_and_test_interactive(
                                feature, file_type, file,
                            ) {
                                Ok(_) => {
                                    // 全部通过，成功退出
                                    return Ok(());
                                }
                                Err(e) => {
                                    println!(
                                        "│ {}",
                                        "✗ Build or tests still failing after manual fix".red()
                                    );

                                    // 尝试解析新错误并查看是否有更多文件
                                    match parse_error_for_files(&e.to_string(), feature) {
                                        Ok(new_files) if !new_files.is_empty() => {
                                            println!("│ {}", "Found additional files in new error, will process them...".yellow());
                                            // 更新文件和错误以进行下一次外层迭代
                                            files = new_files;
                                            current_error = e;
                                            continue 'outer; // 重新开始外层循环处理新文件
                                        }
                                        _ => {
                                            // 没有更多文件需要处理，询问用户是否想再试一次
                                            println!("│");
                                            println!("│ {}", "Build or tests still have errors. What would you like to do?".yellow());
                                            let retry_choice = interaction::prompt_user_choice(
                                                "Build/tests still failing",
                                                false,
                                            )?;

                                            match retry_choice {
                                                interaction::UserChoice::Continue => {
                                                    // 只需使用现有更改重试构建
                                                    continue;
                                                }
                                                interaction::UserChoice::ManualFix => {
                                                    println!("│ {}", "Reopening file in Vim for additional manual fixes...".bright_blue());
                                                    match interaction::open_in_vim(file) {
                                                        Ok(_) => {
                                                            // 循环将重试构建
                                                            continue;
                                                        }
                                                        Err(open_err) => {
                                                            println!(
                                                                "│ {}",
                                                                format!(
                                                                    "Failed to reopen vim: {}",
                                                                    open_err
                                                                )
                                                                .red()
                                                            );
                                                            return Err(open_err).context(format!(
                                                                "Build/tests still failing and could not reopen vim for file {}",
                                                                file.display()
                                                            ));
                                                        }
                                                    }
                                                }
                                                interaction::UserChoice::Exit => {
                                                    return Err(e).context(format!(
                                                        "Build/tests failed after manual fix for file {}",
                                                        file.display()
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("│ {}", format!("Failed to open vim: {}", e).red());
                        return Err(e).context(format!(
                            "Initial test failed and could not open vim for file {}",
                            file.display()
                        ));
                    }
                }
            }
            interaction::UserChoice::Exit => {
                println!("│");
                println!("│ {}", "You chose: Exit".yellow());
                return Err(current_error)
                    .context("User chose to exit during startup test failure handling");
            }
        }
    } // 循环结束
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII guard that restores the process working directory when dropped.
    /// Ensures CWD is restored even if the test panics.
    struct DirGuard {
        original: std::path::PathBuf,
    }
    impl DirGuard {
        fn new() -> Self {
            Self {
                original: std::env::current_dir().expect("Failed to get current dir"),
            }
        }
    }
    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn test_parse_error_pattern_extraction() {
        // 测试我们可以从错误消息中提取文件路径
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope";

        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re
            .captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"src/var_test.rs".to_string()));
        assert!(matches.contains(&"src/fun_helper.rs".to_string()));
    }

    #[test]
    fn test_parse_error_pattern_warnings() {
        // 测试我们也可以从警告中提取文件路径
        let error_msg = "warning: unused variable: `x`
  --> src/var_counter.rs:5:9
   |
5  |     let x = 42;
   |         ^ help: if this is intentional, prefix it with an underscore: `_x`";

        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re
            .captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "src/var_counter.rs");
    }

    #[test]
    fn test_parse_error_multiple_files_same_error() {
        // 测试单个错误中的多个文件引用
        let error_msg = "error[E0308]: mismatched types
  --> src/var_main.rs:15:10
   |
15 |     foo(x);
   |          ^ expected `String`, found `i32`
   |
note: expected signature from here
  --> src/fun_foo.rs:3:1
   |
3  | fn foo(s: String) { }
   | ^^^^^^^^^^^^^^^^^";

        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re
            .captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        // 应该找到两个文件
        assert!(matches.len() >= 2);
        assert!(matches.contains(&"src/var_main.rs".to_string()));
        assert!(matches.contains(&"src/fun_foo.rs".to_string()));
    }

    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_with_real_directory() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;

        // 创建临时目录结构
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();

        // 将当前目录设置为临时目录，以便 find_project_root 可以工作
        let _guard = DirGuard::new();
        env::set_current_dir(project_root).unwrap();

        // 创建特性目录结构
        let feature = "test_feature";
        let c2rust_dir = project_root.join(".c2rust");
        fs::create_dir_all(&c2rust_dir).unwrap();

        let feature_dir = c2rust_dir.join(feature);
        let rust_dir = feature_dir.join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        // 创建测试文件
        let test_file1 = src_dir.join("var_test.rs");
        fs::write(&test_file1, "// test content").unwrap();

        let test_file2 = src_dir.join("fun_helper.rs");
        fs::write(&test_file2, "// helper content").unwrap();

        // 创建应被过滤掉的 rust 目录外的文件
        let outside_file = c2rust_dir.join("outside.rs");
        fs::write(&outside_file, "// outside").unwrap();

        // 测试包含多个文件的错误消息
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope
   
note: some note about outside file
  --> ../../outside.rs:1:1";

        let result = parse_error_for_files(error_msg, feature).unwrap();

        // 应该准确找到 2 个文件（不包括 outside.rs）
        assert_eq!(result.len(), 2);

        // 检查两个文件都存在且是规范化的
        let canonical_file1 = test_file1.canonicalize().unwrap();
        let canonical_file2 = test_file2.canonicalize().unwrap();

        assert!(
            result.contains(&canonical_file1),
            "Should contain var_test.rs"
        );
        assert!(
            result.contains(&canonical_file2),
            "Should contain fun_helper.rs"
        );

        // 验证文件已排序
        assert!(result[0] < result[1], "Files should be sorted");
    }

    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_deduplication() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();

        let _guard = DirGuard::new();
        env::set_current_dir(project_root).unwrap();

        fs::create_dir(project_root.join(".git")).unwrap();

        let feature = "test_feature";
        let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let test_file = src_dir.join("var_test.rs");
        fs::write(&test_file, "// test").unwrap();

        // 多次提及同一文件的错误消息
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    
error[E0308]: another error
   --> src/var_test.rs:15:5
    
note: note about same file
   --> src/var_test.rs:20:1";

        let result = parse_error_for_files(error_msg, feature).unwrap();

        // 尽管多次提及，但应该只有 1 个文件
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("var_test.rs"));
    }

    #[test]
    fn test_parse_error_for_files_validates_feature_name() {
        // 测试无效的特性名称被拒绝
        let error_msg = "error: --> src/test.rs:1:1";

        // 带有路径遍历的特性名称应该失败
        let result = parse_error_for_files(error_msg, "../bad");
        assert!(result.is_err(), "Should reject feature name with ..");

        let result = parse_error_for_files(error_msg, "good/bad");
        assert!(result.is_err(), "Should reject feature name with /");
    }

    #[test]
    fn test_extract_error_for_file_single_file() {
        let error_msg = "error[E0308]: mismatched types\n   --> src/var_test.rs:10:5\n    |\n10  |     let x: i32 = \"hello\";\n    |     ^^^^^^ expected `i32`, found `&str`\n\nerror[E0425]: cannot find value `y` in this scope\n  --> src/fun_helper.rs:20:9\n   |\n20 |         y\n   |         ^ not found in this scope";

        let file_path = std::path::Path::new("src/var_test.rs");
        let result = extract_error_for_file(error_msg, file_path);

        assert!(result.contains("var_test.rs"), "Should contain the target file reference");
        assert!(result.contains("E0308"), "Should contain the error for var_test.rs");
        assert!(!result.contains("fun_helper.rs"), "Should not contain the other file");
    }

    #[test]
    fn test_extract_error_for_file_fallback_on_no_match() {
        let error_msg = "error[E0308]: mismatched types\n   --> src/var_test.rs:10:5\n    |\n10  |     let x: i32 = \"hello\";";

        // File that doesn't appear in the error
        let file_path = std::path::Path::new("src/other_file.rs");
        let result = extract_error_for_file(error_msg, file_path);

        // Should return the full error message as fallback (with normalized line endings)
        let normalized = error_msg.replace("\r\n", "\n").replace('\r', "\n");
        assert_eq!(result, normalized, "Should return full error when no match found");
    }

    #[test]
    fn test_extract_error_for_file_empty_filename() {
        let error_msg = "some error message";
        let file_path = std::path::Path::new("");
        let result = extract_error_for_file(error_msg, file_path);
        // Empty path yields no components → returns normalized full message
        let normalized = error_msg.replace("\r\n", "\n").replace('\r', "\n");
        assert_eq!(result, normalized, "Should return full error for empty path");
    }

    #[test]
    fn test_extract_error_for_file_crlf_normalization() {
        // Simulate Windows CRLF line endings in compiler output
        let error_msg = "error[E0308]: mismatched types\r\n   --> src/var_test.rs:10:5\r\n    |\r\n\r\nerror[E0425]: cannot find value `y`\r\n  --> src/fun_helper.rs:20:9";

        let file_path = std::path::Path::new("src/var_test.rs");
        let result = extract_error_for_file(error_msg, file_path);

        assert!(result.contains("var_test.rs"), "Should find file even with CRLF line endings");
        assert!(!result.contains("fun_helper.rs"), "Should not contain other file");
        assert!(!result.contains('\r'), "Result should not contain carriage returns");
    }

    #[test]
    fn test_extract_error_for_file_path_suffix_matching() {
        // Two files with the same basename in different directories
        let error_msg = "error[E0425]: undeclared\n   --> src/a/mod.rs:5:3\n    |\n5   |   foo();\n\nerror[E0308]: mismatch\n   --> src/b/mod.rs:10:5\n    |\n10  |   bar();";

        // Match the file in src/a/
        let file_a = std::path::Path::new("src/a/mod.rs");
        let result_a = extract_error_for_file(error_msg, file_a);
        assert!(result_a.contains("a/mod.rs"), "Should match src/a/mod.rs");
        assert!(!result_a.contains("b/mod.rs"), "Should not match src/b/mod.rs");

        // Match the file in src/b/
        let file_b = std::path::Path::new("src/b/mod.rs");
        let result_b = extract_error_for_file(error_msg, file_b);
        assert!(result_b.contains("b/mod.rs"), "Should match src/b/mod.rs");
        assert!(!result_b.contains("a/mod.rs"), "Should not match src/a/mod.rs");
    }

    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_ordered_preserves_order() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();

        let _guard = DirGuard::new();
        env::set_current_dir(project_root).unwrap();

        let feature = "test_feature_order";
        let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        // 创建测试文件 (fun_helper.rs 出现在 var_test.rs 之前)
        let file_a = src_dir.join("fun_helper.rs");
        fs::write(&file_a, "// helper").unwrap();

        let file_b = src_dir.join("var_test.rs");
        fs::write(&file_b, "// test").unwrap();

        // fun_helper.rs 先出现，var_test.rs 后出现
        let error_msg = "error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope

error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`";

        let result = parse_error_for_files_ordered(error_msg, feature).unwrap();

        assert_eq!(result.len(), 2, "Should find 2 files");
        // fun_helper.rs 应该排在第一位（首先出现）
        assert!(result[0].ends_with("fun_helper.rs"), "fun_helper.rs should be first");
        assert!(result[1].ends_with("var_test.rs"), "var_test.rs should be second");
    }

    #[test]
    fn test_parse_error_for_files_ordered_validates_feature_name() {
        let error_msg = "error: --> src/test.rs:1:1";

        let result = parse_error_for_files_ordered(error_msg, "../bad");
        assert!(result.is_err(), "Should reject feature name with ..");
    }
}

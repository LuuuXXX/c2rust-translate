//! 功能合并模块
//!
//! 将一个 feature 中分散的 Rust 文件（var_*.rs 和 fun_*.rs）合并为单个文件。
//!
//! # 关于 `use core::ffi::*;` 的说明
//!
//! 在 C 到 Rust 的翻译中，`use core::ffi::*;` 用于引入 C FFI 类型（如 `c_int`、`c_char`、
//! `c_void` 等）。合并时必须保留此导入，绝不能因为"重复"或"被其他导入覆盖"而删除。
//!
//! 本模块的去重策略：仅通过字符串完全匹配去重（使用 `BTreeSet`），不做任何
//! 语义层面的"冗余分析"，从而确保 `use core::ffi::*;` 及所有通配符导入都能被保留。

use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ============================================================================
// Public API
// ============================================================================

/// 将 feature 中所有已翻译的 Rust 文件合并到单个文件。
///
/// 合并步骤：
/// 1. 扫描 `.c2rust/<feature>/rust/src/` 下所有非空的 `var_*.rs` / `fun_*.rs` 文件
/// 2. 从每个文件中提取 `use` 语句和代码正文
/// 3. **去重 `use` 语句（仅精确字符串匹配）**，保留所有唯一导入（包括 `use core::ffi::*;`）
/// 4. 将代码正文按原顺序拼接
/// 5. 写入输出文件
///
/// # 关于 `use core::ffi::*;` 保留的说明
///
/// 去重时仅进行精确字符串匹配，不做任何冗余分析，因此 `use core::ffi::*;`
/// 无论出现多少次，最终都会在输出中保留恰好一次。
///
/// # 参数
/// - `feature`: feature 名称
/// - `output_file`: 可选的输出路径；若为 `None` 则输出到 `.c2rust/<feature>/merged.rs`
pub fn merge_feature(feature: &str, output_file: Option<&Path>) -> Result<()> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let feature_rust_src_dir = project_root
        .join(".c2rust")
        .join(feature)
        .join("rust")
        .join("src");

    if !feature_rust_src_dir.exists() {
        anyhow::bail!(
            "Feature rust src directory not found: {}",
            feature_rust_src_dir.display()
        );
    }

    println!(
        "{}",
        format!("Merging Rust files for feature: {}", feature).bright_blue()
    );

    // 收集所有非空的 var_*.rs / fun_*.rs 文件
    let rs_files = collect_translatable_rs_files(&feature_rust_src_dir)?;

    if rs_files.is_empty() {
        println!("{}", "No translated Rust files found to merge.".yellow());
        return Ok(());
    }

    println!(
        "{}",
        format!("Found {} file(s) to merge", rs_files.len()).bright_blue()
    );

    // 从每个文件中提取 use 语句和代码正文
    let mut use_statements: BTreeSet<String> = BTreeSet::new();
    let mut code_bodies: Vec<String> = Vec::new();

    for file in &rs_files {
        let content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read {}", file.display()))?;

        let (uses, body) = split_use_and_code(&content);

        // 仅精确字符串匹配去重：保留所有唯一导入（包括 use core::ffi::*;）
        for use_stmt in uses {
            use_statements.insert(use_stmt);
        }

        if !body.trim().is_empty() {
            code_bodies.push(body);
        }
    }

    // 构建合并内容
    let merged = build_merged_content(&use_statements, &code_bodies);

    // 确定输出路径
    let output_path = if let Some(path) = output_file {
        path.to_path_buf()
    } else {
        project_root
            .join(".c2rust")
            .join(feature)
            .join("merged.rs")
    };

    // 确保父目录存在
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    fs::write(&output_path, &merged)
        .with_context(|| format!("Failed to write merged file to {}", output_path.display()))?;

    println!(
        "{}",
        format!(
            "✓ Merged {} file(s) into {}",
            rs_files.len(),
            output_path.display()
        )
        .bright_green()
    );

    Ok(())
}

// ============================================================================
// Internal helpers
// ============================================================================

/// 收集 `src_dir` 下所有非空的 `var_*.rs` / `fun_*.rs` 文件（排序后返回）
fn collect_translatable_rs_files(src_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();

    for entry in WalkDir::new(src_dir) {
        let entry = entry.with_context(|| {
            format!("Failed to walk directory {}", src_dir.display())
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };

        if !stem.starts_with("var_") && !stem.starts_with("fun_") {
            continue;
        }

        // 跳过空文件（尚未翻译）
        let len = fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?
            .len();
        if len == 0 {
            continue;
        }

        files.push(path.to_path_buf());
    }

    files.sort();
    Ok(files)
}

/// 将 Rust 文件内容拆分为 (use 语句列表, 代码正文)。
///
/// 解析策略：
/// - 文件开头连续的 `use ...;`、空行、行注释被视为"use 区域"
/// - 一旦遇到非以上形式的行，即视为代码正文的开始，其后所有内容（含 `use`）均属于正文
///
/// 这样可以避免将函数体内的局部 `use` 当作顶层导入来处理。
pub(crate) fn split_use_and_code(content: &str) -> (Vec<String>, String) {
    let mut uses: Vec<String> = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_use_section = true;

    for line in content.lines() {
        let trimmed = line.trim();

        if in_use_section {
            if trimmed.starts_with("use ") && trimmed.ends_with(';') {
                // 顶层单行 use 语句
                uses.push(trimmed.to_string());
                continue;
            } else if trimmed.is_empty() || trimmed.starts_with("//") {
                // 空行或行注释：仍在 use 区域内，跳过
                continue;
            } else {
                // 遇到非 use/空行/注释的行：use 区域结束
                in_use_section = false;
            }
        }

        body_lines.push(line);
    }

    (uses, body_lines.join("\n"))
}

/// 将去重后的 `use` 语句集合和代码正文列表组装为合并文件内容。
fn build_merged_content(use_statements: &BTreeSet<String>, code_bodies: &[String]) -> String {
    let mut merged = String::new();

    // 输出所有唯一的 use 语句（BTreeSet 保证字典序）
    for use_stmt in use_statements {
        merged.push_str(use_stmt);
        merged.push('\n');
    }

    if !use_statements.is_empty() {
        merged.push('\n');
    }

    // 按原顺序输出代码正文
    for body in code_bodies {
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            merged.push_str(trimmed);
            merged.push_str("\n\n");
        }
    }

    merged
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // split_use_and_code
    // -----------------------------------------------------------------------

    #[test]
    fn test_split_use_and_code_basic() {
        let content = "use std::ffi::CStr;\nuse core::ffi::*;\n\nfn foo() {}\n";
        let (uses, body) = split_use_and_code(content);
        assert_eq!(uses, vec!["use std::ffi::CStr;", "use core::ffi::*;"]);
        assert!(body.contains("fn foo()"));
        assert!(!body.contains("use "));
    }

    /// `use core::ffi::*;` 必须出现在 use 列表中，绝不能被过滤掉。
    #[test]
    fn test_split_use_and_code_preserves_core_ffi_wildcard() {
        let content = "use core::ffi::*;\nuse std::os::raw::*;\n\nstatic X: i32 = 0;\n";
        let (uses, _body) = split_use_and_code(content);
        assert!(
            uses.contains(&"use core::ffi::*;".to_string()),
            "use core::ffi::*; must be preserved"
        );
        assert!(
            uses.contains(&"use std::os::raw::*;".to_string()),
            "use std::os::raw::*; must be preserved"
        );
    }

    /// 文件开头注释和空行不影响 use 提取。
    #[test]
    fn test_split_use_and_code_skips_leading_comments() {
        let content = "// generated\n\nuse core::ffi::*;\n\nfn bar() {}\n";
        let (uses, body) = split_use_and_code(content);
        assert_eq!(uses, vec!["use core::ffi::*;"]);
        assert!(body.contains("fn bar()"));
    }

    /// 函数体内的 use（在非 use 区域之后）不应被提取为顶层导入。
    #[test]
    fn test_split_use_and_code_does_not_extract_inline_use() {
        let content = "use core::ffi::*;\n\nfn foo() {\n    use std::mem;\n    let _ = mem::size_of::<i32>();\n}\n";
        let (uses, body) = split_use_and_code(content);
        // 只有顶层的 use core::ffi::*; 被提取
        assert_eq!(uses, vec!["use core::ffi::*;"]);
        // 函数体内的 use std::mem; 留在正文中
        assert!(body.contains("use std::mem;"));
    }

    // -----------------------------------------------------------------------
    // build_merged_content
    // -----------------------------------------------------------------------

    /// 合并后的内容中必须包含 `use core::ffi::*;`（即使来自多个文件的相同语句被去重）。
    #[test]
    fn test_build_merged_content_preserves_core_ffi_wildcard() {
        let mut use_stmts = BTreeSet::new();
        use_stmts.insert("use core::ffi::*;".to_string());
        use_stmts.insert("use std::ffi::CStr;".to_string());

        let bodies = vec!["fn foo() {}".to_string()];
        let merged = build_merged_content(&use_stmts, &bodies);

        assert!(
            merged.contains("use core::ffi::*;"),
            "merged output must contain 'use core::ffi::*;'"
        );
        assert!(merged.contains("use std::ffi::CStr;"));
        assert!(merged.contains("fn foo()"));
    }

    /// 完全相同的 use 语句只应出现一次（去重）。
    #[test]
    fn test_build_merged_content_deduplicates_exact_duplicates() {
        let mut use_stmts = BTreeSet::new();
        use_stmts.insert("use core::ffi::*;".to_string());

        let bodies = vec!["fn a() {}".to_string(), "fn b() {}".to_string()];
        let merged = build_merged_content(&use_stmts, &bodies);

        // "use core::ffi::*;" 只出现一次
        let occurrences = merged.matches("use core::ffi::*;").count();
        assert_eq!(occurrences, 1, "duplicate use statements must be deduplicated");
    }

    /// 当没有任何 use 语句时，输出不应有多余的空行前缀。
    #[test]
    fn test_build_merged_content_no_use_section() {
        let use_stmts: BTreeSet<String> = BTreeSet::new();
        let bodies = vec!["fn foo() {}".to_string()];
        let merged = build_merged_content(&use_stmts, &bodies);
        assert!(!merged.starts_with('\n'));
        assert!(merged.contains("fn foo()"));
    }

    // -----------------------------------------------------------------------
    // merge_feature (integration-level, uses temp filesystem)
    // -----------------------------------------------------------------------

    #[test]
    #[serial_test::serial]
    fn test_merge_feature_preserves_core_ffi_wildcard() {
        use std::env;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();

        // 创建项目结构
        let feature = "test_merge";
        let rust_src_dir = project_root
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src");
        fs::create_dir_all(&rust_src_dir).unwrap();
        // 需要 .git 以便 find_project_root 能工作
        fs::create_dir(project_root.join(".git")).unwrap();

        // 写入两个带 use core::ffi::*; 的翻译文件
        fs::write(
            rust_src_dir.join("var_foo.rs"),
            "use core::ffi::*;\nuse std::ffi::CStr;\n\npub static FOO: i32 = 42;\n",
        )
        .unwrap();
        fs::write(
            rust_src_dir.join("fun_bar.rs"),
            "use core::ffi::*;\n\npub fn bar() -> c_int { 0 }\n",
        )
        .unwrap();

        let output_path = temp_dir.path().join("out.rs");
        let result = merge_feature(feature, Some(&output_path));
        env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok(), "merge_feature failed: {:?}", result);

        let merged_content = fs::read_to_string(&output_path).unwrap();

        // use core::ffi::*; 必须恰好出现一次（去重后保留）
        let occurrences = merged_content.matches("use core::ffi::*;").count();
        assert_eq!(
            occurrences, 1,
            "use core::ffi::*; must appear exactly once after dedup, got:\n{}",
            merged_content
        );

        // 其他内容必须保留
        assert!(merged_content.contains("use std::ffi::CStr;"));
        assert!(merged_content.contains("FOO"));
        assert!(merged_content.contains("fn bar()"));
    }

    #[test]
    #[serial_test::serial]
    fn test_merge_feature_skips_empty_files() {
        use std::env;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();

        let feature = "test_merge_empty";
        let rust_src_dir = project_root
            .join(".c2rust")
            .join(feature)
            .join("rust")
            .join("src");
        fs::create_dir_all(&rust_src_dir).unwrap();
        fs::create_dir(project_root.join(".git")).unwrap();

        // 一个有内容，一个空
        fs::write(
            rust_src_dir.join("var_a.rs"),
            "use core::ffi::*;\n\npub static A: i32 = 1;\n",
        )
        .unwrap();
        fs::write(rust_src_dir.join("fun_b.rs"), "").unwrap();

        let output_path = temp_dir.path().join("out_skip.rs");
        let result = merge_feature(feature, Some(&output_path));
        env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok(), "merge_feature failed: {:?}", result);

        let content = fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("use core::ffi::*;"));
        assert!(content.contains("static A"));
        // 空文件的内容不应出现
        assert!(!content.contains("fun_b"));
    }
}

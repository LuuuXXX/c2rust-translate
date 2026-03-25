use crate::{file_scanner, util};
use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// Use-declaration helpers
// ============================================================================

/// 从 use 声明中提取引入的名称。
///
/// 例如：
/// - `use std::ffi::c_int;` → `Some("c_int")`
/// - `use std::ffi::c_int as my_int;` → `Some("my_int")`
/// - `use std::io::{Read, Write};` → `None`（复合导入，不做简单名称提取）
/// - `use module::*;` → `None`（glob 导入）
fn extract_use_name(use_decl: &str) -> Option<String> {
    // 去掉行首的 pub/pub(crate) 修饰符，再去掉 "use " 前缀和行尾分号
    let stripped = use_decl
        .trim()
        .trim_start_matches("pub(crate) ")
        .trim_start_matches("pub ")
        .trim_start_matches("use ")
        .trim_end_matches(';')
        .trim();

    // 复合导入（含 `{`）或 glob 导入（含 `*`）不处理
    if stripped.contains('{') || stripped.contains('}') || stripped.contains('*') {
        return None;
    }

    // 处理 "as" 别名：取 as 后面的名称
    if let Some(pos) = stripped.find(" as ") {
        return Some(stripped[pos + 4..].trim().to_string());
    }

    // 取路径的最后一段
    if let Some(pos) = stripped.rfind("::") {
        Some(stripped[pos + 2..].trim().to_string())
    } else {
        Some(stripped.to_string())
    }
}

/// 判断 `candidate` 是否比 `existing` 更优先（用于 C FFI 类型的路径选择）。
///
/// 对于 `c_int` 等 C FFI 类型，优先顺序为：
/// 1. `std::ffi` —— Rust 1.64+ 推荐路径
/// 2. `core::ffi` —— no_std 兼容路径
/// 3. `std::os::raw` —— 已弃用但仍有效
/// 4. 其他（如 `libc::`）
fn is_preferred_over(candidate: &str, existing: &str) -> bool {
    fn priority(decl: &str) -> u8 {
        if decl.contains("std::ffi::") {
            0
        } else if decl.contains("core::ffi::") {
            1
        } else if decl.contains("std::os::raw::") {
            2
        } else {
            3
        }
    }
    priority(candidate) < priority(existing)
}

/// 将 use 声明行解析为规范形式（去除尾部分号和多余空格）后用作去重键。
fn canonical_use_key(use_decl: &str) -> String {
    use_decl
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string()
}

// ============================================================================
// File parsing
// ============================================================================

/// 将 .rs 文件内容分割为 (use 声明列表, 其余行列表)。
///
/// 只有位于文件顶部连续的 `use` 块（包含空行）才被视为 use 声明区；
/// 第一个非 use/空行之后的 `use` 语句被视为内容的一部分，原样保留。
fn split_use_and_content(source: &str) -> (Vec<String>, Vec<String>) {
    let mut use_decls: Vec<String> = Vec::new();
    let mut content_lines: Vec<String> = Vec::new();
    let mut in_use_block = true;

    for line in source.lines() {
        let trimmed = line.trim();

        if in_use_block {
            if trimmed.is_empty() {
                // 保留 use 块内部的空行，但先暂存，等到确认还在 use 区时再入队
                use_decls.push(String::new());
            } else if trimmed.starts_with("use ")
                || trimmed.starts_with("pub use ")
                || trimmed.starts_with("pub(crate) use ")
            {
                use_decls.push(line.to_string());
            } else {
                // 第一个非 use/空行：退出 use 块
                in_use_block = false;
                content_lines.push(line.to_string());
            }
        } else {
            content_lines.push(line.to_string());
        }
    }

    // 去掉 use 声明末尾多余的空行
    while use_decls.last().map(|l: &String| l.trim().is_empty()).unwrap_or(false) {
        use_decls.pop();
    }

    (use_decls, content_lines)
}

// ============================================================================
// Core merge logic
// ============================================================================

/// 合并结果：包含汇总后的 use 声明和合并后的正文内容
struct MergeResult {
    use_declarations: Vec<String>,
    merged_body: String,
}

/// 将多个 .rs 文件合并为一个。
///
/// 算法：
/// 1. 解析每个文件，提取 use 声明与正文内容。
/// 2. 汇总 use 声明：
///    - 对于可提取出单一名称的简单 use，按名称去重，并按优先级
///      保留最规范的路径（`std::ffi` > `core::ffi` > `std::os::raw` > 其他）。
///    - 对于复合/glob 导入，按精确字符串去重后全部保留。
/// 3. 将每个文件的正文（非 use 内容）追加到合并体中，以文件名注释分隔。
fn merge_files(files: &[(PathBuf, String)]) -> MergeResult {
    // 简单 use：名称 -> (声明, 精确键)
    let mut named_uses: HashMap<String, (String, String)> = HashMap::new();
    // 复合/glob use：精确键 set（保持插入顺序用 Vec）
    let mut complex_uses: Vec<String> = Vec::new();
    let mut complex_use_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut body_parts: Vec<String> = Vec::new();

    for (path, source) in files {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let (use_decls, content_lines) = split_use_and_content(source);

        for decl in &use_decls {
            if decl.trim().is_empty() {
                continue;
            }
            let key = canonical_use_key(decl);
            if let Some(name) = extract_use_name(decl) {
                // 简单 use：按名称去重，按优先级保留
                if let Some((existing_decl, _existing_key)) = named_uses.get(&name) {
                    if is_preferred_over(decl, existing_decl) {
                        named_uses.insert(name, (decl.trim().to_string(), key));
                    }
                } else {
                    named_uses.insert(name, (decl.trim().to_string(), key));
                }
            } else {
                // 复合/glob use：精确字符串去重
                if !complex_use_keys.contains(&key) {
                    complex_uses.push(decl.trim().to_string());
                    complex_use_keys.insert(key);
                }
            }
        }

        // 追加正文，用注释标注来源文件
        if !content_lines.is_empty() {
            let block = format!(
                "// ─── {} ───\n{}",
                file_name,
                content_lines.join("\n")
            );
            body_parts.push(block);
        }
    }

    // 对简单 use 按声明字符串排序，使输出稳定
    let mut sorted_named: Vec<String> = named_uses
        .into_values()
        .map(|(decl, _)| decl)
        .collect();
    sorted_named.sort();

    let mut use_declarations = sorted_named;
    use_declarations.extend(complex_uses);

    let merged_body = body_parts.join("\n\n");

    MergeResult {
        use_declarations,
        merged_body,
    }
}

// ============================================================================
// Public API
// ============================================================================

/// 将某个 feature 下已翻译的所有 .rs 文件合并为单一文件。
///
/// 输入文件为 `.c2rust/<feature>/rust/src/` 目录下以 `var_` 或 `fun_` 开头的
/// 非空 .rs 文件。
///
/// 合并时正确处理 `use` 声明（包括 `c_int` 等 C FFI 类型导入）：
/// - 按名称去重，不会丢弃声明
/// - 多个文件若以不同路径引入同一类型（如 `std::ffi::c_int` 与
///   `std::os::raw::c_int`），保留最规范的路径，不会将二者全部丢弃
///
/// # 参数
/// - `feature`: feature 名称
/// - `output_path`: 合并输出文件路径；若为 `None`，默认写入
///   `.c2rust/<feature>/rust/src/merged.rs`
pub fn merge_feature(feature: &str, output_path: Option<&Path>) -> Result<PathBuf> {
    util::validate_feature_name(feature)?;

    let project_root = util::find_project_root()?;
    let rust_src_dir = project_root
        .join(".c2rust")
        .join(feature)
        .join("rust")
        .join("src");

    if !rust_src_dir.exists() {
        anyhow::bail!(
            "Source directory does not exist: {}. Has this feature been translated yet?",
            rust_src_dir.display()
        );
    }

    // 收集所有已翻译（非空）的 var_/fun_ .rs 文件
    let all_rs = file_scanner::find_translated_rs_files(&rust_src_dir)
        .with_context(|| {
            format!(
                "Failed to scan translated .rs files in {}",
                rust_src_dir.display()
            )
        })?;

    if all_rs.is_empty() {
        println!(
            "{}",
            "No translated .rs files found to merge.".bright_yellow()
        );
        anyhow::bail!("No translated .rs files found in {}", rust_src_dir.display());
    }

    println!(
        "{}",
        format!("Found {} file(s) to merge.", all_rs.len())
            .bright_blue()
            .bold()
    );

    // 读取每个文件的内容
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for path in &all_rs {
        let source = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        println!("  → {}", path.display());
        files.push((path.clone(), source));
    }

    // 执行合并
    let result = merge_files(&files);

    // 构造合并输出内容
    let mut output_lines: Vec<String> = Vec::new();

    // use 声明区
    if !result.use_declarations.is_empty() {
        for decl in &result.use_declarations {
            // 确保每行都以分号结尾
            if decl.trim_end().ends_with(';') {
                output_lines.push(decl.clone());
            } else {
                output_lines.push(format!("{};", decl));
            }
        }
        output_lines.push(String::new()); // 空行分隔
    }

    // 合并正文
    output_lines.push(result.merged_body);

    let output_content = output_lines.join("\n");

    // 确定输出路径
    let resolved_output = match output_path {
        Some(p) => p.to_path_buf(),
        None => rust_src_dir.join("merged.rs"),
    };

    fs::write(&resolved_output, &output_content)
        .with_context(|| format!("Failed to write merged output to {}", resolved_output.display()))?;

    println!(
        "{}",
        format!("✓ Merged output written to {}", resolved_output.display())
            .bright_green()
            .bold()
    );

    Ok(resolved_output)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    // ── extract_use_name ──────────────────────────────────────────────────

    #[test]
    fn test_extract_use_name_simple() {
        assert_eq!(
            extract_use_name("use std::ffi::c_int;"),
            Some("c_int".to_string())
        );
    }

    #[test]
    fn test_extract_use_name_std_os_raw() {
        assert_eq!(
            extract_use_name("use std::os::raw::c_int;"),
            Some("c_int".to_string())
        );
    }

    #[test]
    fn test_extract_use_name_core_ffi() {
        assert_eq!(
            extract_use_name("use core::ffi::c_int;"),
            Some("c_int".to_string())
        );
    }

    #[test]
    fn test_extract_use_name_alias() {
        assert_eq!(
            extract_use_name("use std::ffi::c_int as my_int;"),
            Some("my_int".to_string())
        );
    }

    #[test]
    fn test_extract_use_name_pub_use() {
        assert_eq!(
            extract_use_name("pub use std::ffi::c_int;"),
            Some("c_int".to_string())
        );
    }

    #[test]
    fn test_extract_use_name_grouped_returns_none() {
        assert_eq!(extract_use_name("use std::{ffi::c_int, fmt};"), None);
    }

    #[test]
    fn test_extract_use_name_glob_returns_none() {
        assert_eq!(extract_use_name("use std::ffi::*;"), None);
    }

    // ── is_preferred_over ────────────────────────────────────────────────

    #[test]
    fn test_std_ffi_preferred_over_std_os_raw() {
        assert!(is_preferred_over(
            "use std::ffi::c_int;",
            "use std::os::raw::c_int;"
        ));
    }

    #[test]
    fn test_std_ffi_preferred_over_core_ffi() {
        assert!(is_preferred_over(
            "use std::ffi::c_int;",
            "use core::ffi::c_int;"
        ));
    }

    #[test]
    fn test_core_ffi_preferred_over_std_os_raw() {
        assert!(is_preferred_over(
            "use core::ffi::c_int;",
            "use std::os::raw::c_int;"
        ));
    }

    #[test]
    fn test_std_os_raw_not_preferred_over_std_ffi() {
        assert!(!is_preferred_over(
            "use std::os::raw::c_int;",
            "use std::ffi::c_int;"
        ));
    }

    // ── split_use_and_content ─────────────────────────────────────────────

    #[test]
    fn test_split_simple_file() {
        let source = "use std::ffi::c_int;\nuse std::fmt;\n\npub fn foo() -> c_int { 0 }\n";
        let (uses, content) = split_use_and_content(source);
        assert_eq!(uses, vec!["use std::ffi::c_int;", "use std::fmt;"]);
        assert!(content.iter().any(|l| l.contains("pub fn foo")));
    }

    #[test]
    fn test_split_no_use_declarations() {
        let source = "pub fn bar() {}\n";
        let (uses, content) = split_use_and_content(source);
        assert!(uses.is_empty());
        assert!(content.iter().any(|l| l.contains("pub fn bar")));
    }

    #[test]
    fn test_split_only_use_declarations() {
        let source = "use std::ffi::c_int;\nuse std::os::raw::c_uint;\n";
        let (uses, content) = split_use_and_content(source);
        assert_eq!(uses.len(), 2);
        assert!(content.is_empty());
    }

    // ── merge_files ───────────────────────────────────────────────────────

    #[test]
    fn test_merge_preserves_c_int_declaration() {
        let file1 = (
            PathBuf::from("var_counter.rs"),
            "use std::ffi::c_int;\n\npub static COUNTER: c_int = 0;\n".to_string(),
        );
        let file2 = (
            PathBuf::from("fun_get_count.rs"),
            "use std::ffi::c_int;\n\npub fn get_count() -> c_int { 0 }\n".to_string(),
        );

        let result = merge_files(&[file1, file2]);

        // c_int declaration must appear exactly once
        let c_int_count = result
            .use_declarations
            .iter()
            .filter(|d| d.contains("c_int"))
            .count();
        assert_eq!(c_int_count, 1, "c_int should appear exactly once in merged use declarations");

        // Content from both files must be present
        assert!(result.merged_body.contains("COUNTER"));
        assert!(result.merged_body.contains("get_count"));
    }

    #[test]
    fn test_merge_deduplicates_identical_use() {
        let file1 = (
            PathBuf::from("a.rs"),
            "use std::fmt;\n\npub fn foo() {}\n".to_string(),
        );
        let file2 = (
            PathBuf::from("b.rs"),
            "use std::fmt;\n\npub fn bar() {}\n".to_string(),
        );

        let result = merge_files(&[file1, file2]);
        let fmt_count = result
            .use_declarations
            .iter()
            .filter(|d| d.contains("std::fmt"))
            .count();
        assert_eq!(fmt_count, 1);
    }

    #[test]
    fn test_merge_picks_preferred_c_int_path() {
        // File A uses deprecated std::os::raw, File B uses recommended std::ffi
        let file_a = (
            PathBuf::from("a.rs"),
            "use std::os::raw::c_int;\n\npub fn a() -> c_int { 1 }\n".to_string(),
        );
        let file_b = (
            PathBuf::from("b.rs"),
            "use std::ffi::c_int;\n\npub fn b() -> c_int { 2 }\n".to_string(),
        );

        let result = merge_files(&[file_a, file_b]);

        assert_eq!(
            result
                .use_declarations
                .iter()
                .filter(|d| d.contains("c_int"))
                .count(),
            1,
            "exactly one c_int declaration"
        );
        assert!(
            result
                .use_declarations
                .iter()
                .any(|d| d.contains("std::ffi::c_int")),
            "preferred std::ffi path should be kept"
        );
    }

    #[test]
    fn test_merge_picks_preferred_c_int_path_reverse_order() {
        // File A uses recommended std::ffi, File B uses deprecated std::os::raw
        let file_a = (
            PathBuf::from("a.rs"),
            "use std::ffi::c_int;\n\npub fn a() -> c_int { 1 }\n".to_string(),
        );
        let file_b = (
            PathBuf::from("b.rs"),
            "use std::os::raw::c_int;\n\npub fn b() -> c_int { 2 }\n".to_string(),
        );

        let result = merge_files(&[file_a, file_b]);

        assert_eq!(
            result
                .use_declarations
                .iter()
                .filter(|d| d.contains("c_int"))
                .count(),
            1
        );
        assert!(result
            .use_declarations
            .iter()
            .any(|d| d.contains("std::ffi::c_int")));
    }

    #[test]
    fn test_merge_no_c_int_lost_when_only_one_path() {
        // All files use core::ffi::c_int — it must be preserved
        let file1 = (
            PathBuf::from("a.rs"),
            "use core::ffi::c_int;\n\npub fn a() -> c_int { 0 }\n".to_string(),
        );
        let file2 = (
            PathBuf::from("b.rs"),
            "use core::ffi::c_int;\n\npub fn b() -> c_int { 1 }\n".to_string(),
        );

        let result = merge_files(&[file1, file2]);
        assert!(
            result
                .use_declarations
                .iter()
                .any(|d| d.contains("c_int")),
            "c_int declaration must not be dropped"
        );
    }

    #[test]
    fn test_merge_complex_uses_preserved() {
        let file1 = (
            PathBuf::from("a.rs"),
            "use std::{ffi::c_int, fmt};\n\npub fn a() {}\n".to_string(),
        );
        let file2 = (
            PathBuf::from("b.rs"),
            "use std::{ffi::c_int, fmt};\n\npub fn b() {}\n".to_string(),
        );

        let result = merge_files(&[file1, file2]);
        let complex_count = result
            .use_declarations
            .iter()
            .filter(|d| d.contains("std::{"))
            .count();
        assert_eq!(complex_count, 1, "identical complex use should be deduplicated");
    }

    // ── merge_feature (integration) ───────────────────────────────────────

    #[test]
    fn test_merge_feature_writes_output_file() {
        use std::env;

        let temp = tempdir().unwrap();
        let project_root = temp.path();

        // Set up .c2rust directory structure
        let c2rust_dir = project_root.join(".c2rust");
        let rust_src = c2rust_dir.join("default").join("rust").join("src");
        fs::create_dir_all(&rust_src).unwrap();

        // Create two translated files
        let mut f1 = fs::File::create(rust_src.join("var_x.rs")).unwrap();
        writeln!(f1, "use std::ffi::c_int;").unwrap();
        writeln!(f1, "").unwrap();
        writeln!(f1, "pub static X: c_int = 42;").unwrap();

        let mut f2 = fs::File::create(rust_src.join("fun_get_x.rs")).unwrap();
        writeln!(f2, "use std::ffi::c_int;").unwrap();
        writeln!(f2, "").unwrap();
        writeln!(f2, "pub fn get_x() -> c_int {{ 42 }}").unwrap();

        // Override project root discovery
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();

        let output = rust_src.join("merged_test.rs");
        let result = merge_feature("default", Some(&output));

        env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok(), "merge_feature should succeed: {:?}", result);
        let content = fs::read_to_string(&output).unwrap();
        assert!(
            content.contains("c_int"),
            "merged output must contain c_int declaration"
        );
        assert!(content.contains("X"), "merged output must contain variable X");
        assert!(
            content.contains("get_x"),
            "merged output must contain function get_x"
        );
    }
}

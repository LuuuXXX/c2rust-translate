use anyhow::{Context, Result};
use colored::Colorize;
use inquire::Text;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 判断路径是否为需要翻译的文件（文件名以 var_ 或 fun_ 开头的 .rs 文件）
fn is_translatable_rs_file(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|name| name.starts_with("var_") || name.starts_with("fun_"))
        .unwrap_or(false)
}

/// 统计给定目录中需要翻译的 .rs 文件（文件名以 var_ 或 fun_ 开头，包括空文件和非空文件）
pub fn count_all_rs_files(rust_dir: &Path) -> Result<usize> {
    let mut count = 0;

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        // 只统计扩展名为 .rs 且文件名以 var_ 或 fun_ 开头的常规文件，不包括目录
        if path.is_file()
            && path.extension().is_some_and(|ext| ext == "rs")
            && is_translatable_rs_file(path)
        {
            count += 1;
        }
    }

    Ok(count)
}

/// 单次遍历统计给定目录中需要翻译的 .rs 文件总数和空文件数。
/// 返回 `(total, empty_count)`。
pub fn count_rs_files_with_empty(rust_dir: &Path) -> Result<(usize, usize)> {
    let mut total = 0;
    let mut empty = 0;

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") && is_translatable_rs_file(path) {
            total += 1;
            if entry.metadata()?.len() == 0 {
                empty += 1;
            }
        }
    }

    Ok((total, empty))
}

/// 查找给定目录中需要翻译的空 .rs 文件（文件名以 var_ 或 fun_ 开头且内容为空）
pub fn find_empty_rs_files(rust_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut empty_files = Vec::new();

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        // 只检查扩展名为 .rs 且文件名以 var_ 或 fun_ 开头的常规文件，不包括目录
        if path.is_file()
            && path.extension().is_some_and(|ext| ext == "rs")
            && is_translatable_rs_file(path)
        {
            let metadata = fs::metadata(path)?;
            if metadata.len() == 0 {
                empty_files.push(path.to_path_buf());
            }
        }
    }

    // 按路径字母顺序排序文件，以确保一致和可预测的顺序
    empty_files.sort();

    Ok(empty_files)
}

/// 从文件名中提取文件类型（var_ 或 fun_ 前缀）
pub fn extract_file_type(filename: &str) -> Option<(&'static str, &str)> {
    if let Some(stripped) = filename.strip_prefix("var_") {
        Some(("var", stripped))
    } else if let Some(stripped) = filename.strip_prefix("fun_") {
        Some(("fn", stripped))
    } else {
        None
    }
}

pub use super::interaction::{parse_file_selection, prompt_file_selection};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_count_rs_files_with_empty_mixed_files() {
        // Create a temp directory with a mix of translatable/non-translatable and
        // empty/non-empty files including a nested subdirectory.
        let temp_dir = tempdir().unwrap();
        let base = temp_dir.path();

        // translatable & empty
        fs::File::create(base.join("var_empty.rs")).unwrap();
        fs::File::create(base.join("fun_empty.rs")).unwrap();

        // translatable & non-empty
        let mut f = fs::File::create(base.join("var_nonempty.rs")).unwrap();
        f.write_all(b"pub static X: i32 = 1;").unwrap();
        let mut f = fs::File::create(base.join("fun_nonempty.rs")).unwrap();
        f.write_all(b"fn foo() {}").unwrap();

        // non-translatable .rs (no var_/fun_ prefix) – must not be counted
        fs::File::create(base.join("other.rs")).unwrap();

        // non-.rs file – must not be counted
        fs::File::create(base.join("var_not_rs.txt")).unwrap();

        // nested subdirectory: one translatable empty file
        let nested = base.join("nested");
        fs::create_dir(&nested).unwrap();
        fs::File::create(nested.join("var_nested.rs")).unwrap();

        // nested non-translatable .rs – must not be counted
        let mut f = fs::File::create(nested.join("other_nested.rs")).unwrap();
        f.write_all(b"// not counted").unwrap();

        let (total, empty) = count_rs_files_with_empty(base).unwrap();

        // translatable: var_empty, fun_empty, var_nonempty, fun_nonempty, nested/var_nested = 5
        assert_eq!(total, 5);
        // empty translatable: var_empty, fun_empty, nested/var_nested = 3
        assert_eq!(empty, 3);

        // Cross-check against the single-function helpers
        let total_only = count_all_rs_files(base).unwrap();
        let empty_files = find_empty_rs_files(base).unwrap();
        assert_eq!(total_only, total);
        assert_eq!(empty_files.len(), empty);
    }

    #[test]
    fn test_count_all_rs_files() {
        use std::io::Write;

        // 创建临时目录
        let temp_dir = tempdir().unwrap();

        // 创建空的 .rs 文件（有 var_/fun_ 前缀，应被统计）
        fs::File::create(temp_dir.path().join("var_test1.rs")).unwrap();
        fs::File::create(temp_dir.path().join("fun_test2.rs")).unwrap();

        // 创建非空的 .rs 文件（有 var_/fun_ 前缀，应被统计）
        let mut file1 = fs::File::create(temp_dir.path().join("var_test3.rs")).unwrap();
        file1.write_all(b"pub static TEST: i32 = 42;").unwrap();

        let mut file2 = fs::File::create(temp_dir.path().join("fun_test4.rs")).unwrap();
        file2.write_all(b"fn test() {}").unwrap();

        // 创建一个非 .rs 文件（不应被计数）
        fs::File::create(temp_dir.path().join("test.txt")).unwrap();

        // 创建一个不以 var_/fun_ 开头的 .rs 文件（不应被计数）
        fs::File::create(temp_dir.path().join("other.rs")).unwrap();

        // 创建以 var/fun 开头但不带下划线的文件（不应被计数，如 variable.rs、function.rs）
        fs::File::create(temp_dir.path().join("variable.rs")).unwrap();
        fs::File::create(temp_dir.path().join("function.rs")).unwrap();

        // 只统计以 var_ 或 fun_ 开头的 .rs 文件
        let total_count = count_all_rs_files(temp_dir.path()).unwrap();
        assert_eq!(total_count, 4); // 只统计有 var_/fun_ 前缀的 .rs 文件

        // 验证空文件数（只统计有 var_/fun_ 前缀的空文件）
        let empty_count = find_empty_rs_files(temp_dir.path()).unwrap().len();
        assert_eq!(empty_count, 2);
    }

    #[test]
    fn test_find_empty_rs_files() {
        // 创建唯一的临时目录结构
        let temp_dir = tempdir().unwrap();

        // 创建空的 .rs 文件
        let empty_file = temp_dir.path().join("var_test.rs");
        fs::File::create(&empty_file).unwrap();

        // 创建非空的 .rs 文件
        let non_empty_file = temp_dir.path().join("fun_test.rs");
        let mut file = fs::File::create(&non_empty_file).unwrap();
        file.write_all(b"fn test() {}").unwrap();

        // 测试查找空文件
        let empty_files = find_empty_rs_files(temp_dir.path()).unwrap();
        assert_eq!(empty_files.len(), 1);
        assert!(empty_files[0].ends_with("var_test.rs"));

        // temp_dir 会在超出作用域时自动删除
    }

    #[test]
    fn test_find_empty_rs_files_sorted() {
        // 创建临时目录
        let temp_dir = tempdir().unwrap();

        // 以非字母顺序创建多个空 .rs 文件
        fs::File::create(temp_dir.path().join("var_zebra.rs")).unwrap();
        fs::File::create(temp_dir.path().join("fun_alpha.rs")).unwrap();
        fs::File::create(temp_dir.path().join("var_middle.rs")).unwrap();

        // 查找空文件
        let empty_files = find_empty_rs_files(temp_dir.path()).unwrap();

        // 验证我们找到了所有 3 个文件
        assert_eq!(empty_files.len(), 3);

        // 验证文件按字母顺序排序
        assert_eq!(empty_files[0].file_name().unwrap(), "fun_alpha.rs");
        assert_eq!(empty_files[1].file_name().unwrap(), "var_middle.rs");
        assert_eq!(empty_files[2].file_name().unwrap(), "var_zebra.rs");
    }

    #[test]
    fn test_extract_file_type_var() {
        let (file_type, name) = extract_file_type("var_counter").unwrap();
        assert_eq!(file_type, "var");
        assert_eq!(name, "counter");
    }

    #[test]
    fn test_extract_file_type_fun() {
        let (file_type, name) = extract_file_type("fun_calculate").unwrap();
        assert_eq!(file_type, "fn");
        assert_eq!(name, "calculate");
    }

    #[test]
    fn test_extract_file_type_invalid() {
        let result = extract_file_type("invalid_name");
        assert!(result.is_none());
    }

    #[test]
    fn test_path_construction() {
        let feature = "my_feature";
        let feature_path = PathBuf::from(feature);
        let rust_dir = feature_path.join("rust");

        // 将路径作为 PathBuf 进行比较而不是字符串，以便在 Windows 上工作
        let expected = PathBuf::from("my_feature").join("rust");
        assert_eq!(rust_dir, expected);
    }
}

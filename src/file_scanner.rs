use anyhow::{Context, Result};
use colored::Colorize;
use inquire::Text;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 统计给定目录中的所有 .rs 文件（包括空文件和非空文件）
pub fn count_all_rs_files(rust_dir: &Path) -> Result<usize> {
    let mut count = 0;

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        // 只统计扩展名为 .rs 的常规文件，不包括目录
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            count += 1;
        }
    }

    Ok(count)
}

/// 查找给定目录中的所有空 .rs 文件
pub fn find_empty_rs_files(rust_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut empty_files = Vec::new();

    for entry in WalkDir::new(rust_dir) {
        let entry = entry?;
        let path = entry.path();
        // 只检查扩展名为 .rs 的常规文件，不包括目录
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
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

/// 解析用户输入的文件选择。
/// 用户提供基于 1 的索引；返回所选文件的基于 0 的索引。
pub fn parse_file_selection(input: &str, total_files: usize) -> Result<Vec<usize>> {
    let input = input.trim();

    // 修剪后检查输入是否为空
    if input.is_empty() {
        anyhow::bail!("No input provided. Please select at least one file.");
    }

    // 解析输入
    if input.eq_ignore_ascii_case("all") {
        // 选择所有文件
        return Ok((0..total_files).collect());
    }

    let mut selected_indices = Vec::new();

    // 按逗号拆分并处理每个部分
    for part in input.split(',') {
        let part = part.trim();

        if part.contains('-') {
            // 处理范围（例如 "1-3"）
            let range_parts: Vec<&str> = part.split('-').collect();
            if range_parts.len() != 2 {
                anyhow::bail!("Invalid range format: {}. Expected format like '1-3'", part);
            }

            let start_str = range_parts[0].trim();
            let end_str = range_parts[1].trim();

            if start_str.is_empty() || end_str.is_empty() {
                anyhow::bail!(
                    "Invalid range format: ranges must have both start and end values (e.g., '1-3')"
                );
            }

            let start: usize = start_str
                .parse()
                .with_context(|| format!("Invalid number in range: {}", start_str))?;
            let end: usize = end_str
                .parse()
                .with_context(|| format!("Invalid number in range: {}", end_str))?;

            if start < 1 || end < 1 || start > total_files || end > total_files {
                anyhow::bail!(
                    "Range {}-{} is out of bounds (valid: 1-{})",
                    start,
                    end,
                    total_files
                );
            }

            if start > end {
                anyhow::bail!("Invalid range: {} is greater than {}", start, end);
            }

            for i in start..=end {
                selected_indices.push(i - 1);
            }
        } else {
            // 处理单个数字
            let num: usize = part
                .parse()
                .with_context(|| format!("Invalid number: {}", part))?;

            if num < 1 || num > total_files {
                anyhow::bail!("Number {} is out of bounds (valid: 1-{})", num, total_files);
            }

            selected_indices.push(num - 1);
        }
    }

    // 删除重复项并排序
    selected_indices.sort_unstable();
    selected_indices.dedup();

    if selected_indices.is_empty() {
        anyhow::bail!("No files selected");
    }

    Ok(selected_indices)
}

/// 提示用户从列表中选择文件
pub fn prompt_file_selection(files: &[&PathBuf], rust_dir: &Path) -> Result<Vec<usize>> {
    println!("\n{}", "Available files to process:".bright_cyan().bold());

    // 显示文件及其索引号和相对路径
    for (idx, file) in files.iter().enumerate() {
        let relative_path = file.strip_prefix(rust_dir).unwrap_or(file);
        println!("  {}. {}", idx + 1, relative_path.display());
    }

    println!();
    println!("{}", "Select files to process:".bright_yellow());
    println!("  - Enter numbers separated by commas (e.g., 1,3,5)");
    println!("  - Enter ranges (e.g., 1-3,5)");
    println!("  - Enter 'all' to process all files");
    println!();

    // Use inquire::Text for better terminal handling (Delete key, arrow keys, etc.)
    let input = match Text::new("Your selection:")
        .with_help_message("Enter file numbers/ranges or 'all'")
        .prompt()
    {
        Ok(s) => s,
        Err(inquire::InquireError::OperationCanceled) => {
            anyhow::bail!("File selection canceled by user");
        }
        Err(e) => return Err(anyhow::Error::new(e)).context("Failed to get file selection"),
    };

    parse_file_selection(&input, files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_count_all_rs_files() {
        use std::io::Write;

        // 创建临时目录
        let temp_dir = tempdir().unwrap();

        // 创建空的 .rs 文件
        fs::File::create(temp_dir.path().join("var_test1.rs")).unwrap();
        fs::File::create(temp_dir.path().join("fun_test2.rs")).unwrap();

        // 创建非空的 .rs 文件
        let mut file1 = fs::File::create(temp_dir.path().join("var_test3.rs")).unwrap();
        file1.write_all(b"pub static TEST: i32 = 42;").unwrap();

        let mut file2 = fs::File::create(temp_dir.path().join("fun_test4.rs")).unwrap();
        file2.write_all(b"fn test() {}").unwrap();

        // 创建一个非 .rs 文件（不应被计数）
        fs::File::create(temp_dir.path().join("test.txt")).unwrap();

        // 统计所有 .rs 文件
        let total_count = count_all_rs_files(temp_dir.path()).unwrap();
        assert_eq!(total_count, 4); // 应该统计空和非空的 .rs 文件

        // 验证空文件数
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

    #[test]
    fn test_parse_file_selection() {
        struct TestCase {
            input: &'static str,
            total_files: usize,
            expected: Result<Vec<usize>, &'static str>,
        }

        let test_cases = vec![
            // 成功案例
            TestCase {
                input: "all",
                total_files: 5,
                expected: Ok(vec![0, 1, 2, 3, 4]),
            },
            TestCase {
                input: "ALL",
                total_files: 3,
                expected: Ok(vec![0, 1, 2]),
            },
            TestCase {
                input: "All",
                total_files: 3,
                expected: Ok(vec![0, 1, 2]),
            },
            TestCase {
                input: "3",
                total_files: 5,
                expected: Ok(vec![2]),
            },
            TestCase {
                input: "1,3,5",
                total_files: 5,
                expected: Ok(vec![0, 2, 4]),
            },
            TestCase {
                input: "2-4",
                total_files: 5,
                expected: Ok(vec![1, 2, 3]),
            },
            TestCase {
                input: "1,3-5,7",
                total_files: 10,
                expected: Ok(vec![0, 2, 3, 4, 6]),
            },
            TestCase {
                input: "1,2,1,3,2",
                total_files: 5,
                expected: Ok(vec![0, 1, 2]),
            },
            TestCase {
                input: " 1 , 3 , 5 ",
                total_files: 5,
                expected: Ok(vec![0, 2, 4]),
            },
            TestCase {
                input: " 2 - 4 ",
                total_files: 5,
                expected: Ok(vec![1, 2, 3]),
            },
            // 错误案例
            TestCase {
                input: "6",
                total_files: 5,
                expected: Err("out of bounds"),
            },
            TestCase {
                input: "1,6",
                total_files: 5,
                expected: Err("out of bounds"),
            },
            TestCase {
                input: "5-2",
                total_files: 5,
                expected: Err("is greater than"),
            },
            TestCase {
                input: "abc",
                total_files: 5,
                expected: Err(""),
            },
            TestCase {
                input: "",
                total_files: 5,
                expected: Err("No input provided"),
            },
            TestCase {
                input: "   ",
                total_files: 5,
                expected: Err("No input provided"),
            },
            TestCase {
                input: "-3",
                total_files: 5,
                expected: Err("ranges must have both start and end values"),
            },
            TestCase {
                input: "1-",
                total_files: 5,
                expected: Err("ranges must have both start and end values"),
            },
            TestCase {
                input: "-",
                total_files: 5,
                expected: Err("ranges must have both start and end values"),
            },
            TestCase {
                input: "0",
                total_files: 5,
                expected: Err(""),
            },
            TestCase {
                input: "1-10",
                total_files: 5,
                expected: Err(""),
            },
        ];

        for (i, tc) in test_cases.iter().enumerate() {
            let result = parse_file_selection(tc.input, tc.total_files);
            match &tc.expected {
                Ok(expected_vec) => {
                    assert!(
                        result.is_ok(),
                        "Test case #{}: expected Ok, got Err for input '{}'",
                        i,
                        tc.input
                    );
                    assert_eq!(
                        &result.unwrap(),
                        expected_vec,
                        "Test case #{}: mismatch for input '{}'",
                        i,
                        tc.input
                    );
                }
                Err(expected_err) => {
                    assert!(
                        result.is_err(),
                        "Test case #{}: expected Err, got Ok for input '{}'",
                        i,
                        tc.input
                    );
                    if !expected_err.is_empty() {
                        let err_msg = result.unwrap_err().to_string();
                        assert!(
                            err_msg.contains(expected_err),
                            "Test case #{}: error message '{}' doesn't contain '{}' for input '{}'",
                            i,
                            err_msg,
                            expected_err,
                            tc.input
                        );
                    }
                }
            }
        }
    }
}

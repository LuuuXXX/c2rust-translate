//! suggestions.txt 的建议文件管理

use crate::util;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// 获取 suggestions.txt 建议文件的路径
pub fn get_suggestion_file_path() -> Result<PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root.join("suggestions.txt"))
}

/// 读取 suggestions.txt 的当前内容（如果存在）
#[cfg(test)]
pub fn read_suggestions() -> Result<Option<String>> {
    let suggestion_file = get_suggestion_file_path()?;

    if !suggestion_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&suggestion_file).with_context(|| {
        format!(
            "Failed to read suggestion file: {}",
            suggestion_file.display()
        )
    })?;

    if content.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(content))
    }
}

/// 将建议追加到 suggestions.txt 文件
pub fn append_suggestion(suggestion: &str) -> Result<()> {
    let suggestion_file = get_suggestion_file_path()?;

    // 如果父目录不存在则创建
    if let Some(parent) = suggestion_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&suggestion_file)
        .with_context(|| {
            format!(
                "Failed to open suggestion file: {}",
                suggestion_file.display()
            )
        })?;

    // 以纯文本格式追加建议
    writeln!(file, "{}", suggestion)?;

    println!(
        "│ {}",
        format!("✓ Suggestion saved to {}", suggestion_file.display()).bright_green()
    );

    Ok(())
}

/// 清除 suggestions.txt 文件中的所有建议
/// 这在开始全新重试时很有用，以避免建议积累
pub fn clear_suggestions() -> Result<()> {
    let suggestion_file = get_suggestion_file_path()?;

    if suggestion_file.exists() {
        fs::remove_file(&suggestion_file).with_context(|| {
            format!(
                "Failed to remove suggestion file: {}",
                suggestion_file.display()
            )
        })?;
        println!(
            "│ {}",
            "✓ Cleared previous suggestions for fresh retry".bright_yellow()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_suggestion_file_path() {
        // 创建临时目录作为项目根目录
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // 在临时项目根目录内创建 .c2rust 目录
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = get_suggestion_file_path();

        // 恢复原始工作目录
        env::set_current_dir(old_dir).unwrap();

        // 路径应该有效且指向项目根目录中的 suggestions.txt
        assert!(result.is_ok());
        let path = result.unwrap();
        assert_eq!(path.file_name().unwrap(), "suggestions.txt");
    }

    #[test]
    #[serial]
    fn test_read_nonexistent_suggestions() {
        // 创建临时目录
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // 创建 .c2rust 目录
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = read_suggestions();

        // 恢复目录
        env::set_current_dir(old_dir).unwrap();

        // 对于不存在的文件应返回 Ok(None)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    #[serial]
    fn test_append_suggestion() {
        // 创建临时目录作为项目根目录
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // 在临时项目根目录内创建 .c2rust 目录
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // 追加一个建议
        let suggestion_text = "Use std::ffi::CStr instead of raw pointers";
        let result = append_suggestion(suggestion_text);
        assert!(result.is_ok());

        // 读回文件并验证内容
        let suggestion_file = get_suggestion_file_path().unwrap();
        assert!(suggestion_file.exists());

        let content = fs::read_to_string(&suggestion_file).unwrap();
        assert!(content.contains(suggestion_text));
        // 纯文本格式 - 无时间戳
        assert!(!content.contains("## Suggestion added at"));

        // 追加另一个建议
        let second_suggestion = "Ensure proper lifetime annotations";
        let result2 = append_suggestion(second_suggestion);
        assert!(result2.is_ok());

        // 验证两个建议都存在
        let content2 = fs::read_to_string(&suggestion_file).unwrap();
        assert!(content2.contains(suggestion_text));
        assert!(content2.contains(second_suggestion));

        // 纯文本格式 - 无时间戳头
        assert!(!content2.contains("## Suggestion added at"));

        // 在 temp_dir 被删除前恢复原始工作目录
        env::set_current_dir(&old_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_clear_suggestions() {
        // 创建临时目录作为项目根目录
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        // 在临时项目根目录内创建 .c2rust 目录
        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // 首先，创建带有一些内容的建议文件
        let suggestion_text = "Test suggestion";
        let result = append_suggestion(suggestion_text);
        assert!(result.is_ok());

        let suggestion_file = get_suggestion_file_path().unwrap();
        assert!(suggestion_file.exists());

        // 现在清除建议
        let clear_result = clear_suggestions();
        assert!(clear_result.is_ok());

        // 验证文件不再存在
        assert!(!suggestion_file.exists());

        // 再次清除应该是无操作且不会出错
        let clear_again = clear_suggestions();
        assert!(clear_again.is_ok());

        // 在 temp_dir 被删除前恢复原始工作目录
        env::set_current_dir(&old_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_suggestion_workflow_with_retry() {
        // 模拟重试工作流：添加建议 -> 清除 -> 添加新建议
        let temp_dir = TempDir::new().unwrap();
        let old_dir = env::current_dir().unwrap();

        fs::create_dir(temp_dir.path().join(".c2rust")).unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // 第一次尝试 - 添加建议
        let first_suggestion = "First attempt: Use smart pointers";
        append_suggestion(first_suggestion).unwrap();

        let content1 = read_suggestions().unwrap();
        assert!(content1.is_some());
        assert!(content1.unwrap().contains(first_suggestion));

        // 重试 - 在重试前清除建议
        clear_suggestions().unwrap();

        let content_after_clear = read_suggestions().unwrap();
        assert!(content_after_clear.is_none());

        // 第二次尝试 - 添加不同的建议
        let second_suggestion = "Second attempt: Use Option<T> for nullable values";
        append_suggestion(second_suggestion).unwrap();

        let content2 = read_suggestions().unwrap();
        assert!(content2.is_some());
        let final_content = content2.unwrap();

        // 应该只包含第二个建议，不包含第一个
        assert!(final_content.contains(second_suggestion));
        assert!(!final_content.contains(first_suggestion));

        // 在 temp_dir 被删除前恢复原始工作目录
        env::set_current_dir(&old_dir).unwrap();
    }
}

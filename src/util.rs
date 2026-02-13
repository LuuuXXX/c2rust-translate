use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

// ============================================================================
// Constants
// ============================================================================

/// 翻译文件的最大尝试次数（1 次初始 + 2 次重试）
pub const MAX_TRANSLATION_ATTEMPTS: usize = 3;

/// 从代码文件预览的行数（C 源代码或 Rust 代码）
pub const CODE_PREVIEW_LINES: usize = 15;

/// 从错误消息预览的行数
pub const ERROR_PREVIEW_LINES: usize = 10;

// ============================================================================
// Progress Tracking
// ============================================================================

#[derive(Debug, Default)]
pub struct ProgressState {
    /// 已处理文件的总数（包括之前运行和当前会话的文件）
    pub processed_count: usize,
    /// 要处理的文件总数（用于显示目的）
    pub total_count: usize,
}

impl ProgressState {
    /// 创建具有总计数的新进度状态
    pub fn new(total_count: usize) -> Self {
        Self {
            processed_count: 0,
            total_count,
        }
    }

    /// 创建具有总计数和已处理计数的新进度状态。
    /// `already_processed` 值被限制为不超过 `total_count`。
    pub fn with_initial_progress(total_count: usize, already_processed: usize) -> Self {
        Self {
            processed_count: already_processed.min(total_count),
            total_count,
        }
    }

    /// 将文件标记为已处理（递增计数器）
    pub fn mark_processed(&mut self) {
        self.processed_count += 1;
    }

    /// 获取当前进度位置（从 1 开始索引用于显示）
    pub fn get_current_position(&self) -> usize {
        self.processed_count + 1
    }

    /// 获取要处理的文件总数
    pub fn get_total_count(&self) -> usize {
        self.total_count
    }
}

// ============================================================================
// Path and Project Utilities
// ============================================================================

/// 从起始路径向上搜索 .c2rust 目录以查找项目根目录
fn find_project_root_from(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path.to_path_buf();

    loop {
        let c2rust_dir = current.join(".c2rust");

        // 使用 metadata 正确处理 IO 错误
        match std::fs::metadata(&c2rust_dir) {
            Ok(metadata) if metadata.is_dir() => {
                return Ok(current);
            }
            Ok(_) => {
                // .c2rust 存在但不是目录，继续搜索
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // .c2rust 不存在，继续搜索
            }
            Err(e) => {
                // 其他 IO 错误（权限等）
                return Err(e).with_context(|| {
                    format!(
                        "Failed to access .c2rust directory at {}",
                        c2rust_dir.display()
                    )
                });
            }
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => anyhow::bail!("Could not find .c2rust directory in any parent directory"),
        }
    }
}

/// 从当前目录向上搜索 .c2rust 目录以查找项目根目录
pub fn find_project_root() -> Result<PathBuf> {
    let current = std::env::current_dir().context("Failed to get current directory")?;
    find_project_root_from(&current)
}

/// 验证功能名称以防止路径遍历攻击
pub fn validate_feature_name(feature: &str) -> Result<()> {
    if feature.contains('/')
        || feature.contains('\\')
        || feature.contains("..")
        || feature.is_empty()
    {
        anyhow::bail!(
            "Invalid feature name '{}': must be a simple directory name without path separators or '..'",
            feature
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_project_root_from_nested_dir() {
        // 创建临时目录结构：
        // temp/
        //   .c2rust/
        //   subdir1/
        //     subdir2/
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();

        let subdir1 = temp_dir.path().join("subdir1");
        let subdir2 = subdir1.join("subdir2");
        fs::create_dir_all(&subdir2).unwrap();

        // 应该从嵌套子目录找到 .c2rust 目录
        let result = find_project_root_from(&subdir2);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_find_project_root_not_found() {
        // 创建没有 .c2rust 的临时目录
        let temp_dir = tempdir().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        // 应该无法找到 .c2rust 目录
        let result = find_project_root_from(&subdir);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Could not find .c2rust"));
    }

    #[test]
    fn test_find_project_root_from_root_dir() {
        // 创建根目录带有 .c2rust 的临时目录
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();

        // 应该在起始目录中找到 .c2rust
        let result = find_project_root_from(temp_dir.path());

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_validate_feature_name_valid() {
        assert!(validate_feature_name("valid_feature").is_ok());
        assert!(validate_feature_name("feature123").is_ok());
        assert!(validate_feature_name("my-feature").is_ok());
    }

    #[test]
    fn test_validate_feature_name_invalid() {
        // 测试路径分隔符
        assert!(validate_feature_name("feature/path").is_err());
        assert!(validate_feature_name("feature\\path").is_err());

        // 测试路径遍历
        assert!(validate_feature_name("..").is_err());
        assert!(validate_feature_name("../feature").is_err());
        assert!(validate_feature_name("feature/../other").is_err());

        // 测试空字符串
        assert!(validate_feature_name("").is_err());
    }

    // ========================================================================
    // Progress State Tests
    // ========================================================================

    #[test]
    fn test_progress_state_default() {
        let state = ProgressState::default();
        assert_eq!(state.processed_count, 0);
        assert_eq!(state.total_count, 0);
    }

    #[test]
    fn test_progress_state_new() {
        let state = ProgressState::new(10);
        assert_eq!(state.processed_count, 0);
        assert_eq!(state.total_count, 10);
    }

    #[test]
    fn test_mark_processed() {
        let mut state = ProgressState::new(5);
        assert_eq!(state.processed_count, 0);

        state.mark_processed();
        assert_eq!(state.processed_count, 1);

        state.mark_processed();
        assert_eq!(state.processed_count, 2);
    }

    #[test]
    fn test_get_current_position() {
        let mut state = ProgressState::new(10);
        assert_eq!(state.get_current_position(), 1);

        state.mark_processed();
        assert_eq!(state.get_current_position(), 2);

        state.mark_processed();
        assert_eq!(state.get_current_position(), 3);
    }

    #[test]
    fn test_get_total_count() {
        let state = ProgressState::new(25);
        assert_eq!(state.get_total_count(), 25);
    }

    #[test]
    fn test_with_initial_progress() {
        // 测试使用已处理文件创建进度状态
        let state = ProgressState::with_initial_progress(10, 3);
        assert_eq!(state.processed_count, 3);
        assert_eq!(state.total_count, 10);

        // 当前位置应该是 4（3 个已处理 + 1）
        assert_eq!(state.get_current_position(), 4);
    }

    #[test]
    fn test_with_initial_progress_continuation() {
        // 模拟 10 个文件中已处理 5 个的场景
        let mut state = ProgressState::with_initial_progress(10, 5);

        // 下一个要处理的文件应显示为 [6/10]
        assert_eq!(state.get_current_position(), 6);

        // 处理一个文件后，应显示为 [7/10]
        state.mark_processed();
        assert_eq!(state.get_current_position(), 7);
        assert_eq!(state.processed_count, 6);
    }

    #[test]
    fn test_with_initial_progress_clamping() {
        // 测试 already_processed 被限制为 total_count
        let state = ProgressState::with_initial_progress(10, 15);
        assert_eq!(state.processed_count, 10); // 应该被限制为 10
        assert_eq!(state.total_count, 10);
        assert_eq!(state.get_current_position(), 11); // 10 + 1

        // 测试边界情况：already_processed 等于 total_count
        let state2 = ProgressState::with_initial_progress(10, 10);
        assert_eq!(state2.processed_count, 10);
        assert_eq!(state2.get_current_position(), 11);
    }
}

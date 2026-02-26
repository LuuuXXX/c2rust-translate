use anyhow::{Context, Result};
use std::collections::HashMap;
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
// Translation Statistics Tracking
// ============================================================================

#[derive(Debug, Clone)]
pub struct FileAttemptStat {
    /// 翻译尝试次数（1-3）
    pub translation_attempts: usize,
    /// 修复尝试次数（每次翻译的修复次数总和）
    pub fix_attempts: usize,
    /// 是否使用了"重来"功能
    pub had_restart: bool,
}

#[derive(Debug, Default, Clone)]
pub struct TranslationStats {
    /// 总文件数
    pub total_files: usize,
    /// 一次性通过的文件数
    pub success_first_try: usize,
    /// 重试1次后成功的文件数
    pub success_retry_1: usize,
    /// 重试2次后成功的文件数
    pub success_retry_2: usize,
    /// 重试3次及以上后成功的文件数
    pub success_retry_3_plus: usize,
    /// 需要"重来"（RetryDirectly）的文件数
    pub restart_count: usize,
    /// 每个文件的详细统计（文件名 -> 尝试次数）
    pub file_attempts: HashMap<String, FileAttemptStat>,
    /// 被用户跳过的文件列表（文件名）
    pub skipped_files: Vec<String>,
}

impl TranslationStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录文件翻译完成
    pub fn record_file_completion(
        &mut self,
        file_name: String,
        attempts: usize,
        had_restart: bool,
        fix_attempts: usize,
    ) {
        self.total_files += 1;

        debug_assert!(attempts > 0, "attempts must be at least 1, got: {}", attempts);
        match attempts {
            1 => self.success_first_try += 1,
            2 => self.success_retry_1 += 1,
            3 => self.success_retry_2 += 1,
            _ => self.success_retry_3_plus += 1,
        }

        if had_restart {
            self.restart_count += 1;
        }

        self.file_attempts.insert(
            file_name,
            FileAttemptStat {
                translation_attempts: attempts,
                fix_attempts,
                had_restart,
            },
        );
    }

    /// 记录文件被跳过
    pub fn record_file_skipped(&mut self, file_name: String) {
        if !self.skipped_files.contains(&file_name) {
            self.skipped_files.push(file_name);
        }
    }

    /// 打印统计报告
    pub fn print_summary(&self) {
        use colored::Colorize;

        println!("\n{}", "═".repeat(80).bright_cyan());
        println!("{}", "📊 Translation Statistics Summary".bright_cyan().bold());
        println!("{}", "═".repeat(80).bright_cyan());

        if self.total_files == 0 {
            println!("\n{}", "No files were successfully translated.".yellow());
            println!("\n{}", "═".repeat(80).bright_cyan());
            return;
        }

        // 总体统计
        println!("\n{}", "Overall Statistics:".bright_white().bold());
        println!(
            "  Total files successfully translated: {}",
            self.total_files.to_string().bright_green()
        );
        println!(
            "  Files with restart:          {}",
            self.restart_count.to_string().bright_yellow()
        );

        // 计算总重试次数（translation_attempts 始终 >= 1，saturating_sub 防御性处理）
        let total_retries: usize = self
            .file_attempts
            .values()
            .map(|stat| stat.translation_attempts.saturating_sub(1))
            .sum();
        println!(
            "  Total retries:               {}",
            total_retries.to_string().bright_yellow()
        );

        // 按重试次数分类
        println!("\n{}", "Success Rate by Attempts:".bright_white().bold());
        println!(
            "  ✓ First try (no retry):      {} ({:.1}%)",
            self.success_first_try.to_string().bright_green(),
            self.percentage(self.success_first_try)
        );
        println!(
            "  ↻ Retry 1 time:              {} ({:.1}%)",
            self.success_retry_1.to_string().bright_cyan(),
            self.percentage(self.success_retry_1)
        );
        println!(
            "  ↻ Retry 2 times:             {} ({:.1}%)",
            self.success_retry_2.to_string().bright_yellow(),
            self.percentage(self.success_retry_2)
        );
        println!(
            "  ↻ Retry 3+ times:            {} ({:.1}%)",
            self.success_retry_3_plus.to_string().bright_red(),
            self.percentage(self.success_retry_3_plus)
        );

        // 详细文件列表
        if !self.file_attempts.is_empty() {
            println!(
                "\n{}",
                "Detailed File Statistics (Top 10 by translation attempts):"
                    .bright_white()
                    .bold()
            );
            let mut files: Vec<_> = self.file_attempts.iter().collect();
            files.sort_by(|a, b| {
                b.1.translation_attempts
                    .cmp(&a.1.translation_attempts)
                    .then_with(|| b.1.fix_attempts.cmp(&a.1.fix_attempts))
            });

            for (file_name, stat) in files.iter().take(10) {
                let restart_indicator = if stat.had_restart {
                    " [RESTART]".bright_red().to_string()
                } else {
                    String::new()
                };
                println!(
                    "  {} - {} translation attempt(s), {} fix attempt(s){}",
                    file_name.bright_white(),
                    stat.translation_attempts.to_string().bright_cyan(),
                    stat.fix_attempts.to_string().bright_yellow(),
                    restart_indicator
                );
            }

            if self.file_attempts.len() > 10 {
                println!("  ... and {} more files", self.file_attempts.len() - 10);
            }
        }

        // 跳过的文件
        if !self.skipped_files.is_empty() {
            println!("\n{}", "Skipped Files:".bright_yellow().bold());
            for (idx, file_name) in self.skipped_files.iter().enumerate() {
                println!("  {}. {}", idx + 1, file_name.bright_yellow());
            }
            println!(
                "  {}",
                format!("Total skipped: {}", self.skipped_files.len()).bright_yellow()
            );
        }

        println!("\n{}", "═".repeat(80).bright_cyan());
        println!(
            "{}",
            "💡 Tip: Use these statistics to evaluate and select the optimal LLM model"
                .bright_blue()
        );
        println!("{}", "═".repeat(80).bright_cyan());
    }

    fn percentage(&self, count: usize) -> f64 {
        if self.total_files == 0 {
            0.0
        } else {
            (count as f64 / self.total_files as f64) * 100.0
        }
    }
}

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

    // ========================================================================
    // TranslationStats Tests
    // ========================================================================

    #[test]
    fn test_translation_stats_default() {
        let stats = TranslationStats::default();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.success_first_try, 0);
        assert_eq!(stats.success_retry_1, 0);
        assert_eq!(stats.success_retry_2, 0);
        assert_eq!(stats.success_retry_3_plus, 0);
        assert_eq!(stats.restart_count, 0);
        assert!(stats.file_attempts.is_empty());
        assert!(stats.skipped_files.is_empty());
    }

    #[test]
    fn test_translation_stats_percentage_empty() {
        let stats = TranslationStats::new();
        assert_eq!(stats.percentage(0), 0.0);
        assert_eq!(stats.percentage(5), 0.0);
    }

    #[test]
    fn test_record_file_completion_first_try() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("foo.rs".to_string(), 1, false, 0);

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.success_first_try, 1);
        assert_eq!(stats.success_retry_1, 0);
        assert_eq!(stats.restart_count, 0);

        let entry = stats.file_attempts.get("foo.rs").unwrap();
        assert_eq!(entry.translation_attempts, 1);
        assert_eq!(entry.fix_attempts, 0);
        assert!(!entry.had_restart);
    }

    #[test]
    fn test_record_file_completion_retry_1() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("bar.rs".to_string(), 2, false, 3);

        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.success_first_try, 0);
        assert_eq!(stats.success_retry_1, 1);

        let entry = stats.file_attempts.get("bar.rs").unwrap();
        assert_eq!(entry.translation_attempts, 2);
        assert_eq!(entry.fix_attempts, 3);
    }

    #[test]
    fn test_record_file_completion_retry_2() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("baz.rs".to_string(), 3, false, 5);

        assert_eq!(stats.success_retry_2, 1);
    }

    #[test]
    fn test_record_file_completion_retry_3_plus() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("qux.rs".to_string(), 4, false, 8);

        assert_eq!(stats.success_retry_3_plus, 1);
    }

    #[test]
    fn test_record_file_completion_with_restart() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("restart.rs".to_string(), 2, true, 2);

        assert_eq!(stats.restart_count, 1);

        let entry = stats.file_attempts.get("restart.rs").unwrap();
        assert!(entry.had_restart);
    }

    #[test]
    fn test_translation_stats_percentage() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("a.rs".to_string(), 1, false, 0);
        stats.record_file_completion("b.rs".to_string(), 1, false, 0);
        stats.record_file_completion("c.rs".to_string(), 1, false, 0);
        stats.record_file_completion("d.rs".to_string(), 2, false, 1);

        assert_eq!(stats.total_files, 4);
        assert!((stats.percentage(3) - 75.0).abs() < f64::EPSILON);
        assert!((stats.percentage(1) - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_translation_stats_multiple_files() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("a.rs".to_string(), 1, false, 0);
        stats.record_file_completion("b.rs".to_string(), 2, true, 3);
        stats.record_file_completion("c.rs".to_string(), 3, false, 5);

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.success_first_try, 1);
        assert_eq!(stats.success_retry_1, 1);
        assert_eq!(stats.success_retry_2, 1);
        assert_eq!(stats.restart_count, 1);
        assert_eq!(stats.file_attempts.len(), 3);
    }

    #[test]
    fn test_record_file_skipped() {
        let mut stats = TranslationStats::new();
        assert!(stats.skipped_files.is_empty());

        stats.record_file_skipped("foo.rs".to_string());
        assert_eq!(stats.skipped_files.len(), 1);
        assert_eq!(stats.skipped_files[0], "foo.rs");

        stats.record_file_skipped("bar.rs".to_string());
        assert_eq!(stats.skipped_files.len(), 2);
        assert_eq!(stats.skipped_files[1], "bar.rs");

        // Duplicate entries should not be added
        stats.record_file_skipped("foo.rs".to_string());
        assert_eq!(stats.skipped_files.len(), 2);
    }
}

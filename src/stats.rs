use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttemptStat {
    /// 翻译尝试次数（1-3）
    pub translation_attempts: usize,
    /// 修复尝试次数（每次翻译的修复次数总和）
    pub fix_attempts: usize,
    /// 是否使用了"重来"功能
    pub had_restart: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
    /// 翻译步骤本身失败的文件列表（文件名）。
    /// 与 `skipped_files`（用户主动/自动跳过）不同，此列表仅记录翻译命令非零退出的情况。
    #[serde(default)]
    pub translation_failed_files: Vec<String>,
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
        let file_name = canonicalize_stats_file_key(&file_name);
        self.total_files += 1;

        debug_assert!(
            attempts > 0,
            "attempts must be at least 1, got: {}",
            attempts
        );
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
        let file_name = canonicalize_stats_file_key(&file_name);
        if !self.skipped_files.contains(&file_name) {
            self.skipped_files.push(file_name);
        }
    }

    /// 记录翻译命令失败（与用户主动跳过区分）
    pub fn record_file_translation_failed(&mut self, file_name: String) {
        let file_name = canonicalize_stats_file_key(&file_name);
        if !self.translation_failed_files.contains(&file_name) {
            self.translation_failed_files.push(file_name);
        }
    }

    /// 为定点重跑清理单个目标的历史状态。
    ///
    /// 该操作不会影响其它文件的统计，只移除当前目标在
    /// `skipped_files` / `translation_failed_files` / `file_attempts`
    /// 中的历史痕迹，并同步调整聚合成功计数。
    pub fn clear_target_history(&mut self, file_name: &str) {
        let canonical_name = canonicalize_stats_file_key(file_name);
        self.skipped_files.retain(|item| item != &canonical_name);
        self.translation_failed_files
            .retain(|item| item != &canonical_name);

        if let Some(previous) = self.file_attempts.remove(&canonical_name) {
            self.total_files = self.total_files.saturating_sub(1);
            match previous.translation_attempts {
                1 => self.success_first_try = self.success_first_try.saturating_sub(1),
                2 => self.success_retry_1 = self.success_retry_1.saturating_sub(1),
                3 => self.success_retry_2 = self.success_retry_2.saturating_sub(1),
                _ => self.success_retry_3_plus = self.success_retry_3_plus.saturating_sub(1),
            }
            if previous.had_restart {
                self.restart_count = self.restart_count.saturating_sub(1);
            }
        }
    }

    /// 打印统计报告
    pub fn print_summary(&self) {
        use colored::Colorize;

        println!("\n{}", "═".repeat(80).bright_cyan());
        println!(
            "{}",
            "📊 Translation Statistics Summary".bright_cyan().bold()
        );
        println!("{}", "═".repeat(80).bright_cyan());

        if self.total_files == 0 {
            println!("\n{}", "No files were successfully translated.".yellow());
            if self.skipped_files.is_empty() && self.translation_failed_files.is_empty() {
                println!("\n{}", "═".repeat(80).bright_cyan());
                return;
            }
        } else {
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
        } // close `else` from total_files != 0 check

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

        // 翻译失败的文件（区别于用户主动跳过）
        if !self.translation_failed_files.is_empty() {
            println!("\n{}", "Translation-Failed Files:".bright_red().bold());
            for (idx, file_name) in self.translation_failed_files.iter().enumerate() {
                println!("  {}. {}", idx + 1, file_name.bright_red());
            }
            println!(
                "  {}",
                format!(
                    "Total translation failures: {}",
                    self.translation_failed_files.len()
                )
                .bright_red()
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

    /// 获取统计文件路径
    pub fn get_stats_file_path(feature: &str) -> Result<PathBuf> {
        crate::util::validate_feature_name(feature)?;
        let project_root = crate::util::find_project_root()?;
        Ok(project_root
            .join(".c2rust")
            .join(feature)
            .join("translation_stats.json"))
    }

    /// 从 JSON 文件加载统计数据
    pub fn load_from_file(feature: &str) -> Result<Option<Self>> {
        let path = Self::get_stats_file_path(feature)?;
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                let mut stats: Self = serde_json::from_str(&contents)
                    .with_context(|| format!("Failed to parse stats file: {}", path.display()))?;
                let _ = stats.normalize_file_keys();
                Ok(Some(stats))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => {
                Err(e).with_context(|| format!("Failed to read stats file: {}", path.display()))
            }
        }
    }

    /// 保存统计数据到 JSON 文件
    pub fn save_to_file(&self, feature: &str) -> Result<()> {
        let path = Self::get_stats_file_path(feature)?;
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        let contents = serde_json::to_string_pretty(self).context("Failed to serialize stats")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write stats file: {}", path.display()))?;
        Ok(())
    }

    /// Reconcile persisted stats with the current workspace state.
    ///
    /// This is intentionally conservative:
    /// - only non-empty translatable `fun_*.rs` / `var_*.rs` files are considered
    /// - existing per-file stats are preserved
    /// - newly discovered completed files are backfilled as first-try successes
    ///
    /// This keeps `translation_stats.json` usable after workspace migration,
    /// partial metadata loss, or flows where translated files already exist on disk
    /// but stats were not written correctly.
    pub fn reconcile_with_workspace(&mut self, rust_dir: &Path) -> Result<bool> {
        let mut discovered = Vec::new();
        for entry in WalkDir::new(rust_dir) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !stem.starts_with("fun_") && !stem.starts_with("var_") {
                continue;
            }
            if entry.metadata()?.len() == 0 {
                continue;
            }
            let rel = path
                .strip_prefix(rust_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            discovered.push(canonicalize_stats_file_key(&rel));
        }
        discovered.sort();
        discovered.dedup();

        let mut changed = false;
        for file_name in discovered {
            if self.file_attempts.contains_key(&file_name)
                || self.skipped_files.iter().any(|item| item == &file_name)
                || self
                    .translation_failed_files
                    .iter()
                    .any(|item| item == &file_name)
            {
                continue;
            }
            self.record_file_completion(file_name, 1, false, 0);
            changed = true;
        }
        Ok(changed)
    }

    /// 清空统计文件（开始新会话）
    pub fn clear_stats_file(feature: &str) -> Result<()> {
        let path = Self::get_stats_file_path(feature)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                Err(e).with_context(|| format!("Failed to remove stats file: {}", path.display()))
            }
        }
    }

    /// Returns the names of all files that have been attempted (including successfully translated ones).
    ///
    /// Note: the main translation loop relies on disk-based empty-file detection
    /// (`find_empty_rs_files`) to skip already-translated files, so this method is
    /// not called directly in the current workflow. It is kept as part of the public
    /// statistics API for future use (e.g., progress visualisation, report generation).
    pub fn get_completed_files(&self) -> Vec<String> {
        self.file_attempts.keys().cloned().collect()
    }

    pub(crate) fn normalize_file_keys(&mut self) -> bool {
        let before = serde_json::to_string(self).ok();
        let previous_attempts = std::mem::take(&mut self.file_attempts);
        let previous_skipped = std::mem::take(&mut self.skipped_files);
        let previous_failed = std::mem::take(&mut self.translation_failed_files);

        self.total_files = 0;
        self.success_first_try = 0;
        self.success_retry_1 = 0;
        self.success_retry_2 = 0;
        self.success_retry_3_plus = 0;
        self.restart_count = 0;

        for (file_name, stat) in previous_attempts {
            let canonical_name = canonicalize_stats_file_key(&file_name);
            match self.file_attempts.entry(canonical_name) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(stat);
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    if stat.translation_attempts > existing.translation_attempts
                        || (stat.translation_attempts == existing.translation_attempts
                            && stat.fix_attempts > existing.fix_attempts)
                    {
                        existing.translation_attempts = stat.translation_attempts;
                        existing.fix_attempts = stat.fix_attempts;
                    }
                    existing.had_restart |= stat.had_restart;
                }
            }
        }

        for stat in self.file_attempts.values() {
            self.total_files += 1;
            match stat.translation_attempts {
                1 => self.success_first_try += 1,
                2 => self.success_retry_1 += 1,
                3 => self.success_retry_2 += 1,
                _ => self.success_retry_3_plus += 1,
            }
            if stat.had_restart {
                self.restart_count += 1;
            }
        }

        for file_name in previous_skipped {
            self.record_file_skipped(file_name);
        }
        for file_name in previous_failed {
            self.record_file_translation_failed(file_name);
        }

        let after = serde_json::to_string(self).ok();
        before != after
    }
}

fn canonicalize_stats_file_key(file_name: &str) -> String {
    let normalized = file_name.replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./").trim_start_matches('/');
    if trimmed.is_empty() {
        return "src".to_string();
    }
    if trimmed.starts_with("src/") || trimmed == "src" {
        trimmed.to_string()
    } else {
        format!("src/{}", trimmed)
    }
}

// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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

        let entry = stats.file_attempts.get("src/foo.rs").unwrap();
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

        let entry = stats.file_attempts.get("src/bar.rs").unwrap();
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

        let entry = stats.file_attempts.get("src/restart.rs").unwrap();
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
        assert_eq!(stats.skipped_files[0], "src/foo.rs");

        stats.record_file_skipped("bar.rs".to_string());
        assert_eq!(stats.skipped_files.len(), 2);
        assert_eq!(stats.skipped_files[1], "src/bar.rs");

        // Duplicate entries should not be added
        stats.record_file_skipped("foo.rs".to_string());
        assert_eq!(stats.skipped_files.len(), 2);
    }

    #[test]
    fn test_record_file_translation_failed() {
        let mut stats = TranslationStats::new();
        assert!(stats.translation_failed_files.is_empty());
        assert!(stats.skipped_files.is_empty());

        stats.record_file_translation_failed("bad.rs".to_string());
        assert_eq!(stats.translation_failed_files.len(), 1);
        assert_eq!(stats.translation_failed_files[0], "src/bad.rs");

        // Duplicate entries should not be added
        stats.record_file_translation_failed("bad.rs".to_string());
        assert_eq!(stats.translation_failed_files.len(), 1);

        // Should not affect skipped_files
        assert!(stats.skipped_files.is_empty());
    }

    // ========================================================================
    // TranslationStats Persistence Tests
    // ========================================================================

    #[test]
    fn test_get_completed_files() {
        let mut stats = TranslationStats::new();
        assert!(stats.get_completed_files().is_empty());

        stats.record_file_completion("a.rs".to_string(), 1, false, 0);
        stats.record_file_completion("b.rs".to_string(), 2, true, 3);

        let mut completed = stats.get_completed_files();
        completed.sort();
        assert_eq!(completed, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn test_save_and_load_from_file() {
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();
        let feature_dir = c2rust_dir.join("test_feature");
        fs::create_dir(&feature_dir).unwrap();

        // Build stats
        let mut stats = TranslationStats::new();
        stats.record_file_completion("src/foo.rs".to_string(), 1, false, 0);
        stats.record_file_completion("src/bar.rs".to_string(), 2, true, 5);
        stats.record_file_skipped("src/skip.rs".to_string());

        // Save directly to the expected path (bypassing find_project_root)
        let stats_path = feature_dir.join("translation_stats.json");
        let contents = serde_json::to_string_pretty(&stats).unwrap();
        fs::write(&stats_path, &contents).unwrap();

        // Load back
        let loaded: TranslationStats =
            serde_json::from_str(&fs::read_to_string(&stats_path).unwrap()).unwrap();

        assert_eq!(loaded.total_files, 2);
        assert_eq!(loaded.success_first_try, 1);
        assert_eq!(loaded.success_retry_1, 1);
        assert_eq!(loaded.restart_count, 1);
        assert_eq!(loaded.skipped_files, vec!["src/skip.rs"]);
        assert!(loaded.file_attempts.contains_key("src/foo.rs"));
        assert!(loaded.file_attempts.contains_key("src/bar.rs"));
    }

    #[test]
    fn test_stats_json_roundtrip() {
        let mut stats = TranslationStats::new();
        stats.record_file_completion("x.rs".to_string(), 3, true, 7);
        stats.record_file_skipped("y.rs".to_string());
        stats.record_file_translation_failed("z.rs".to_string());

        let json = serde_json::to_string(&stats).unwrap();
        let restored: TranslationStats = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.total_files, 1);
        assert_eq!(restored.success_retry_2, 1);
        assert_eq!(restored.restart_count, 1);
        assert_eq!(restored.skipped_files, vec!["src/y.rs"]);
        assert_eq!(restored.translation_failed_files, vec!["src/z.rs"]);

        let entry = restored.file_attempts.get("src/x.rs").unwrap();
        assert_eq!(entry.translation_attempts, 3);
        assert_eq!(entry.fix_attempts, 7);
        assert!(entry.had_restart);
    }

    #[test]
    fn test_reconcile_with_workspace_backfills_nonempty_translatable_files() {
        let temp_dir = tempdir().unwrap();
        let rust_dir = temp_dir.path().join("rust").join("src");
        fs::create_dir_all(rust_dir.join("mod_a")).unwrap();
        fs::write(rust_dir.join("mod_a").join("fun_done.rs"), "pub fn done() {}\n").unwrap();
        fs::write(rust_dir.join("mod_a").join("var_done.rs"), "pub static X: i32 = 1;\n").unwrap();
        fs::write(rust_dir.join("mod_a").join("fun_empty.rs"), "").unwrap();
        fs::write(rust_dir.join("mod_a").join("decl_only.rs"), "extern \"C\" {}\n").unwrap();

        let mut stats = TranslationStats::new();
        let changed = stats.reconcile_with_workspace(&rust_dir).unwrap();

        assert!(changed);
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.success_first_try, 2);
        assert!(stats.file_attempts.contains_key("src/mod_a/fun_done.rs"));
        assert!(stats.file_attempts.contains_key("src/mod_a/var_done.rs"));
        assert!(!stats.file_attempts.contains_key("src/mod_a/fun_empty.rs"));
        assert!(!stats.file_attempts.contains_key("src/mod_a/decl_only.rs"));
    }

    #[test]
    fn test_reconcile_with_workspace_preserves_existing_records() {
        let temp_dir = tempdir().unwrap();
        let rust_dir = temp_dir.path().join("rust").join("src");
        fs::create_dir_all(rust_dir.join("mod_a")).unwrap();
        fs::write(rust_dir.join("mod_a").join("fun_done.rs"), "pub fn done() {}\n").unwrap();

        let mut stats = TranslationStats::new();
        stats.record_file_completion("src/mod_a/fun_done.rs".to_string(), 2, true, 3);

        let changed = stats.reconcile_with_workspace(&rust_dir).unwrap();

        assert!(!changed);
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.success_retry_1, 1);
        let entry = stats.file_attempts.get("src/mod_a/fun_done.rs").unwrap();
        assert_eq!(entry.translation_attempts, 2);
        assert_eq!(entry.fix_attempts, 3);
        assert!(entry.had_restart);
    }

    #[test]
    fn test_reconcile_with_workspace_merges_legacy_and_canonical_keys() {
        let temp_dir = tempdir().unwrap();
        let rust_dir = temp_dir.path().join("rust").join("src");
        fs::create_dir_all(rust_dir.join("mod_a")).unwrap();
        fs::write(rust_dir.join("mod_a").join("fun_done.rs"), "pub fn done() {}\n").unwrap();

        let mut stats = TranslationStats::new();
        stats.file_attempts.insert(
            "mod_a/fun_done.rs".to_string(),
            FileAttemptStat {
                translation_attempts: 2,
                fix_attempts: 3,
                had_restart: true,
            },
        );
        stats.normalize_file_keys();

        let changed = stats.reconcile_with_workspace(&rust_dir).unwrap();

        assert!(!changed);
        assert_eq!(stats.total_files, 1);
        assert!(stats.file_attempts.contains_key("src/mod_a/fun_done.rs"));
        assert!(!stats.file_attempts.contains_key("mod_a/fun_done.rs"));
    }

    #[test]
    fn test_clear_target_history_clears_legacy_key_via_canonical_target() {
        let mut stats = TranslationStats::new();
        stats.file_attempts.insert(
            "mod_a/fun_done.rs".to_string(),
            FileAttemptStat {
                translation_attempts: 1,
                fix_attempts: 0,
                had_restart: false,
            },
        );
        stats.skipped_files.push("mod_a/fun_done.rs".to_string());
        stats.translation_failed_files
            .push("mod_a/fun_done.rs".to_string());
        stats.normalize_file_keys();

        stats.clear_target_history("src/mod_a/fun_done.rs");

        assert_eq!(stats.total_files, 0);
        assert!(stats.file_attempts.is_empty());
        assert!(stats.skipped_files.is_empty());
        assert!(stats.translation_failed_files.is_empty());
    }

    #[test]
    fn test_canonicalize_stats_file_key_prefixes_src() {
        assert_eq!(canonicalize_stats_file_key("mod_a/fun_done.rs"), "src/mod_a/fun_done.rs");
        assert_eq!(canonicalize_stats_file_key("src/mod_a/fun_done.rs"), "src/mod_a/fun_done.rs");
        assert_eq!(canonicalize_stats_file_key("./mod_a/fun_done.rs"), "src/mod_a/fun_done.rs");
        assert_eq!(canonicalize_stats_file_key(r"mod_a\fun_done.rs"), "src/mod_a/fun_done.rs");
    }
}


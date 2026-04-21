use std::path::PathBuf;

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

    /// 将文件标记为已处理（递增计数器，不超过 total_count）
    pub fn mark_processed(&mut self) {
        if self.processed_count < self.total_count {
            self.processed_count += 1;
        }
    }

    /// Update progress state from caller-supplied actuals.
    pub(crate) fn refresh(&mut self, total: usize, processed: usize) {
        self.total_count = total;
        self.processed_count = processed.min(total);
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

// Suppress unused import warning - PathBuf is used in the public API type signature
// via the module re-export chain.
const _: fn() = || {
    let _: PathBuf = PathBuf::new();
};

#[cfg(test)]
mod tests {
    use super::*;

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
        let state = ProgressState::with_initial_progress(10, 3);
        assert_eq!(state.processed_count, 3);
        assert_eq!(state.total_count, 10);
        assert_eq!(state.get_current_position(), 4);
    }

    #[test]
    fn test_with_initial_progress_continuation() {
        let mut state = ProgressState::with_initial_progress(10, 5);

        assert_eq!(state.get_current_position(), 6);

        state.mark_processed();
        assert_eq!(state.get_current_position(), 7);
        assert_eq!(state.processed_count, 6);
    }

    #[test]
    fn test_with_initial_progress_clamping() {
        let state = ProgressState::with_initial_progress(10, 15);
        assert_eq!(state.processed_count, 10);
        assert_eq!(state.total_count, 10);
        assert_eq!(state.get_current_position(), 11);

        let state2 = ProgressState::with_initial_progress(10, 10);
        assert_eq!(state2.processed_count, 10);
        assert_eq!(state2.get_current_position(), 11);
    }

    #[test]
    fn test_refresh_updates_counts() {
        let mut state = ProgressState::new(10);
        state.refresh(20, 15);
        assert_eq!(state.total_count, 20);
        assert_eq!(state.processed_count, 15);
    }

    #[test]
    fn test_refresh_clamps_processed_to_total() {
        let mut state = ProgressState::new(10);
        state.refresh(5, 8);
        assert_eq!(state.total_count, 5);
        assert_eq!(state.processed_count, 5);
    }

    #[test]
    fn test_refresh_processed_equals_total() {
        let mut state = ProgressState::new(3);
        state.refresh(3, 3);
        assert_eq!(state.total_count, 3);
        assert_eq!(state.processed_count, 3);
    }

    #[test]
    fn test_mark_processed_caps_at_total_count() {
        let mut state = ProgressState::new(2);
        state.mark_processed();
        state.mark_processed();
        assert_eq!(state.processed_count, 2);
        state.mark_processed();
        assert_eq!(state.processed_count, 2);
    }
}

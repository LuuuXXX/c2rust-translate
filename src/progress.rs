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

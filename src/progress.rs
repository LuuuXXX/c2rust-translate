#[derive(Debug, Default)]
pub struct ProgressState {
    /// Total number of files processed (includes files from previous runs and current session)
    pub processed_count: usize,
    /// Total number of files to process (for display purposes)
    pub total_count: usize,
}

impl ProgressState {
    /// Create a new progress state with total count
    pub fn new(total_count: usize) -> Self {
        Self {
            processed_count: 0,
            total_count,
        }
    }

    /// Create a new progress state with both total and already-processed counts.
    /// The `already_processed` value is clamped to not exceed `total_count`.
    pub fn with_initial_progress(total_count: usize, already_processed: usize) -> Self {
        Self {
            processed_count: already_processed.min(total_count),
            total_count,
        }
    }

    /// Mark a file as processed (increment counter)
    pub fn mark_processed(&mut self) {
        self.processed_count += 1;
    }

    /// Get the current progress position (1-indexed for display)
    pub fn get_current_position(&self) -> usize {
        self.processed_count + 1
    }

    /// Get the total count of files to process
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
        // Test creating progress state with already-processed files
        let state = ProgressState::with_initial_progress(10, 3);
        assert_eq!(state.processed_count, 3);
        assert_eq!(state.total_count, 10);
        
        // Current position should be 4 (3 processed + 1)
        assert_eq!(state.get_current_position(), 4);
    }

    #[test]
    fn test_with_initial_progress_continuation() {
        // Simulate a scenario where 5 out of 10 files are already processed
        let mut state = ProgressState::with_initial_progress(10, 5);
        
        // Next file to process should show as [6/10]
        assert_eq!(state.get_current_position(), 6);
        
        // After processing one more file, should show as [7/10]
        state.mark_processed();
        assert_eq!(state.get_current_position(), 7);
        assert_eq!(state.processed_count, 6);
    }

    #[test]
    fn test_with_initial_progress_clamping() {
        // Test that already_processed is clamped to total_count
        let state = ProgressState::with_initial_progress(10, 15);
        assert_eq!(state.processed_count, 10); // Should be clamped to 10
        assert_eq!(state.total_count, 10);
        assert_eq!(state.get_current_position(), 11); // 10 + 1
        
        // Test edge case: already_processed equals total_count
        let state2 = ProgressState::with_initial_progress(10, 10);
        assert_eq!(state2.processed_count, 10);
        assert_eq!(state2.get_current_position(), 11);
    }
}

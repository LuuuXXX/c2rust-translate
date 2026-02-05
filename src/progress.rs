use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProgressState {
    /// Total number of files processed
    pub processed_count: usize,
    /// List of processed file paths (relative to rust directory)
    pub processed_files: Vec<String>,
    /// Total number of files to process (for display purposes)
    #[serde(default)]
    pub total_count: usize,
}

impl ProgressState {
    /// Get the progress file path for a feature
    fn get_progress_file_path(feature: &str) -> Result<PathBuf> {
        let project_root = crate::util::find_project_root()?;
        let progress_file = project_root
            .join(".c2rust")
            .join(feature)
            .join("progress.json");
        Ok(progress_file)
    }

    /// Load progress state from file, or return default if file doesn't exist
    pub fn load(feature: &str) -> Result<Self> {
        let progress_file = Self::get_progress_file_path(feature)?;

        if !progress_file.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&progress_file)
            .with_context(|| format!("Failed to read progress file: {}", progress_file.display()))?;

        let state: ProgressState = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse progress file: {}", progress_file.display()))?;

        Ok(state)
    }

    /// Save progress state to file
    pub fn save(&self, feature: &str) -> Result<()> {
        let progress_file = Self::get_progress_file_path(feature)?;

        // Ensure parent directory exists
        if let Some(parent) = progress_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize progress state")?;

        fs::write(&progress_file, content)
            .with_context(|| format!("Failed to write progress file: {}", progress_file.display()))?;

        Ok(())
    }

    /// Check if a file has been processed
    pub fn is_processed(&self, file_path: &Path, rust_dir: &Path) -> bool {
        if let Ok(relative_path) = file_path.strip_prefix(rust_dir) {
            if let Some(path_str) = relative_path.to_str() {
                return self.processed_files.contains(&path_str.to_string());
            }
        }
        false
    }

    /// Mark a file as processed
    pub fn mark_processed(&mut self, file_path: &Path, rust_dir: &Path) -> Result<()> {
        let relative_path = file_path.strip_prefix(rust_dir)
            .context("File path is not within rust directory")?;
        
        let path_str = relative_path.to_str()
            .context("Non-UTF8 path")?
            .to_string();

        if !self.processed_files.contains(&path_str) {
            self.processed_files.push(path_str);
            self.processed_count += 1;
        }

        Ok(())
    }

    /// Get the current progress count (1-indexed for display)
    pub fn get_current_position(&self) -> usize {
        self.processed_count + 1
    }

    /// Set the total count of files to process
    pub fn set_total_count(&mut self, total: usize) {
        self.total_count = total;
    }

    /// Get the total count of files to process
    pub fn get_total_count(&self) -> usize {
        self.total_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_progress_state_default() {
        let state = ProgressState::default();
        assert_eq!(state.processed_count, 0);
        assert_eq!(state.processed_files.len(), 0);
    }

    #[test]
    fn test_mark_processed() {
        let temp_dir = tempdir().unwrap();
        let rust_dir = temp_dir.path().join("rust");
        fs::create_dir(&rust_dir).unwrap();
        
        let file_path = rust_dir.join("var_test.rs");
        fs::File::create(&file_path).unwrap();

        let mut state = ProgressState::default();
        state.mark_processed(&file_path, &rust_dir).unwrap();

        assert_eq!(state.processed_count, 1);
        assert_eq!(state.processed_files.len(), 1);
        assert_eq!(state.processed_files[0], "var_test.rs");
    }

    #[test]
    fn test_is_processed() {
        let temp_dir = tempdir().unwrap();
        let rust_dir = temp_dir.path().join("rust");
        fs::create_dir(&rust_dir).unwrap();
        
        let file_path = rust_dir.join("var_test.rs");
        fs::File::create(&file_path).unwrap();

        let mut state = ProgressState::default();
        assert!(!state.is_processed(&file_path, &rust_dir));

        state.mark_processed(&file_path, &rust_dir).unwrap();
        assert!(state.is_processed(&file_path, &rust_dir));
    }

    #[test]
    fn test_get_current_position() {
        let mut state = ProgressState::default();
        assert_eq!(state.get_current_position(), 1);

        state.processed_count = 5;
        assert_eq!(state.get_current_position(), 6);
    }

    #[test]
    fn test_mark_processed_duplicate() {
        let temp_dir = tempdir().unwrap();
        let rust_dir = temp_dir.path().join("rust");
        fs::create_dir(&rust_dir).unwrap();
        
        let file_path = rust_dir.join("var_test.rs");
        fs::File::create(&file_path).unwrap();

        let mut state = ProgressState::default();
        state.mark_processed(&file_path, &rust_dir).unwrap();
        state.mark_processed(&file_path, &rust_dir).unwrap();

        // Should not duplicate
        assert_eq!(state.processed_count, 1);
        assert_eq!(state.processed_files.len(), 1);
    }
}

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

// Re-exports for backward compatibility
pub use crate::stats::{FileAttemptStat, TranslationStats};
pub use crate::progress::{
    CODE_PREVIEW_LINES, ERROR_PREVIEW_LINES, MAX_TRANSLATION_ATTEMPTS, ProgressState,
};

/// 从起始路径向上搜索 .c2rust 目录以查找项目根目录
fn find_project_root_from(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path.to_path_buf();

    loop {
        let c2rust_dir = current.join(".c2rust");

        match std::fs::metadata(&c2rust_dir) {
            Ok(metadata) if metadata.is_dir() => {
                return Ok(current);
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
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
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();

        let subdir1 = temp_dir.path().join("subdir1");
        let subdir2 = subdir1.join("subdir2");
        fs::create_dir_all(&subdir2).unwrap();

        let result = find_project_root_from(&subdir2);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), temp_dir.path());
    }

    #[test]
    fn test_find_project_root_not_found() {
        let temp_dir = tempdir().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let result = find_project_root_from(&subdir);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Could not find .c2rust"));
    }

    #[test]
    fn test_find_project_root_from_root_dir() {
        let temp_dir = tempdir().unwrap();
        let c2rust_dir = temp_dir.path().join(".c2rust");
        fs::create_dir(&c2rust_dir).unwrap();

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
        assert!(validate_feature_name("feature/path").is_err());
        assert!(validate_feature_name("feature\\path").is_err());
        assert!(validate_feature_name("..").is_err());
        assert!(validate_feature_name("../feature").is_err());
        assert!(validate_feature_name("feature/../other").is_err());
        assert!(validate_feature_name("").is_err());
    }
}

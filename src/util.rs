use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

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
                    format!("Failed to access .c2rust directory at {}", c2rust_dir.display())
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
    let current = std::env::current_dir()
        .context("Failed to get current directory")?;
    find_project_root_from(&current)
}

/// 验证功能名称以防止路径遍历攻击
pub fn validate_feature_name(feature: &str) -> Result<()> {
    if feature.contains('/') || feature.contains('\\') || feature.contains("..") || feature.is_empty() {
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
}

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::Write;
use crate::util;

/// Get the translate script directory from environment variable
/// 
/// The environment variable should contain the path to the directory
/// containing the translate_and_fix.py script.
fn get_translate_script_dir() -> Result<PathBuf> {
    match std::env::var("C2RUST_TRANSLATE_DIT") {
        Ok(path) => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                anyhow::bail!("Environment variable C2RUST_TRANSLATE_DIT is empty. Please set it to the directory containing translate_and_fix.py script.");
            }
            Ok(PathBuf::from(trimmed))
        }
        Err(std::env::VarError::NotPresent) => {
            anyhow::bail!("Environment variable C2RUST_TRANSLATE_DIT is not set. Please set it to the directory containing translate_and_fix.py script.");
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("Environment variable C2RUST_TRANSLATE_DIT contains non-UTF8 data. Please ensure it contains a valid UTF-8 path.");
        }
    }
}

/// Get the full path to the translate_and_fix.py script
/// 
/// This reads the directory path from C2RUST_TRANSLATE_DIT environment variable
/// and appends the script filename.
fn get_translate_script_full_path() -> Result<PathBuf> {
    let translate_script_dir = get_translate_script_dir()?;
    Ok(translate_script_dir.join("translate_and_fix.py"))
}

/// Get the config.toml path by searching for .c2rust directory
fn get_config_path() -> Result<PathBuf> {
    let project_root = util::find_project_root()?;
    Ok(project_root.join(".c2rust/config.toml"))
}

/// Build the argument list for the fix command
/// 
/// Returns a vector of arguments to be passed to translate_and_fix.py for fixing errors.
/// The arguments follow the format: --config --type fix --code --output --error
/// 
/// # Parameters
/// - `script_path`: Path to the translate_and_fix.py script
/// - `config_path`: Path to the config.toml file
/// - `code_file`: Path to the Rust file to be fixed (input)
/// - `output_file`: Path where the fixed result should be written (typically same as code_file)
/// - `error_file`: Path to the temporary file containing compiler error messages
fn build_fix_args<'a>(
    script_path: &'a str,
    config_path: &'a str,
    code_file: &'a str,
    output_file: &'a str,
    error_file: &'a str,
) -> Vec<&'a str> {
    vec![
        script_path,
        "--config",
        config_path,
        "--type",
        "fix",
        "--code",
        code_file,
        "--output",
        output_file,
        "--error",
        error_file,
    ]
}

/// Translate a C file to Rust using the translation tool
pub fn translate_c_to_rust(feature: &str, file_type: &str, c_file: &Path, rs_file: &Path) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let config_path = get_config_path()?;
    let work_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    // Verify working directory exists
    if !work_dir.exists() {
        anyhow::bail!(
            "Working directory does not exist: {}. Expected directory structure: <project_root>/.c2rust/<feature>/rust",
            work_dir.display()
        );
    }
    
    // Get translate script path from environment variable
    let script_path = get_translate_script_full_path()?;
    let script_str = script_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", script_path.display()))?;
    
    let config_str = config_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", config_path.display()))?;
    let c_file_str = c_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", c_file.display()))?;
    let rs_file_str = rs_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", rs_file.display()))?;
    
    println!("Executing translation command:");
    println!("python {} --config {} --type {} --code {} --output {}", 
        script_str, config_str, file_type, c_file_str, rs_file_str);
    println!();
    
    let output = Command::new("python")
        .args(&[
            script_str,
            "--config",
            config_str,
            "--type",
            file_type,
            "--code",
            c_file_str,
            "--output",
            rs_file_str,
        ])
        .output()
        .context("Failed to execute translate_and_fix.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Translation failed: {}", stderr);
    }

    Ok(())
}

/// Fix translation errors using the translation tool
/// 
/// # Parameters
/// - `feature`: The feature name being translated
/// - `_file_type`: Kept for backward compatibility but not used (fix always uses type="fix")
/// - `rs_file`: The Rust file to be fixed (serves as both input --code and output --output)
/// - `error_msg`: The compiler error message to be written to a temporary file
pub fn fix_translation_error(feature: &str, _file_type: &str, rs_file: &Path, error_msg: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let config_path = get_config_path()?;
    let work_dir = project_root.join(".c2rust").join(feature).join("rust");
    
    // Verify working directory exists
    if !work_dir.exists() {
        anyhow::bail!(
            "Working directory does not exist: {}. Expected directory structure: <project_root>/.c2rust/<feature>/rust",
            work_dir.display()
        );
    }
    
    // Create a unique temporary file with error message
    let mut temp_file = tempfile::NamedTempFile::new()
        .context("Failed to create temporary error file")?;
    write!(temp_file, "{}", error_msg)
        .context("Failed to write error message to temp file")?;
    
    // Get translate script path from environment variable
    let script_path = get_translate_script_full_path()?;
    let script_str = script_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", script_path.display()))?;
    
    let config_str = config_path.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", config_path.display()))?;
    let error_file_str = temp_file.path().to_str()
        .with_context(|| format!("Non-UTF8 path: {}", temp_file.path().display()))?;
    let rs_file_str = rs_file.to_str()
        .with_context(|| format!("Non-UTF8 path: {}", rs_file.display()))?;

    // Note: --code and --output both point to rs_file, which means the Python script
    // will read the original file and overwrite it with the fixed version.
    // This is the intended behavior as specified in the requirements.
    println!("Executing error fix command:");
    println!("python {} --config {} --type fix --code {} --output {} --error {}", 
        script_str, config_str, rs_file_str, rs_file_str, error_file_str);
    println!();

    // Build fix command arguments.
    // Note: rs_file_str is used for both code_file and output_file parameters,
    // meaning the Python script reads from rs_file and overwrites it with the fix.
    let args = build_fix_args(
        script_str,
        config_str,
        rs_file_str,  // code_file: input file to fix
        rs_file_str,  // output_file: where to write fixed result (overwrites input)
        error_file_str,
    );

    let output = Command::new("python")
        .args(&args)
        .output()
        .context("Failed to execute translate_and_fix.py for fixing")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Fix failed: {}", stderr);
    }

    // temp_file is automatically deleted when it goes out of scope
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use super::*;
    use serial_test::serial;
    
    /// Guard to ensure environment variable is restored even on panic
    struct EnvVarGuard {
        key: &'static str,
        original_value: Option<std::ffi::OsString>,
    }
    
    impl EnvVarGuard {
        fn new(key: &'static str) -> Self {
            let original_value = std::env::var_os(key);
            Self { key, original_value }
        }
        
        fn set(&self, value: &str) {
            std::env::set_var(self.key, value);
        }
        
        fn remove(&self) {
            std::env::remove_var(self.key);
        }
    }
    
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original_value {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
    
    #[test]
    fn test_temp_error_file_creation() {
        let test_msg = "test error message";

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", test_msg).unwrap();

        let path = temp_file.path();
        let content = std::fs::read_to_string(path).unwrap();

        assert_eq!(content, test_msg);
        // temp_file is automatically deleted when it goes out of scope
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_not_set() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.remove();
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("not set"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_empty() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("");
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("empty"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_whitespace() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("   ");
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("empty"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_valid() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("/path/to/scripts");
        
        let result = get_translate_script_dir();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/path/to/scripts"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_full_path() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("/path/to/scripts");
        
        let result = get_translate_script_full_path();
        assert!(result.is_ok());
        
        let path = result.unwrap();
        assert_eq!(path, PathBuf::from("/path/to/scripts/translate_and_fix.py"));
    }
    
    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_get_translate_script_dir_non_utf8() {
        use std::os::unix::ffi::OsStringExt;
        
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        
        // Create an invalid UTF-8 sequence
        let invalid_utf8 = std::ffi::OsString::from_vec(vec![0xFF, 0xFE, 0xFD]);
        std::env::set_var("C2RUST_TRANSLATE_DIT", &invalid_utf8);
        
        let result = get_translate_script_dir();
        assert!(result.is_err());
        
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("C2RUST_TRANSLATE_DIT"));
        assert!(err_msg.contains("non-UTF8"));
    }
    
    #[test]
    #[serial]
    fn test_get_translate_script_dir_whitespace_trimming() {
        let _guard = EnvVarGuard::new("C2RUST_TRANSLATE_DIT");
        _guard.set("  /path/to/scripts  ");
        
        let result = get_translate_script_dir();
        assert!(result.is_ok());
        // Should be trimmed
        assert_eq!(result.unwrap(), PathBuf::from("/path/to/scripts"));
    }
    
    #[test]
    fn test_build_fix_args() {
        let script = "/path/to/translate_and_fix.py";
        let config = "/project/.c2rust/config.toml";
        let code = "/project/feature/rust/code.rs";
        let output = "/project/feature/rust/code.rs";
        let error = "/tmp/error.txt";
        
        let args = build_fix_args(script, config, code, output, error);
        
        // Verify the exact sequence of arguments
        assert_eq!(args.len(), 11);
        assert_eq!(args[0], script);
        assert_eq!(args[1], "--config");
        assert_eq!(args[2], config);
        assert_eq!(args[3], "--type");
        assert_eq!(args[4], "fix");
        assert_eq!(args[5], "--code");
        assert_eq!(args[6], code);
        assert_eq!(args[7], "--output");
        assert_eq!(args[8], output);
        assert_eq!(args[9], "--error");
        assert_eq!(args[10], error);
    }
}

//! C to Rust translation workflow orchestration
//!
//! This module provides the main translation workflow that coordinates initialization,
//! gate verification, file selection, and translation execution across multiple modules.

pub mod analyzer;
pub mod build;
pub mod git;
pub(crate) mod progress;
pub(crate) mod stats;
pub mod suggestion;
pub mod translation;
pub mod ui;
pub mod util;
pub(crate) mod workflow;

pub use workflow::{translate_feature, verify_feature};
pub(crate) use workflow::{apply_error_fix, apply_warning_fix};

// ============================================================================
// Environment-Variable Helpers
// ============================================================================

/// Returns `true` when warning processing is enabled (the default).
///
/// Set `C2RUST_PROCESS_WARNINGS=0` (or `false`) to skip Phase 2 (warning
/// detection and auto-fix) for every file processed in a run.
pub(crate) fn should_process_warnings() -> bool {
    match std::env::var("C2RUST_PROCESS_WARNINGS") {
        Ok(val) => {
            let val = val.trim();
            val != "0" && !val.eq_ignore_ascii_case("false")
        }
        Err(_) => true,
    }
}

/// Returns `true` when test failures should not interrupt the workflow.
///
/// Set `C2RUST_TEST_CONTINUE_ON_ERROR=1` (or `true`/`yes`) to treat
/// `c2rust_test` failures as non-fatal warnings, allowing subsequent tasks to
/// continue running instead of aborting.  By default (env var absent or set to
/// any other value) a test failure is fatal.
pub(crate) fn should_continue_on_test_error() -> bool {
    match std::env::var("C2RUST_TEST_CONTINUE_ON_ERROR") {
        Ok(val) => {
            let val = val.trim();
            val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes")
        }
        Err(_) => false,
    }
}

/// Returns `true` when the process should automatically retry translation upon reaching the
/// maximum number of fix attempts, without prompting the user.
///
/// Set `C2RUST_AUTO_RETRY_ON_MAX_FIX=1` (or `true`/`yes`) to automatically choose
/// "Retry directly" when fix attempts are exhausted, ensuring fully unattended runs.
/// When on the last translation attempt (no more retries available), the file is
/// automatically skipped instead so the overall process can continue.
/// By default (env var absent or set to any other value) the interactive prompt is shown.
pub(crate) fn should_auto_retry_on_max_fix_attempts() -> bool {
    match std::env::var("C2RUST_AUTO_RETRY_ON_MAX_FIX") {
        Ok(val) => {
            let val = val.trim();
            val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes")
        }
        Err(_) => false,
    }
}

/// Returns the test interval: run hybrid build tests once every N successful translations.
///
/// Set `C2RUST_TEST_INTERVAL=N` (a positive integer) to run tests only after every N-th
/// completed translation instead of after every single translation.  The default is `1`
/// (run tests after every translation), which preserves the existing behaviour.
///
/// Invalid values (zero, non-numeric, or empty) fall back to the default of `1`.
pub(crate) fn get_test_interval() -> usize {
    match std::env::var("C2RUST_TEST_INTERVAL") {
        Ok(val) => match val.trim().parse::<usize>() {
            Ok(n) if n > 0 => n,
            _ => 1,
        },
        Err(_) => 1,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        prior: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, prior }
        }
        fn remove(key: &'static str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, prior }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prior {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    struct CurrentDirGuard {
        prior: PathBuf,
    }

    impl CurrentDirGuard {
        fn change_to(path: &Path) -> Self {
            let prior = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { prior }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.prior).unwrap();
        }
    }

    fn create_temp_feature_workspace(
        feature: &str,
    ) -> (tempfile::TempDir, CurrentDirGuard, PathBuf, PathBuf) {
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let feature_root = project_root.join(".c2rust").join(feature);
        let rust_dir = feature_root.join("rust");
        fs::create_dir_all(rust_dir.join("src")).unwrap();
        let guard = CurrentDirGuard::change_to(&project_root);
        (temp_dir, guard, feature_root, rust_dir)
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_default() {
        let _guard = EnvGuard::remove("C2RUST_PROCESS_WARNINGS");
        assert!(should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_disabled_with_zero() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "0");
        assert!(!should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_disabled_with_false() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "false");
        assert!(!should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_disabled_with_false_uppercase() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "FALSE");
        assert!(!should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_enabled_with_one() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "1");
        assert!(should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_process_warnings_enabled_with_true() {
        let _guard = EnvGuard::set("C2RUST_PROCESS_WARNINGS", "true");
        assert!(should_process_warnings());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_default() {
        let _guard = EnvGuard::remove("C2RUST_TEST_CONTINUE_ON_ERROR");
        assert!(!should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_one() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "1");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_true() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "true");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_true_uppercase() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "TRUE");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_yes() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "yes");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_enabled_with_yes_uppercase() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "YES");
        assert!(should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_disabled_with_zero() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "0");
        assert!(!should_continue_on_test_error());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_continue_on_test_error_disabled_with_false() {
        let _guard = EnvGuard::set("C2RUST_TEST_CONTINUE_ON_ERROR", "false");
        assert!(!should_continue_on_test_error());
    }

    // should_auto_retry_on_max_fix_attempts Tests
    // ========================================================================

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_default() {
        let _guard = EnvGuard::remove("C2RUST_AUTO_RETRY_ON_MAX_FIX");
        assert!(!should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_one() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "1");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_true() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "true");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_true_uppercase() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "TRUE");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_yes() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "yes");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_enabled_with_yes_uppercase() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "YES");
        assert!(should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_disabled_with_zero() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "0");
        assert!(!should_auto_retry_on_max_fix_attempts());
    }

    #[test]
    #[serial_test::serial]
    fn test_should_auto_retry_on_max_fix_disabled_with_false() {
        let _guard = EnvGuard::set("C2RUST_AUTO_RETRY_ON_MAX_FIX", "false");
        assert!(!should_auto_retry_on_max_fix_attempts());
    }

    // ========================================================================
    // get_test_interval Tests
    // ========================================================================

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_default() {
        let _guard = EnvGuard::remove("C2RUST_TEST_INTERVAL");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_explicit_one() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "1");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_five() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "5");
        assert_eq!(get_test_interval(), 5);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_large_value() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "100");
        assert_eq!(get_test_interval(), 100);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_zero_falls_back_to_default() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "0");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_invalid_falls_back_to_default() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "abc");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_empty_falls_back_to_default() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "");
        assert_eq!(get_test_interval(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_get_test_interval_whitespace_trimmed() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "  3  ");
        assert_eq!(get_test_interval(), 3);
    }

    // ========================================================================

}

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

/// Determine whether to skip the test phase for the next translation based on the
/// current interval counter.
///
/// Returns `(should_run_test, skip_interval_test)`:
/// - `should_run_test` is `true` when the interval is reached (test should execute).
/// - `skip_interval_test` is the inverse of `should_run_test`.
pub(crate) fn compute_interval_test_decision(translations_since_last_test: usize) -> (bool, bool) {
    let interval = get_test_interval();
    let proposed_count = translations_since_last_test.saturating_add(1);
    let should_run_test = proposed_count % interval == 0;
    (should_run_test, !should_run_test)
}

/// Update the interval counter after a successful translation.
///
/// - Resets the counter to `0` when tests actually ran (`tests_ran == true`).
/// - Increments the counter by 1 (with saturation) when tests were not run.
pub(crate) fn update_interval_counter(translations_since_last_test: &mut usize, tests_ran: bool) {
    if tests_ran {
        *translations_since_last_test = 0;
    } else {
        *translations_since_last_test = translations_since_last_test.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ========================================================================
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
    // compute_interval_test_decision Tests
    // ========================================================================

    #[test]
    #[serial_test::serial]
    fn test_compute_interval_decision_interval_1_always_runs() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "1");
        for counter in 0..5 {
            let (should_run, skip) = compute_interval_test_decision(counter);
            assert!(should_run, "counter={}: expected test to run", counter);
            assert!(!skip, "counter={}: expected skip=false", counter);
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_compute_interval_decision_interval_3() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "3");
        let (should_run, skip) = compute_interval_test_decision(0);
        assert!(!should_run);
        assert!(skip);
        let (should_run, skip) = compute_interval_test_decision(1);
        assert!(!should_run);
        assert!(skip);
        let (should_run, skip) = compute_interval_test_decision(2);
        assert!(should_run);
        assert!(!skip);
        let (should_run, skip) = compute_interval_test_decision(3);
        assert!(!should_run);
        assert!(skip);
        let (should_run, skip) = compute_interval_test_decision(5);
        assert!(should_run);
        assert!(!skip);
    }

    #[test]
    #[serial_test::serial]
    fn test_compute_interval_decision_returns_inverse_pair() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "4");
        for counter in 0..12 {
            let (should_run, skip) = compute_interval_test_decision(counter);
            assert_eq!(should_run, !skip, "counter={}: should_run and skip are not inverses", counter);
        }
    }

    // ========================================================================
    // update_interval_counter Tests
    // ========================================================================

    #[test]
    fn test_update_interval_counter_resets_when_test_ran() {
        let mut counter = 4usize;
        update_interval_counter(&mut counter, true);
        assert_eq!(counter, 0);
    }

    #[test]
    fn test_update_interval_counter_increments_when_test_skipped() {
        let mut counter = 2usize;
        update_interval_counter(&mut counter, false);
        assert_eq!(counter, 3);
    }

    #[test]
    fn test_update_interval_counter_increments_from_zero() {
        let mut counter = 0usize;
        update_interval_counter(&mut counter, false);
        assert_eq!(counter, 1);
    }

    #[test]
    #[serial_test::serial]
    fn test_full_interval_cycle_counter_behaviour() {
        let _guard = EnvGuard::set("C2RUST_TEST_INTERVAL", "3");
        let mut counter = 0usize;

        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(!should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 1);

        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(!should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 2);

        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 0);

        let (should_run, _) = compute_interval_test_decision(counter);
        assert!(!should_run);
        update_interval_counter(&mut counter, should_run);
        assert_eq!(counter, 1);
    }
}

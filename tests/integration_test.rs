use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[test]
#[serial]
fn test_progress_state_in_memory() {
    // Test that progress state works correctly in memory
    let mut progress = c2rust_translate::util::ProgressState::new(10);

    // Initially at position 1 (0 processed + 1)
    assert_eq!(progress.get_current_position(), 1);
    assert_eq!(progress.get_total_count(), 10);

    // After processing one file
    progress.mark_processed();
    assert_eq!(progress.get_current_position(), 2);
    assert_eq!(progress.processed_count, 1);

    // After processing two more files
    progress.mark_processed();
    progress.mark_processed();
    assert_eq!(progress.get_current_position(), 4);
    assert_eq!(progress.processed_count, 3);
}

#[test]
#[serial]
fn test_file_content_based_progress_tracking() {
    use std::io::Write;

    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let rust_dir = temp_dir.path().join("rust");
    fs::create_dir(&rust_dir).unwrap();

    // Create empty .rs files (unprocessed)
    fs::File::create(rust_dir.join("var_test1.rs")).unwrap();
    fs::File::create(rust_dir.join("fun_test2.rs")).unwrap();

    // Create a non-empty .rs file (processed)
    let mut processed_file = fs::File::create(rust_dir.join("var_test3.rs")).unwrap();
    processed_file
        .write_all(b"pub static TEST: i32 = 42;")
        .unwrap();

    // Use file_scanner to find empty files
    let empty_files = c2rust_translate::file_scanner::find_empty_rs_files(&rust_dir).unwrap();

    // Should only find the two empty files, not the processed one
    assert_eq!(empty_files.len(), 2);
    assert!(empty_files
        .iter()
        .any(|f| f.file_name().unwrap() == "var_test1.rs"));
    assert!(empty_files
        .iter()
        .any(|f| f.file_name().unwrap() == "fun_test2.rs"));
    assert!(!empty_files
        .iter()
        .any(|f| f.file_name().unwrap() == "var_test3.rs"));

    // Create a progress state for this session
    let mut progress = c2rust_translate::util::ProgressState::new(empty_files.len());
    assert_eq!(progress.get_total_count(), 2);

    // Simulate processing files
    for _ in 0..empty_files.len() {
        progress.mark_processed();
    }

    assert_eq!(progress.processed_count, 2);
    assert_eq!(progress.get_current_position(), 3); // Next file would be #3
}

#[test]
#[serial]
fn test_progress_numbering_across_rerun() {
    use std::io::Write;

    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let rust_dir = temp_dir.path().join("rust");
    fs::create_dir(&rust_dir).unwrap();

    // Simulate initial state: 10 translatable .rs files (var_/fun_ prefix)
    // First 6 are already processed (non-empty)
    for i in 1..=3 {
        let mut file = fs::File::create(rust_dir.join(format!("var_file{}.rs", i))).unwrap();
        file.write_all(format!("// Processed file {}", i).as_bytes())
            .unwrap();
    }
    for i in 4..=6 {
        let mut file = fs::File::create(rust_dir.join(format!("fun_file{}.rs", i))).unwrap();
        file.write_all(format!("// Processed file {}", i).as_bytes())
            .unwrap();
    }

    // Remaining 4 are unprocessed (empty)
    for i in 7..=8 {
        fs::File::create(rust_dir.join(format!("var_file{}.rs", i))).unwrap();
    }
    for i in 9..=10 {
        fs::File::create(rust_dir.join(format!("fun_file{}.rs", i))).unwrap();
    }

    // Non-translatable files (no var_/fun_ prefix) should not be counted
    fs::File::create(rust_dir.join("other.rs")).unwrap();
    {
        let mut f = fs::File::create(rust_dir.join("mod.rs")).unwrap();
        f.write_all(b"// module file").unwrap();
    }

    // Count total translatable .rs files and empty translatable files
    let total_files = c2rust_translate::file_scanner::count_all_rs_files(&rust_dir).unwrap();
    let empty_files = c2rust_translate::file_scanner::find_empty_rs_files(&rust_dir).unwrap();

    assert_eq!(total_files, 10);
    assert_eq!(empty_files.len(), 4);

    // Create progress state with initial progress
    let already_processed = total_files - empty_files.len();
    let mut progress = c2rust_translate::util::ProgressState::with_initial_progress(
        total_files,
        already_processed,
    );

    // First file to process should show [7/10] (6 already done + 1)
    assert_eq!(progress.get_current_position(), 7);
    assert_eq!(progress.get_total_count(), 10);

    // After processing one empty file
    progress.mark_processed();
    assert_eq!(progress.get_current_position(), 8); // Should show [8/10]

    // After processing another
    progress.mark_processed();
    assert_eq!(progress.get_current_position(), 9); // Should show [9/10]

    // Verify processed count
    assert_eq!(progress.processed_count, 8); // 6 already + 2 just processed
}

#[test]
#[serial]
fn test_calculate_progress_with_file_system() {
    use std::io::Write;

    let temp_dir = TempDir::new().unwrap();
    let rust_dir = temp_dir.path().join("rust");
    fs::create_dir(&rust_dir).unwrap();

    // Create 8 translatable files: 5 processed (non-empty), 3 empty
    for i in 1..=5 {
        let mut f = fs::File::create(rust_dir.join(format!("var_file{}.rs", i))).unwrap();
        f.write_all(format!("// processed {}", i).as_bytes()).unwrap();
    }
    for i in 6..=8 {
        fs::File::create(rust_dir.join(format!("fun_file{}.rs", i))).unwrap();
    }

    let total = c2rust_translate::file_scanner::count_all_rs_files(&rust_dir).unwrap();
    let empty = c2rust_translate::file_scanner::find_empty_rs_files(&rust_dir).unwrap().len();

    // calculate_progress should return 5/8 = 0.625
    let progress = c2rust_translate::util::calculate_progress(total, empty);
    assert_eq!(total, 8);
    assert_eq!(empty, 3);
    assert!((progress - 0.625).abs() < 1e-9);
}

#[test]
#[serial]
fn test_calculate_progress_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let rust_dir = temp_dir.path().join("rust");
    fs::create_dir(&rust_dir).unwrap();

    let total = c2rust_translate::file_scanner::count_all_rs_files(&rust_dir).unwrap();
    let empty = c2rust_translate::file_scanner::find_empty_rs_files(&rust_dir).unwrap().len();

    // No files → progress is 0.0 (not NaN or infinity)
    let progress = c2rust_translate::util::calculate_progress(total, empty);
    assert_eq!(progress, 0.0);
}

#[test]
#[serial]
fn test_is_interrupted_api_accessible() {
    // Verify the public interrupt-flag API is callable from integration tests.
    // The function must return a bool; both true and false are valid depending on
    // test execution order, so we just assert the type and that it doesn't panic.
    let flag: bool = c2rust_translate::util::is_interrupted();
    assert!(flag == true || flag == false);
}

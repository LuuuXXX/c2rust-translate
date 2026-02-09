use std::fs;
use tempfile::TempDir;
use serial_test::serial;

#[test]
#[serial]
fn test_logger_creates_output_directory() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    
    // Create the .c2rust directory (required by find_project_root)
    fs::create_dir(project_root.join(".c2rust")).unwrap();
    
    // Change to the project directory
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(project_root).unwrap();
    
    // Initialize logger
    c2rust_translate::logger::init_logger().unwrap();
    
    // Verify the output directory exists
    let output_dir = project_root.join(".c2rust").join("output");
    assert!(output_dir.exists(), "Output directory should exist");
    assert!(output_dir.is_dir(), "Output path should be a directory");
    
    // Verify a log file was created
    let entries: Vec<_> = fs::read_dir(&output_dir).unwrap().collect();
    assert!(!entries.is_empty(), "At least one log file should exist");
    
    // Verify the log file has the expected name pattern
    let log_file = entries[0].as_ref().unwrap();
    let filename = log_file.file_name();
    let filename_str = filename.to_str().unwrap();
    assert!(filename_str.starts_with("translate_"), "Log file should start with 'translate_'");
    assert!(filename_str.ends_with(".log"), "Log file should end with '.log'");
    
    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_progress_state_in_memory() {
    // Test that progress state works correctly in memory
    let mut progress = c2rust_translate::progress::ProgressState::new(10);
    
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
    processed_file.write_all(b"pub static TEST: i32 = 42;").unwrap();
    
    // Use file_scanner to find empty files
    let empty_files = c2rust_translate::file_scanner::find_empty_rs_files(&rust_dir).unwrap();
    
    // Should only find the two empty files, not the processed one
    assert_eq!(empty_files.len(), 2);
    assert!(empty_files.iter().any(|f| f.file_name().unwrap() == "var_test1.rs"));
    assert!(empty_files.iter().any(|f| f.file_name().unwrap() == "fun_test2.rs"));
    assert!(!empty_files.iter().any(|f| f.file_name().unwrap() == "var_test3.rs"));
    
    // Create a progress state for this session
    let mut progress = c2rust_translate::progress::ProgressState::new(empty_files.len());
    assert_eq!(progress.get_total_count(), 2);
    
    // Simulate processing files
    for _ in 0..empty_files.len() {
        progress.mark_processed();
    }
    
    assert_eq!(progress.processed_count, 2);
    assert_eq!(progress.get_current_position(), 3); // Next file would be #3
}

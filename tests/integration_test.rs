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

use std::fs;
use tempfile::TempDir;
use serial_test::serial;

#[test]
#[serial]
fn test_progress_file_in_rust_directory() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    
    // Create the .c2rust directory first (required by find_project_root)
    fs::create_dir(project_root.join(".c2rust")).unwrap();
    
    // Create the necessary directory structure
    let rust_dir = project_root.join(".c2rust").join("test_feature").join("rust");
    fs::create_dir_all(&rust_dir).unwrap();
    
    // Change to the project directory
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(project_root).unwrap();
    
    // Load progress state (which will create the file on save)
    let mut progress = c2rust_translate::progress::ProgressState::default();
    progress.set_total_count(10);
    progress.save("test_feature").unwrap();
    
    // Verify the progress file is in the rust directory
    let expected_path = rust_dir.join("progress.json");
    assert!(expected_path.exists(), "Progress file should exist at {:?}", expected_path);
    
    // Verify the content
    let content = fs::read_to_string(&expected_path).unwrap();
    let loaded: c2rust_translate::progress::ProgressState = serde_json::from_str(&content).unwrap();
    assert_eq!(loaded.get_total_count(), 10);
    
    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_progress_file_migration() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    
    // Create the .c2rust directory
    let feature_dir = project_root.join(".c2rust").join("test_feature");
    fs::create_dir_all(&feature_dir).unwrap();
    
    // Create the rust directory
    let rust_dir = feature_dir.join("rust");
    fs::create_dir_all(&rust_dir).unwrap();
    
    // Create an old progress file at the old location
    let old_progress_path = feature_dir.join("progress.json");
    let old_progress_content = r#"{
        "processed_count": 5,
        "processed_files": ["file1.rs", "file2.rs"],
        "total_count": 10
    }"#;
    fs::write(&old_progress_path, old_progress_content).unwrap();
    
    // Change to the project directory
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(project_root).unwrap();
    
    // Load progress - should migrate from old location
    let progress = c2rust_translate::progress::ProgressState::load("test_feature").unwrap();
    
    // Verify migration happened
    assert_eq!(progress.processed_count, 5);
    assert_eq!(progress.processed_files.len(), 2);
    assert_eq!(progress.get_total_count(), 10);
    
    // Verify old file was removed
    assert!(!old_progress_path.exists(), "Old progress file should be removed after migration");
    
    // Verify new file exists
    let new_progress_path = rust_dir.join("progress.json");
    assert!(new_progress_path.exists(), "New progress file should exist after migration");
    
    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
}

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

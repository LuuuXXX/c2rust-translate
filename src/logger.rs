use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static GLOBAL_LOGGER: OnceLock<Mutex<Option<File>>> = OnceLock::new();

/// Initialize the logger with a timestamped log file
pub fn init_logger() -> Result<()> {
    let project_root = crate::util::find_project_root()?;
    let output_dir = project_root.join(".c2rust").join("output");
    
    // Create output directory if it doesn't exist
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;
    
    // Generate timestamped filename
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let log_filename = format!("translate_{}.log", timestamp);
    let log_path = output_dir.join(log_filename);
    
    // Create the log file
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .with_context(|| format!("Failed to create log file: {}", log_path.display()))?;
    
    println!("Log file created: {}", log_path.display());
    
    // Initialize the global logger
    GLOBAL_LOGGER.get_or_init(|| Mutex::new(Some(file)));
    
    Ok(())
}

/// Log a message to the file (if logger is initialized)
fn log_to_file(text: &str) {
    if let Some(logger_mutex) = GLOBAL_LOGGER.get() {
        if let Ok(mut logger_opt) = logger_mutex.lock() {
            if let Some(file) = logger_opt.as_mut() {
                let _ = writeln!(file, "{}", text);
                let _ = file.flush();
            }
        }
    }
}

/// Macro to print to both stdout and log file
#[macro_export]
macro_rules! log_println {
    ($($arg:tt)*) => {{
        let text = format!($($arg)*);
        println!("{}", text);
        $crate::logger::log_message(&text);
    }};
}

/// Macro to print to both stderr and log file
#[macro_export]
macro_rules! log_eprintln {
    ($($arg:tt)*) => {{
        let text = format!($($arg)*);
        eprintln!("{}", text);
        $crate::logger::log_message(&text);
    }};
}

/// Public function to log a message (for use by macros)
pub fn log_message(text: &str) {
    log_to_file(text);
}

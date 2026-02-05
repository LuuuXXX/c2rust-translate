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
    
    // Generate timestamped filename with milliseconds to avoid collisions
    let timestamp = Local::now().format("%Y%m%d_%H%M%S%.3f");
    let log_filename = format!("translate_{}.log", timestamp);
    let log_path = output_dir.join(&log_filename);
    
    // Create the log file
    let (file, actual_path) = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&log_path)
        .map(|f| (f, log_path))
        .or_else(|_| {
            // If file exists (unlikely with milliseconds but possible), append a counter
            for i in 1..100 {
                let alternate_filename = format!("translate_{}_{}.log", timestamp, i);
                let alternate_path = output_dir.join(&alternate_filename);
                match OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&alternate_path)
                {
                    Ok(f) => return Ok((f, alternate_path)),
                    Err(_) => continue,
                }
            }
            anyhow::bail!("Failed to create log file: too many files with same timestamp")
        })
        .with_context(|| format!("Failed to create log file in directory: {}", output_dir.display()))?;
    
    println!("Log file created: {}", actual_path.display());
    
    // Initialize or update the global logger
    match GLOBAL_LOGGER.get() {
        Some(logger_mutex) => {
            // Logger already exists, replace the file
            if let Ok(mut logger_opt) = logger_mutex.lock() {
                *logger_opt = Some(file);
            }
        }
        None => {
            // First time initialization
            GLOBAL_LOGGER.get_or_init(|| Mutex::new(Some(file)));
        }
    }
    
    Ok(())
}

/// Log a message to the file (if logger is initialized)
fn log_to_file(text: &str) {
    if let Some(logger_mutex) = GLOBAL_LOGGER.get() {
        if let Ok(mut logger_opt) = logger_mutex.lock() {
            if let Some(file) = logger_opt.as_mut() {
                if let Err(e) = writeln!(file, "{}", text) {
                    eprintln!("Warning: Failed to write to log file: {}", e);
                    return;
                }
                if let Err(e) = file.flush() {
                    eprintln!("Warning: Failed to flush log file: {}", e);
                }
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

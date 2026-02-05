use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex};

/// A writer that duplicates output to both stdout and a file
pub struct DualWriter {
    file: Arc<Mutex<File>>,
}

impl DualWriter {
    /// Initialize the logger with a timestamped log file
    pub fn init() -> Result<Self> {
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
        
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }
    
    /// Write a line to both stdout and the log file
    pub fn writeln(&self, text: &str) -> Result<()> {
        // Write to stdout
        println!("{}", text);
        
        // Write to log file
        let mut file = self.file.lock().unwrap();
        writeln!(file, "{}", text)
            .context("Failed to write to log file")?;
        file.flush()
            .context("Failed to flush log file")?;
        
        Ok(())
    }
    
    /// Write to both stderr and the log file
    pub fn writeln_err(&self, text: &str) -> Result<()> {
        // Write to stderr
        eprintln!("{}", text);
        
        // Write to log file
        let mut file = self.file.lock().unwrap();
        writeln!(file, "{}", text)
            .context("Failed to write to log file")?;
        file.flush()
            .context("Failed to flush log file")?;
        
        Ok(())
    }
}

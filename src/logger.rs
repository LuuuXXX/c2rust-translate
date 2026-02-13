use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static GLOBAL_LOGGER: OnceLock<Mutex<Option<File>>> = OnceLock::new();

/// 使用带时间戳的日志文件初始化日志记录器
pub fn init_logger() -> Result<()> {
    let project_root = crate::util::find_project_root()?;
    let output_dir = project_root.join(".c2rust").join("output");

    // 如果输出目录不存在则创建
    std::fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    // 生成带毫秒的时间戳文件名以避免冲突
    let timestamp = Local::now().format("%Y%m%d_%H%M%S%.3f");
    let log_filename = format!("translate_{}.log", timestamp);
    let log_path = output_dir.join(&log_filename);

    // 创建日志文件
    let (file, actual_path) = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&log_path)
        .map(|f| (f, log_path))
        .or_else(|_| {
            // 如果文件已存在（使用毫秒时不太可能但仍有可能），追加计数器
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
        .with_context(|| {
            format!(
                "Failed to create log file in directory: {}",
                output_dir.display()
            )
        })?;

    println!("Log file created: {}", actual_path.display());

    // 初始化或更新全局日志记录器
    match GLOBAL_LOGGER.get() {
        Some(logger_mutex) => {
            // 日志记录器已存在，替换文件
            if let Ok(mut logger_opt) = logger_mutex.lock() {
                *logger_opt = Some(file);
            }
        }
        None => {
            // 首次初始化
            GLOBAL_LOGGER.get_or_init(|| Mutex::new(Some(file)));
        }
    }

    Ok(())
}

/// 将消息记录到文件（如果日志记录器已初始化）
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

/// 记录消息的公共函数
pub fn log_message(text: &str) {
    log_to_file(text);
}

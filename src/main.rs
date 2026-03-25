use clap::{Parser, Subcommand};
use std::path::PathBuf;

fn parse_positive_usize(s: &str) -> Result<usize, String> {
    let value: usize = s
        .parse()
        .map_err(|_| format!("`{s}` is not a valid number"))?;
    if value == 0 {
        return Err(String::from("value must be greater than 0"));
    }
    Ok(value)
}

#[derive(Parser)]
#[command(name = "c2rust-translate")]
#[command(about = "A tool for translating C code to Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 为特定功能将 C 代码翻译为 Rust
    Translate {
        /// 功能名称（如未指定则默认为 "default"）
        #[arg(long, default_value = "default")]
        feature: String,

        /// 允许处理所有未处理的文件，无需提示选择
        #[arg(long)]
        allow_all: bool,

        /// 构建错误的最大修复尝试次数（必须 > 0，默认为 10）
        #[arg(long, default_value = "10", value_parser = parse_positive_usize)]
        max_fix_attempts: usize,

        /// 显示代码和错误的完整输出，不进行截断
        #[arg(long)]
        show_full_output: bool,
    },

    /// 将 feature 中分散的 Rust 文件合并为单个文件
    ///
    /// 扫描 .c2rust/<feature>/rust/src/ 下所有已翻译的 var_*.rs 和 fun_*.rs
    /// 文件，提取并去重 use 语句（保留 use core::ffi::*; 等 FFI 导入），然后
    /// 将代码正文按原顺序拼接，写入合并文件。
    Merge {
        /// 功能名称（如未指定则默认为 "default"）
        #[arg(long, default_value = "default")]
        feature: String,

        /// 合并输出文件路径（可选；默认为 .c2rust/<feature>/merged.rs）
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Translate {
            feature,
            allow_all,
            max_fix_attempts,
            show_full_output,
        } => c2rust_translate::translate_feature(
            &feature,
            allow_all,
            max_fix_attempts,
            show_full_output,
        ),
        Commands::Merge { feature, output } => {
            c2rust_translate::merge_feature(&feature, output.as_deref())
        }
    };

    if let Err(e) = result {
        let error_msg = format!("Error: {:#}", e);
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
}

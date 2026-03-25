use clap::{Parser, Subcommand};

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

    /// 将已翻译的 .rs 文件合并为单一输出文件
    ///
    /// 读取 .c2rust/<feature>/rust/src/ 下所有已翻译（非空）的 var_/fun_ .rs 文件，
    /// 合并为一个文件。合并时正确处理 `use` 声明：按名称去重，并对 c_int 等 C FFI
    /// 类型优先保留最规范的路径（std::ffi > core::ffi > std::os::raw）。
    /// glob 导入（如 `use core::ffi::*;`）始终被保留，不会被删除。
    Merge {
        /// 功能名称（如未指定则默认为 "default"）
        #[arg(long, default_value = "default")]
        feature: String,

        /// 合并输出文件路径；若未指定，默认写入
        /// .c2rust/<feature>/rust/src/merged.rs
        #[arg(long)]
        output: Option<std::path::PathBuf>,
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
            c2rust_translate::merger::merge_feature(&feature, output.as_deref()).map(|_| ())
        }
    };

    if let Err(e) = result {
        let error_msg = format!("Error: {:#}", e);
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
}

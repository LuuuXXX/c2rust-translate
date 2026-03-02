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
}

/// 注册 Ctrl+C（SIGINT）处理程序以便在中断时保存进度
///
/// 当用户按下 Ctrl+C 时，设置全局中断标志并退出进程。
/// 在退出之前打印提示信息，告知用户进度已保存。
/// 仅在 Unix 系统上安装自定义处理程序；非 Unix 系统使用默认行为。
fn register_interrupt_handler() {
    #[cfg(unix)]
    {
        extern "C" fn sigint_handler(_: libc::c_int) {
            c2rust_translate::util::set_interrupted();
            eprintln!("\n中断信号收到，正在保存进度...");
            eprintln!("Translation interrupted. Progress has been saved.");
            eprintln!("Re-run to continue from where you left off.");
            std::process::exit(130); // 130 = 128 + SIGINT(2)
        }

        let ret = unsafe {
            libc::signal(libc::SIGINT, sigint_handler as *const () as libc::sighandler_t)
        };

        if ret == libc::SIG_ERR {
            eprintln!("Warning: Failed to register SIGINT handler. Ctrl+C will not save progress gracefully.");
        }
    }
}

fn main() {
    register_interrupt_handler();

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
    };

    if let Err(e) = result {
        let error_msg = format!("Error: {:#}", e);
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
}

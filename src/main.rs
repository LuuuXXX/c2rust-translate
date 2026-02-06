use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "c2rust-translate")]
#[command(about = "A tool for translating C code to Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Translate C code to Rust for a specific feature
    Translate {
        /// Feature name (defaults to "default" if not specified)
        #[arg(long, default_value = "default")]
        feature: String,
        
        /// Allow processing all unprocessed files without prompting for selection
        #[arg(long)]
        allow_all: bool,
        
        /// Maximum number of fix attempts for build errors (must be > 0, defaults to 10)
        #[arg(long, default_value_t = 10)]
        max_fix_attempts: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    // Initialize logging before running the command
    if matches!(cli.command, Commands::Translate { .. }) {
        if let Err(e) = c2rust_translate::logger::init_logger() {
            eprintln!("Warning: Failed to initialize logger: {:#}", e);
            eprintln!("Continuing without file logging...");
        }
    }

    let result = match cli.command {
        Commands::Translate { feature, allow_all, max_fix_attempts } => {
            // Validate max_fix_attempts
            if max_fix_attempts == 0 {
                eprintln!("Error: max-fix-attempts must be a positive integer (greater than 0)");
                std::process::exit(1);
            }
            c2rust_translate::translate_feature(&feature, allow_all, max_fix_attempts)
        }
    };

    if let Err(e) = result {
        let error_msg = format!("Error: {:#}", e);
        eprintln!("{}", error_msg);
        c2rust_translate::logger::log_message(&error_msg);
        std::process::exit(1);
    }
}

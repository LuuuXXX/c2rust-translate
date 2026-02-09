use clap::{Parser, Subcommand};

fn parse_positive_usize(s: &str) -> Result<usize, String> {
    let value: usize = s.parse()
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
    /// Translate C code to Rust for a specific feature
    Translate {
        /// Feature name (defaults to "default" if not specified)
        #[arg(long, default_value = "default")]
        feature: String,
        
        /// Allow processing all unprocessed files without prompting for selection
        #[arg(long)]
        allow_all: bool,
        
        /// Maximum number of fix attempts for build errors (must be > 0, defaults to 10)
        #[arg(long, default_value = "10", value_parser = parse_positive_usize)]
        max_fix_attempts: usize,
        
        /// Show full output for code and errors without truncation
        #[arg(long)]
        show_full_output: bool,
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
        Commands::Translate { feature, allow_all, max_fix_attempts, show_full_output } => {
            c2rust_translate::translate_feature(&feature, allow_all, max_fix_attempts, show_full_output)
        }
    };

    if let Err(e) = result {
        // Check if this is a user-requested exit (exit code 0)
        if e.downcast_ref::<c2rust_translate::UserRequestedExit>().is_some() {
            eprintln!("Exiting as requested.");
            std::process::exit(0);
        }
        
        let error_msg = format!("Error: {:#}", e);
        eprintln!("{}", error_msg);
        c2rust_translate::logger::log_message(&error_msg);
        std::process::exit(1);
    }
}

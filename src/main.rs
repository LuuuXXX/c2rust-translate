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
        Commands::Translate { feature } => c2rust_translate::translate_feature(&feature),
    };

    if let Err(e) = result {
        let error_msg = format!("Error: {:#}", e);
        eprintln!("{}", error_msg);
        c2rust_translate::logger::log_message(&error_msg);
        std::process::exit(1);
    }
}

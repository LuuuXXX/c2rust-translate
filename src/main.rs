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
    let logger_result = match &cli.command {
        Commands::Translate { .. } => c2rust_translate::logger::DualWriter::init(),
    };

    let _logger = match logger_result {
        Ok(logger) => {
            println!("Logger initialized successfully");
            Some(logger)
        }
        Err(e) => {
            eprintln!("Warning: Failed to initialize logger: {:#}", e);
            eprintln!("Continuing without file logging...");
            None
        }
    };

    let result = match cli.command {
        Commands::Translate { feature } => c2rust_translate::translate_feature(&feature),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

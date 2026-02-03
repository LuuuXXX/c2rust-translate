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

    let result = match cli.command {
        Commands::Translate { feature } => c2rust_translate::translate_feature(&feature),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

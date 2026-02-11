use clap::{Parser, Subcommand};
use shadow_rs::shadow;
use std::path::PathBuf;

mod cli;

shadow!(build);

/// Mashi - Browser compression technology for the 20th century
#[derive(Parser)]
#[clap(name = "mashi", about, long_version = build::CLAP_LONG_VERSION, arg_required_else_help(true)
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pack a JS file into an index.html file, with an optional WASM binary to be passed into the JS code
    Pack {
        /// The filename of the Javascript file to include
        #[arg(value_hint = clap::ValueHint::FilePath)]
        js_filename: PathBuf,

        /// The filename of the WASM binary to include
        #[arg(long = "wasm", value_hint = clap::ValueHint::FilePath)]
        wasm_filename: Option<PathBuf>,

        /// Output filename
        #[arg(default_value = "index.html")]
        output_filename: PathBuf,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            wasm_filename, js_filename, output_filename
        } => {
            cli::pack(wasm_filename, js_filename, output_filename)?;
        }
    }

    Ok(())
}
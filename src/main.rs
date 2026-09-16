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
    /// Pack a JS file into an index.html file, with an optional WASM or raw binary payload to be passed into the JS code
    Pack {
        /// The filename of the Javascript file to include
        #[arg(value_hint = clap::ValueHint::FilePath)]
        js_filename: PathBuf,

        /// The filename of the WASM binary to include. Uses a WASM-specific context model to improve the compression ratio.
        #[arg(long = "wasm", value_hint = clap::ValueHint::FilePath, conflicts_with = "bin_filename")]
        wasm_filename: Option<PathBuf>,

        /// The filename of the raw binary data to include, not using a WASM context model.
        #[arg(long = "bin", value_hint = clap::ValueHint::FilePath, conflicts_with = "wasm_filename")]
        bin_filename: Option<PathBuf>,

        /// Don't show a progress bar while depacking. The generated page leaves the document
        /// untouched and decompresses everything in one go, which makes the depacker smaller.
        #[arg(long = "no-progress-bar")]
        no_progress_bar: bool,

        /// Output filename
        #[arg(default_value = "index.html")]
        output_filename: PathBuf,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            wasm_filename, bin_filename, js_filename, no_progress_bar, output_filename
        } => {
            cli::pack(wasm_filename, bin_filename, js_filename, no_progress_bar, output_filename)?;
        }
    }

    Ok(())
}
use clap::{Parser, Subcommand};

use shadow_rs::shadow;
shadow!(build);

// If this doesn't exist, run `cargo xtask build-prerequisites`
const WASM_DECOMPRESSOR: &[u8] = include_bytes!("../../target/decompressor.wasm");

/// Mashi - WASM compression technology for the 20th century
#[derive(Parser)]
#[clap(name = "mashi", about, long_version = build::CLAP_LONG_VERSION, arg_required_else_help(true)
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pack a WASM and JS file together into an index.html file
    Pack {
        /// The filename of the WASM binary to include
        wasm_filename: String,

        /// The filename of the Javascript file to include
        js_filename: String,

        /// Output filename
        #[arg(default_value = "index.html")]
        output_filename: String,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Pack {
            wasm_filename, js_filename, output_filename
        } => {
            println!("{output_filename}");
        }
    }

    Ok(())
}

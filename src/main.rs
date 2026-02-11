use std::fs;
use clap::{Parser, Subcommand};
use shadow_rs::shadow;
use mashi::compress;

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
    /// Pack a JS file into an index.html file, with an optional WASM payload
    Pack {
        /// The filename of the Javascript file to include
        js_filename: String,

        /// The filename of the WASM binary to include
        wasm_filename: Option<String>,

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
            let js_input = fs::read(&js_filename)?;
            let wasm_input = match wasm_filename {
                Some(filename) => fs::read(&filename)?,
                None => vec![]
            };

            compress(&js_input, &wasm_input, |progress| {
                println!("{progress}");
            });
            println!("{output_filename}");
        }
    }

    Ok(())
}
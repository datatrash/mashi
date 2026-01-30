use anyhow::Result;
use clap::{Parser, Subcommand};
use duct::cmd;
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[clap(arg_required_else_help(true))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    BuildPrerequisites,
}

fn workspace() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    path.into()
}

fn build_prerequisites() -> Result<()> {
    println!("Building prerequisites...");
    println!("WASM Decompressor...");
    const RUST_FLAGS: &str = "-Clink-args=--import-memory -Zunstable-options -Cpanic=immediate-abort -Zlocation-detail=none -Zfmt-debug=none -Cllvm-args=-inline-threshold=10 -Cllvm-args=-inlinedefault-threshold=10 -Cllvm-args=-inlinehint-threshold=40";
    cmd!(
        "cargo",
        "+nightly",
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "-Z",
        "build-std=core,alloc,panic_abort",
        "-Z",
        "build-std-features=optimize_for_size",
        "-p",
        "mashi-core",
        "--profile",
        "tiny",
        "--no-default-features",
        "--features",
        "tiny"
    )
        .env("RUSTFLAGS", RUST_FLAGS)
        .run()?;

    let src_wasm = workspace().join("target/wasm32-unknown-unknown/tiny/mashi_core.wasm");
    let wasm = workspace().join("target/decompressor.wasm");
    cmd!(
        "wasm-opt",
        "-n",
        "--enable-bulk-memory-opt",
        "--duplicate-function-elimination",
        "-Oz",
        "--converge",
        &src_wasm,
        "-o",
        &wasm
    )
        .run()?;

    let mut compressed_wasm = vec![];
    zopfli::compress(zopfli::Options::default(), zopfli::Format::Deflate, File::open(&wasm)?, &mut compressed_wasm)?;
    println!("Estimated size of decompressor in final executable: {} bytes (equivalent to ~{} superogues)", compressed_wasm.len(), compressed_wasm.len() / 64);

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::BuildPrerequisites) => {
            build_prerequisites()?;
        }
        None => (),
    }

    Ok(())
}
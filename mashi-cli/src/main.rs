use argh::FromArgs;
use fs_err as fs;
use mashi_core::codec::{Decoder, Encoder};
use std::io::{Read, Write};

pub type Result<T> = anyhow::Result<T>;

#[derive(FromArgs, PartialEq, Debug)]
/// Compress or decompress a file.
struct Args {
    #[argh(subcommand)]
    command: Commands,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Commands {
    Compress(CompressArgs),
    Decompress(DecompressArgs),
}

#[derive(FromArgs, PartialEq, Debug)]
/// Compresses a file.
#[argh(subcommand, name = "compress")]
struct CompressArgs {
    #[argh(positional)]
    filename: String,
}

#[derive(FromArgs, PartialEq, Debug)]
/// Decompresses a file.
#[argh(subcommand, name = "decompress")]
struct DecompressArgs {
    #[argh(positional)]
    filename: String,
}

fn execute(args: Args) -> Result<()> {
    match args.command {
        Commands::Compress(args) => {
            let mut input_file = fs::File::open(&args.filename)?;
            let mut input = vec![];
            input_file.read_to_end(&mut input)?;

            let mut encoder = Encoder::new();
            let compressed = encoder.encode(&input);

            let mut output_file = fs::File::create(format!("{}.mashi", &args.filename))?;
            output_file.write_all(&compressed)?;

            println!(
                "Mashed '{}' from {} bytes to {} bytes",
                &args.filename,
                input.len(),
                compressed.len()
            );

            Ok(())
        }
        Commands::Decompress(args) => {
            let mut input_file = fs::File::open(&args.filename)?;
            let mut input = vec![];
            input_file.read_to_end(&mut input)?;

            let mut decoder = Decoder::new();
            let decompressed = decoder.decode(&input);

            let mut output_file = fs::File::create(args.filename.replace(".mashi", ""))?;
            output_file.write_all(&decompressed)?;

            println!(
                "Unmashed '{}' from {} bytes to {} bytes",
                &args.filename,
                input.len(),
                decompressed.len()
            );

            Ok(())
        }
    }
}

fn main() -> Result<()> {
    execute(argh::from_env())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_err as fs;
    use std::path::PathBuf;
    use std::str::FromStr;

    #[test]
    fn roundtrip() -> Result<()> {
        let src_path = PathBuf::from_str(&format!(
            "{}/../tests/test.wasm",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let dst_path = PathBuf::from_str(&format!(
            "{}/../target/test.wasm",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let compressed_dst_path = PathBuf::from_str(&format!(
            "{}/../target/test.wasm.mashi",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        if dst_path.exists() {
            fs::remove_file(&dst_path)?;
        }
        if compressed_dst_path.exists() {
            fs::remove_file(&compressed_dst_path)?;
        }

        // Compress the file
        fs::copy(&src_path, &dst_path)?;
        execute(Args {
            command: Commands::Compress(CompressArgs {
                filename: dst_path.to_string_lossy().to_string(),
            }),
        })?;

        // Did we compress the file successfully?
        let metadata = fs::metadata(&compressed_dst_path)?;
        assert_eq!(metadata.len(), 12083);

        // Decompress it, first removing the original
        if dst_path.exists() {
            fs::remove_file(&dst_path)?;
        }

        execute(Args {
            command: Commands::Decompress(DecompressArgs {
                filename: compressed_dst_path.to_string_lossy().to_string(),
            }),
        })?;

        // Did we decompress the file successfully?
        let metadata = fs::metadata(&dst_path)?;
        assert_eq!(metadata.len(), 35151);

        Ok(())
    }
}

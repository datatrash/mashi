#![allow(unused)]
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use wast::parser::ParseBuffer;
use wast::{parser, Wast, WastDirective};
use mashi::{compress, wasm_decompress};

/// Mashi Test Suite runner, runs as many WAST tests from the official test suite through a
/// compression and decompression roundtrip to make sure Mashi doesn't mangle anything in the process.
#[derive(Parser)]
struct Cli {
    /// Specifies the .wast file to start the test suite at
    filename: Option<String>,

    /// Specifies the (1-based) module index within the .wast file to start the test suite in
    #[arg(requires = "filename")]
    module_index: Option<usize>,
}

#[cfg(feature = "manual-tests")]
#[test]
fn wasm_test_suite() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let suite_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wasm-test-suite");
    let mut wast_files = fs::read_dir(suite_path)?
        .filter(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    path.extension() == Some(OsStr::new("wast"))
                }
                Err(_) => false,
            }
        })
        .map(|file| file.unwrap().path())
        .collect::<Vec<_>>();
    wast_files.sort();

    let mut failed = vec![];
    let mut is_skipping_files = cli.filename.is_some();
    for wast_file in wast_files {
        let mut should_select_module = false;
        if let Some(filename) = &cli.filename {
            if filename == wast_file.file_name().unwrap().to_str().unwrap() {
                is_skipping_files = false;
                should_select_module = cli.module_index.is_some();
            }
        }
        if is_skipping_files {
            continue;
        }

        let str = fs::read_to_string(&wast_file)?;
        let buf = ParseBuffer::new(&str)?;
        let mut wast = match parser::parse::<Wast>(&buf) {
            Ok(wast) => wast,
            Err(_) => continue,
        };
        let binaries = wast.directives.iter_mut()
            .filter_map(|directive| {
                match directive {
                    WastDirective::Module(module) => {
                        match module.encode() {
                            Ok(binary) => Some(binary),
                            Err(_) => {
                                // Couldn't compile this test, so let's just move on
                                None
                            }
                        }
                    },
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        for (idx, binary) in binaries.iter().enumerate() {
            if should_select_module && idx + 1 < cli.module_index.unwrap() {
                // skip, not at the selected module yet
                continue;
            }

            let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/out");
            if !fs::exists(&target)? {
                fs::create_dir(&target)?;
            }
            let binary_path = target.join(PathBuf::from(format!("{}-{}.wasm", wast_file.file_stem().unwrap().display(), idx + 1)));
            fs::write(binary_path, binary)?;

            let prefix = format!("{:>40} [{:>3}/{:>3}]: ", wast_file.file_name().unwrap().display(), idx + 1, binaries.len());
            let p = ProgressBar::new(binary.len() as u64).with_prefix(prefix);
            p.set_style(ProgressStyle::with_template("{prefix} [{bar:40.cyan/blue} {pos:>7}/{len:7}] {msg}")?
                .progress_chars("##-"));
            let (compressed, _) = compress(&[], &binary, |pos| {
                p.set_position(pos as u64);
            });
            p.set_style(ProgressStyle::with_template("{prefix} [{bar:40.green/red} {pos:>7}/{len:7}] {msg}")?
                .progress_chars("##-"));
            let decompressed = wasm_decompress(&compressed, |pos| {
                p.set_position(pos as u64);
            });
            let result = *binary == decompressed;
            if result {
                p.finish_with_message("OK");
            } else {
                failed.push((wast_file.clone(), idx + 1));
                p.finish_with_message("FAIL");
            }
        }
    }

    if !failed.is_empty() {
        println!("There were test failures:");
        for (file, idx) in failed {
            println!("\t{}: {}", file.display(), idx);
        }
    }

    Ok(())
}
use std::fs;
use std::num::NonZero;
use duct::cmd;
use zopfli::{Format, Options};

fn main() -> anyhow::Result<()> {
    println!("cargo::rerun-if-changed=src/decompress.wat");
    cmd!("wasm-as", "--enable-bulk-memory-opt", "--enable-simd", "src/decompress.wat", "-o", "target/decompress.wasm").run()?;

    fs::write("target/decompress.jsonlike", r#"
[
  { "name": "outside", "reaches": ["export-decompress"], "root": true },
  { "name": "export-decompress", "export": "d" }
]
"#)?;
    cmd!("wasm-metadce", "--enable-bulk-memory-opt", "--enable-simd", "target/decompress.wasm", "--graph-file", "target/decompress.jsonlike", "-o", "target/decompress.wasm").run()?;
    cmd!("wasm-opt", "--enable-bulk-memory-opt", "--enable-simd", "target/decompress.wasm", "-Oz", "-o", "target/decompress.wasm").run()?;

    zopfli::compress(Options {
        iteration_count: NonZero::new(1000).unwrap(),
        ..Default::default()
    }, Format::Deflate, fs::File::open("target/decompress.wasm")?, fs::File::create("target/decompress.wasm.zopfli")?)?;

    Ok(())
}
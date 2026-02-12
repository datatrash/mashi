use duct::cmd;
use std::{env, fs};

fn main() -> anyhow::Result<()> {
    shadow_rs::ShadowBuilder::builder().build()?;

    // Don't run this on CI, because we don't want to install all the various dependencies
    if env::var_os("CI").is_some() {
        return Ok(());
    }

    if !fs::exists("generated")? {
        fs::create_dir("generated")?;
    }

    println!("cargo::rerun-if-changed=src/decompress.wat");
    cmd!("wasm-as", "--enable-bulk-memory-opt", "--enable-simd", "src/decompress.wat", "-o", "generated/decompress.wasm").run()?;

    fs::write("target/decompress.jsonlike", r#"
[
  { "name": "outside", "reaches": ["export-decompress", "export-memory"], "root": true },
  { "name": "export-decompress", "export": "d" },
  { "name": "export-memory", "export": "memory" }
]
"#)?;
    cmd!("wasm-metadce", "--enable-bulk-memory-opt", "--enable-simd", "generated/decompress.wasm", "--graph-file", "target/decompress.jsonlike", "-o", "target/decompress.wasm").run()?;
    cmd!("wasm-opt", "--enable-bulk-memory-opt", "--enable-simd", "generated/decompress.wasm", "-Oz", "-o", "generated/decompress.wasm").run()?;

    println!("cargo::rerun-if-changed=src/cli/depacker.js");
    cmd!("bun", "build", "src/cli/depacker.js", "--minify", "--outfile=generated/depacker.js.min").run()?;

    Ok(())
}
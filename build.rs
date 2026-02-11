use duct::cmd;
use std::fs;

fn main() -> anyhow::Result<()> {
    shadow_rs::ShadowBuilder::builder().build()?;

    println!("cargo::rerun-if-changed=src/decompress.wat");
    cmd!("wasm-as", "--enable-bulk-memory-opt", "--enable-simd", "src/decompress.wat", "-o", "target/decompress.wasm").run()?;

    fs::write("target/decompress.jsonlike", r#"
[
  { "name": "outside", "reaches": ["export-decompress", "export-memory"], "root": true },
  { "name": "export-decompress", "export": "d" },
  { "name": "export-memory", "export": "memory" }
]
"#)?;
    cmd!("wasm-metadce", "--enable-bulk-memory-opt", "--enable-simd", "target/decompress.wasm", "--graph-file", "target/decompress.jsonlike", "-o", "target/decompress.wasm").run()?;
    cmd!("wasm-opt", "--enable-bulk-memory-opt", "--enable-simd", "target/decompress.wasm", "-Oz", "-o", "target/decompress.wasm").run()?;

    println!("cargo::rerun-if-changed=src/cli/depacker.js");
    cmd!("bun", "build", "src/cli/depacker.js", "--minify", "--outfile=target/depacker.js.min").run()?;

    Ok(())
}
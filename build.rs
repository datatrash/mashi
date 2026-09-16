use duct::cmd;
use std::{env, fs};

#[path = "src/wat_flag.rs"]
mod wat_flag;

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

    // Both variants of the decompressor stub share the same metadce root graph: only the "d"
    // (decompress) and "m" (memory) exports need to survive.
    fs::write("target/decompress.jsonlike", r#"
[
  { "name": "outside", "reaches": ["export-decompress", "export-memory"], "root": true },
  { "name": "export-decompress", "export": "d" },
  { "name": "export-memory", "export": "m" }
]
"#)?;

    // The WASM-context-model variant: decompress.wat as-is.
    build_decompress_stub("src/decompress.wat", "generated/decompress.wasm")?;

    // The binary/JS-only variant: decompress.wat with $use_wasm_model flipped to 0, so binaryen's
    // constant propagation + DCE removes the entire WASM disassembler/context-model subtree.
    let wat = fs::read_to_string("src/decompress.wat")?;
    let wat_without_wasm_model = wat_flag::without_wasm_model(&wat);
    let bin_wat_path = "target/decompress_bin.wat";
    fs::write(bin_wat_path, &wat_without_wasm_model)?;
    build_decompress_stub(bin_wat_path, "generated/decompress_bin.wasm")?;

    // Guard against a future binaryen version that stops folding the constant guards: if it ever
    // did, the bin stub would silently ship the (unreachable but present) WASM model code again.
    let wasm_stub_len = fs::metadata("generated/decompress.wasm")?.len();
    let bin_stub_len = fs::metadata("generated/decompress_bin.wasm")?.len();
    anyhow::ensure!(
        bin_stub_len < wasm_stub_len,
        "generated/decompress_bin.wasm ({bin_stub_len} bytes) is not smaller than \
         generated/decompress.wasm ({wasm_stub_len} bytes) -- binaryen may no longer be \
         constant-folding $use_wasm_model, so the WASM context model isn't being dead-code \
         eliminated from the binary/JS-only stub anymore."
    );

    println!("cargo::rerun-if-changed=src/cli/depacker.js");

    // Two variants of the JS depacker, from one source: `PROGRESS` is substituted by bun and the
    // dead branch is eliminated, so the --no-progress-bar depacker contains no reference to the
    // document at all.
    build_depacker(true, "generated/depacker.js.min")?;
    build_depacker(false, "generated/depacker_noprogress.js.min")?;

    // Same guard as above: if bun ever stops folding the `PROGRESS` define, fail the build rather
    // than silently shipping the progress bar in the --no-progress-bar depacker.
    let progress_len = fs::metadata("generated/depacker.js.min")?.len();
    let noprogress_len = fs::metadata("generated/depacker_noprogress.js.min")?.len();
    anyhow::ensure!(
        noprogress_len < progress_len,
        "generated/depacker_noprogress.js.min ({noprogress_len} bytes) is not smaller than \
         generated/depacker.js.min ({progress_len} bytes) -- bun may no longer be \
         substituting and dead-code-eliminating the `PROGRESS` define."
    );

    Ok(())
}

/// Minifies one variant of the JS depacker, with the progress bar compiled in or out.
fn build_depacker(progress: bool, out_path: &str) -> anyhow::Result<()> {
    cmd!(
        "bun",
        "build",
        "src/cli/depacker.js",
        "--minify",
        "--define",
        format!("PROGRESS={progress}"),
        format!("--outfile={out_path}")
    )
    .run()?;
    Ok(())
}

/// Assembles, dead-code-eliminates and shrinks one variant of the decompressor stub.
fn build_decompress_stub(wat_path: &str, out_path: &str) -> anyhow::Result<()> {
    cmd!("wasm-as", "--enable-bulk-memory-opt", "--enable-simd", wat_path, "-o", out_path).run()?;
    cmd!("wasm-metadce", "--enable-bulk-memory-opt", "--enable-simd", out_path, "--graph-file", "target/decompress.jsonlike", "-o", out_path).run()?;
    cmd!("wasm-opt", "--enable-bulk-memory-opt", "--enable-simd", out_path, "-Oz", "-o", out_path).run()?;
    Ok(())
}

use indicatif::{ProgressBar, ProgressStyle};
use mashi::compress;
use std::io::Write;
use std::path::PathBuf;
use std::{fs, io};
use wasm_encoder::reencode::{Reencode, RoundtripReencoder};
use wasm_encoder::{ConstExpr, DataSection};
use wasmparser::Payload;
use zopfli::{Format, Options};

pub fn add_bytes_into_existing_data_section(
    input: &[u8],
    bytes: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let mut out = wasm_encoder::Module::new();
    let parser = wasmparser::Parser::new(0);

    let mut pending_code: Option<(wasm_encoder::CodeSection, u32)> = None;

    for p in parser.parse_all(input) {
        match p? {
            // When we hit the existing data section, copy & extend it.
            Payload::DataSection(sec) => {
                let mut data = DataSection::new();
                RoundtripReencoder.parse_data_section(&mut data, sec)?;
                data.active(0, &ConstExpr::i32_const(0x700000i32), bytes.iter().copied());
                out.section(&data);
            }

            // Everything else we pass through unchanged using the round-trip helpers.
            Payload::TypeSection(s) => {
                use wasm_encoder::TypeSection;
                let mut b = TypeSection::new();
                RoundtripReencoder.parse_type_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::ImportSection(s) => {
                use wasm_encoder::ImportSection;
                let mut b = ImportSection::new();
                RoundtripReencoder.parse_import_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::FunctionSection(s) => {
                use wasm_encoder::FunctionSection;
                let mut b = FunctionSection::new();
                RoundtripReencoder.parse_function_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::TableSection(s) => {
                use wasm_encoder::TableSection;
                let mut b = TableSection::new();
                RoundtripReencoder.parse_table_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::MemorySection(s) => {
                use wasm_encoder::MemorySection;
                let mut b = MemorySection::new();
                RoundtripReencoder.parse_memory_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::GlobalSection(s) => {
                use wasm_encoder::GlobalSection;
                let mut b = GlobalSection::new();
                RoundtripReencoder.parse_global_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::ExportSection(s) => {
                use wasm_encoder::ExportSection;
                let mut b = ExportSection::new();
                RoundtripReencoder.parse_export_section(&mut b, s)?;
                out.section(&b);
            }
            Payload::StartSection { func, .. } => {
                // If present, preserve the start section.
                out.section(&wasm_encoder::StartSection { function_index: func });
            }
            Payload::ElementSection(s) => {
                use wasm_encoder::ElementSection;
                let mut b = ElementSection::new();
                RoundtripReencoder.parse_element_section(&mut b, s)?;
                out.section(&b);
            }

            Payload::CodeSectionStart { count, .. } => {
                // Create a fresh encoder section and remember the number of bodies we expect.
                pending_code = Some((wasm_encoder::CodeSection::new(), count));
            }
            Payload::CodeSectionEntry(func_body) => {
                let (code, remaining) = pending_code
                    .as_mut()
                    .expect("CodeSectionEntry without CodeSectionStart");
                RoundtripReencoder.parse_function_body(code, func_body)?;
                *remaining -= 1;
                if *remaining == 0 {
                    // We consumed all function bodies; now emit the single code section.
                    let (finished_code, _) = pending_code.take().unwrap();
                    out.section(&finished_code);
                }
            }
            Payload::DataCountSection { count, .. } => {
                // If present (bulk memory), you *may* need to bump this.
                // Adding an *active* segment usually doesn't require DataCount,
                // but if the original module already has DataCount, keep it consistent:
                out.section(&wasm_encoder::DataCountSection { count: count + 1 });
            }
            Payload::CustomSection(cs) => {
                // Preserve custom sections exactly.
                RoundtripReencoder.parse_custom_section(&mut out, cs)?;
            }
            Payload::Version { .. } | Payload::End(_) => {}
            // Handle any other payloads as needed…
            other => {
                // If you encounter new proposals in your inputs, consider
                // mapping them via the matching RoundtripReencoder helpers.
                eprintln!("Unhandled payload: {:?}", other);
            }
        }
    }

    Ok(out.finish())
}

pub fn pack(wasm_filename: Option<PathBuf>, js_filename: PathBuf, output_filename: PathBuf) -> anyhow::Result<()> {
    let js_input = fs::read(&js_filename)?;
    let wasm_input = match wasm_filename {
        Some(filename) => fs::read(&filename)?,
        None => vec![]
    };

    let total_length = js_input.len() + wasm_input.len();
    if total_length > 7 * 1024 * 1024 {
        anyhow::bail!("Input/depacked length exceeds 7mb, this is currently not supported.");
    }

    let p = ProgressBar::new(total_length as u64).with_prefix("Compressing...");
    p.set_style(ProgressStyle::with_template("{prefix} [{bar:40.cyan/blue} {pos:>7}/{len:7}] {msg}")?
        .progress_chars("##-"));
    let (compressed_data, _) = compress(&js_input, &wasm_input, |progress| {
        p.set_position(progress as u64);
    });
    p.finish_with_message("Done!");

    // Create a new WASM module that has the compressed data already inserted in its memory
    let js_depacker: &[u8] = include_bytes!("../../generated/depacker.js.min");
    let decompressor = add_bytes_into_existing_data_section(include_bytes!("../../generated/decompress.wasm"), &compressed_data)?;

    let mut bundle = vec![];
    bundle.extend(js_depacker);
    bundle.extend(&decompressor);

    // Zopflify it
    print!("Zopfli... ");
    io::stdout().flush()?;
    let mut deflated = vec![];
    zopfli::compress(Options {
        iteration_count: std::num::NonZero::new(1000).unwrap(),
        ..Default::default()
    }, Format::Deflate, &*bundle, &mut deflated)?;
    println!("Done!");

    if deflated.len() > 1024 * 1024 {
        anyhow::bail!("The packed data exceeds 1mb, this is currently not supported.");
    }

    let header = include_str!("header.html");
    let header = header.replace("@@@", &header.len().to_string());
    let header = header.replace("%%%", &js_depacker.len().to_string());
    let mut output = fs::File::create(&output_filename)?;
    output.write_all(header.as_bytes())?;
    output.write_all(&deflated)?;
    println!("Wrote {} bytes to: {}", console::style(output.metadata()?.len()).green(), output_filename.display());

    Ok(())
}
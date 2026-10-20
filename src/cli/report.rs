use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

const HOT_SPOT_SIZE: usize = 256;
const HOT_SPOT_COUNT: usize = 5;

/// Heatmap byte for an input byte: its cost in 1/16 bits, saturating at 255 (15.9 bits).
fn heat(bits: f32) -> u8 {
    (bits * 16.0).round().clamp(0.0, 255.0) as u8
}

/// Writes one heat byte per input byte to `<prefix>.<name>.heat`.
pub fn write_heatmap(prefix: &Path, name: &str, bits_per_byte: &[f32]) -> anyhow::Result<PathBuf> {
    let mut filename = prefix.as_os_str().to_owned();
    filename.push(format!(".{name}.heat"));
    let filename = PathBuf::from(filename);
    let heat_bytes: Vec<u8> = bits_per_byte.iter().map(|&b| heat(b)).collect();
    fs::write(&filename, heat_bytes)?;
    Ok(filename)
}

fn total_bytes(bits_per_byte: &[f32]) -> f64 {
    bits_per_byte.iter().map(|&b| b as f64).sum::<f64>() / 8.0
}

fn print_line(label: &str, bits_per_byte: &[f32]) {
    if bits_per_byte.is_empty() {
        return;
    }
    let packed = total_bytes(bits_per_byte);
    println!(
        "  {label:<14} {:>9} -> {:>11.1} bytes  ({:.3} bits/byte)",
        bits_per_byte.len(),
        packed,
        packed * 8.0 / bits_per_byte.len() as f64
    );
}

fn print_hot_spots(bits_per_byte: &[f32]) {
    let mut regions: Vec<(usize, f64)> = bits_per_byte
        .chunks(HOT_SPOT_SIZE)
        .enumerate()
        .map(|(i, chunk)| (i * HOT_SPOT_SIZE, chunk.iter().map(|&b| b as f64).sum::<f64>() / chunk.len() as f64))
        .collect();
    regions.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (offset, avg) in regions.into_iter().take(HOT_SPOT_COUNT) {
        println!("    offset {offset:>8}  {avg:.3} bits/byte  (next {HOT_SPOT_SIZE} bytes)");
    }
}

/// Prints where the compressed bits went. `code_section` is absolute into `js ++ payload`.
pub fn print_report(js_len: usize, bits_per_byte: &[f32], code_section: Range<u64>) {
    let (js, payload) = bits_per_byte.split_at(js_len);
    println!("Compression report (model cost, excluding the decompressor stub and zopfli):");
    print_line("total", bits_per_byte);
    print_line("javascript", js);
    print_line("payload", payload);
    if code_section.end > code_section.start {
        let (start, end) = (code_section.start as usize, code_section.end as usize);
        print_line("  wasm code", &bits_per_byte[start..end]);
        print_line("  wasm other", &bits_per_byte[js_len..start]);
        print_line("  wasm trailing", &bits_per_byte[end..]);
    }
    for (label, region) in [("javascript", js), ("payload", payload)] {
        if !region.is_empty() {
            println!("Most expensive regions in {label}:");
            print_hot_spots(region);
        }
    }
}

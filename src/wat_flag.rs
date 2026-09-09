//! Single source of truth for the `$use_wasm_model` compile-time switch in `decompress.wat`.
//!
//! `build.rs` uses this to produce the binary/JS-only variant of the decompressor stub, and
//! `src/lib.rs`'s `WasmDecompressor` uses it to instantiate the matching wasmi module for tests.
//! Keeping the marker strings here means the two can't silently drift apart.

/// The line as it appears in `decompress.wat` when the WASM context model is compiled in.
pub const FLAG_ON: &str = "(global $use_wasm_model i32 (i32.const 1))";

/// The line to substitute in to compile the WASM context model back out.
pub const FLAG_OFF: &str = "(global $use_wasm_model i32 (i32.const 0))";

/// Returns `wat` with the compile-time switch flipped to 0 (WASM context model compiled out).
///
/// Panics if the marker line isn't found exactly once, so a future edit to `decompress.wat` that
/// changes the global's declaration can't silently produce a stub that still contains the model.
pub fn without_wasm_model(wat: &str) -> String {
    let count = wat.matches(FLAG_ON).count();
    assert_eq!(
        count, 1,
        "expected exactly one occurrence of `{FLAG_ON}` in decompress.wat, found {count}"
    );
    wat.replacen(FLAG_ON, FLAG_OFF, 1)
}

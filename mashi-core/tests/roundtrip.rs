use mashi_core::codec::{Decoder, Encoder};

#[test]
fn can_roundtrip() {
    let input: &[u8] = include_bytes!("../../tests/test.wasm");

    let mut encoder = Encoder::new();
    let compressed = encoder.encode(input);
    println!("compressed size: {:?}", compressed.len());

    let mut decoder = Decoder::new();
    let output = decoder.decode(&compressed);
    assert_eq!(input, output);
}
